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

#[test]
fn valid_names() {
    for name in ["wow", "Witcher-3", "game_1.2"] {
        assert_eq!(validate_name(name), Ok(()), "{name}");
    }
}

#[test]
fn invalid_names() {
    for name in ["", ".", "..", "-wow", "my game", "a/b", "wöw"] {
        assert!(validate_name(name).is_err(), "{name}");
    }
}

#[test]
fn issue_display() {
    let issue = Issue {
        severity: Severity::Error,
        subject: Subject::Game("wow".into()),
        message: "broken".into(),
    };

    assert_eq!(issue.to_string(), "error: game `wow`: broken");
}

#[cfg(feature = "wine")]
#[test]
fn minimal_wine_profile_is_valid() {
    assert_eq!(wine_profile().validate(&ctx()), Vec::new());
}

#[cfg(feature = "umu")]
#[test]
fn minimal_umu_profile_is_valid() {
    assert_eq!(umu_profile().validate(&ctx()), Vec::new());
}

#[test]
fn native_profile_is_always_available() {
    assert_eq!(
        Profile::new("native", Runner::Native).validate(&ctx()),
        Vec::new()
    );
}

#[cfg(not(feature = "umu"))]
#[test]
fn runner_without_feature_is_an_error() {
    let issues = umu_profile().validate(&ctx());

    assert_eq!(
        errors(&issues),
        ["runner `umu` is not available: lootrs was built without the `umu` feature"]
    );
}

#[test]
fn prefix_required_for_wine_and_umu() {
    for runner in [Runner::Wine, Runner::Umu] {
        let issues = Profile::new("p", runner).validate(&ctx());

        assert!(
            errors(&issues)
                .iter()
                .any(|message| message.starts_with("`prefix` is required")),
            "{runner}: {issues:?}"
        );
    }
}

#[test]
fn relative_paths_are_errors() {
    let profile = Profile {
        prefix: Some("prefixes/default".into()),
        dxvk: Some("dxvk".into()),
        wine: Some(WineSection {
            path: Some("wine".into()),
        }),
        ..wine_profile()
    };

    let issues = profile.validate(&ctx());
    let errors = errors(&issues);

    assert!(errors.iter().any(|m| m.starts_with("`prefix` must be")));
    assert!(errors.iter().any(|m| m.starts_with("`dxvk` must be")));
    assert!(errors.iter().any(|m| m.starts_with("`wine.path` must be")));
}

#[test]
fn umu_requires_proton() {
    let mut profile = umu_profile();
    profile.umu = None;
    assert!(errors(&profile.validate(&ctx())).contains(&PROTON_REQUIRED_MESSAGE));

    profile.umu = Some(UmuSection::default());
    assert!(errors(&profile.validate(&ctx())).contains(&PROTON_REQUIRED_MESSAGE));
}

const PROTON_REQUIRED_MESSAGE: &str = "`[umu].proton` is required: an absolute path to a Proton \
     build (without it umu would download Proton)";

#[test]
fn dxvk_and_vkd3d_ignored_for_umu() {
    let profile = Profile {
        dxvk: Some("/dxvk".into()),
        vkd3d: Some("relative-is-not-checked".into()),
        ..umu_profile()
    };

    let issues = profile.validate(&ctx());

    assert!(
        errors(&issues)
            .iter()
            .all(|message| !message.contains("dxvk") && !message.contains("vkd3d"))
    );
    let warnings = warnings(&issues);
    assert!(warnings[0].starts_with("`dxvk` is ignored for runner `umu`"));
    assert!(warnings[1].starts_with("`vkd3d` is ignored for runner `umu`"));
}

#[test]
fn mismatched_sections_are_ignored_with_warnings() {
    let profile = Profile {
        umu: Some(UmuSection::default()),
        ..wine_profile()
    };
    assert!(warnings(&profile.validate(&ctx())).contains(&"`[umu]` is ignored for runner `wine`"));

    let profile = Profile {
        prefix: Some("/pfx".into()),
        wine: Some(WineSection::default()),
        dll_overrides: BTreeMap::from([("d3d11".into(), "n".into())]),
        anticheat: Anticheat {
            battleye: true,
            eac: false,
        },
        ..Profile::new("native", Runner::Native)
    };
    let issues = profile.validate(&ctx());

    assert_eq!(errors(&issues), Vec::<&str>::new());
    assert_eq!(
        warnings(&issues),
        [
            "`prefix` is ignored for runner `native`",
            "`[wine]` is ignored for runner `native`",
            "`dll_overrides` is ignored for runner `native`",
            "`[anticheat]` is ignored for runner `native`",
        ]
    );
}

#[test]
fn dll_override_modes() {
    for mode in ["n", "b", "n,b", "b,n", "native,builtin", ""] {
        assert_eq!(
            validate_dll_override_for_test("winhttp", mode),
            Ok(()),
            "{mode}"
        );
    }
    for (dll, mode) in [("winhttp", "x"), ("winhttp", "n,"), ("", "n"), ("a=b", "n")] {
        assert!(
            validate_dll_override_for_test(dll, mode).is_err(),
            "{dll}={mode}"
        );
    }
}

fn validate_dll_override_for_test(dll: &str, mode: &str) -> Result<(), ()> {
    let profile = Profile {
        dll_overrides: BTreeMap::from([(dll.to_owned(), mode.to_owned())]),
        ..wine_profile()
    };
    let issues = profile.validate(&ctx());
    if errors(&issues).iter().any(|m| m.contains("dll_overrides")) {
        Err(())
    } else {
        Ok(())
    }
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

    let missing = profile.validate(&ctx());
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
        errors(&profile.validate(&configured))
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
        warnings(&profile.validate(&configured))
            .contains(&"anti-cheat runtimes only work with Proton-based wine builds")
    );
}

#[test]
fn bad_env_and_wrappers() {
    let profile = Profile {
        env: BTreeMap::from([("A=B".into(), "1".into())]),
        wrappers: vec!["mangohud".into(), " ".into()],
        ..wine_profile()
    };

    let issues = profile.validate(&ctx());

    assert!(errors(&issues).contains(&"`wrappers` must not contain empty entries"));
    assert!(errors(&issues).contains(&"`env` key `A=B` is not a valid variable name"));
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

    let issues = game.validate(None, &ctx());
    let errors = errors(&issues);

    assert!(errors[0].contains("contains ` `"));
    assert_eq!(errors[1], "profile `default` does not exist");
    assert!(errors[2].starts_with("`exe` must be an absolute path"));
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
