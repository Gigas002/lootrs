use std::path::Path;

use super::*;

const EXAMPLE_CONFIG: &str = include_str!("../../../examples/config.toml");

#[test]
fn empty_file_is_all_unset() {
    assert_eq!(parse("").unwrap(), FileConfig::default());
}

#[test]
fn example_config_parses() {
    let config = parse(EXAMPLE_CONFIG).unwrap();

    assert_eq!(config.log_level, Some(LogLevel::Info));
    assert_eq!(config.log_keep, Some(10));
    assert_eq!(config.stop_timeout_secs, Some(5));
    assert_eq!(config.umu_runtime_update, Some(false));
}

#[test]
fn unknown_keys_are_ignored() {
    let config = parse("log_keep = 3\nfuture_option = true\n").unwrap();

    assert_eq!(config.log_keep, Some(3));
}

#[test]
fn invalid_log_level_is_rejected() {
    assert!(parse("log_level = \"loud\"").is_err());
}

#[test]
fn missing_file_is_none() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(load(dir.path()).unwrap(), None);
}

#[test]
fn load_reads_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE_NAME), "log_dir = \"/var/log/lootrs\"").unwrap();

    let config = load(dir.path()).unwrap().unwrap();

    assert_eq!(
        config.log_dir.as_deref(),
        Some(Path::new("/var/log/lootrs"))
    );
}

#[test]
fn parse_error_names_the_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(FILE_NAME), "log_keep = \"many\"").unwrap();

    let err = load(dir.path()).unwrap_err();

    assert!(matches!(err, ConfigError::Parse { .. }));
    assert!(err.to_string().contains(FILE_NAME));
}
