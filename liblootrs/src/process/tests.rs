use std::process::{Child, Command};

use super::*;

fn process(pid: u32, comm: &str, owner: Option<Owner>, prefix: Option<&str>) -> Process {
    Process {
        pid,
        comm: comm.into(),
        exe: None,
        tags: Tags {
            owner,
            prefix: prefix.map(PathBuf::from),
        },
    }
}

fn game(name: &str) -> Option<Owner> {
    Some(Owner::Game(name.into()))
}

#[test]
fn parse_tags() {
    let environ = b"PATH=/bin\0LOOTRS_GAME=wow\0LOOTRS_PREFIX=/pfx\0X\0";

    assert_eq!(
        parse_environ(environ),
        Tags {
            owner: game("wow"),
            prefix: Some("/pfx".into())
        }
    );
    assert_eq!(
        parse_environ(b"LOOTRS_SESSION=execute:p:1\0").owner,
        Some(Owner::Session("execute:p:1".into()))
    );
    assert_eq!(parse_environ(b"HOME=/h\0"), Tags::default());
}

#[test]
fn game_tag_wins_over_session() {
    let tags = parse_environ(b"LOOTRS_GAME=wow\0LOOTRS_SESSION=s\0");

    assert_eq!(tags.owner, game("wow"));
}

#[test]
fn scan_fake_proc_tree() {
    let proc_dir = tempfile::tempdir().unwrap();
    let uid = std::fs::metadata(proc_dir.path()).unwrap().uid();
    let add = |pid: &str, comm: &str, environ: &[u8]| {
        let dir = proc_dir.path().join(pid);
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        std::fs::write(dir.join("environ"), environ).unwrap();
        std::os::unix::fs::symlink("/usr/bin/wine-preloader", dir.join("exe")).unwrap();
    };
    add("20", "Wow.exe", b"LOOTRS_GAME=wow\0");
    add("10", "bash", b"HOME=/h\0");
    add("30", "self", b"");
    std::fs::create_dir(proc_dir.path().join("sys")).unwrap();

    let processes = scan_dir(proc_dir.path(), uid, 30);

    assert_eq!(processes.len(), 2);
    assert_eq!(processes[0].pid, 10);
    assert_eq!(processes[1].comm, "Wow.exe");
    assert_eq!(processes[1].tags.owner, game("wow"));
    assert_eq!(
        processes[1].exe.as_deref(),
        Some(Path::new("/usr/bin/wine-preloader"))
    );
    assert!(scan_dir(proc_dir.path(), uid + 1, 0).is_empty());
}

#[test]
fn infrastructure_alone_is_not_running() {
    let processes = [
        process(1, "wineserver", game("a"), Some("/p")),
        process(2, "Game.exe", game("b"), Some("/p")),
        process(3, "services.exe", game("b"), Some("/p")),
        process(4, "bash", None, None),
    ];

    let running = running(&processes);

    assert_eq!(
        running.keys().collect::<Vec<_>>(),
        [&Owner::Game("b".into())]
    );
    assert_eq!(running[&Owner::Game("b".into())].len(), 2);
    assert!(is_running(&processes, &Owner::Game("b".into())));
    assert!(!is_running(&processes, &Owner::Game("a".into())));
}

#[test]
fn stop_alone_in_prefix_takes_infrastructure() {
    let processes = [
        process(1, "wineserver", game("a"), Some("/p")),
        process(2, "A.exe", game("a"), Some("/p")),
        process(3, "services.exe", game("old"), Some("/p")),
        process(4, "wineserver", game("c"), Some("/other")),
        process(5, "bash", None, None),
    ];

    assert_eq!(
        stop_targets(&processes, &Owner::Game("a".into())),
        [1, 2, 3]
    );
}

#[test]
fn stop_spares_shared_infrastructure() {
    let processes = [
        process(1, "wineserver", game("a"), Some("/p")),
        process(2, "A.exe", game("a"), Some("/p")),
        process(3, "B.exe", game("b"), Some("/p")),
    ];

    assert_eq!(stop_targets(&processes, &Owner::Game("a".into())), [2]);
    // The last game out takes the infrastructure, whatever its tag.
    assert_eq!(stop_targets(&processes, &Owner::Game("b".into())), [3]);
    assert_eq!(stop_targets(&processes[..1], &Owner::Game("a".into())), [1]);
}

#[test]
fn stop_native_game_without_prefix() {
    let processes = [
        process(1, "game", game("native"), None),
        process(2, "wineserver", game("x"), Some("/p")),
    ];

    assert_eq!(stop_targets(&processes, &Owner::Game("native".into())), [1]);
    assert!(stop_targets(&processes, &Owner::Game("none".into())).is_empty());
}

#[test]
fn killall_scopes() {
    let mut preloader = process(4, "Long Game Name", None, None);
    preloader.exe = Some("/opt/proton/files/bin/wine64-preloader".into());
    let processes = [
        process(1, "A.exe", game("a"), Some("/p")),
        process(2, "Steam.exe", None, None),
        process(3, "firefox", None, None),
        preloader,
        process(5, "umu-run", None, None),
    ];

    assert_eq!(killall_targets(&processes, false), [1]);
    assert_eq!(killall_targets(&processes, true), [1, 2, 4, 5]);
}

/// Spawns `sh -c script` tagged with a test-unique game name.
fn spawn_tagged(tag: &str, script: &str) -> Child {
    Command::new("sh")
        .args(["-c", script])
        .env(TAG_GAME, tag)
        .spawn()
        .unwrap()
}

fn tagged(tag: &str) -> impl Fn(&[Process]) -> Vec<u32> {
    let owner = Owner::Game(tag.to_owned());
    move |processes| stop_targets(processes, &owner)
}

#[test]
fn terminate_real_processes() {
    let tag = format!("test-terminate-{}", std::process::id());
    let mut children = [
        spawn_tagged(&tag, "sleep 30"),
        spawn_tagged(&tag, "sleep 30 & wait"),
    ];
    thread::sleep(Duration::from_millis(200));
    assert!(tagged(&tag)(&scan()).len() >= 3);

    let outcome = terminate(scan, tagged(&tag), Duration::from_secs(5));

    assert!(outcome.signaled >= 3, "{outcome:?}");
    assert_eq!(outcome.killed, 0);
    for child in &mut children {
        child.wait().unwrap();
    }
    assert!(tagged(&tag)(&scan()).is_empty());
}

#[test]
fn terminate_escalates_to_sigkill() {
    let tag = format!("test-escalate-{}", std::process::id());
    let mut child = spawn_tagged(&tag, "trap '' TERM; while :; do sleep 0.05; done");
    thread::sleep(Duration::from_millis(200));

    let outcome = terminate(scan, tagged(&tag), Duration::from_millis(300));

    assert!(outcome.killed >= 1, "{outcome:?}");
    child.wait().unwrap();
}

#[test]
fn orphaned_helpers_detection() {
    let processes = [
        process(1, "winedevice.exe", game("a"), Some("/p")),
        process(2, "wineserver", game("a"), Some("/p")),
        process(3, "Game.exe", game("a"), Some("/p")),
    ];

    assert!(orphaned_helpers(&processes, &[1]));
    assert!(!orphaned_helpers(&processes, &[1, 2]));
    assert!(!orphaned_helpers(&processes, &[1, 3]));
}

#[test]
fn terminate_nothing() {
    let outcome = terminate(Vec::new, |_| Vec::new(), Duration::from_secs(1));

    assert_eq!(outcome, Outcome::default());
}
