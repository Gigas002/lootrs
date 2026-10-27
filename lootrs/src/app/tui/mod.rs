//! The TUI's state machine. Keys arrive as [`Key`]s and results of background
//! work through [`Tui::on_scan`]/[`Tui::on_job_done`]; anything that needs other
//! processes or the terminal itself goes out as an [`Effect`] for the event loop
//! in [`run`]. Rendering lives in `ui/`.

use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

use liblootrs::{
    model::{Issue, Runner, Severity, Subject},
    process::{self, Owner, Process},
    runlog::{self, RunKind},
    store::{self, Catalog},
};

use self::form::{Form, Kind, Outcome};
use super::{AppError, commands, supervisor, validation_context};
use crate::{
    settings::{Action, LaunchMode, Settings},
    ui,
};

pub mod form;

#[cfg(test)]
mod tests;

/// How often running processes are scanned (and the log pane refreshed).
const SCAN_INTERVAL: Duration = Duration::from_secs(1);
const INPUT_POLL: Duration = Duration::from_millis(100);
/// Lines kept in the log pane.
const LOG_LINES: usize = 200;

/// A key press, independent of the terminal library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Games,
    Profiles,
}

/// Live processes of one game or session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub procs: usize,
    /// Distinct command names, wine infrastructure left out.
    pub commands: Vec<String>,
}

/// Groups tagged processes by owner, see [`process::running`].
pub fn summarize(processes: &[Process]) -> BTreeMap<Owner, Running> {
    process::running(processes)
        .into_iter()
        .map(|(owner, group)| {
            let mut commands: Vec<String> = Vec::new();
            for process in group.iter().filter(|p| !p.is_wine_infrastructure()) {
                if !commands.contains(&process.comm) {
                    commands.push(process.comm.clone());
                }
            }
            let procs = group.len();
            (owner, Running { procs, commands })
        })
        .collect()
}

/// Work that runs on a background thread; its result comes back as a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    Run { game: String, debug: bool },
    Execute { profile: String, program: PathBuf },
    Stop { game: String },
    Killall,
}

/// Commands that take over the terminal; the TUI comes back afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreground {
    Init { profile: String },
    Tricks { profile: String, verbs: Vec<String> },
    Pager { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Quit,
    Job(Job),
    Foreground(Foreground),
}

/// What a confirmation dialog confirms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    DeleteGame(String),
    DeleteProfile(String),
    Killall,
    DiscardForm,
}

/// What a text prompt asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptFor {
    Tricks { profile: String },
    Execute { profile: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    Help,
    Confirm {
        message: String,
        pending: Pending,
    },
    Prompt {
        title: String,
        hint: String,
        input: form::Input,
        purpose: PromptFor,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub error: bool,
}

/// The tail of the selected game's (or profile's) latest run log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogPane {
    pub path: Option<PathBuf>,
    pub lines: Vec<String>,
}

pub struct Tui {
    pub settings: Settings,
    pub catalog: Catalog,
    /// Validation issues per game and per profile, from the last load.
    pub game_issues: BTreeMap<String, Vec<Issue>>,
    pub profile_issues: BTreeMap<String, Vec<Issue>>,
    /// Modification times at load, to refuse overwriting hand edits.
    mtimes: BTreeMap<PathBuf, Option<SystemTime>>,
    pub tab: Tab,
    selected: [usize; 2],
    pub running: BTreeMap<Owner, Running>,
    /// Shown when `Some`.
    pub log: Option<LogPane>,
    pub status: Option<Status>,
    /// Background jobs still running.
    pub jobs: usize,
    pub form: Option<Form>,
    pub modal: Option<Modal>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl Tui {
    pub fn new(settings: Settings) -> Self {
        let mut tui = Self {
            settings,
            catalog: Catalog::default(),
            game_issues: BTreeMap::new(),
            profile_issues: BTreeMap::new(),
            mtimes: BTreeMap::new(),
            tab: Tab::Games,
            selected: [0, 0],
            running: BTreeMap::new(),
            log: None,
            status: None,
            jobs: 0,
            form: None,
            modal: None,
        };
        tui.reload();
        if !tui.catalog.load_errors.is_empty() {
            tui.error(format!(
                "{} failed to load; see `lootrs check`",
                crate::utils::plural(tui.catalog.load_errors.len(), "file")
            ));
        }
        tui
    }

    /// Reads games and profiles from disk again.
    pub fn reload(&mut self) {
        let config = &self.settings.dirs.config;
        self.catalog = Catalog::load(config);
        for err in &self.catalog.load_errors {
            tracing::warn!("{err}");
        }

        self.mtimes.clear();
        let games = store::games_path(config);
        self.mtimes.insert(games.clone(), mtime(&games));
        for entry in std::fs::read_dir(store::profiles_dir(config))
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            self.mtimes.insert(path.clone(), mtime(&path));
        }

        self.game_issues.clear();
        self.profile_issues.clear();
        let ctx = validation_context(&self.settings);
        for issue in self.catalog.validate(&ctx) {
            match &issue.subject {
                Subject::Game(name) => self.game_issues.entry(name.clone()),
                Subject::Profile(name) => self.profile_issues.entry(name.clone()),
                Subject::File(_) => continue,
            }
            .or_default()
            .push(issue);
        }

        self.selected[0] = self.selected[0].min(self.catalog.games.len().saturating_sub(1));
        self.selected[1] = self.selected[1].min(self.catalog.profiles.len().saturating_sub(1));
        self.refresh_log();
    }

    pub fn selected(&self) -> usize {
        self.selected[self.tab as usize]
    }

    fn len(&self) -> usize {
        match self.tab {
            Tab::Games => self.catalog.games.len(),
            Tab::Profiles => self.catalog.profiles.len(),
        }
    }

    fn select(&mut self, index: usize) {
        let last = self.len().saturating_sub(1);
        self.selected[self.tab as usize] = index.min(last);
        self.refresh_log();
    }

    fn move_selection(&mut self, delta: isize) {
        self.select(self.selected().saturating_add_signed(delta));
    }

    pub fn selected_game(&self) -> Option<&liblootrs::model::Game> {
        self.catalog.games.get(self.selected[Tab::Games as usize])
    }

    pub fn selected_profile(&self) -> Option<&liblootrs::model::Profile> {
        self.catalog
            .profiles
            .get(self.selected[Tab::Profiles as usize])
    }

    pub fn is_running(&self, game: &str) -> bool {
        self.running.contains_key(&Owner::Game(game.to_owned()))
    }

    fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            error: false,
        });
    }

    fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        tracing::warn!("{text}");
        self.status = Some(Status { text, error: true });
    }

    /// New process scan results; also refreshes the log pane.
    pub fn on_scan(&mut self, running: BTreeMap<Owner, Running>) {
        self.running = running;
        self.refresh_log();
    }

    pub fn on_job_done(&mut self, result: Result<String, String>) {
        self.jobs = self.jobs.saturating_sub(1);
        match result {
            Ok(text) => self.info(text),
            Err(text) => self.error(text),
        }
    }

    /// The latest run log for the selection: a game's runs, or a profile's
    /// newest `execute`/`tricks`/`init` log.
    pub fn log_path(&self) -> Option<PathBuf> {
        let logs = &self.settings.logging.dir;
        match self.tab {
            Tab::Games => runlog::latest(logs, RunKind::Game(&self.selected_game()?.name)),
            Tab::Profiles => {
                let name = &self.selected_profile()?.name;
                [
                    RunKind::Execute(name),
                    RunKind::Tricks(name),
                    RunKind::Init(name),
                ]
                .into_iter()
                .filter_map(|kind| runlog::latest(logs, kind))
                .max_by_key(|path| mtime(path))
            }
        }
    }

    fn refresh_log(&mut self) {
        if self.log.is_none() {
            return;
        }
        let path = self.log_path();
        let lines = path
            .as_deref()
            .and_then(|path| runlog::tail(path, LOG_LINES).ok())
            .unwrap_or_default();
        self.log = Some(LogPane { path, lines });
    }

    pub fn handle_key(&mut self, key: Key) -> Option<Effect> {
        if key == Key::Ctrl('c') {
            return Some(Effect::Quit);
        }
        if let Some(modal) = self.modal.take() {
            return self.modal_key(modal, key);
        }
        if let Some(form) = &mut self.form {
            match form.handle_key(key) {
                Outcome::Continue => {}
                Outcome::Save => self.save_form(),
                Outcome::Cancel if form.dirty => {
                    self.modal = Some(Modal::Confirm {
                        message: "Discard unsaved changes?".into(),
                        pending: Pending::DiscardForm,
                    });
                }
                Outcome::Cancel => self.form = None,
            }
            return None;
        }
        self.main_key(key)
    }

    fn modal_key(&mut self, modal: Modal, key: Key) -> Option<Effect> {
        match modal {
            Modal::Help => None,
            Modal::Confirm { pending, .. } if key == Key::Char('y') => self.confirmed(pending),
            Modal::Confirm { .. } if matches!(key, Key::Char('n') | Key::Esc) => None,
            Modal::Confirm { .. } => {
                self.modal = Some(modal);
                None
            }
            Modal::Prompt { purpose, input, .. } if key == Key::Enter => {
                self.prompted(purpose, input.text().trim())
            }
            Modal::Prompt { .. } if key == Key::Esc => None,
            Modal::Prompt {
                title,
                hint,
                mut input,
                purpose,
            } => {
                input.handle(key);
                self.modal = Some(Modal::Prompt {
                    title,
                    hint,
                    input,
                    purpose,
                });
                None
            }
        }
    }

    fn main_key(&mut self, key: Key) -> Option<Effect> {
        match key {
            Key::Char('q') => return Some(Effect::Quit),
            Key::Tab | Key::BackTab => {
                self.tab = match self.tab {
                    Tab::Games => Tab::Profiles,
                    Tab::Profiles => Tab::Games,
                };
                self.refresh_log();
            }
            Key::Char('1') => {
                self.tab = Tab::Games;
                self.refresh_log();
            }
            Key::Char('2') => {
                self.tab = Tab::Profiles;
                self.refresh_log();
            }
            Key::Char('?') => self.modal = Some(Modal::Help),
            Key::Up | Key::Char('k') => self.move_selection(-1),
            Key::Down | Key::Char('j') => self.move_selection(1),
            Key::PageUp => self.move_selection(-10),
            Key::PageDown => self.move_selection(10),
            Key::Home | Key::Char('g') => self.select(0),
            Key::End | Key::Char('G') => self.select(usize::MAX),
            Key::Char('l') => {
                self.log = match self.log {
                    Some(_) => None,
                    None => Some(LogPane::default()),
                };
                self.refresh_log();
            }
            Key::Char('L') => match self.log_path() {
                Some(path) => return Some(Effect::Foreground(Foreground::Pager { path })),
                None => self.error("no logs yet"),
            },
            Key::Char('R') => {
                self.reload();
                self.info("reloaded games and profiles");
            }
            Key::Char('K') if self.running.is_empty() => self.info("nothing running"),
            Key::Char('K') => {
                self.modal = Some(Modal::Confirm {
                    message: format!(
                        "Kill everything lootrs started? ({} running)",
                        self.running.len()
                    ),
                    pending: Pending::Killall,
                });
            }
            Key::Char('a') => self.open_form(None),
            Key::Char('e') => {
                let name = match self.tab {
                    Tab::Games => self.selected_game().map(|g| g.name.clone()),
                    Tab::Profiles => self.selected_profile().map(|p| p.name.clone()),
                };
                if name.is_some() {
                    self.open_form(name.as_deref());
                }
            }
            Key::Char('d') => self.ask_delete(),
            key => {
                return match self.tab {
                    Tab::Games => self.games_key(key),
                    Tab::Profiles => self.profiles_key(key),
                };
            }
        }
        None
    }

    fn games_key(&mut self, key: Key) -> Option<Effect> {
        let name = self.selected_game()?.name.clone();
        match key {
            Key::Enter | Key::Char('D') if self.is_running(&name) => {
                self.error(format!("`{name}` is already running"));
                None
            }
            Key::Enter => self.job(Job::Run {
                game: name,
                debug: false,
            }),
            Key::Char('D') => self.job(Job::Run {
                game: name,
                debug: true,
            }),
            Key::Char('s') if !self.is_running(&name) => {
                self.error(format!("`{name}` is not running"));
                None
            }
            Key::Char('s') => self.job(Job::Stop { game: name }),
            _ => None,
        }
    }

    fn profiles_key(&mut self, key: Key) -> Option<Effect> {
        let profile = self.selected_profile()?;
        let name = profile.name.clone();
        let runner = profile.runner;
        if matches!(key, Key::Char('i' | 't')) && runner == Runner::Native {
            self.error(format!(
                "`{name}` uses runner `native`, which has no prefix"
            ));
            return None;
        }
        match key {
            Key::Char('i') => Some(Effect::Foreground(Foreground::Init { profile: name })),
            Key::Char('t') => {
                let hint = match runner {
                    Runner::Umu => "verbs, e.g. `corefonts vcrun2022` (required with umu)",
                    _ => "verbs, e.g. `corefonts vcrun2022`; empty opens the winetricks GUI",
                };
                self.prompt(
                    format!("winetricks in `{name}`"),
                    hint,
                    PromptFor::Tricks { profile: name },
                );
                None
            }
            Key::Char('x') => {
                self.prompt(
                    format!("execute with `{name}`"),
                    "path to a program, or a wine builtin such as `winecfg`",
                    PromptFor::Execute { profile: name },
                );
                None
            }
            _ => None,
        }
    }

    fn prompt(&mut self, title: String, hint: &str, purpose: PromptFor) {
        self.modal = Some(Modal::Prompt {
            title,
            hint: hint.to_owned(),
            input: form::Input::default(),
            purpose,
        });
    }

    fn prompted(&mut self, purpose: PromptFor, text: &str) -> Option<Effect> {
        match purpose {
            PromptFor::Tricks { profile } => {
                let verbs = text.split_whitespace().map(str::to_owned).collect();
                Some(Effect::Foreground(Foreground::Tricks { profile, verbs }))
            }
            PromptFor::Execute { .. } if text.is_empty() => None,
            PromptFor::Execute { profile } => self.job(Job::Execute {
                profile,
                program: text.into(),
            }),
        }
    }

    fn job(&mut self, job: Job) -> Option<Effect> {
        let text = match &job {
            Job::Run { game, .. } => format!("starting `{game}`…"),
            Job::Execute { program, .. } => format!("starting {}…", program.display()),
            Job::Stop { game } => format!("stopping `{game}`…"),
            Job::Killall => "killing everything lootrs started…".to_owned(),
        };
        self.info(text);
        self.jobs += 1;
        Some(Effect::Job(job))
    }

    fn ask_delete(&mut self) {
        let (message, pending) = match self.tab {
            Tab::Games => {
                let Some(game) = self.selected_game() else {
                    return;
                };
                (
                    format!("Delete game `{}`? (its logs and files are kept)", game.name),
                    Pending::DeleteGame(game.name.clone()),
                )
            }
            Tab::Profiles => {
                let Some(profile) = self.selected_profile() else {
                    return;
                };
                let name = profile.name.clone();
                let users = self.games_using(&name);
                if !users.is_empty() {
                    self.error(format!(
                        "`{name}` is used by {}; change their profile first",
                        users.join(", ")
                    ));
                    return;
                }
                (
                    format!("Delete profile `{name}`? (its prefix is kept)"),
                    Pending::DeleteProfile(name),
                )
            }
        };
        self.modal = Some(Modal::Confirm { message, pending });
    }

    fn games_using(&self, profile: &str) -> Vec<String> {
        self.catalog
            .games
            .iter()
            .filter(|game| game.profile == profile)
            .map(|game| game.name.clone())
            .collect()
    }

    fn confirmed(&mut self, pending: Pending) -> Option<Effect> {
        match pending {
            Pending::DiscardForm => self.form = None,
            Pending::Killall => return self.job(Job::Killall),
            Pending::DeleteGame(name) => {
                let mut games = self.catalog.games.clone();
                games.retain(|game| game.name != name);
                match self.write_games(&games) {
                    Ok(()) => self.info(format!("deleted game `{name}`")),
                    Err(err) => self.error(err),
                }
            }
            Pending::DeleteProfile(name) => {
                let path = store::profile_path(&self.settings.dirs.config, &name);
                let result = self.check_unchanged(&path).and_then(|()| {
                    store::remove_profile(&self.settings.dirs.config, &name)
                        .map_err(|err| err.to_string())
                });
                self.reload();
                match result {
                    Ok(()) => self.info(format!("deleted profile `{name}`")),
                    Err(err) => self.error(err),
                }
            }
        }
        None
    }

    fn open_form(&mut self, name: Option<&str>) {
        let form = match self.tab {
            Tab::Games => {
                if self.catalog.profiles.is_empty() {
                    self.error("add a profile first (Profiles tab)");
                    return;
                }
                let runners = self
                    .catalog
                    .profiles
                    .iter()
                    .map(|p| (p.name.clone(), p.runner))
                    .collect();
                Form::game(name.and_then(|n| self.catalog.game(n)), runners)
            }
            Tab::Profiles => Form::profile(
                name.and_then(|n| self.catalog.profile(n)),
                &self.settings.dirs.prefix_root,
            ),
        };
        self.form = Some(form);
    }

    /// Refuses to overwrite a file someone else changed since it was loaded.
    fn check_unchanged(&self, path: &Path) -> Result<(), String> {
        let loaded = self.mtimes.get(path).copied().flatten();
        if mtime(path) == loaded {
            Ok(())
        } else {
            Err(format!(
                "{} changed on disk since lootrs loaded it; press R to reload",
                path.display()
            ))
        }
    }

    fn write_games(&mut self, games: &[liblootrs::model::Game]) -> Result<(), String> {
        let config = &self.settings.dirs.config;
        let path = store::games_path(config);
        if self
            .catalog
            .load_errors
            .iter()
            .any(|err| err.path() == Some(path.as_path()))
        {
            return Err(format!(
                "{} failed to load; fix it by hand first (see `lootrs check`)",
                path.display()
            ));
        }
        self.check_unchanged(&path)?;
        let result = store::save_games(config, games).map_err(|err| err.to_string());
        self.reload();
        result
    }

    fn save_form(&mut self) {
        let Some(form) = self.form.take() else {
            return;
        };
        let result = match form.kind {
            Kind::Game => self.save_game(&form),
            Kind::Profile => self.save_profile(&form),
        };
        match result {
            Ok((name, warnings)) => {
                // Select what was just saved.
                let index = match form.kind {
                    Kind::Game => self.catalog.games.iter().position(|g| g.name == name),
                    Kind::Profile => self.catalog.profiles.iter().position(|p| p.name == name),
                };
                if let Some(index) = index {
                    self.select(index);
                }
                match warnings.first() {
                    None => self.info(format!("saved `{name}`")),
                    Some(first) => self.info(format!(
                        "saved `{name}` with {}: {first}",
                        crate::utils::plural(warnings.len(), "warning")
                    )),
                }
            }
            Err(errors) => {
                let mut form = form;
                form.errors = errors;
                self.form = Some(form);
            }
        }
    }

    /// Splits validation issues into errors (which block saving) and warnings.
    fn split(issues: Vec<Issue>) -> Result<Vec<String>, Vec<String>> {
        let (errors, warnings): (Vec<Issue>, Vec<Issue>) = issues
            .into_iter()
            .partition(|issue| issue.severity == Severity::Error);
        if errors.is_empty() {
            Ok(warnings.into_iter().map(|i| i.message).collect())
        } else {
            Err(errors.into_iter().map(|i| i.message).collect())
        }
    }

    fn save_game(&mut self, form: &Form) -> Result<(String, Vec<String>), Vec<String>> {
        let game = form.to_game();
        let original = form.original.as_deref();
        if Some(game.name.as_str()) != original && self.catalog.game(&game.name).is_some() {
            return Err(vec![format!("a game named `{}` already exists", game.name)]);
        }
        if original.is_some_and(|name| name != game.name && self.is_running(name)) {
            return Err(vec!["stop the game before renaming it".into()]);
        }
        let ctx = validation_context(&self.settings);
        let warnings = Self::split(game.validate(self.catalog.profile(&game.profile), &ctx))?;

        let mut games = self.catalog.games.clone();
        match original.and_then(|name| games.iter().position(|g| g.name == name)) {
            Some(index) => games[index] = game.clone(),
            None => games.push(game.clone()),
        }
        self.write_games(&games).map_err(|err| vec![err])?;
        Ok((game.name, warnings))
    }

    fn save_profile(&mut self, form: &Form) -> Result<(String, Vec<String>), Vec<String>> {
        let profile = form.to_profile();
        let config = &self.settings.dirs.config;
        let path = store::profile_path(config, &profile.name);
        if form.original.is_none()
            && (self.catalog.profile(&profile.name).is_some() || path.exists())
        {
            return Err(vec![format!(
                "a profile named `{}` already exists",
                profile.name
            )]);
        }
        let warnings = Self::split(profile.validate(&validation_context(&self.settings)))?;

        self.check_unchanged(&path).map_err(|err| vec![err])?;
        let result = store::save_profile(config, &profile).map_err(|err| vec![err.to_string()]);
        self.reload();
        result.map(|()| (profile.name, warnings))
    }
}

/// Messages from background threads.
enum Message {
    Scan(BTreeMap<Owner, Running>),
    JobDone(Result<String, String>),
}

/// Runs the TUI until the user quits.
pub fn run(settings: &Settings) -> Result<u8, AppError> {
    let (tx, rx) = mpsc::channel();
    let scan_tx = tx.clone();
    thread::spawn(move || {
        // Ends when the TUI is gone and the channel closed.
        while scan_tx
            .send(Message::Scan(summarize(&process::scan())))
            .is_ok()
        {
            thread::sleep(SCAN_INTERVAL);
        }
    });

    let mut terminal = ui::Terminal::enter()?;
    let mut tui = Tui::new(settings.clone());
    tracing::info!("TUI started");
    loop {
        terminal.draw(&tui)?;
        if let Some(key) = terminal.poll_key(INPUT_POLL)? {
            match tui.handle_key(key) {
                None => {}
                Some(Effect::Quit) => break,
                Some(Effect::Job(job)) => {
                    let settings = settings.clone();
                    let tx = tx.clone();
                    thread::spawn(move || {
                        let _ = tx.send(Message::JobDone(run_job(&settings, &job)));
                    });
                }
                Some(Effect::Foreground(command)) => {
                    terminal.suspend(|| foreground(settings, &command))??;
                    tui.reload();
                }
            }
        }
        while let Ok(message) = rx.try_recv() {
            match message {
                Message::Scan(running) => tui.on_scan(running),
                Message::JobDone(result) => tui.on_job_done(result),
            }
        }
    }
    tracing::info!("TUI closed");
    Ok(0)
}

/// Runs a background job; the text is shown in the status bar.
fn run_job(settings: &Settings, job: &Job) -> Result<String, String> {
    let mut out = Vec::new();
    let result = match job {
        Job::Run { game, debug } => {
            let settings = Settings {
                action: Action::Run {
                    game: game.clone(),
                    mode: LaunchMode::Detach,
                    debug: *debug,
                    args: Vec::new(),
                },
                ..settings.clone()
            };
            supervisor::launch(
                &settings,
                supervisor::Request::Game { name: game },
                LaunchMode::Detach,
                *debug,
                &[],
                &mut out,
            )
            .map(drop)
        }
        Job::Execute { profile, program } => {
            let settings = Settings {
                action: Action::Execute {
                    profile: profile.clone(),
                    target: program.clone(),
                    mode: LaunchMode::Detach,
                    debug: false,
                    args: Vec::new(),
                },
                ..settings.clone()
            };
            supervisor::launch(
                &settings,
                supervisor::Request::Execute { profile, program },
                LaunchMode::Detach,
                false,
                &[],
                &mut out,
            )
            .map(drop)
        }
        Job::Stop { game } => commands::stop(settings, game, &mut out),
        Job::Killall => commands::killall(
            &process::scan(),
            false,
            true,
            &mut io::empty(),
            &mut out,
            || {
                process::terminate(
                    process::scan,
                    |processes| process::killall_targets(processes, false),
                    settings.stop_timeout,
                )
            },
        ),
    };
    match result {
        // The first line is the summary ("started `x` (pid 1)"); the log path follows.
        Ok(()) => Ok(String::from_utf8_lossy(&out)
            .lines()
            .next()
            .unwrap_or("done")
            .to_owned()),
        Err(err) => Err(err.to_string().replace('\n', " ")),
    }
}

/// Runs a command in the plain terminal, then waits for Enter.
fn foreground(settings: &Settings, command: &Foreground) -> io::Result<()> {
    let mut out = io::stdout();
    let result = match command {
        Foreground::Pager { path } => {
            let mut words = settings.pager.split_whitespace();
            let program = words.next().unwrap_or(crate::settings::DEFAULT_PAGER);
            match Command::new(program).args(words).arg(path).status() {
                Ok(_) => return Ok(()),
                Err(err) => Err(format!("failed to run `{program}`: {err}")),
            }
        }
        Foreground::Init { profile } => {
            writeln!(out, "lootrs: setting up the prefix of `{profile}`…")?;
            supervisor::init(settings, profile, &mut out).map_err(|err| err.to_string())
        }
        Foreground::Tricks { profile, verbs } => supervisor::launch(
            settings,
            supervisor::Request::Tricks { profile, verbs },
            LaunchMode::Wait,
            false,
            &[],
            &mut out,
        )
        .map_err(|err| err.to_string())
        .and_then(|code| match code {
            0 => Ok(()),
            code => Err(format!("winetricks exited with code {code}")),
        }),
    };
    if let Err(err) = result {
        tracing::warn!("{err}");
        writeln!(out, "lootrs: {err}")?;
    }
    write!(out, "\npress Enter to return to lootrs")?;
    out.flush()?;
    io::stdin().lock().read_line(&mut String::new())?;
    Ok(())
}
