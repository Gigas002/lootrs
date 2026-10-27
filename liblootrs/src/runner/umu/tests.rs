use std::{collections::BTreeMap, os::unix::fs::PermissionsExt};

use crate::{
    launch::{LaunchSpec, Tag, Target, prepare},
    model::{Game, Profile, Runner, UmuSection},
};

use super::*;

fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Temp dir with `bin/umu-run`, a Proton build in `proton/` and `game/Game.exe`.
struct Fixture {
    dir: tempfile::TempDir,
    search_path: Vec<PathBuf>,
    artifacts: PathBuf,
    runtime_update: bool,
    game: Game,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        executable(&dir.path().join("bin/umu-run"));
        executable(&dir.path().join("proton/proton"));
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
            runtime_update: false,
            dir,
            game,
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn ctx(&self, debug: bool) -> LaunchContext<'_> {
        LaunchContext {
            home: self.dir.path(),
            search_path: &self.search_path,
            debug,
            extra_args: &[],
            artifacts_dir: &self.artifacts,
            battleye_runtime: None,
            eac_runtime: None,
            umu_runtime_update: self.runtime_update,
        }
    }

    fn profile(&self) -> Profile {
        Profile {
            prefix: Some("~/pfx".into()),
            umu: Some(UmuSection {
                proton: Some(self.path("proton")),
                ..UmuSection::default()
            }),
            ..Profile::new("p", Runner::Umu)
        }
    }

    fn prepare(&self, target: Target, profile: &Profile) -> Result<LaunchSpec, Vec<String>> {
        prepare(target, profile, &Tag::Game("g".into()), &self.ctx(false))
            .map_err(|err| err.problems)
    }

    fn run(&self, profile: &Profile) -> Result<LaunchSpec, Vec<String>> {
        self.prepare(Target::Game(&self.game), profile)
    }
}

#[test]
fn umu_run_from_path_with_proton_env() {
    let fx = Fixture::new();
    let profile = Profile {
        runner_args: vec!["--runner-arg".into()],
        ..fx.profile()
    };

    let spec = fx.run(&profile).unwrap();

    let umu_run = fx.path("bin/umu-run");
    assert_eq!(spec.program, umu_run);
    assert_eq!(spec.runner.as_deref(), Some(umu_run.as_path()));
    assert_eq!(
        spec.args,
        [
            "--runner-arg".into(),
            fx.game.exe.clone().into_os_string(),
            "-windowed".into(),
        ]
    );
    assert_eq!(spec.env["WINEPREFIX"], fx.path("pfx").as_os_str());
    assert_eq!(spec.env["PROTONPATH"], fx.path("proton").as_os_str());
    assert_eq!(spec.env["GAMEID"], DEFAULT_GAME_ID);
    assert_eq!(spec.env["UMU_RUNTIME_UPDATE"], "0");
    for unset in ["STORE", "UMU_LOG", "PROTON_LOG", "WINEARCH", "WINEDEBUG"] {
        assert!(!spec.env.contains_key(unset), "{unset}");
    }
}

#[test]
fn runtime_update_can_be_allowed() {
    let mut fx = Fixture::new();
    fx.runtime_update = true;

    let spec = fx.run(&fx.profile()).unwrap();

    assert!(!spec.env.contains_key("UMU_RUNTIME_UPDATE"));
}

#[test]
fn game_id_from_game_over_profile_and_store() {
    let mut fx = Fixture::new();
    let mut profile = fx.profile();
    profile.umu = Some(UmuSection {
        game_id: Some("umu-profile".into()),
        store: Some("gog".into()),
        ..profile.umu.unwrap()
    });

    assert_eq!(fx.run(&profile).unwrap().env["GAMEID"], "umu-profile");
    fx.game.game_id = Some("umu-1234".into());
    let spec = fx.run(&profile).unwrap();
    assert_eq!(spec.env["GAMEID"], "umu-1234");
    assert_eq!(spec.env["STORE"], "gog");
}

#[test]
fn debug_sets_proton_log_into_run_dir() {
    let fx = Fixture::new();

    let spec = prepare(
        Target::Game(&fx.game),
        &fx.profile(),
        &Tag::Game("g".into()),
        &fx.ctx(true),
    )
    .unwrap();

    assert!(!spec.env.contains_key("UMU_LOG"));
    assert_eq!(spec.env["PROTON_LOG"], "1");
    assert_eq!(spec.env["PROTON_LOG_DIR"], fx.artifacts.as_os_str());
}

#[test]
fn only_profile_dll_overrides_dxvk_ignored() {
    let fx = Fixture::new();
    let dll = fx.path("dxvk/x64/d3d11.dll");
    std::fs::create_dir_all(dll.parent().unwrap()).unwrap();
    std::fs::write(dll, "").unwrap();
    let mut profile = Profile {
        dxvk: Some("~/dxvk".into()),
        ..fx.profile()
    };

    assert!(
        !fx.run(&profile)
            .unwrap()
            .env
            .contains_key(DLL_OVERRIDES_VAR)
    );
    profile.dll_overrides = BTreeMap::from([("winhttp".into(), "n,b".into())]);
    assert_eq!(
        fx.run(&profile).unwrap().env[DLL_OVERRIDES_VAR],
        "winhttp=n,b"
    );
}

#[test]
fn missing_umu_run_and_proton_are_reported_together() {
    let mut fx = Fixture::new();
    fx.search_path.clear();
    let mut profile = fx.profile();
    profile.umu = Some(UmuSection {
        proton: Some("/nonexistent/proton".into()),
        ..UmuSection::default()
    });

    let problems = fx.run(&profile).unwrap_err();

    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].starts_with("`umu-run` not found in PATH"));
    assert_eq!(
        problems[1],
        "no Proton build at /nonexistent/proton (expected a `proton` script there)"
    );
}

#[test]
fn proton_path_must_be_absolute_and_present() {
    let fx = Fixture::new();
    let with_proton = |proton: Option<&str>| Profile {
        umu: Some(UmuSection {
            proton: proton.map(PathBuf::from),
            ..UmuSection::default()
        }),
        ..fx.profile()
    };

    assert_eq!(
        fx.run(&with_proton(None)).unwrap_err(),
        ["`[umu].proton` is required for runner `umu`"]
    );
    assert_eq!(
        fx.run(&with_proton(Some("GE-Proton"))).unwrap_err(),
        ["`[umu].proton` must be an absolute path: GE-Proton"]
    );
    assert!(fx.run(&with_proton(Some("~/proton"))).is_ok());
}

#[test]
fn custom_umu_run_path() {
    let fx = Fixture::new();
    executable(&fx.path("opt/umu-run"));
    let mut profile = fx.profile();
    profile.umu.as_mut().unwrap().umu_run = Some("~/opt/umu-run".into());

    assert_eq!(fx.run(&profile).unwrap().program, fx.path("opt/umu-run"));
    profile.umu.as_mut().unwrap().umu_run = Some("/nonexistent/umu-run".into());
    assert_eq!(
        fx.run(&profile).unwrap_err(),
        ["umu-run not found at /nonexistent/umu-run"]
    );
}

#[test]
fn tricks_runs_umu_run_winetricks() {
    let fx = Fixture::new();
    executable(&fx.path("bin/mangohud"));
    let profile = Profile {
        wrappers: vec!["mangohud".into()],
        runner_args: vec!["--ignored".into()],
        ..fx.profile()
    };
    let verbs = ["vcrun2022".to_owned()];

    let spec = fx
        .prepare(Target::Tricks { verbs: &verbs }, &profile)
        .unwrap();

    assert_eq!(spec.program, fx.path("bin/umu-run"));
    assert_eq!(spec.args, ["winetricks", "vcrun2022"]);
    assert!(!spec.env.contains_key("WINE"));
    assert_eq!(spec.env["PROTONPATH"], fx.path("proton").as_os_str());
}

#[test]
fn tricks_needs_verbs() {
    let fx = Fixture::new();

    let problems = fx
        .prepare(Target::Tricks { verbs: &[] }, &fx.profile())
        .unwrap_err();

    assert_eq!(
        problems,
        ["umu runs winetricks without its GUI; name the verbs to install"]
    );
}

#[test]
fn execute_builtin_program_is_passed_through() {
    let fx = Fixture::new();

    let spec = fx
        .prepare(
            Target::Execute {
                program: Path::new("winecfg"),
            },
            &fx.profile(),
        )
        .unwrap();

    assert_eq!(spec.args, ["winecfg"]);
}
