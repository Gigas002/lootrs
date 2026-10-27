//! Command dispatch. Each command lands in its milestone (see `docs/PLAN.md`).

use crate::settings::{Action, Settings};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("`{command}` is not implemented yet (planned for {milestone})")]
    NotImplemented {
        command: &'static str,
        milestone: &'static str,
    },
}

impl AppError {
    /// Process exit code: 1 runtime failure, 2 validation/usage,
    /// 3 not running, 4 already running.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NotImplemented { .. } => 1,
        }
    }
}

pub fn run(settings: &Settings) -> Result<(), AppError> {
    let action = &settings.action;
    let _span = tracing::info_span!("command", name = action.name()).entered();
    tracing::debug!(?settings, "resolved settings");

    let milestone = match action {
        Action::List { .. } | Action::Profiles { .. } | Action::Check { .. } => "M1",
        Action::Run { .. } | Action::Execute { .. } | Action::Logs { .. } => "M2",
        Action::Stop { .. } | Action::Killall { .. } | Action::Status { .. } => "M3",
        Action::Init { .. } | Action::Tricks { .. } => "M4",
        Action::Tui => "M6",
    };

    Err(AppError::NotImplemented {
        command: action.name(),
        milestone,
    })
}
