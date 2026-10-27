//! Game and profile forms: the fields, key handling, and conversion to the
//! model types. Validation happens when saving, with the same rules as `check`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use liblootrs::model::{Anticheat, Game, Profile, Runner, UmuSection, WineSection};

use super::Key;

#[cfg(test)]
mod tests;

/// Single-line text input; the cursor counts chars, not bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    cursor: usize,
}

impl Input {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        Self { text, cursor }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Applies an editing key; `false` if `key` doesn't edit text.
    pub fn handle(&mut self, key: Key) -> bool {
        let len = self.text.chars().count();
        match key {
            Key::Char(c) => {
                let at = self.byte(self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
            }
            Key::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            Key::Delete if self.cursor < len => {
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            Key::Left => self.cursor = self.cursor.saturating_sub(1),
            Key::Right => self.cursor = (self.cursor + 1).min(len),
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = len,
            Key::Ctrl('u') => *self = Self::default(),
            Key::Backspace | Key::Delete => {}
            _ => return false,
        }
        true
    }

    fn byte(&self, index: usize) -> usize {
        self.text
            .char_indices()
            .nth(index)
            .map_or(self.text.len(), |(at, _)| at)
    }
}

/// Which setting a field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKey {
    Name,
    Exe,
    GameProfile,
    Args,
    Cwd,
    GameId,
    Env,
    Runner,
    Prefix,
    Wrappers,
    RunnerArgs,
    Debug,
    Dxvk,
    Vkd3d,
    DllOverrides,
    WinePath,
    Proton,
    UmuRun,
    UmuGameId,
    Store,
    Battleye,
    Eac,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(Input),
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    Toggle(bool),
    /// Edited in a [`ListEditor`].
    List(Vec<String>),
    /// `KEY=VALUE` entries, edited in a [`ListEditor`].
    Map(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: FieldKey,
    pub label: &'static str,
    pub value: Value,
    /// Shown dimmed while a text field is empty.
    pub placeholder: String,
    /// Runners this setting applies to; others keep but ignore it.
    pub runners: &'static [Runner],
    pub read_only: bool,
}

const ALL: &[Runner] = &[Runner::Wine, Runner::Umu, Runner::Native];
const PREFIXED: &[Runner] = &[Runner::Wine, Runner::Umu];
const WINE: &[Runner] = &[Runner::Wine];
const UMU: &[Runner] = &[Runner::Umu];
const RUNNERS: [Runner; 3] = [Runner::Wine, Runner::Umu, Runner::Native];

impl Field {
    fn new(key: FieldKey, label: &'static str, value: Value) -> Self {
        Self {
            key,
            label,
            value,
            placeholder: String::new(),
            runners: ALL,
            read_only: false,
        }
    }

    fn text(key: FieldKey, label: &'static str, text: impl Into<String>) -> Self {
        Self::new(key, label, Value::Text(Input::new(text)))
    }

    fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    fn runners(mut self, runners: &'static [Runner]) -> Self {
        self.runners = runners;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Game,
    Profile,
}

/// What the form wants after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Save,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub kind: Kind,
    /// The name being edited; `None` when adding.
    pub original: Option<String>,
    pub fields: Vec<Field>,
    pub focus: usize,
    /// Open while editing a list or map field.
    pub editor: Option<ListEditor>,
    /// Why the last save was refused.
    pub errors: Vec<String>,
    pub dirty: bool,
    /// Runner of each profile, to tell which game fields apply.
    profile_runners: BTreeMap<String, Runner>,
    /// Default prefix location for new profiles (`<prefix_root>/<name>`).
    prefix_root: PathBuf,
}

fn path_text(path: Option<&Path>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn map_entries(map: &BTreeMap<String, String>) -> Vec<String> {
    map.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect()
}

impl Form {
    /// Form for a new game (`None`) or an existing one. `profile_runners` lists
    /// the profiles to choose from.
    pub fn game(game: Option<&Game>, profile_runners: BTreeMap<String, Runner>) -> Self {
        let mut options: Vec<String> = profile_runners.keys().cloned().collect();
        // Keep a reference to a missing profile instead of silently changing it.
        if let Some(game) = game
            && !options.contains(&game.profile)
        {
            options.push(game.profile.clone());
        }
        let selected = game
            .and_then(|game| options.iter().position(|name| *name == game.profile))
            .unwrap_or_default();

        let fields = vec![
            Field::text(FieldKey::Name, "name", game.map_or("", |g| &g.name))
                .placeholder("used by `lootrs run <name>`"),
            Field::text(
                FieldKey::Exe,
                "exe",
                path_text(game.map(|g| g.exe.as_path())),
            )
            .placeholder("/path/to/Game.exe"),
            Field::new(
                FieldKey::GameProfile,
                "profile",
                Value::Choice { options, selected },
            ),
            Field::new(
                FieldKey::Args,
                "args",
                Value::List(game.map(|g| g.args.clone()).unwrap_or_default()),
            ),
            Field::text(
                FieldKey::Cwd,
                "cwd",
                path_text(game.and_then(|g| g.cwd.as_deref())),
            )
            .placeholder("default: the executable's directory"),
            Field::text(
                FieldKey::GameId,
                "game_id",
                game.and_then(|g| g.game_id.as_deref()).unwrap_or_default(),
            )
            .placeholder("default: the profile's [umu].game_id")
            .runners(UMU),
            Field::new(
                FieldKey::Env,
                "env",
                Value::Map(game.map(|g| map_entries(&g.env)).unwrap_or_default()),
            ),
        ];
        Self::with_fields(
            Kind::Game,
            game.map(|g| g.name.clone()),
            fields,
            profile_runners,
            PathBuf::new(),
        )
    }

    /// Form for a new profile (`None`) or an existing one. Existing profiles
    /// can't be renamed: games refer to them by name.
    pub fn profile(profile: Option<&Profile>, prefix_root: &Path) -> Self {
        let runner = profile.map_or(Runner::Wine, |p| p.runner);
        let wine = profile.and_then(|p| p.wine.as_ref());
        let umu = profile.and_then(|p| p.umu.as_ref());
        let anticheat = profile.map(|p| p.anticheat).unwrap_or_default();
        let umu_text =
            |get: fn(&UmuSection) -> Option<String>| umu.and_then(get).unwrap_or_default();

        let mut name = Field::text(FieldKey::Name, "name", profile.map_or("", |p| &p.name))
            .placeholder("file name in profiles/");
        name.read_only = profile.is_some();
        let fields = vec![
            name,
            Field::new(
                FieldKey::Runner,
                "runner",
                Value::Choice {
                    options: RUNNERS.iter().map(|r| r.as_str().to_owned()).collect(),
                    selected: RUNNERS
                        .iter()
                        .position(|r| *r == runner)
                        .unwrap_or_default(),
                },
            ),
            Field::text(
                FieldKey::Prefix,
                "prefix",
                path_text(profile.and_then(|p| p.prefix.as_deref())),
            )
            .placeholder(format!("default: {}/<name>", prefix_root.display()))
            .runners(PREFIXED),
            Field::new(
                FieldKey::Wrappers,
                "wrappers",
                Value::List(profile.map(|p| p.wrappers.clone()).unwrap_or_default()),
            ),
            Field::new(
                FieldKey::RunnerArgs,
                "runner_args",
                Value::List(profile.map(|p| p.runner_args.clone()).unwrap_or_default()),
            )
            .runners(PREFIXED),
            Field::new(
                FieldKey::Debug,
                "debug",
                Value::Toggle(profile.is_some_and(|p| p.debug)),
            ),
            Field::new(
                FieldKey::Env,
                "env",
                Value::Map(profile.map(|p| map_entries(&p.env)).unwrap_or_default()),
            ),
            Field::new(
                FieldKey::DllOverrides,
                "dll_overrides",
                Value::Map(
                    profile
                        .map(|p| map_entries(&p.dll_overrides))
                        .unwrap_or_default(),
                ),
            )
            .runners(PREFIXED),
            Field::text(
                FieldKey::Dxvk,
                "dxvk",
                path_text(profile.and_then(|p| p.dxvk.as_deref())),
            )
            .placeholder("DXVK directory with x64/ (optional)")
            .runners(WINE),
            Field::text(
                FieldKey::Vkd3d,
                "vkd3d",
                path_text(profile.and_then(|p| p.vkd3d.as_deref())),
            )
            .placeholder("VKD3D-Proton directory with x64/ (optional)")
            .runners(WINE),
            Field::text(
                FieldKey::WinePath,
                "wine.path",
                path_text(wine.and_then(|w| w.path.as_deref())),
            )
            .placeholder("default: wine from PATH")
            .runners(WINE),
            Field::text(
                FieldKey::Proton,
                "umu.proton",
                umu_text(|u| u.proton.as_deref().map(|p| p.display().to_string())),
            )
            .placeholder("required: Proton directory")
            .runners(UMU),
            Field::text(
                FieldKey::UmuRun,
                "umu.umu_run",
                umu_text(|u| u.umu_run.as_deref().map(|p| p.display().to_string())),
            )
            .placeholder("default: umu-run from PATH")
            .runners(UMU),
            Field::text(
                FieldKey::UmuGameId,
                "umu.game_id",
                umu_text(|u| u.game_id.clone()),
            )
            .placeholder("default: umu-default")
            .runners(UMU),
            Field::text(FieldKey::Store, "umu.store", umu_text(|u| u.store.clone()))
                .placeholder("STORE for protonfixes (optional)")
                .runners(UMU),
            Field::new(
                FieldKey::Battleye,
                "anticheat.battleye",
                Value::Toggle(anticheat.battleye),
            )
            .runners(PREFIXED),
            Field::new(FieldKey::Eac, "anticheat.eac", Value::Toggle(anticheat.eac))
                .runners(PREFIXED),
        ];
        Self::with_fields(
            Kind::Profile,
            profile.map(|p| p.name.clone()),
            fields,
            BTreeMap::new(),
            prefix_root.to_path_buf(),
        )
    }

    fn with_fields(
        kind: Kind,
        original: Option<String>,
        fields: Vec<Field>,
        profile_runners: BTreeMap<String, Runner>,
        prefix_root: PathBuf,
    ) -> Self {
        // Start on the first field that can be edited.
        let focus = fields.iter().position(|f| !f.read_only).unwrap_or_default();
        Self {
            kind,
            original,
            fields,
            focus,
            editor: None,
            errors: Vec::new(),
            dirty: false,
            profile_runners,
            prefix_root,
        }
    }

    pub fn title(&self) -> String {
        let what = match self.kind {
            Kind::Game => "game",
            Kind::Profile => "profile",
        };
        match &self.original {
            Some(name) => format!("edit {what} `{name}`"),
            None => format!("new {what}"),
        }
    }

    /// The runner the settings apply to: the profile's own, or the game's profile's.
    pub fn runner(&self) -> Option<Runner> {
        match self.kind {
            Kind::Profile => RUNNERS
                .iter()
                .copied()
                .find(|r| r.as_str() == self.choice(FieldKey::Runner)),
            Kind::Game => self
                .profile_runners
                .get(self.choice(FieldKey::GameProfile))
                .copied(),
        }
    }

    /// Whether `field` matters for the current runner (unknown runner: yes).
    pub fn applies(&self, field: &Field) -> bool {
        self.runner()
            .is_none_or(|runner| field.runners.contains(&runner))
    }

    pub fn handle_key(&mut self, key: Key) -> Outcome {
        if let Some(editor) = &mut self.editor {
            let field = &mut self.fields[self.focus];
            let (Value::List(items) | Value::Map(items)) = &mut field.value else {
                self.editor = None;
                return Outcome::Continue;
            };
            match editor.handle_key(key, items) {
                EditorOutcome::Continue => {}
                EditorOutcome::Changed => self.dirty = true,
                EditorOutcome::Close => self.editor = None,
            }
            return Outcome::Continue;
        }

        let count = self.fields.len();
        match key {
            Key::Ctrl('s') => return Outcome::Save,
            Key::Esc => return Outcome::Cancel,
            Key::Up | Key::BackTab => self.focus = (self.focus + count - 1) % count,
            Key::Down | Key::Tab => self.focus = (self.focus + 1) % count,
            key => self.edit_focused(key),
        }
        Outcome::Continue
    }

    fn edit_focused(&mut self, key: Key) {
        let field = &mut self.fields[self.focus];
        if field.read_only {
            return;
        }
        let changed = match (&mut field.value, key) {
            // Enter moves on, like Tab.
            (Value::Text(_), Key::Enter) => {
                self.focus = (self.focus + 1) % self.fields.len();
                false
            }
            (Value::Text(input), key) => input.handle(key),
            (Value::Choice { options, selected }, Key::Left) if !options.is_empty() => {
                *selected = (*selected + options.len() - 1) % options.len();
                true
            }
            (Value::Choice { options, selected }, Key::Right | Key::Char(' '))
                if !options.is_empty() =>
            {
                *selected = (*selected + 1) % options.len();
                true
            }
            (Value::Toggle(on), Key::Char(' ') | Key::Enter) => {
                *on = !*on;
                true
            }
            (Value::List(_), Key::Enter) => {
                self.editor = Some(ListEditor::new(false));
                false
            }
            (Value::Map(_), Key::Enter) => {
                self.editor = Some(ListEditor::new(true));
                false
            }
            _ => false,
        };
        if changed {
            self.dirty = true;
        }
    }

    fn field(&self, key: FieldKey) -> &Field {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .expect("form has the field")
    }

    fn text(&self, key: FieldKey) -> &str {
        match &self.field(key).value {
            Value::Text(input) => input.text().trim(),
            _ => "",
        }
    }

    /// Empty text means unset.
    fn optional(&self, key: FieldKey) -> Option<String> {
        Some(self.text(key).to_owned()).filter(|text| !text.is_empty())
    }

    fn choice(&self, key: FieldKey) -> &str {
        match &self.field(key).value {
            Value::Choice { options, selected } => {
                options.get(*selected).map_or("", String::as_str)
            }
            _ => "",
        }
    }

    fn toggle(&self, key: FieldKey) -> bool {
        matches!(self.field(key).value, Value::Toggle(true))
    }

    fn list(&self, key: FieldKey) -> Vec<String> {
        match &self.field(key).value {
            Value::List(items) => items.clone(),
            _ => Vec::new(),
        }
    }

    fn map(&self, key: FieldKey) -> BTreeMap<String, String> {
        match &self.field(key).value {
            Value::Map(items) => items
                .iter()
                .filter_map(|entry| entry.split_once('='))
                .map(|(key, value)| (key.trim().to_owned(), value.to_owned()))
                .collect(),
            _ => BTreeMap::new(),
        }
    }

    /// The game as entered (unvalidated).
    pub fn to_game(&self) -> Game {
        Game {
            name: self.text(FieldKey::Name).to_owned(),
            exe: self.text(FieldKey::Exe).into(),
            profile: self.choice(FieldKey::GameProfile).to_owned(),
            args: self.list(FieldKey::Args),
            cwd: self.optional(FieldKey::Cwd).map(PathBuf::from),
            game_id: self.optional(FieldKey::GameId),
            env: self.map(FieldKey::Env),
        }
    }

    /// The profile as entered (unvalidated). An empty prefix becomes
    /// `<prefix_root>/<name>` for runners that need one.
    pub fn to_profile(&self) -> Profile {
        let name = self.text(FieldKey::Name).to_owned();
        let runner = self.runner().unwrap_or(Runner::Wine);
        let prefix = self
            .optional(FieldKey::Prefix)
            .map(PathBuf::from)
            .or_else(|| {
                (runner != Runner::Native && !name.is_empty()).then(|| self.prefix_root.join(&name))
            });
        let path = |key| self.optional(key).map(PathBuf::from);

        let wine = path(FieldKey::WinePath).map(|path| WineSection { path: Some(path) });
        let umu = UmuSection {
            umu_run: path(FieldKey::UmuRun),
            proton: path(FieldKey::Proton),
            game_id: self.optional(FieldKey::UmuGameId),
            store: self.optional(FieldKey::Store),
        };

        Profile {
            name,
            runner,
            prefix,
            runner_args: self.list(FieldKey::RunnerArgs),
            wrappers: self.list(FieldKey::Wrappers),
            debug: self.toggle(FieldKey::Debug),
            dxvk: path(FieldKey::Dxvk),
            vkd3d: path(FieldKey::Vkd3d),
            env: self.map(FieldKey::Env),
            dll_overrides: self.map(FieldKey::DllOverrides),
            wine,
            umu: (umu != UmuSection::default()).then_some(umu),
            anticheat: Anticheat {
                battleye: self.toggle(FieldKey::Battleye),
                eac: self.toggle(FieldKey::Eac),
            },
        }
    }
}

/// What a [`ListEditor`] did with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorOutcome {
    Continue,
    Changed,
    Close,
}

/// Editor for list fields (`args`, `wrappers`, ...) and `KEY=VALUE` maps (`env`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEditor {
    /// Entries are `KEY=VALUE`.
    pub map: bool,
    pub selected: usize,
    /// The entry being typed, if any.
    pub input: Option<Input>,
    /// Index being edited; `None` while adding.
    editing: Option<usize>,
    pub error: Option<String>,
}

impl ListEditor {
    fn new(map: bool) -> Self {
        Self {
            map,
            selected: 0,
            input: None,
            editing: None,
            error: None,
        }
    }

    pub fn handle_key(&mut self, key: Key, items: &mut Vec<String>) -> EditorOutcome {
        if let Some(input) = &mut self.input {
            match key {
                Key::Enter => return self.commit(items),
                Key::Esc => {
                    self.input = None;
                    self.error = None;
                }
                key => {
                    input.handle(key);
                }
            }
            return EditorOutcome::Continue;
        }

        let count = items.len();
        match key {
            Key::Esc | Key::Char('q') => return EditorOutcome::Close,
            Key::Up | Key::Char('k') => self.selected = self.selected.saturating_sub(1),
            Key::Down | Key::Char('j') => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1));
            }
            Key::Char('a') => {
                self.input = Some(Input::default());
                self.editing = None;
            }
            Key::Enter | Key::Char('e') if count > 0 => {
                self.input = Some(Input::new(items[self.selected].clone()));
                self.editing = Some(self.selected);
            }
            Key::Char('d') | Key::Delete if count > 0 => {
                items.remove(self.selected);
                self.selected = self.selected.min(items.len().saturating_sub(1));
                return EditorOutcome::Changed;
            }
            // Order matters for wrappers and args.
            Key::Char('K') if self.selected > 0 => {
                items.swap(self.selected, self.selected - 1);
                self.selected -= 1;
                return EditorOutcome::Changed;
            }
            Key::Char('J') if self.selected + 1 < count => {
                items.swap(self.selected, self.selected + 1);
                self.selected += 1;
                return EditorOutcome::Changed;
            }
            _ => {}
        }
        EditorOutcome::Continue
    }

    fn commit(&mut self, items: &mut Vec<String>) -> EditorOutcome {
        let text = self.input.as_ref().map(Input::text).unwrap_or_default();
        if text.trim().is_empty() {
            self.error = Some("empty entry (Esc cancels)".into());
            return EditorOutcome::Continue;
        }
        if self.map
            && !text
                .split_once('=')
                .is_some_and(|(key, _)| !key.trim().is_empty())
        {
            self.error = Some("use KEY=VALUE".into());
            return EditorOutcome::Continue;
        }
        let text = text.to_owned();
        match self.editing {
            Some(index) => items[index] = text,
            None => {
                items.push(text);
                self.selected = items.len() - 1;
            }
        }
        self.input = None;
        self.error = None;
        EditorOutcome::Changed
    }
}
