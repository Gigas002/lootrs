//! Command dispatch.

use std::{
    io::{self, Write},
    path::PathBuf,
};

use liblootrs::{
    launch::{LaunchContext, LaunchError},
    model::ValidationContext,
    prefix::PrefixError,
    process,
    store::Catalog,
};

use crate::settings::{Action, LaunchMode, Settings};

pub mod commands;
pub mod supervisor;
#[cfg(feature = "tui")]
pub mod tui;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("no game named `{0}` in games.toml")]
    UnknownGame(String),

    #[error("no profile named `{0}`")]
    UnknownProfile(String),

    #[error("check found {0}")]
    CheckFailed(String),

    #[error("not launching:\n{0}")]
    Invalid(String),

    #[error("not launching:\n{0}")]
    Preflight(LaunchError),

    #[error("failed to start {}: {source}", program.display())]
    Spawn { program: PathBuf, source: io::Error },

    #[error("failed to write the run log: {0}")]
    RunLog(io::Error),

    #[error("failed to start the supervisor: {0}")]
    Detach(io::Error),

    /// An error reported by the detached supervisor.
    #[error("{message}")]
    Supervisor { code: u8, message: String },

    #[error("no run logs for `{0}` yet")]
    NoLogs(String),

    #[error("`{0}` is not running")]
    NotRunning(String),

    #[error("`{0}` is already running; stop it first with `lootrs stop {0}`")]
    AlreadyRunning(String),

    #[error("cancelled")]
    Cancelled,

    #[error(transparent)]
    Prefix(PrefixError),

    #[error("failed to write output: {0}")]
    Output(#[from] io::Error),
}

impl AppError {
    /// Stdout was closed early (`lootrs list | head`); not worth reporting.
    pub fn is_broken_pipe(&self) -> bool {
        matches!(self, Self::Output(err) if err.kind() == io::ErrorKind::BrokenPipe)
    }

    /// Process exit code: 1 runtime failure, 2 validation/usage,
    /// 3 not running, 4 already running.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Spawn { .. }
            | Self::RunLog(_)
            | Self::Detach(_)
            | Self::NoLogs(_)
            | Self::Cancelled
            | Self::Output(_) => 1,
            Self::UnknownGame(_)
            | Self::UnknownProfile(_)
            | Self::CheckFailed(_)
            | Self::Invalid(_)
            | Self::Preflight(_) => 2,
            Self::Prefix(PrefixError::Busy { .. }) => 4,
            Self::Prefix(_) => 1,
            Self::NotRunning(_) => 3,
            Self::AlreadyRunning(_) => 4,
            Self::Supervisor { code, .. } => *code,
        }
    }
}

/// Runs the resolved action; `Ok` carries the process exit code.
pub fn run(settings: &Settings) -> Result<u8, AppError> {
    let action = &settings.action;
    let _span = tracing::info_span!("command", name = action.name()).entered();
    tracing::debug!(?settings, "resolved settings");

    // Not locked: the supervisor's output threads write to stdout too.
    let mut out = io::stdout();
    let done = |result: Result<(), AppError>| result.map(|()| 0);
    let code = match action {
        Action::Run {
            game,
            mode,
            debug,
            args,
        } => supervisor::launch(
            settings,
            supervisor::Request::Game { name: game },
            *mode,
            *debug,
            args,
            &mut out,
        ),
        Action::Execute {
            profile,
            target,
            mode,
            debug,
            args,
        } => supervisor::launch(
            settings,
            supervisor::Request::Execute {
                profile,
                program: target,
            },
            *mode,
            *debug,
            args,
            &mut out,
        ),
        Action::Logs { game, path_only } => {
            done(commands::logs(settings, game, *path_only, &mut out))
        }
        Action::List { json: false } => done(commands::list(&load_catalog(settings), &mut out)),
        Action::List { json: true } => done(commands::list_json(
            &load_catalog(settings),
            &process::scan(),
            &mut out,
        )),
        Action::Profiles { json: false } => {
            done(commands::profiles(&load_catalog(settings), &mut out))
        }
        Action::Profiles { json: true } => {
            done(commands::profiles_json(&load_catalog(settings), &mut out))
        }
        Action::Check { target } => done(commands::check(
            &Catalog::load(&settings.dirs.config),
            &validation_context(settings),
            &launch_context(settings),
            target,
            &mut out,
        )),
        Action::Stop { game } => done(commands::stop(settings, game, &mut out)),
        Action::Status { json: false } => done(commands::status(&process::scan(), &mut out)),
        Action::Status { json: true } => done(commands::status_json(&process::scan(), &mut out)),
        Action::Killall { everything, yes } => done(commands::killall(
            &process::scan(),
            *everything,
            *yes,
            &mut io::stdin().lock(),
            &mut out,
            || {
                process::terminate(
                    process::scan,
                    |processes| process::killall_targets(processes, *everything),
                    settings.stop_timeout,
                )
            },
        )),
        Action::Init { profile } => done(supervisor::init(settings, profile, &mut out)),
        // Interactive, so always in the foreground.
        Action::Tricks { profile, verbs } => supervisor::launch(
            settings,
            supervisor::Request::Tricks { profile, verbs },
            LaunchMode::Wait,
            false,
            &[],
            &mut out,
        ),
        #[cfg(feature = "tui")]
        Action::Tui => tui::run(settings),
        // The CLI requires a subcommand without the `tui` feature.
        #[cfg(not(feature = "tui"))]
        Action::Tui => Err(AppError::Invalid(
            "lootrs was built without the `tui` feature; run a subcommand (see --help)".into(),
        )),
    }?;
    out.flush()?;
    Ok(code)
}

/// Loads the catalog, logging files that failed to load (`check` reports them itself).
fn load_catalog(settings: &Settings) -> Catalog {
    let catalog = Catalog::load(&settings.dirs.config);
    for err in &catalog.load_errors {
        tracing::warn!("{err}");
    }
    catalog
}

/// Launch context for preflight checks (nothing is launched, so no run directory).
pub fn launch_context(settings: &Settings) -> LaunchContext<'_> {
    LaunchContext {
        home: &settings.dirs.home,
        search_path: &settings.search_path,
        debug: false,
        extra_args: &[],
        artifacts_dir: &settings.logging.dir,
        battleye_runtime: settings.anticheat.battleye.as_deref(),
        eac_runtime: settings.anticheat.eac.as_deref(),
        umu_runtime_update: settings.umu_runtime_update,
    }
}

pub fn validation_context(settings: &Settings) -> ValidationContext<'_> {
    ValidationContext {
        home: &settings.dirs.home,
        battleye_runtime: settings.anticheat.battleye.is_some(),
        eac_runtime: settings.anticheat.eac.is_some(),
    }
}
