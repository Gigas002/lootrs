use std::{collections::HashMap, ffi::OsString, io::Write};

use liblootrs::process::{Process, Tags};

use super::*;
use crate::settings::CliOverrides;

/// Settings rooted in a temp dir: config in `config/`, logs in `logs/`.
pub(crate) fn settings(dir: &Path) -> Settings {
    let vars: HashMap<&str, OsString> = HashMap::from([("HOME", dir.into())]);
    let cli = CliOverrides {
        config_dir: Some(dir.join("config")),
        log_dir: Some(dir.join("logs")),
        ..CliOverrides::default()
    };
    Settings::resolve(cli, move |key| vars.get(key).cloned()).unwrap()
}

const NATIVE: &str = "name = \"n\"\nrunner = \"native\"\n";
const WINE: &str = "name = \"w\"\nrunner = \"wine\"\nprefix = \"/pfx/w\"\n";

/// A config dir with games `a` and `b` (native profile `n`) and wine profile `w`.
fn fixture() -> (tempfile::TempDir, Tui) {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    std::fs::create_dir_all(config.join("profiles")).unwrap();
    std::fs::write(config.join("profiles/n.toml"), NATIVE).unwrap();
    std::fs::write(config.join("profiles/w.toml"), WINE).unwrap();
    std::fs::write(
        config.join("games.toml"),
        "[[game]]\nname = \"a\"\nexe = \"/bin/sh\"\nprofile = \"n\"\n\n\
         [[game]]\nname = \"b\"\nexe = \"/bin/sh\"\nprofile = \"n\"\n",
    )
    .unwrap();
    let tui = Tui::new(settings(dir.path()));
    (dir, tui)
}

fn press(tui: &mut Tui, keys: &[Key]) -> Option<Effect> {
    let mut last = None;
    for key in keys {
        last = tui.handle_key(*key);
    }
    last
}

fn type_text(tui: &mut Tui, text: &str) {
    for c in text.chars() {
        tui.handle_key(Key::Char(c));
    }
}

fn running(game: &str) -> BTreeMap<Owner, Running> {
    BTreeMap::from([(
        Owner::Game(game.into()),
        Running {
            procs: 2,
            commands: vec!["game.exe".into()],
        },
    )])
}

fn status(tui: &Tui) -> (&str, bool) {
    let status = tui.status.as_ref().expect("a status message");
    (status.text.as_str(), status.error)
}

fn games_toml(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("config/games.toml")).unwrap()
}

#[test]
fn loads_and_navigates() {
    let (_dir, mut tui) = fixture();

    assert_eq!(tui.catalog.games.len(), 2);
    assert_eq!(tui.selected_game().unwrap().name, "a");
    press(&mut tui, &[Key::Down, Key::Down, Key::Char('j')]);
    assert_eq!(tui.selected_game().unwrap().name, "b");
    press(&mut tui, &[Key::Char('k')]);
    assert_eq!(tui.selected_game().unwrap().name, "a");

    press(&mut tui, &[Key::Tab]);
    assert_eq!(tui.tab, Tab::Profiles);
    press(&mut tui, &[Key::End]);
    assert_eq!(tui.selected_profile().unwrap().name, "w");
    press(&mut tui, &[Key::Char('1')]);
    assert_eq!(tui.selected_game().unwrap().name, "a");
}

#[test]
fn quit_keys() {
    let (_dir, mut tui) = fixture();

    assert_eq!(tui.handle_key(Key::Char('q')), Some(Effect::Quit));
    tui.handle_key(Key::Char('e'));
    assert!(tui.form.is_some());
    // `q` is text inside a form; Ctrl-C always quits.
    assert_eq!(tui.handle_key(Key::Char('q')), None);
    assert_eq!(tui.handle_key(Key::Ctrl('c')), Some(Effect::Quit));
}

#[test]
fn run_and_stop_depend_on_state() {
    let (_dir, mut tui) = fixture();

    assert_eq!(
        tui.handle_key(Key::Enter),
        Some(Effect::Job(Job::Run {
            game: "a".into(),
            debug: false
        }))
    );
    assert_eq!(tui.jobs, 1);
    assert_eq!(status(&tui), ("starting `a`…", false));
    tui.on_job_done(Ok("started `a` (pid 7)".into()));
    assert_eq!(
        (tui.jobs, status(&tui)),
        (0, ("started `a` (pid 7)", false))
    );

    assert_eq!(tui.handle_key(Key::Char('s')), None);
    assert_eq!(status(&tui), ("`a` is not running", true));

    tui.on_scan(running("a"));
    assert_eq!(tui.handle_key(Key::Char('D')), None);
    assert_eq!(status(&tui), ("`a` is already running", true));
    assert_eq!(
        tui.handle_key(Key::Char('s')),
        Some(Effect::Job(Job::Stop { game: "a".into() }))
    );

    press(&mut tui, &[Key::Down]);
    assert_eq!(
        tui.handle_key(Key::Char('D')),
        Some(Effect::Job(Job::Run {
            game: "b".into(),
            debug: true
        }))
    );
    tui.on_job_done(Err("not launching: boom".into()));
    assert_eq!(status(&tui), ("not launching: boom", true));
}

#[test]
fn killall_asks_first() {
    let (_dir, mut tui) = fixture();

    tui.handle_key(Key::Char('K'));
    assert_eq!(status(&tui), ("nothing running", false));
    assert!(tui.modal.is_none());

    tui.on_scan(running("a"));
    tui.handle_key(Key::Char('K'));
    assert!(matches!(tui.modal, Some(Modal::Confirm { .. })));
    assert_eq!(tui.handle_key(Key::Char('x')), None);
    assert!(tui.modal.is_some(), "other keys keep the dialog open");
    assert_eq!(tui.handle_key(Key::Char('n')), None);
    assert!(tui.modal.is_none());

    tui.handle_key(Key::Char('K'));
    assert_eq!(
        tui.handle_key(Key::Char('y')),
        Some(Effect::Job(Job::Killall))
    );
}

#[test]
fn profile_actions() {
    let (_dir, mut tui) = fixture();
    press(&mut tui, &[Key::Char('2')]);

    // `n` is native.
    assert_eq!(tui.handle_key(Key::Char('i')), None);
    assert!(status(&tui).1);

    press(&mut tui, &[Key::Down]);
    assert_eq!(
        tui.handle_key(Key::Char('i')),
        Some(Effect::Foreground(Foreground::Init {
            profile: "w".into()
        }))
    );

    tui.handle_key(Key::Char('t'));
    type_text(&mut tui, "corefonts  vcrun2022");
    assert_eq!(
        tui.handle_key(Key::Enter),
        Some(Effect::Foreground(Foreground::Tricks {
            profile: "w".into(),
            verbs: vec!["corefonts".into(), "vcrun2022".into()],
        }))
    );

    tui.handle_key(Key::Char('x'));
    type_text(&mut tui, "winecfg");
    assert_eq!(
        tui.handle_key(Key::Enter),
        Some(Effect::Job(Job::Execute {
            profile: "w".into(),
            program: "winecfg".into(),
        }))
    );

    tui.handle_key(Key::Char('x'));
    assert_eq!(tui.handle_key(Key::Esc), None);
    assert!(tui.modal.is_none());
}

#[test]
fn add_profile_with_default_prefix() {
    let (dir, mut tui) = fixture();
    press(&mut tui, &[Key::Char('2'), Key::Char('a')]);
    type_text(&mut tui, "new");

    tui.handle_key(Key::Ctrl('s'));

    assert!(
        tui.form.is_none(),
        "{:?}",
        tui.form.as_ref().map(|f| &f.errors)
    );
    assert_eq!(status(&tui), ("saved `new`", false));
    let saved = store::load_profile(&dir.path().join("config/profiles/new.toml")).unwrap();
    assert_eq!(saved.runner, Runner::Wine);
    assert_eq!(
        saved.prefix.unwrap(),
        dir.path().join(".local/share/lootrs/prefixes/new")
    );
    assert_eq!(tui.selected_profile().unwrap().name, "new");
}

#[test]
fn invalid_form_stays_open_with_errors() {
    let (dir, mut tui) = fixture();
    press(&mut tui, &[Key::Char('2'), Key::Char('a')]);
    type_text(&mut tui, "u");
    // runner: wine → umu, which needs `[umu].proton`.
    press(&mut tui, &[Key::Down, Key::Right, Key::Ctrl('s')]);

    let form = tui.form.as_ref().expect("form still open");
    assert!(
        form.errors
            .iter()
            .any(|e| e.contains("`[umu].proton` is required")),
        "{:?}",
        form.errors
    );
    assert!(!dir.path().join("config/profiles/u.toml").exists());

    // Existing names are refused too.
    let (_dir, mut tui) = fixture();
    press(&mut tui, &[Key::Char('2'), Key::Char('a')]);
    type_text(&mut tui, "w");
    tui.handle_key(Key::Ctrl('s'));
    assert_eq!(
        tui.form.as_ref().unwrap().errors,
        ["a profile named `w` already exists"]
    );
}

#[test]
fn edit_and_rename_game() {
    let (dir, mut tui) = fixture();
    tui.handle_key(Key::Char('e'));
    // Name field: replace `a` with `c`.
    press(&mut tui, &[Key::Backspace, Key::Char('c'), Key::Ctrl('s')]);

    assert!(tui.form.is_none());
    let games = games_toml(dir.path());
    assert!(
        games.contains("name = \"c\"") && !games.contains("name = \"a\""),
        "{games}"
    );
    assert_eq!(tui.selected_game().unwrap().name, "c");

    // Renaming onto another game is refused.
    tui.handle_key(Key::Char('e'));
    press(&mut tui, &[Key::Backspace, Key::Char('b'), Key::Ctrl('s')]);
    assert_eq!(
        tui.form.as_ref().unwrap().errors,
        ["a game named `b` already exists"]
    );
}

#[test]
fn add_game_needs_a_profile() {
    let dir = tempfile::tempdir().unwrap();
    let mut tui = Tui::new(settings(dir.path()));

    tui.handle_key(Key::Char('a'));

    assert!(tui.form.is_none());
    assert_eq!(status(&tui), ("add a profile first (Profiles tab)", true));
}

#[test]
fn hand_edits_are_not_overwritten() {
    let (dir, mut tui) = fixture();
    let games = dir.path().join("config/games.toml");
    std::thread::sleep(Duration::from_millis(10));
    std::fs::write(&games, games_toml(dir.path()) + "# hand edit\n").unwrap();

    tui.handle_key(Key::Char('e'));
    press(&mut tui, &[Key::Char('x'), Key::Ctrl('s')]);

    let errors = &tui.form.as_ref().unwrap().errors;
    assert!(errors[0].contains("changed on disk"), "{errors:?}");
    assert!(games_toml(dir.path()).ends_with("# hand edit\n"));

    // After a reload, saving works.
    press(
        &mut tui,
        &[Key::Esc, Key::Char('y'), Key::Char('R'), Key::Char('e')],
    );
    press(&mut tui, &[Key::Char('x'), Key::Ctrl('s')]);
    assert!(tui.form.is_none());
    assert!(games_toml(dir.path()).contains("name = \"ax\""));
}

#[test]
fn broken_games_file_is_never_overwritten() {
    let (dir, _) = fixture();
    std::fs::write(dir.path().join("config/games.toml"), "[[game]\n").unwrap();
    let mut tui = Tui::new(settings(dir.path()));
    assert!(status(&tui).1, "load errors are reported");

    press(&mut tui, &[Key::Char('a')]);
    type_text(&mut tui, "new");
    tui.handle_key(Key::Down);
    type_text(&mut tui, "/bin/sh");
    tui.handle_key(Key::Ctrl('s'));

    let errors = &tui.form.as_ref().unwrap().errors;
    assert!(
        errors.iter().any(|e| e.contains("failed to load")),
        "{errors:?}"
    );
    assert_eq!(games_toml(dir.path()), "[[game]\n");
}

#[test]
fn delete_game_and_profile() {
    let (dir, mut tui) = fixture();

    press(&mut tui, &[Key::Char('d'), Key::Char('n')]);
    assert_eq!(tui.catalog.games.len(), 2);
    press(&mut tui, &[Key::Char('d'), Key::Char('y')]);
    assert_eq!(tui.catalog.games.len(), 1);
    assert!(!games_toml(dir.path()).contains("name = \"a\""));

    // `n` is still used by `b`.
    press(&mut tui, &[Key::Char('2'), Key::Char('d')]);
    assert!(tui.modal.is_none());
    assert_eq!(
        status(&tui),
        ("`n` is used by b; change their profile first", true)
    );

    press(&mut tui, &[Key::Down, Key::Char('d'), Key::Char('y')]);
    assert!(!dir.path().join("config/profiles/w.toml").exists());
    assert_eq!(tui.catalog.profiles.len(), 1);
}

#[test]
fn discarding_a_changed_form_asks() {
    let (_dir, mut tui) = fixture();

    press(&mut tui, &[Key::Char('e'), Key::Esc]);
    assert!(tui.form.is_none(), "unchanged forms close right away");

    press(&mut tui, &[Key::Char('e'), Key::Char('x'), Key::Esc]);
    assert!(tui.form.is_some());
    press(&mut tui, &[Key::Char('n')]);
    assert!(tui.form.is_some());
    press(&mut tui, &[Key::Esc, Key::Char('y')]);
    assert!(tui.form.is_none());
}

#[test]
fn log_pane_shows_latest_run() {
    let (dir, mut tui) = fixture();
    let logs = dir.path().join("logs");
    let mut log = runlog::create(&logs, RunKind::Game("a"), "20260101-000000", 5).unwrap();
    writeln!(log.file, "first\nsecond").unwrap();

    assert_eq!(
        tui.handle_key(Key::Char('L')),
        Some(Effect::Foreground(Foreground::Pager {
            path: log.path.clone()
        }))
    );
    tui.handle_key(Key::Char('l'));
    let pane = tui.log.as_ref().unwrap();
    assert_eq!(pane.path.as_deref(), Some(log.path.as_path()));
    assert_eq!(pane.lines, ["first", "second"]);

    press(&mut tui, &[Key::Down]);
    assert_eq!(tui.log, Some(LogPane::default()));
    assert_eq!(tui.handle_key(Key::Char('L')), None);
    assert_eq!(status(&tui), ("no logs yet", true));

    tui.handle_key(Key::Char('l'));
    assert!(tui.log.is_none());
}

#[test]
fn summarize_groups_by_owner() {
    let process = |pid, comm: &str, owner: Option<&str>| Process {
        pid,
        comm: comm.into(),
        exe: None,
        tags: Tags {
            owner: owner.map(|o| Owner::Game(o.into())),
            prefix: None,
        },
    };
    let processes = [
        process(1, "wineserver", Some("a")),
        process(2, "game.exe", Some("a")),
        process(3, "game.exe", Some("a")),
        process(4, "bash", None),
        // Only infrastructure left: not running.
        process(5, "wineserver", Some("b")),
    ];

    assert_eq!(
        summarize(&processes),
        BTreeMap::from([(
            Owner::Game("a".into()),
            Running {
                procs: 3,
                commands: vec!["game.exe".into()],
            }
        )])
    );
}
