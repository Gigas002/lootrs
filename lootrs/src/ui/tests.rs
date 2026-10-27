use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    path::Path,
};

use liblootrs::process::Owner;
use ratatui::{
    backend::TestBackend,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};

use super::*;
use crate::{
    app::tui::{LogPane, Running},
    settings::{CliOverrides, Settings},
};

fn tui(dir: &Path, games: &str) -> Tui {
    let config = dir.join("config");
    std::fs::create_dir_all(config.join("profiles")).unwrap();
    std::fs::write(
        config.join("profiles/w.toml"),
        "name = \"w\"\nrunner = \"wine\"\nprefix = \"/pfx/w\"\n",
    )
    .unwrap();
    std::fs::write(config.join("games.toml"), games).unwrap();

    let vars: HashMap<&str, OsString> = HashMap::from([("HOME", dir.into())]);
    let cli = CliOverrides {
        config_dir: Some(config),
        log_dir: Some(dir.join("logs")),
        ..CliOverrides::default()
    };
    Tui::new(Settings::resolve(cli, move |key| vars.get(key).cloned()).unwrap())
}

const GAMES: &str = "[[game]]\nname = \"wow\"\nexe = \"/bin/sh\"\nprofile = \"w\"\n\n\
                     [[game]]\nname = \"broken\"\nexe = \"/bin/sh\"\nprofile = \"missing\"\n";

/// The screen as text, one line per row.
fn render(tui: &Tui) -> String {
    let mut terminal = ratatui::Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| draw(frame, tui)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn games_tab_shows_state_and_problems() {
    let dir = tempfile::tempdir().unwrap();
    let mut tui = tui(dir.path(), GAMES);
    tui.on_scan(BTreeMap::from([(
        Owner::Game("wow".into()),
        Running {
            procs: 3,
            commands: vec!["Wow.exe".into()],
        },
    )]));

    let screen = render(&tui);

    assert!(screen.contains("1 Games"), "{screen}");
    assert!(screen.contains("● 1 running"), "{screen}");
    assert!(screen.contains("running: Wow.exe (3 procs)"), "{screen}");
    assert!(screen.contains("⚠ broken"), "{screen}");
    assert!(
        screen.contains("profile `missing` does not exist"),
        "{screen}"
    );
    assert!(screen.contains("⏎ run"), "{screen}");
}

#[test]
fn empty_and_broken_files() {
    let dir = tempfile::tempdir().unwrap();
    let tui = tui(dir.path(), "[[game]\n");

    let screen = render(&tui);

    assert!(screen.contains("no games — press a to add one"), "{screen}");
    assert!(screen.contains("⚠ games.toml: invalid TOML"), "{screen}");
    assert!(screen.contains("1 file failed to load"), "{screen}");
}

#[test]
fn profiles_tab() {
    let dir = tempfile::tempdir().unwrap();
    let mut tui = tui(dir.path(), GAMES);
    std::fs::write(dir.path().join("config/profiles/bad.toml"), "runner = 1\n").unwrap();
    tui.handle_key(Key::Char('R'));
    tui.handle_key(Key::Char('2'));

    let screen = render(&tui);

    assert!(screen.contains("/pfx/w"), "{screen}");
    assert!(screen.contains("⚠ bad.toml: invalid TOML"), "{screen}");
    assert!(screen.contains("i init  t tricks"), "{screen}");
}

#[test]
fn profile_form_dims_ignored_fields() {
    let dir = tempfile::tempdir().unwrap();
    let mut tui = tui(dir.path(), GAMES);
    tui.handle_key(Key::Char('2'));
    tui.handle_key(Key::Char('a'));

    let screen = render(&tui);

    assert!(screen.contains("new profile"), "{screen}");
    assert!(screen.contains("umu.proton"), "{screen}");
    assert!(screen.contains("(ignored for wine)"), "{screen}");
    assert!(screen.contains("Ctrl-S save"), "{screen}");
}

#[test]
fn form_errors_and_list_editor() {
    let dir = tempfile::tempdir().unwrap();
    let mut tui = tui(dir.path(), GAMES);
    tui.handle_key(Key::Char('2'));
    tui.handle_key(Key::Char('a'));
    tui.handle_key(Key::Ctrl('s'));

    let screen = render(&tui);
    assert!(screen.contains("not saved:"), "{screen}");

    // wrappers
    for key in [Key::Down, Key::Down, Key::Down, Key::Enter, Key::Char('a')] {
        tui.handle_key(key);
    }
    let screen = render(&tui);
    assert!(screen.contains("wrappers (one entry per line)"), "{screen}");
    assert!(screen.contains("J/K reorder"), "{screen}");
}

#[test]
fn log_pane_and_dialogs() {
    let dir = tempfile::tempdir().unwrap();
    let mut tui = tui(dir.path(), GAMES);
    tui.log = Some(LogPane {
        path: Some("/logs/games/wow/1.log".into()),
        lines: (0..40).map(|i| format!("line {i}")).collect(),
    });

    let screen = render(&tui);
    assert!(screen.contains("log: /logs/games/wow/1.log"), "{screen}");
    assert!(screen.contains("line 39"), "{screen}");
    assert!(!screen.contains("line 0\n"), "only the tail fits: {screen}");

    tui.handle_key(Key::Char('d'));
    assert!(render(&tui).contains("Delete game `wow`?"));
    tui.handle_key(Key::Char('n'));

    tui.handle_key(Key::Char('?'));
    assert!(render(&tui).contains("kill everything lootrs started"));
}

#[test]
fn tiny_terminal_does_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let mut tui = tui(dir.path(), GAMES);
    tui.handle_key(Key::Char('?'));
    let mut terminal = ratatui::Terminal::new(TestBackend::new(10, 3)).unwrap();
    terminal.draw(|frame| draw(frame, &tui)).unwrap();
}

#[test]
fn key_mapping() {
    let event = |code, modifiers| KeyEvent::new(code, modifiers);

    assert_eq!(
        key(event(KeyCode::Char('a'), KeyModifiers::NONE)),
        Some(Key::Char('a'))
    );
    assert_eq!(
        key(event(KeyCode::Char('S'), KeyModifiers::CONTROL)),
        Some(Key::Ctrl('s'))
    );
    assert_eq!(
        key(event(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Key::Enter)
    );
    assert_eq!(key(event(KeyCode::F(1), KeyModifiers::NONE)), None);
}
