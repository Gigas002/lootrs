use std::{collections::HashMap, ffi::OsString, path::Path};

use super::*;

const EXAMPLE_CONFIG: &str = include_str!("../../../examples/config.toml");

/// Environment with `HOME` and XDG dirs inside `root`.
fn env(root: &Path) -> impl Fn(&str) -> Option<OsString> + use<> {
    let vars: HashMap<&str, OsString> = HashMap::from([
        ("HOME", root.join("home").into()),
        ("XDG_CONFIG_HOME", root.join("config").into()),
        ("XDG_RUNTIME_DIR", root.join("run").into()),
    ]);
    move |key| vars.get(key).cloned()
}

fn write_config(root: &Path, content: &str) {
    let dir = root.join("config/lootrs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), content).unwrap();
}

fn list_action() -> CliOverrides {
    CliOverrides {
        action: Action::List { json: false },
        ..CliOverrides::default()
    }
}

#[test]
fn defaults_without_config_file() {
    let root = tempfile::tempdir().unwrap();
    let settings = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap();

    assert_eq!(settings.dirs.config, root.path().join("config/lootrs"));
    assert_eq!(settings.dirs.runtime, root.path().join("run/lootrs"));
    assert_eq!(
        settings.dirs.prefix_root,
        root.path().join("home/.local/share/lootrs/prefixes")
    );
    assert_eq!(
        settings.logging.dir,
        root.path().join("home/.local/state/lootrs/logs")
    );
    assert_eq!(settings.logging.level, LogLevel::Info);
    assert_eq!(settings.logging.keep, DEFAULT_LOG_KEEP);
    assert_eq!(
        settings.stop_timeout,
        Duration::from_secs(DEFAULT_STOP_TIMEOUT_SECS)
    );
    assert!(!settings.umu_runtime_update);
    assert_eq!(settings.anticheat, AnticheatRuntimes::default());
}

#[test]
fn example_config_matches_defaults() {
    let root = tempfile::tempdir().unwrap();
    let defaults = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap();
    write_config(root.path(), EXAMPLE_CONFIG);

    let settings = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap();

    assert_eq!(settings, defaults);
}

#[test]
fn config_overrides_defaults() {
    let root = tempfile::tempdir().unwrap();
    write_config(
        root.path(),
        r#"
log_level = "debug"
log_dir = "~/logs"
log_keep = 3
stop_timeout_secs = 0
prefix_root = "/games/prefixes"
umu_runtime_update = true
battleye_runtime = "~/be"
eac_runtime = ""
"#,
    );

    let settings = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap();

    assert_eq!(settings.logging.level, LogLevel::Debug);
    assert_eq!(settings.logging.dir, root.path().join("home/logs"));
    assert_eq!(settings.logging.keep, 3);
    assert_eq!(settings.stop_timeout, Duration::ZERO);
    assert_eq!(settings.dirs.prefix_root, Path::new("/games/prefixes"));
    assert!(settings.umu_runtime_update);
    assert_eq!(
        settings.anticheat.battleye,
        Some(root.path().join("home/be"))
    );
    assert_eq!(settings.anticheat.eac, None);
}

#[test]
fn cli_overrides_config() {
    let root = tempfile::tempdir().unwrap();
    write_config(
        root.path(),
        "log_level = \"debug\"\nlog_dir = \"/from/config\"",
    );

    let cli = CliOverrides {
        log_dir: Some(root.path().join("cli-logs")),
        log_level: Some(LogLevel::Error),
        ..CliOverrides::default()
    };
    let settings = Settings::resolve(cli, env(root.path())).unwrap();

    assert_eq!(settings.logging.level, LogLevel::Error);
    assert_eq!(settings.logging.dir, root.path().join("cli-logs"));
}

#[test]
fn cli_config_dir_selects_config_file() {
    let root = tempfile::tempdir().unwrap();
    let custom = root.path().join("custom");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(custom.join("config.toml"), "log_keep = 7").unwrap();

    let cli = CliOverrides {
        config_dir: Some(custom.clone()),
        ..CliOverrides::default()
    };
    let settings = Settings::resolve(cli, env(root.path())).unwrap();

    assert_eq!(settings.dirs.config, custom);
    assert_eq!(settings.logging.keep, 7);
}

#[test]
fn relative_config_path_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    write_config(root.path(), "log_dir = \"logs\"");

    let err = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap_err();

    assert!(matches!(
        err,
        SettingsError::RelativePath { key: "log_dir", .. }
    ));
}

#[test]
fn zero_log_keep_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    write_config(root.path(), "log_keep = 0");

    let err = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap_err();

    assert!(matches!(err, SettingsError::LogKeep));
}

#[test]
fn invalid_config_file_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    write_config(root.path(), "log_level = 5");

    let err = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap_err();

    assert!(matches!(err, SettingsError::Config(_)));
}

#[test]
fn stderr_logging_only_outside_tui() {
    let root = tempfile::tempdir().unwrap();

    let tui = Settings::resolve(CliOverrides::default(), env(root.path())).unwrap();
    let cli = Settings::resolve(list_action(), env(root.path())).unwrap();

    assert!(!tui.logging.stderr);
    assert!(cli.logging.stderr);
    assert_eq!(cli.action, Action::List { json: false });
}

#[test]
fn missing_home_is_an_error() {
    let err = Settings::resolve(CliOverrides::default(), |_| None).unwrap_err();

    assert!(matches!(err, SettingsError::Paths(PathsError::NoHome)));
}
