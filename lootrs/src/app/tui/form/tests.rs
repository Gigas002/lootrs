use std::collections::BTreeMap;

use liblootrs::model::{Anticheat, Game, Profile, Runner, UmuSection, WineSection};

use super::*;

fn runners() -> BTreeMap<String, Runner> {
    BTreeMap::from([("n".into(), Runner::Native), ("u".into(), Runner::Umu)])
}

fn game() -> Game {
    Game {
        name: "g".into(),
        exe: "/games/g/G.exe".into(),
        profile: "u".into(),
        args: vec!["-a".into(), "two words".into()],
        cwd: Some("/games/g".into()),
        game_id: Some("umu-1".into()),
        env: BTreeMap::from([("A".into(), "1=2".into())]),
    }
}

fn press(form: &mut Form, keys: &[Key]) -> Outcome {
    keys.iter()
        .fold(Outcome::Continue, |_, key| form.handle_key(*key))
}

fn focus(form: &mut Form, key: FieldKey) {
    form.focus = form.fields.iter().position(|f| f.key == key).unwrap();
}

#[test]
fn input_edits_by_char() {
    let mut input = Input::new("héllo");
    assert_eq!(input.cursor(), 5);

    input.handle(Key::Left);
    input.handle(Key::Backspace);
    input.handle(Key::Char('ł'));
    assert_eq!(input.text(), "hélło");
    input.handle(Key::Home);
    input.handle(Key::Delete);
    input.handle(Key::End);
    input.handle(Key::Char('!'));
    assert_eq!((input.text(), input.cursor()), ("élło!", 5));
    assert!(!input.handle(Key::Up));
    input.handle(Key::Ctrl('u'));
    assert_eq!(input, Input::default());
    // Nothing to delete is still an editing key.
    assert!(input.handle(Key::Backspace));
}

#[test]
fn game_round_trip() {
    let form = Form::game(Some(&game()), runners());

    assert_eq!(form.to_game(), game());
    assert_eq!(form.title(), "edit game `g`");
    assert_eq!(form.runner(), Some(Runner::Umu));
}

#[test]
fn game_keeps_a_missing_profile() {
    let mut missing = game();
    missing.profile = "gone".into();

    let form = Form::game(Some(&missing), runners());

    assert_eq!(form.to_game().profile, "gone");
    assert_eq!(form.runner(), None);
}

#[test]
fn game_id_applies_only_to_umu_profiles() {
    let mut form = Form::game(Some(&game()), runners());
    let game_id = |form: &Form| {
        let field = form
            .fields
            .iter()
            .find(|f| f.key == FieldKey::GameId)
            .unwrap();
        form.applies(field)
    };

    assert!(game_id(&form));
    focus(&mut form, FieldKey::GameProfile);
    press(&mut form, &[Key::Right]);
    assert_eq!(form.to_game().profile, "n");
    assert!(!game_id(&form));
    assert!(form.dirty);
}

#[test]
fn profile_round_trip() {
    let wine = Profile {
        prefix: Some("~/pfx".into()),
        runner_args: vec!["--x".into()],
        wrappers: vec!["mangohud".into(), "gamemoderun".into()],
        debug: true,
        dxvk: Some("/opt/dxvk".into()),
        vkd3d: Some("/opt/vkd3d".into()),
        env: BTreeMap::from([("DXVK_HUD".into(), "fps".into())]),
        dll_overrides: BTreeMap::from([("winhttp".into(), "n,b".into())]),
        wine: Some(WineSection {
            path: Some("/opt/wine".into()),
        }),
        anticheat: Anticheat {
            battleye: true,
            eac: false,
        },
        ..Profile::new("w", Runner::Wine)
    };
    let umu = Profile {
        prefix: Some("/pfx/u".into()),
        umu: Some(UmuSection {
            umu_run: Some("/usr/bin/umu-run".into()),
            proton: Some("/proton".into()),
            game_id: Some("umu-1".into()),
            store: Some("gog".into()),
        }),
        ..Profile::new("u", Runner::Umu)
    };
    let native = Profile::new("n", Runner::Native);

    for profile in [wine, umu, native] {
        let form = Form::profile(Some(&profile), Path::new("/root"));
        assert_eq!(form.to_profile(), profile);
    }
}

#[test]
fn new_profile_defaults() {
    let mut form = Form::profile(None, Path::new("/prefixes"));
    assert_eq!(form.title(), "new profile");
    assert_eq!(form.focus, 0);
    for c in "p".chars() {
        form.handle_key(Key::Char(c));
    }

    let profile = form.to_profile();
    assert_eq!(profile.runner, Runner::Wine);
    assert_eq!(profile.prefix.as_deref(), Some(Path::new("/prefixes/p")));
    assert_eq!(profile.umu, None);
    assert_eq!(profile.wine, None);

    // native: no prefix.
    focus(&mut form, FieldKey::Runner);
    press(&mut form, &[Key::Left]);
    assert_eq!(form.runner(), Some(Runner::Native));
    assert_eq!(form.to_profile().prefix, None);
}

#[test]
fn runner_decides_which_fields_apply() {
    let mut form = Form::profile(None, Path::new("/p"));
    let applies = |form: &Form, key| {
        let field = form.fields.iter().find(|f| f.key == key).unwrap();
        form.applies(field)
    };

    assert!(applies(&form, FieldKey::Dxvk));
    assert!(!applies(&form, FieldKey::Proton));
    focus(&mut form, FieldKey::Runner);
    press(&mut form, &[Key::Char(' ')]);
    assert_eq!(form.runner(), Some(Runner::Umu));
    assert!(!applies(&form, FieldKey::Dxvk));
    assert!(applies(&form, FieldKey::Proton));
    assert!(applies(&form, FieldKey::Env));
}

#[test]
fn existing_profile_name_is_read_only() {
    let mut form = Form::profile(Some(&Profile::new("w", Runner::Wine)), Path::new("/p"));
    assert_eq!(form.fields[form.focus].key, FieldKey::Runner);

    focus(&mut form, FieldKey::Name);
    press(&mut form, &[Key::Char('x'), Key::Backspace]);

    assert_eq!(form.to_profile().name, "w");
    assert!(!form.dirty);
}

#[test]
fn navigation_toggles_and_outcomes() {
    let mut form = Form::profile(None, Path::new("/p"));

    assert_eq!(press(&mut form, &[Key::Up]), Outcome::Continue);
    assert_eq!(form.fields[form.focus].key, FieldKey::Eac);
    press(&mut form, &[Key::Enter]);
    assert!(form.to_profile().anticheat.eac);
    press(&mut form, &[Key::Tab]);
    assert_eq!(form.focus, 0);
    // Enter in a text field moves on.
    press(&mut form, &[Key::Enter]);
    assert_eq!(form.fields[form.focus].key, FieldKey::Runner);

    assert_eq!(form.handle_key(Key::Ctrl('s')), Outcome::Save);
    assert_eq!(form.handle_key(Key::Esc), Outcome::Cancel);
}

#[test]
fn list_editor_add_edit_reorder_delete() {
    let mut form = Form::profile(None, Path::new("/p"));
    focus(&mut form, FieldKey::Wrappers);

    press(&mut form, &[Key::Enter, Key::Char('a')]);
    for c in "mangohud".chars() {
        form.handle_key(Key::Char(c));
    }
    press(&mut form, &[Key::Enter, Key::Char('a')]);
    for c in "gamemoderun".chars() {
        form.handle_key(Key::Char(c));
    }
    press(&mut form, &[Key::Enter]);
    assert_eq!(form.to_profile().wrappers, ["mangohud", "gamemoderun"]);

    press(&mut form, &[Key::Char('K')]);
    assert_eq!(form.to_profile().wrappers, ["gamemoderun", "mangohud"]);

    press(&mut form, &[Key::Char('j'), Key::Char('e'), Key::End]);
    for c in " --dlsym".chars() {
        form.handle_key(Key::Char(c));
    }
    press(&mut form, &[Key::Enter]);
    assert_eq!(
        form.to_profile().wrappers,
        ["gamemoderun", "mangohud --dlsym"]
    );

    press(&mut form, &[Key::Char('k'), Key::Char('d'), Key::Esc]);
    assert_eq!(form.to_profile().wrappers, ["mangohud --dlsym"]);
    assert!(form.editor.is_none());
    assert!(form.dirty);
}

#[test]
fn map_editor_requires_key_value() {
    let mut form = Form::profile(None, Path::new("/p"));
    focus(&mut form, FieldKey::Env);

    press(
        &mut form,
        &[Key::Enter, Key::Char('a'), Key::Char('X'), Key::Enter],
    );
    let editor = form.editor.as_ref().unwrap();
    assert_eq!(editor.error.as_deref(), Some("use KEY=VALUE"));
    assert!(editor.input.is_some());

    press(&mut form, &[Key::Char('='), Key::Char('1'), Key::Enter]);
    assert_eq!(form.editor.as_ref().unwrap().error, None);
    press(&mut form, &[Key::Char('a'), Key::Enter]);
    assert_eq!(
        form.editor.as_ref().unwrap().error.as_deref(),
        Some("empty entry (Esc cancels)")
    );
    press(&mut form, &[Key::Esc, Key::Esc]);

    assert_eq!(
        form.to_profile().env,
        BTreeMap::from([("X".into(), "1".into())])
    );
}
