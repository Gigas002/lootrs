//! Native runner: the executable runs as is.

use super::RunnerCommand;

pub fn command() -> RunnerCommand {
    RunnerCommand::default()
}
