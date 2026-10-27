use std::{collections::HashMap, ffi::OsString, os::unix::process::ExitStatusExt};

use liblootrs::model::{Game, Runner};

use super::*;
use crate::settings::CliOverrides;

fn settings(action: Action) -> Settings {
    let vars: HashMap<&str, OsString> = HashMap::from([("HOME", "/nonexistent-home".into())]);
    let cli = CliOverrides {
        action,
        ..CliOverrides::default()
    };
    Settings::resolve(cli, move |key| vars.get(key).cloned()).unwrap()
}

fn strings(args: Vec<OsString>) -> Vec<String> {
    args.into_iter()
        .map(|arg| arg.into_string().unwrap())
        .collect()
}

#[test]
fn status_ok() {
    let started = read_status("ok\t42\t/logs/x.log\n".as_bytes()).unwrap();

    assert_eq!(
        started,
        Started {
            pid: 42,
            log: "/logs/x.log".into()
        }
    );
}

#[test]
fn status_error_carries_code_and_message() {
    let err =
        read_status("error\t2\nnot launching:\nexecutable not found\n".as_bytes()).unwrap_err();

    assert_eq!(err.exit_code(), 2);
    assert_eq!(err.to_string(), "not launching:\nexecutable not found");
}

#[test]
fn status_missing() {
    let err = read_status("".as_bytes()).unwrap_err();

    assert_eq!(err.exit_code(), 1);
    assert!(err.to_string().contains("without reporting"));
}

#[test]
fn supervised_args_for_run() {
    let settings = settings(Action::Run {
        game: "wow".into(),
        mode: LaunchMode::Detach,
        debug: true,
        args: vec!["-x".into()],
    });

    let args = strings(supervised_args(&settings));

    assert_eq!(
        args[6..],
        ["run", "wow", "--supervised", "--debug", "--", "-x"]
    );
    assert_eq!(args[0], "--config-dir");
    assert_eq!(args[1], "/nonexistent-home/.config/lootrs");
    assert_eq!(args[4..6], ["--log-level", "info"]);
}

#[test]
fn supervised_args_for_execute() {
    let settings = settings(Action::Execute {
        profile: "p".into(),
        target: "/dl/setup.exe".into(),
        mode: LaunchMode::Detach,
        debug: false,
        args: Vec::new(),
    });

    assert_eq!(
        strings(supervised_args(&settings))[6..],
        ["execute", "--profile", "p", "/dl/setup.exe", "--supervised"]
    );
}

#[test]
fn footer_and_exit_codes() {
    let code = ExitStatus::from_raw(3 << 8);
    let killed = ExitStatus::from_raw(9);

    assert_eq!(
        footer(code, Duration::from_secs(3725)),
        "=== exit: code 3 — duration 01:02:05 ===\n"
    );
    assert_eq!(
        footer(killed, Duration::ZERO),
        "=== exit: signal 9 — duration 00:00:00 ===\n"
    );
    assert_eq!(exit_code(code), 3);
    assert_eq!(exit_code(killed), 137);
}

#[test]
fn header_sections() {
    let game = Game {
        name: "g".into(),
        exe: "/g/game".into(),
        profile: "native".into(),
        args: Vec::new(),
        cwd: None,
        game_id: None,
        env: Default::default(),
    };
    let profile = Profile::new("native", Runner::Native);
    let spec = LaunchSpec {
        program: "/g/game".into(),
        args: vec!["a b".into()],
        env: [("LOOTRS_GAME".to_owned(), OsString::from("g"))].into(),
        cwd: "/g".into(),
        runner: None,
    };

    let header = header(
        Request::Game { name: "g" },
        Target::Game(&game),
        &profile,
        Some(&spec),
        None,
        Local::now(),
    );

    assert!(header.starts_with(&format!(
        "=== lootrs {} — run g — ",
        env!("CARGO_PKG_VERSION")
    )));
    for section in [
        "--- game ---",
        "--- profile ---",
        "--- command ---\n/g/game 'a b'\n",
        "LOOTRS_GAME=g\n",
        "--- system ---",
    ] {
        assert!(header.contains(section), "missing {section:?} in\n{header}");
    }
    assert!(header.ends_with("=== output ===\n"));
}
