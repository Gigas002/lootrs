//! End-to-end checks of the `lootrs` binary against an isolated HOME.

use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{Duration, Instant},
};

fn lootrs(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lootrs"))
        .args(args)
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_STATE_HOME")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_RUNTIME_DIR")
        .output()
        .unwrap()
}

#[test]
fn help_lists_subcommands() {
    let home = tempfile::tempdir().unwrap();
    let output = lootrs(home.path(), &["--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for command in [
        "run", "stop", "killall", "init", "tricks", "execute", "check",
    ] {
        assert!(stdout.contains(command), "missing `{command}` in --help");
    }
}

#[test]
fn version_flag() {
    let home = tempfile::tempdir().unwrap();
    let output = lootrs(home.path(), &["--version"]);

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("lootrs ")
    );
}

#[test]
fn command_logs_to_stderr_and_xdg_state_dir() {
    let home = tempfile::tempdir().unwrap();
    let output = lootrs(home.path(), &["status"]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("`status` is not implemented yet"),
        "{stderr}"
    );

    let log_dir = home.path().join(".local/state/lootrs/logs");
    let logs: String = std::fs::read_dir(&log_dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert!(logs.contains("`status` is not implemented yet"));
}

#[test]
fn invalid_config_exits_with_usage_code() {
    let home = tempfile::tempdir().unwrap();
    let config_dir = home.path().join(".config/lootrs");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), "log_keep = 0").unwrap();

    let output = lootrs(home.path(), &["list"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("log_keep")
    );
}

#[test]
fn bad_arguments_are_usage_errors() {
    let home = tempfile::tempdir().unwrap();

    assert_eq!(lootrs(home.path(), &["init"]).status.code(), Some(2));
    assert_eq!(lootrs(home.path(), &["nope"]).status.code(), Some(2));
}

/// Copies `examples/` into a fresh config dir.
fn config_with_examples(home: &Path) -> std::path::PathBuf {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let config = home.join(".config/lootrs");
    std::fs::create_dir_all(config.join("profiles")).unwrap();
    std::fs::copy(examples.join("games.toml"), config.join("games.toml")).unwrap();
    for entry in std::fs::read_dir(examples.join("profiles")).unwrap() {
        let path = entry.unwrap().path();
        std::fs::copy(
            &path,
            config.join("profiles").join(path.file_name().unwrap()),
        )
        .unwrap();
    }
    config
}

#[test]
fn list_and_profiles_show_examples() {
    let home = tempfile::tempdir().unwrap();
    config_with_examples(home.path());

    let list = lootrs(home.path(), &["list"]);
    let profiles = lootrs(home.path(), &["profiles"]);

    assert!(list.status.success());
    let list = String::from_utf8(list.stdout).unwrap();
    assert!(list.starts_with("NAME"));
    assert!(list.contains("witcher3  proton"));
    assert!(profiles.status.success());
    assert!(
        String::from_utf8(profiles.stdout)
            .unwrap()
            .contains("native   native")
    );
}

#[test]
fn check_examples_has_only_warnings() {
    let home = tempfile::tempdir().unwrap();
    config_with_examples(home.path());

    let output = lootrs(home.path(), &["check"]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    if cfg!(all(feature = "wine", feature = "umu")) {
        assert!(output.status.success(), "{stdout}");
        assert!(
            stdout.contains("checked 3 games and 3 profiles: 0 errors"),
            "{stdout}"
        );
    } else {
        assert_eq!(output.status.code(), Some(2), "{stdout}");
        assert!(stdout.contains("is not available"), "{stdout}");
    }
}

#[test]
fn check_reports_broken_profile_and_fails() {
    let home = tempfile::tempdir().unwrap();
    let config = config_with_examples(home.path());
    std::fs::write(
        config.join("profiles/native.toml"),
        "name = \"wrong\"\nrunner = \"native\"\n",
    )
    .unwrap();

    let output = lootrs(home.path(), &["check"]);
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stdout.contains("native.toml: `name = \"wrong\"` must match the file name `native`"),
        "{stdout}"
    );
    assert!(
        stdout.contains("game `openmw`: profile `native` does not exist"),
        "{stdout}"
    );
}

#[test]
fn check_unknown_game_is_usage_error() {
    let home = tempfile::tempdir().unwrap();

    let output = lootrs(home.path(), &["check", "nope"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("no game named `nope`")
    );
}

/// A native profile plus a fake game script that echoes its args and env, then exits 3.
fn fake_game(home: &Path) -> PathBuf {
    let config = home.join(".config/lootrs");
    std::fs::create_dir_all(config.join("profiles")).unwrap();
    std::fs::create_dir_all(home.join("game")).unwrap();

    let script = home.join("game/game.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\necho \"args=$* tag=$LOOTRS_GAME var=$GAME_VAR cwd=$(pwd)\"\necho oops >&2\nexit 3\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    std::fs::write(
        config.join("games.toml"),
        "[[game]]\nname = \"fake\"\nexe = \"~/game/game.sh\"\nprofile = \"native\"\nargs = [\"-a\"]\nenv = { GAME_VAR = \"v\" }\n",
    )
    .unwrap();
    std::fs::write(
        config.join("profiles/native.toml"),
        "name = \"native\"\nrunner = \"native\"\n",
    )
    .unwrap();
    home.join(".local/state/lootrs/logs/games/fake")
}

fn read_latest(logs: &Path) -> String {
    std::fs::read_to_string(logs.join("latest.log")).unwrap()
}

#[test]
fn run_wait_passes_exit_code_and_logs_output() {
    let home = tempfile::tempdir().unwrap();
    let logs = fake_game(home.path());

    let output = lootrs(home.path(), &["run", "fake", "--wait", "-q", "--", "extra"]);

    assert_eq!(output.status.code(), Some(3));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let expected = format!(
        "args=-a extra tag=fake var=v cwd={}",
        home.path().join("game").display()
    );
    assert_eq!(stdout.trim(), expected);
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "oops\n");

    let log = read_latest(&logs);
    assert!(log.contains(&format!("[stdout] {expected}")), "{log}");
    assert!(log.contains("[stderr] oops"), "{log}");
    assert!(log.contains("=== exit: code 3"), "{log}");
}

#[test]
fn run_detached_returns_and_supervisor_finishes_log() {
    let home = tempfile::tempdir().unwrap();
    let logs = fake_game(home.path());

    let output = lootrs(home.path(), &["run", "fake"]);

    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("started `fake` (pid "), "{stdout}");
    assert!(stdout.contains(&logs.display().to_string()), "{stdout}");

    let deadline = Instant::now() + Duration::from_secs(10);
    while !read_latest(&logs).contains("=== exit:") {
        assert!(
            Instant::now() < deadline,
            "supervisor never wrote the footer"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(read_latest(&logs).contains("=== exit: code 3"));

    let path = lootrs(home.path(), &["logs", "fake", "--path"]);
    assert_eq!(
        String::from_utf8(path.stdout).unwrap().trim(),
        std::fs::canonicalize(logs.join("latest.log"))
            .unwrap()
            .display()
            .to_string()
    );
}

#[test]
fn run_preflight_failure_is_logged() {
    let home = tempfile::tempdir().unwrap();
    let logs = fake_game(home.path());
    std::fs::remove_file(home.path().join("game/game.sh")).unwrap();

    let output = lootrs(home.path(), &["run", "fake"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("executable not found"), "{stderr}");
    assert!(read_latest(&logs).contains("=== preflight failed ==="));
}

#[test]
fn execute_native_program_from_path() {
    let home = tempfile::tempdir().unwrap();
    fake_game(home.path());

    let output = lootrs(
        home.path(),
        &[
            "execute",
            "-p",
            "native",
            "--wait",
            "-q",
            "sh",
            "--",
            "-c",
            "echo $LOOTRS_SESSION",
        ],
    );

    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("execute:native:")
    );
}

#[test]
fn run_unknown_game() {
    let home = tempfile::tempdir().unwrap();
    fake_game(home.path());

    let output = lootrs(home.path(), &["run", "nope"]);

    assert_eq!(output.status.code(), Some(2));
}
