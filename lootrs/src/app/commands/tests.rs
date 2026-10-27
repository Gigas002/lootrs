use std::{collections::BTreeMap, path::Path};

use liblootrs::model::{Game, Profile, Runner};

use super::*;

fn ctx() -> ValidationContext<'static> {
    ValidationContext {
        home: Path::new("/home/u"),
        battleye_runtime: false,
        eac_runtime: false,
    }
}

fn game(name: &str, profile: &str, exe: &Path) -> Game {
    Game {
        name: name.into(),
        exe: exe.to_path_buf(),
        profile: profile.into(),
        args: Vec::new(),
        cwd: None,
        game_id: None,
        env: BTreeMap::new(),
    }
}

fn catalog(exe: &Path) -> Catalog {
    Catalog {
        games: vec![
            game("openmw", "native", exe),
            game("orphan", "missing", exe),
        ],
        profiles: vec![
            Profile {
                prefix: Some("/pfx/default".into()),
                ..Profile::new("default", Runner::Wine)
            },
            Profile::new("native", Runner::Native),
        ],
        load_errors: Vec::new(),
    }
}

fn output(f: impl FnOnce(&mut Vec<u8>) -> Result<(), AppError>) -> (String, Result<(), AppError>) {
    let mut out = Vec::new();
    let result = f(&mut out);
    (String::from_utf8(out).unwrap(), result)
}

#[test]
fn list_shows_runner_or_question_mark() {
    let (out, result) = output(|out| list(&catalog(Path::new("/bin/true")), out));

    result.unwrap();
    assert_eq!(
        out,
        "NAME    PROFILE  RUNNER  EXE\n\
         openmw  native   native  /bin/true\n\
         orphan  missing  ?       /bin/true\n"
    );
}

#[test]
fn profiles_shows_game_counts() {
    let (out, result) = output(|out| profiles(&catalog(Path::new("/bin/true")), out));

    result.unwrap();
    assert_eq!(
        out,
        "NAME     RUNNER  GAMES  PREFIX\n\
         default  wine    0      /pfx/default\n\
         native   native  1      -\n"
    );
}

#[test]
fn empty_lists() {
    let empty = Catalog::default();

    assert_eq!(output(|out| list(&empty, out)).0, "no games\n");
    assert_eq!(output(|out| profiles(&empty, out)).0, "no profiles\n");
}

#[test]
fn check_all_reports_errors() {
    // Only native profiles, so the result doesn't depend on compiled-in runners.
    let mut catalog = catalog(Path::new("/bin/true"));
    catalog
        .profiles
        .retain(|profile| profile.runner == Runner::Native);

    let (out, result) = output(|out| check(&catalog, &ctx(), &CheckTarget::All, out));

    assert!(matches!(result, Err(AppError::CheckFailed(ref s)) if s == "1 error"));
    assert!(out.contains("error: game `orphan`: profile `missing` does not exist"));
    assert!(out.ends_with("checked 2 games and 1 profile: 1 error, 0 warnings\n"));
}

#[test]
fn check_single_game_ok() {
    let (out, result) = output(|out| {
        check(
            &catalog(Path::new("/bin/true")),
            &ctx(),
            &CheckTarget::Game("openmw".into()),
            out,
        )
    });

    result.unwrap();
    assert_eq!(out, "checked game `openmw`: no problems\n");
}

#[test]
fn check_warnings_do_not_fail() {
    let (out, result) = output(|out| {
        check(
            &catalog(Path::new("/nonexistent/game")),
            &ctx(),
            &CheckTarget::Game("openmw".into()),
            out,
        )
    });

    result.unwrap();
    assert!(out.contains("warning: game `openmw`: `exe` not found"));
    assert!(out.ends_with("checked game `openmw`: 0 errors, 1 warning\n"));
}

#[test]
fn check_unknown_targets() {
    let catalog = catalog(Path::new("/bin/true"));

    let (_, game) = output(|out| check(&catalog, &ctx(), &CheckTarget::Game("x".into()), out));
    let (_, profile) =
        output(|out| check(&catalog, &ctx(), &CheckTarget::Profile("x".into()), out));

    assert!(matches!(game, Err(AppError::UnknownGame(_))));
    assert!(matches!(profile, Err(AppError::UnknownProfile(_))));
}

fn write(dir: &Path, relative: &str, content: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn check_profile_ignores_unrelated_broken_files() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "profiles/native.toml",
        "name = \"native\"\nrunner = \"native\"\n",
    );
    write(dir.path(), "profiles/bad.toml", "name = 1");

    let (out, result) = output(|out| {
        check(
            &Catalog::load(dir.path()),
            &ctx(),
            &CheckTarget::Profile("native".into()),
            out,
        )
    });

    result.unwrap();
    assert_eq!(out, "checked profile `native`: no problems\n");
}

#[test]
fn check_game_explains_broken_profile_file() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "games.toml",
        "[[game]]\nname = \"g\"\nexe = \"/bin/true\"\nprofile = \"native\"\n",
    );
    write(dir.path(), "profiles/native.toml", "name = 1");
    write(dir.path(), "profiles/other.toml", "name = 1");

    let (out, result) = output(|out| {
        check(
            &Catalog::load(dir.path()),
            &ctx(),
            &CheckTarget::Game("g".into()),
            out,
        )
    });

    assert!(result.is_err());
    assert!(out.contains("native.toml: invalid TOML"), "{out}");
    assert!(!out.contains("other.toml"), "{out}");
    assert!(out.contains("profile `native` does not exist"), "{out}");
}
