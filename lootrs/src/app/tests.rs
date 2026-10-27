use std::{collections::HashMap, ffi::OsString};

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
    let err = run(&settings(Action::Status { json: false })).unwrap_err();

    assert_eq!(err.exit_code(), 1);
    assert_eq!(
        err.to_string(),
        "`status` is not implemented yet (planned for M3)"
    );
}

#[test]
fn tui_points_at_its_milestone() {
    let err = run(&settings(Action::Tui)).unwrap_err();

    assert!(err.to_string().contains("M6"));
}
