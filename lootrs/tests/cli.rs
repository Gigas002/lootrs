//! End-to-end checks of the `lootrs` binary against an isolated HOME.

use std::{
    path::Path,
    process::{Command, Output},
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
    let output = lootrs(home.path(), &["list"]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("`list` is not implemented yet"), "{stderr}");

    let log_dir = home.path().join(".local/state/lootrs/logs");
    let logs: String = std::fs::read_dir(&log_dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert!(logs.contains("`list` is not implemented yet"));
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
