use std::path::PathBuf;

use crate::model::{Anticheat, Profile, Runner};

use super::*;

fn ctx<'a>(home: &'a Path, battleye: Option<&'a Path>, eac: Option<&'a Path>) -> LaunchContext<'a> {
    LaunchContext {
        home,
        search_path: &[],
        debug: false,
        extra_args: &[],
        artifacts_dir: home,
        battleye_runtime: battleye,
        eac_runtime: eac,
        umu_runtime_update: false,
    }
}

fn profile(battleye: bool, eac: bool) -> Profile {
    Profile {
        anticheat: Anticheat { battleye, eac },
        ..Profile::new("p", Runner::Wine)
    }
}

#[test]
fn anticheat_runtimes_are_exported() {
    let dir = tempfile::tempdir().unwrap();
    let battleye = dir.path().join("battleye");
    let eac = dir.path().join("eac");
    std::fs::create_dir_all(&battleye).unwrap();
    std::fs::create_dir_all(&eac).unwrap();
    let ctx = ctx(dir.path(), Some(&battleye), Some(&eac));

    let env = anticheat_env(&profile(true, true), &ctx).unwrap();

    assert_eq!(env["PROTON_BATTLEYE_RUNTIME"], battleye.as_os_str());
    assert_eq!(env["PROTON_EAC_RUNTIME"], eac.as_os_str());
    assert!(
        anticheat_env(&profile(false, false), &ctx)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn missing_anticheat_runtimes_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let missing = PathBuf::from("/nonexistent/battleye");

    let problems =
        anticheat_env(&profile(true, true), &ctx(dir.path(), Some(&missing), None)).unwrap_err();

    assert_eq!(
        problems,
        [
            "`battleye_runtime` directory not found: /nonexistent/battleye",
            "`eac_runtime` is not set in config.toml",
        ]
    );
}

#[test]
fn native_ignores_anticheat() {
    let dir = tempfile::tempdir().unwrap();
    let profile = Profile {
        runner: Runner::Native,
        ..profile(true, false)
    };

    let command = command(&profile, None, None, false, &ctx(dir.path(), None, None)).unwrap();

    assert!(command.env.is_empty());
}
