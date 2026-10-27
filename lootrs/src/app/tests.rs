use std::{collections::HashMap, ffi::OsString, path::Path};

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

#[test]
fn unimplemented_commands_fail_with_runtime_code() {
    let err = run(&settings(Action::Init {
        profile: "p".into(),
    }))
    .unwrap_err();

    assert_eq!(err.exit_code(), 1);
    assert_eq!(
        err.to_string(),
        "`init` is not implemented yet (planned for M4)"
    );
}

#[test]
fn process_state_exit_codes() {
    assert_eq!(AppError::NotRunning("g".into()).exit_code(), 3);
    assert_eq!(AppError::AlreadyRunning("g".into()).exit_code(), 4);
}

#[test]
fn logs_without_runs() {
    let err = run(&settings(Action::Logs {
        game: "wow".into(),
        path_only: true,
    }))
    .unwrap_err();

    assert_eq!(err.exit_code(), 1);
    assert_eq!(err.to_string(), "no run logs for `wow` yet");
}

#[test]
fn json_output_is_not_implemented_yet() {
    let err = run(&settings(Action::List { json: true })).unwrap_err();

    assert_eq!(
        err.to_string(),
        "`--json` is not implemented yet (planned for M7)"
    );
}

#[test]
fn tui_points_at_its_milestone() {
    let err = run(&settings(Action::Tui)).unwrap_err();

    assert!(err.to_string().contains("M6"));
}

#[test]
fn validation_errors_exit_with_usage_code() {
    assert_eq!(AppError::UnknownGame("x".into()).exit_code(), 2);
    assert_eq!(AppError::CheckFailed("1 error".into()).exit_code(), 2);
}

#[test]
fn validation_context_from_settings() {
    let mut settings = settings(Action::Tui);
    settings.anticheat.eac = Some("/eac".into());

    let ctx = validation_context(&settings);

    assert_eq!(ctx.home, Path::new("/nonexistent-home"));
    assert!(!ctx.battleye_runtime);
    assert!(ctx.eac_runtime);
}
