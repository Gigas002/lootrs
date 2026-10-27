//! Command dispatch. Commands not built yet report their milestone (see `docs/PLAN.md`).

use std::io::{self, Write};

use liblootrs::{model::ValidationContext, store::Catalog};

use crate::settings::{Action, Settings};

pub mod commands;

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("`{command}` is not implemented yet (planned for {milestone})")]
    NotImplemented {
        command: &'static str,
        milestone: &'static str,
    },

    #[error("no game named `{0}` in games.toml")]
    UnknownGame(String),

    #[error("no profile named `{0}`")]
    UnknownProfile(String),

    #[error("check found {0}")]
    CheckFailed(String),

    #[error("failed to write output: {0}")]
    Output(#[from] io::Error),
}

impl AppError {
    /// Process exit code: 1 runtime failure, 2 validation/usage,
    /// 3 not running, 4 already running.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NotImplemented { .. } | Self::Output(_) => 1,
            Self::UnknownGame(_) | Self::UnknownProfile(_) | Self::CheckFailed(_) => 2,
        }
    }
}

pub fn run(settings: &Settings) -> Result<(), AppError> {
    let action = &settings.action;
    let _span = tracing::info_span!("command", name = action.name()).entered();
    tracing::debug!(?settings, "resolved settings");

    let mut out = io::stdout().lock();
    match action {
        Action::List { json: false } => commands::list(&load_catalog(settings), &mut out),
        Action::Profiles { json: false } => commands::profiles(&load_catalog(settings), &mut out),
        Action::Check { target } => commands::check(
            &Catalog::load(&settings.dirs.config),
            &validation_context(settings),
            target,
            &mut out,
        ),
        Action::List { json: true } | Action::Profiles { json: true } => {
            not_implemented("--json", "M7")
        }
        Action::Run { .. } | Action::Execute { .. } | Action::Logs { .. } => {
            not_implemented(action.name(), "M2")
        }
        Action::Stop { .. } | Action::Killall { .. } | Action::Status { .. } => {
            not_implemented(action.name(), "M3")
        }
        Action::Init { .. } | Action::Tricks { .. } => not_implemented(action.name(), "M4"),
        Action::Tui => not_implemented(action.name(), "M6"),
    }?;
    out.flush()?;
    Ok(())
}

/// Loads the catalog, logging files that failed to load (`check` reports them itself).
fn load_catalog(settings: &Settings) -> Catalog {
    let catalog = Catalog::load(&settings.dirs.config);
    for err in &catalog.load_errors {
        tracing::warn!("{err}");
    }
    catalog
}

pub fn validation_context(settings: &Settings) -> ValidationContext<'_> {
    ValidationContext {
        home: &settings.dirs.home,
        battleye_runtime: settings.anticheat.battleye.is_some(),
        eac_runtime: settings.anticheat.eac.is_some(),
    }
}

fn not_implemented(command: &'static str, milestone: &'static str) -> Result<(), AppError> {
    Err(AppError::NotImplemented { command, milestone })
}
