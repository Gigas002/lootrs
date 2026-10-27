use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, path::PathBuf};

use crate::{
    launch::{LaunchContext, Tag, Target, prepare},
    model::{Game, Profile, Runner, WineSection},
};

use super::*;

fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct Fixture {
    dir: tempfile::TempDir,
    search_path: Vec<PathBuf>,
    artifacts: PathBuf,
    game: Game,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        executable(&dir.path().join("bin/wine"));
        std::fs::create_dir_all(dir.path().join("game")).unwrap();
        std::fs::write(dir.path().join("game/Game.exe"), "").unwrap();
        let game = Game {
            name: "g".into(),
            exe: dir.path().join("game/Game.exe"),
            profile: "p".into(),
            args: vec!["-windowed".into()],
            cwd: None,
            game_id: None,
            env: BTreeMap::new(),
        };
        Self {
            search_path: vec![dir.path().join("bin")],
            artifacts: dir.path().join("artifacts"),
            dir,
            game,
        }
    }

    fn ctx(&self, debug: bool) -> LaunchContext<'_> {
        LaunchContext {
            home: self.dir.path(),
            search_path: &self.search_path,
            debug,
            extra_args: &[],
            artifacts_dir: &self.artifacts,
        }
    }

    fn profile(&self) -> Profile {
        Profile {
            prefix: Some("~/pfx".into()),
            ..Profile::new("p", Runner::Wine)
        }
    }

    fn prepare(
        &self,
        profile: &Profile,
        debug: bool,
    ) -> Result<crate::launch::LaunchSpec, Vec<String>> {
        prepare(
            Target::Game(&self.game),
            profile,
            &Tag::Game("g".into()),
            &self.ctx(debug),
        )
        .map_err(|err| err.problems)
    }
}

#[test]
fn wine_from_path() {
    let fx = Fixture::new();
    let profile = Profile {
        runner_args: vec!["--runner-arg".into()],
        ..fx.profile()
    };

    let spec = fx.prepare(&profile, false).unwrap();

    let wine = fx.dir.path().join("bin/wine");
    assert_eq!(spec.program, wine);
    assert_eq!(spec.runner.as_deref(), Some(wine.as_path()));
    assert_eq!(
        spec.args,
        [
            "--runner-arg".into(),
            fx.game.exe.clone().into_os_string(),
            "-windowed".into(),
        ]
    );
    let prefix = fx.dir.path().join("pfx");
    assert_eq!(spec.env["WINEPREFIX"], prefix.as_os_str());
    assert_eq!(spec.env[crate::launch::TAG_PREFIX], prefix.as_os_str());
    assert_eq!(spec.env["WINEARCH"], "win64");
    assert_eq!(spec.env["WINEDEBUG"], DEFAULT_WINEDEBUG);
    assert_eq!(
        spec.env["DXVK_LOG_PATH"],
        fx.dir.path().join("artifacts").as_os_str()
    );
    assert!(!spec.env.contains_key("DXVK_LOG_LEVEL"));
    assert!(!spec.env.contains_key(DLL_OVERRIDES_VAR));
}

#[test]
fn debug_from_flag_or_profile() {
    let fx = Fixture::new();

    let flag = fx.prepare(&fx.profile(), true).unwrap();
    let profile = fx
        .prepare(
            &Profile {
                debug: true,
                ..fx.profile()
            },
            false,
        )
        .unwrap();

    for spec in [flag, profile] {
        assert_eq!(spec.env["WINEDEBUG"], DEBUG_WINEDEBUG);
        assert_eq!(spec.env["DXVK_LOG_LEVEL"], "info");
    }
}

#[test]
fn wine_path_as_binary_or_build_dir() {
    let fx = Fixture::new();
    let custom = fx.dir.path().join("wine-ge/bin/wine");
    executable(&custom);

    for path in ["~/wine-ge", "~/wine-ge/bin/wine"] {
        let profile = Profile {
            wine: Some(WineSection {
                path: Some(path.into()),
            }),
            ..fx.profile()
        };
        assert_eq!(
            fx.prepare(&profile, false).unwrap().program,
            custom,
            "{path}"
        );
    }
}

#[test]
fn missing_wine_is_reported() {
    let fx = Fixture::new();
    let profile = Profile {
        wine: Some(WineSection {
            path: Some("/nonexistent/wine".into()),
        }),
        ..fx.profile()
    };

    assert_eq!(
        fx.prepare(&profile, false).unwrap_err(),
        ["wine not found at /nonexistent/wine"]
    );

    let mut fx = Fixture::new();
    fx.search_path.clear();
    assert!(
        fx.prepare(&fx.profile(), false).unwrap_err()[0].starts_with("`wine` not found in PATH")
    );
}

#[test]
fn dll_overrides_and_builtin_programs() {
    let fx = Fixture::new();
    let profile = Profile {
        dll_overrides: BTreeMap::from([("winhttp".into(), "n,b".into())]),
        ..fx.profile()
    };

    let spec = prepare(
        Target::Execute {
            program: Path::new("winecfg"),
        },
        &profile,
        &Tag::Session("execute:p:1".into()),
        &fx.ctx(false),
    )
    .unwrap();

    assert_eq!(spec.env[DLL_OVERRIDES_VAR], "winhttp=n,b");
    assert_eq!(spec.args, ["winecfg"]);
    assert_eq!(spec.cwd, fx.dir.path());
}
