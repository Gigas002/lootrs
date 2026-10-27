use std::{collections::BTreeMap, path::Path};

use super::*;
use crate::model::{Runner, UmuSection};

fn ctx(home: &Path) -> ValidationContext<'_> {
    ValidationContext {
        home,
        battleye_runtime: false,
        eac_runtime: false,
    }
}

fn write(dir: &Path, relative: &str, content: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn game(name: &str, profile: &str) -> Game {
    Game {
        name: name.into(),
        exe: "/games/game.exe".into(),
        profile: profile.into(),
        args: vec!["-a".into()],
        cwd: None,
        game_id: None,
        env: BTreeMap::from([("K".into(), "v".into())]),
    }
}

#[test]
fn empty_config_dir_loads_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog::load(dir.path());

    assert!(catalog.games.is_empty());
    assert!(catalog.profiles.is_empty());
    assert!(catalog.load_errors.is_empty());
}

#[test]
fn loads_games_and_sorted_profiles() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "games.toml",
        "[[game]]\nname = \"a\"\nexe = \"/a.exe\"\nprofile = \"z\"\n",
    );
    write(
        dir.path(),
        "profiles/z.toml",
        "name = \"z\"\nrunner = \"native\"\n",
    );
    write(
        dir.path(),
        "profiles/b.toml",
        "name = \"b\"\nrunner = \"native\"\n",
    );
    write(dir.path(), "profiles/notes.txt", "ignored");
    write(dir.path(), "profiles/c.toml.tmp", "ignored");

    let catalog = Catalog::load(dir.path());

    assert!(catalog.load_errors.is_empty(), "{:?}", catalog.load_errors);
    assert_eq!(catalog.games.len(), 1);
    let names: Vec<_> = catalog.profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["b", "z"]);
    assert_eq!(catalog.profile("z").unwrap().runner, Runner::Native);
    assert_eq!(catalog.game("a").unwrap().profile, "z");
    assert!(catalog.game("nope").is_none());
}

#[test]
fn broken_files_do_not_block_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "games.toml", "[[game]]\nname = 1\n");
    write(
        dir.path(),
        "profiles/good.toml",
        "name = \"good\"\nrunner = \"native\"\n",
    );
    write(dir.path(), "profiles/bad.toml", "runner = \"dosbox\"\n");
    write(
        dir.path(),
        "profiles/renamed.toml",
        "name = \"other\"\nrunner = \"native\"\n",
    );

    let catalog = Catalog::load(dir.path());

    assert_eq!(catalog.profiles.len(), 1);
    assert_eq!(catalog.load_errors.len(), 3);
    assert!(catalog.load_errors.iter().any(|err| matches!(
        err,
        StoreError::NameMismatch { name, stem, .. } if name == "other" && stem == "renamed"
    )));

    let issues = catalog.validate(&ctx(dir.path()));
    let file_errors = issues
        .iter()
        .filter(|issue| matches!(issue.subject, Subject::File(_)))
        .count();
    assert_eq!(file_errors, 3);
}

#[test]
fn duplicate_game_names_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog {
        games: vec![game("wow", "native"), game("wow", "native")],
        profiles: vec![Profile::new("native", Runner::Native)],
        load_errors: Vec::new(),
    };

    let issues = catalog.validate(&ctx(dir.path()));

    assert!(
        issues.iter().any(
            |issue| issue.severity == Severity::Error && issue.message == "duplicate game name"
        )
    );
}

#[test]
fn validate_single_game_includes_its_profile() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = Catalog {
        games: vec![game("wow", "proton")],
        profiles: vec![Profile::new("proton", Runner::Umu)],
        load_errors: Vec::new(),
    };
    let ctx = ctx(dir.path());

    let issues = catalog.validate_game("wow", &ctx).unwrap();

    assert!(
        issues
            .iter()
            .any(|i| i.subject == Subject::Game("wow".into()))
    );
    assert!(
        issues
            .iter()
            .any(|i| i.subject == Subject::Profile("proton".into()))
    );
    assert!(catalog.validate_game("nope", &ctx).is_none());
    assert!(catalog.validate_profile("proton", &ctx).is_some());
    assert!(catalog.validate_profile("nope", &ctx).is_none());
}

#[test]
fn games_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let games = vec![game("a", "p"), game("b", "p")];

    save_games(dir.path(), &games).unwrap();

    assert_eq!(load_games(dir.path()).unwrap(), games);
    assert!(!dir.path().join("games.toml.tmp").exists());
}

#[test]
fn profile_round_trip_with_all_sections() {
    let dir = tempfile::tempdir().unwrap();
    let profile = Profile {
        prefix: Some("~/pfx".into()),
        runner_args: vec!["--x".into()],
        wrappers: vec!["mangohud".into()],
        debug: true,
        dxvk: Some("/dxvk".into()),
        env: BTreeMap::from([("A".into(), "1".into())]),
        dll_overrides: BTreeMap::from([("winhttp".into(), "n,b".into())]),
        umu: Some(UmuSection {
            proton: Some("/proton".into()),
            game_id: Some("umu-1".into()),
            ..UmuSection::default()
        }),
        ..Profile::new("full", Runner::Umu)
    };

    save_profile(dir.path(), &profile).unwrap();
    let path = profile_path(dir.path(), "full");

    assert_eq!(load_profile(&path).unwrap(), profile);
}

#[test]
fn minimal_profile_saves_minimal_file() {
    let dir = tempfile::tempdir().unwrap();

    save_profile(dir.path(), &Profile::new("native", Runner::Native)).unwrap();

    let content = std::fs::read_to_string(profile_path(dir.path(), "native")).unwrap();
    assert_eq!(content, "name = \"native\"\nrunner = \"native\"\n");
}

#[test]
fn save_rejects_bad_names() {
    let dir = tempfile::tempdir().unwrap();

    let err = save_profile(dir.path(), &Profile::new("../evil", Runner::Native)).unwrap_err();

    assert!(matches!(err, StoreError::InvalidName(_)));
    assert!(err.path().is_none());
}

#[test]
fn remove_profile_deletes_only_the_file() {
    let dir = tempfile::tempdir().unwrap();
    save_profile(dir.path(), &Profile::new("p", Runner::Native)).unwrap();

    remove_profile(dir.path(), "p").unwrap();

    assert!(!profile_path(dir.path(), "p").exists());
    assert!(dir.path().join("profiles").is_dir());
    assert!(remove_profile(dir.path(), "p").is_err());
}

#[test]
fn load_issue_message_omits_the_path() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "profiles/renamed.toml",
        "name = \"other\"\nrunner = \"native\"\n",
    );

    let catalog = Catalog::load(dir.path());
    let issue = catalog.load_issues().next().unwrap();

    assert_eq!(
        issue.subject,
        Subject::File(dir.path().join("profiles/renamed.toml"))
    );
    assert_eq!(
        issue.message,
        "`name = \"other\"` must match the file name `renamed`"
    );
}
