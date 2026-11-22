//! lootrs' own checks. The profile format and its file-level checks are
//! libgameprofiles' and tested there.

use std::{collections::BTreeMap, path::Path};

use super::*;

const HOME: &str = "/home/u";

fn ctx() -> ValidationContext<'static> {
    ValidationContext {
        home: Path::new(HOME),
        battleye_runtime: false,
        eac_runtime: false,
    }
}

fn messages(issues: &[Issue], severity: Severity) -> Vec<&str> {
    issues
        .iter()
        .filter(|issue| issue.severity == severity)
        .map(|issue| issue.message.as_str())
        .collect()
}

fn errors(issues: &[Issue]) -> Vec<&str> {
    messages(issues, Severity::Error)
}

fn warnings(issues: &[Issue]) -> Vec<&str> {
    messages(issues, Severity::Warning)
}

fn wine_profile() -> Profile {
    Profile {
        prefix: Some("~/prefixes/default".into()),
        ..Profile::new("default", Runner::Wine)
    }
}

fn umu_profile() -> Profile {
    Profile {
        prefix: Some("/pfx/proton".into()),
        umu: Some(UmuSection {
            proton: Some("/proton/GE-Proton10-20".into()),
            ..UmuSection::default()
        }),
        ..Profile::new("proton", Runner::Umu)
    }
}

fn game(exe: &Path) -> Game {
    Game {
        name: "wow".into(),
        exe: exe.to_path_buf(),
        profile: "default".into(),
        args: Vec::new(),
        cwd: None,
        game_id: None,
        env: BTreeMap::new(),
    }
}

#[cfg(feature = "wine")]
#[test]
fn minimal_wine_profile_is_valid() {
    assert_eq!(validate_profile(&wine_profile(), &ctx()), Vec::new());
}

#[cfg(feature = "umu")]
#[test]
fn minimal_umu_profile_is_valid() {
    assert_eq!(validate_profile(&umu_profile(), &ctx()), Vec::new());
}

#[test]
fn native_profile_is_always_available() {
    assert!(runner_available(Runner::Native));
    assert_eq!(
        validate_profile(&Profile::new("native", Runner::Native), &ctx()),
        Vec::new()
    );
}

#[test]
fn file_issues_come_first() {
    // libgameprofiles' own checks run too: a relative prefix is its error.
    let profile = Profile {
        prefix: Some("relative".into()),
        ..Profile::new("p", Runner::Wine)
    };
    let issues = validate_profile(&profile, &ctx());
    assert!(errors(&issues)[0].starts_with("`prefix` must be an absolute path"));
}

#[test]
fn proton_is_not_launched() {
    assert!(!runner_available(Runner::Proton));
    let profile = Profile {
        prefix: Some("/pfx".into()),
        proton: Some(ProtonSection {
            build: Some("experimental".into()),
        }),
        steam: Some(SteamIntegration::Host),
        ..Profile::new("proton", Runner::Proton)
    };
    assert_eq!(
        errors(&validate_profile(&profile, &ctx())),
        ["runner `proton` runs through Steam; lootrs can't launch it"]
    );
    assert!(warnings(&validate_profile(&profile, &ctx())).is_empty());
}

#[test]
fn slr_profiles_are_sruman_s() {
    let profile = Profile {
        prefix: Some("/pfx".into()),
        slr: Some(SlrSection::new("sniper")),
        ..Profile::new("sniper", Runner::Native)
    };
    assert_eq!(
        errors(&validate_profile(&profile, &ctx())),
        [SLR_UNSUPPORTED]
    );
}

#[cfg(not(feature = "umu"))]
#[test]
fn runner_without_feature_is_an_error() {
    let issues = validate_profile(&umu_profile(), &ctx());

    assert_eq!(
        errors(&issues),
        ["runner `umu` is not available: lootrs was built without the `umu` feature"]
    );
}

#[test]
fn prefix_required_for_wine_and_umu() {
    for runner in [Runner::Wine, Runner::Umu] {
        let issues = validate_profile(&Profile::new("p", runner), &ctx());

        assert!(
            errors(&issues)
                .iter()
                .any(|message| message.starts_with("`prefix` is required")),
            "{runner}: {issues:?}"
        );
    }
}

#[test]
fn umu_requires_proton() {
    let mut profile = umu_profile();
    profile.umu = None;
    assert!(errors(&validate_profile(&profile, &ctx())).contains(&PROTON_REQUIRED));

    profile.umu = Some(UmuSection::default());
    assert!(errors(&validate_profile(&profile, &ctx())).contains(&PROTON_REQUIRED));
}

#[test]
fn anticheat_needs_configured_runtime() {
    let profile = Profile {
        anticheat: Anticheat {
            battleye: true,
            eac: true,
        },
        ..umu_profile()
    };

    let missing = validate_profile(&profile, &ctx());
    assert!(
        errors(&missing).contains(&"`anticheat.battleye` needs `battleye_runtime` in config.toml")
    );
    assert!(errors(&missing).contains(&"`anticheat.eac` needs `eac_runtime` in config.toml"));

    let configured = ValidationContext {
        battleye_runtime: true,
        eac_runtime: true,
        ..ctx()
    };
    assert!(
        errors(&validate_profile(&profile, &configured))
            .iter()
            .all(|m| !m.contains("anticheat"))
    );
}

#[test]
fn anticheat_with_wine_warns() {
    let profile = Profile {
        anticheat: Anticheat {
            battleye: false,
            eac: true,
        },
        ..wine_profile()
    };
    let configured = ValidationContext {
        eac_runtime: true,
        ..ctx()
    };

    assert!(
        warnings(&validate_profile(&profile, &configured))
            .contains(&"anti-cheat runtimes only work with Proton-based wine builds")
    );
}

#[test]
fn game_with_existing_exe_is_valid() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("game.exe");
    std::fs::write(&exe, "").unwrap();
    let profile = wine_profile();

    assert_eq!(game(&exe).validate(Some(&profile), &ctx()), Vec::new());
}

#[test]
fn game_missing_files_are_warnings() {
    let mut game = game(Path::new("/nonexistent/game.exe"));
    game.cwd = Some("~/nonexistent".into());
    let profile = wine_profile();

    let issues = game.validate(Some(&profile), &ctx());

    assert_eq!(errors(&issues), Vec::<&str>::new());
    assert_eq!(
        warnings(&issues),
        [
            "`exe` not found: /nonexistent/game.exe",
            "`cwd` not found: /home/u/nonexistent",
        ]
    );
}

#[test]
fn game_errors() {
    let mut game = game(Path::new("game.exe"));
    game.name = "my game".into();
    game.args = vec!["a\0b".into()];
    game.env = BTreeMap::from([("A=B".into(), "1".into())]);

    let issues = game.validate(None, &ctx());
    let errors = errors(&issues);

    assert!(errors[0].contains("contains ` `"));
    assert_eq!(errors[1], "profile `default` does not exist");
    assert!(errors[2].starts_with("`exe` must be an absolute path"));
    assert_eq!(errors[3], "`args` must not contain a NUL character");
    assert_eq!(errors[4], "`env` key `A=B` is not a valid variable name");
}

#[test]
fn game_id_ignored_outside_umu() {
    let mut game = game(Path::new("/nonexistent/game.exe"));
    game.game_id = Some("umu-1".into());
    let profile = wine_profile();

    assert!(
        warnings(&game.validate(Some(&profile), &ctx()))
            .contains(&"`game_id` is ignored: profile `default` uses runner `wine`")
    );
}
