use std::{collections::HashMap, ffi::OsString, path::Path};

use super::*;
use crate::settings::{Action, CliOverrides};

fn settings(root: &Path, level: LogLevel) -> Settings {
    let vars: HashMap<&str, OsString> = HashMap::from([("HOME", root.into())]);
    let cli = CliOverrides {
        log_dir: Some(root.join("logs")),
        log_level: Some(level),
        action: Action::Tui,
        ..CliOverrides::default()
    };
    Settings::resolve(cli, move |key| vars.get(key).cloned()).unwrap()
}

fn read_logs(dir: &Path) -> String {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect()
}

#[test]
fn level_filter_mapping() {
    assert_eq!(level_filter(LogLevel::Error), LevelFilter::ERROR);
    assert_eq!(level_filter(LogLevel::Info), LevelFilter::INFO);
    assert_eq!(level_filter(LogLevel::Trace), LevelFilter::TRACE);
}

#[test]
fn writes_filtered_events_to_log_dir() {
    let root = tempfile::tempdir().unwrap();
    let settings = settings(root.path(), LogLevel::Info);

    let (subscriber, guard) = subscriber(&settings).unwrap();
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("kept message");
        tracing::debug!("filtered message");
    });
    drop(guard);

    let files: Vec<_> = std::fs::read_dir(root.path().join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(files.len(), 1);
    assert!(files[0].starts_with("lootrs.") && files[0].ends_with(".log"));

    let logs = read_logs(&root.path().join("logs"));
    assert!(logs.contains("kept message"));
    assert!(!logs.contains("filtered message"));
}

#[test]
fn unwritable_log_dir_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("logs"), "not a directory").unwrap();

    let result = subscriber(&settings(root.path(), LogLevel::Info));

    assert!(matches!(result, Err(LoggerError::CreateDir(_))));
}
