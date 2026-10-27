use std::{collections::BTreeMap, os::unix::fs::PermissionsExt};

use super::*;
use crate::model::Runner;

/// Temp dir with `bin/` (on the search path) and `game/game.bin`.
struct Fixture {
    dir: tempfile::TempDir,
    search_path: Vec<PathBuf>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("bin")).unwrap();
        std::fs::create_dir_all(dir.path().join("game")).unwrap();
        let fixture = Self {
            search_path: vec![dir.path().join("bin")],
            dir,
        };
        fixture.executable("game/game.bin");
        fixture
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn executable(&self, relative: &str) -> PathBuf {
        let path = self.path(relative);
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn ctx<'a>(&'a self, extra_args: &'a [String]) -> LaunchContext<'a> {
        LaunchContext {
            home: self.dir.path(),
            search_path: &self.search_path,
            debug: false,
            extra_args,
            artifacts_dir: self.dir.path(),
            battleye_runtime: None,
            eac_runtime: None,
            umu_runtime_update: false,
        }
    }

    fn game(&self) -> Game {
        Game {
            name: "g".into(),
            exe: self.path("game/game.bin"),
            profile: "p".into(),
            args: vec!["-a".into()],
            cwd: None,
            game_id: None,
            env: BTreeMap::from([("GAME_VAR".into(), "1".into())]),
        }
    }
}

fn tag() -> Tag {
    Tag::Game("g".into())
}

fn args(spec: &LaunchSpec) -> Vec<&str> {
    spec.args.iter().map(|a| a.to_str().unwrap()).collect()
}

#[test]
fn native_game() {
    let fx = Fixture::new();
    let profile = Profile {
        env: BTreeMap::from([
            ("PROFILE_VAR".into(), "x".into()),
            ("GAME_VAR".into(), "0".into()),
        ]),
        ..Profile::new("p", Runner::Native)
    };
    let extra = ["--extra".to_owned()];

    let spec = prepare(Target::Game(&fx.game()), &profile, &tag(), &fx.ctx(&extra)).unwrap();

    assert_eq!(spec.program, fx.path("game/game.bin"));
    assert_eq!(args(&spec), ["-a", "--extra"]);
    assert_eq!(spec.cwd, fx.path("game"));
    assert_eq!(spec.runner, None);
    assert_eq!(spec.env["GAME_VAR"], "1");
    assert_eq!(spec.env["PROFILE_VAR"], "x");
    assert_eq!(spec.env[TAG_GAME], "g");
    assert!(!spec.env.contains_key(TAG_PREFIX));
}

#[test]
fn wrappers_go_first_and_split_on_whitespace() {
    let fx = Fixture::new();
    let mangohud = fx.executable("bin/mangohud");
    let gamemode = fx.executable("bin/gamemoderun");
    let profile = Profile {
        wrappers: vec!["gamemoderun".into(), "mangohud --dlsym".into()],
        ..Profile::new("p", Runner::Native)
    };

    let spec = prepare(Target::Game(&fx.game()), &profile, &tag(), &fx.ctx(&[])).unwrap();

    assert_eq!(spec.program, gamemode);
    assert_eq!(
        spec.args,
        [
            mangohud.into_os_string(),
            "--dlsym".into(),
            fx.path("game/game.bin").into_os_string(),
            "-a".into(),
        ]
    );
}

#[test]
fn all_problems_are_reported() {
    let fx = Fixture::new();
    let mut game = fx.game();
    game.exe = fx.path("missing.exe");
    game.cwd = Some(fx.path("missing-dir"));
    let profile = Profile {
        wrappers: vec!["nope".into()],
        ..Profile::new("p", Runner::Native)
    };

    let err = prepare(Target::Game(&game), &profile, &tag(), &fx.ctx(&[])).unwrap_err();

    assert_eq!(err.problems.len(), 3, "{err}");
    assert!(err.problems[0].starts_with("executable not found"));
    assert!(err.problems[1].starts_with("working directory not found"));
    assert_eq!(err.problems[2], "wrapper `nope` not found");
}

#[test]
fn game_cwd_and_tilde() {
    let fx = Fixture::new();
    let mut game = fx.game();
    game.exe = "~/game/game.bin".into();
    game.cwd = Some("~".into());

    let spec = prepare(
        Target::Game(&game),
        &Profile::new("p", Runner::Native),
        &tag(),
        &fx.ctx(&[]),
    )
    .unwrap();

    assert_eq!(spec.program, fx.path("game/game.bin"));
    assert_eq!(spec.cwd, fx.dir.path());
}

#[test]
fn execute_native_resolves_path_programs() {
    let fx = Fixture::new();
    fx.executable("bin/tool");
    let profile = Profile::new("p", Runner::Native);
    let session = Tag::Session("execute:p:1".into());

    let spec = prepare(
        Target::Execute {
            program: Path::new("tool"),
        },
        &profile,
        &session,
        &fx.ctx(&["x".into()]),
    )
    .unwrap();

    assert_eq!(spec.program, fx.path("bin/tool"));
    assert_eq!(args(&spec), ["x"]);
    assert_eq!(spec.cwd, fx.path("bin"));
    assert_eq!(spec.env[TAG_SESSION], "execute:p:1");
    assert!(!spec.env.contains_key(TAG_GAME));

    let err = prepare(
        Target::Execute {
            program: Path::new("missing-tool"),
        },
        &profile,
        &session,
        &fx.ctx(&[]),
    )
    .unwrap_err();
    assert_eq!(err.problems, ["`missing-tool` not found in PATH"]);
}

#[test]
fn dll_overrides_parse_and_format() {
    let map = parse_dll_overrides("d3d11,dxgi=n,b; winhttp=n ;bad;x=");

    assert_eq!(map["d3d11"], "n,b");
    assert_eq!(map["dxgi"], "n,b");
    assert_eq!(map["winhttp"], "n");
    assert_eq!(map["x"], "");
    assert!(!map.contains_key("bad"));
    assert_eq!(
        format_dll_overrides(&map),
        "d3d11=n,b;dxgi=n,b;winhttp=n;x="
    );
}

#[test]
fn env_layers_merge_dll_overrides_per_dll() {
    let runner = BTreeMap::from([
        (DLL_OVERRIDES_VAR.to_owned(), "a=n;b=n".into()),
        ("KEEP".to_owned(), "runner".into()),
    ]);
    let profile = BTreeMap::from([(DLL_OVERRIDES_VAR.to_owned(), "b=b;c=n".to_owned())]);
    let game = BTreeMap::from([
        (DLL_OVERRIDES_VAR.to_owned(), "c=".to_owned()),
        ("KEEP".to_owned(), "game".to_owned()),
    ]);

    let env = merge_env(runner, &profile, Some(&game));

    assert_eq!(env[DLL_OVERRIDES_VAR], "a=n;b=b;c=");
    assert_eq!(env["KEEP"], "game");
}

#[test]
fn find_program_rules() {
    let fx = Fixture::new();
    std::fs::write(fx.path("bin/not-exec"), "").unwrap();

    assert_eq!(
        find_program(Path::new("game.bin"), &[fx.path("game")]),
        Some(fx.path("game/game.bin"))
    );
    assert_eq!(find_program(Path::new("not-exec"), &fx.search_path), None);
    assert_eq!(
        find_program(&fx.path("game/game.bin"), &[]),
        Some(fx.path("game/game.bin"))
    );
}

#[cfg(not(feature = "wine"))]
#[test]
fn wine_without_feature_is_a_problem() {
    let fx = Fixture::new();
    let profile = Profile {
        prefix: Some("/pfx".into()),
        ..Profile::new("p", Runner::Wine)
    };

    let err = prepare(Target::Game(&fx.game()), &profile, &tag(), &fx.ctx(&[])).unwrap_err();

    assert!(err.problems[0].contains("built without the `wine` feature"));
}

#[test]
fn tricks_need_a_prefix_runner() {
    let fx = Fixture::new();

    let err = prepare(
        Target::Tricks { verbs: &[] },
        &Profile::new("p", Runner::Native),
        &Tag::Session("tricks:p:1".into()),
        &fx.ctx(&[]),
    )
    .unwrap_err();

    assert_eq!(err.problems, ["`tricks` needs a wine or umu profile"]);
}
