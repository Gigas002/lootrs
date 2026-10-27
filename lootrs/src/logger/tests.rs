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
fn stderr_hides_info_by_default() {
    assert_eq!(stderr_filter(LogLevel::Info), LevelFilter::WARN);
    assert_eq!(stderr_filter(LogLevel::Error), LevelFilter::ERROR);
    assert_eq!(stderr_filter(LogLevel::Debug), LevelFilter::DEBUG);
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

#[test]
fn cli_format_is_plain() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("out.log");
    let file = std::fs::File::create(&path).unwrap();
    let subscriber = tracing_subscriber::registry().with(
        fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .event_format(CliFormat),
    );

    tracing::subscriber::with_default(subscriber, || {
        let _span = tracing::info_span!("command", name = "list").entered();
        tracing::warn!("broken file");
        tracing::error!(code = 2, "failed");
    });

    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "lootrs: warning: broken file\nlootrs: error: failed code=2\n"
    );
}
