//! Command-line definitions. Parsed only in `main`, then converted into
//! [`CliOverrides`] so no `clap` type crosses the settings boundary.

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

use crate::{
    config::LogLevel,
    settings::{Action, CheckTarget, CliOverrides},
};

#[cfg(test)]
mod tests;

/// Minimal launcher for Windows games through wine, Proton (umu) or natively.
#[derive(Debug, Parser)]
#[command(
    name = "lootrs",
    version,
    about,
    subcommand_required = !cfg!(feature = "tui"),
    arg_required_else_help = !cfg!(feature = "tui"),
)]
pub struct Cli {
    /// Directory with config.toml, games.toml and profiles/ [default: $XDG_CONFIG_HOME/lootrs].
    #[arg(long, global = true, value_name = "DIR")]
    pub config_dir: Option<PathBuf>,

    /// Directory for logs [default: $XDG_STATE_HOME/lootrs/logs].
    #[arg(long, global = true, value_name = "DIR")]
    pub log_dir: Option<PathBuf>,

    /// Log level for lootrs's own log.
    #[arg(long, global = true, value_enum, conflicts_with_all = ["verbose", "quiet"])]
    pub log_level: Option<LogLevel>,

    /// Log more: -v debug, -vv trace.
    #[arg(short, long, global = true, action = ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Log errors only.
    #[arg(short, long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Launch a game from games.toml.
    Run {
        /// Game name from games.toml.
        game: String,
        /// Stay attached: stream output and exit with the game's exit code.
        #[arg(long)]
        wait: bool,
        /// Verbose runner logging for this run.
        #[arg(long)]
        debug: bool,
        /// Extra arguments appended to the game's own.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Stop a running game.
    Stop {
        /// Game name from games.toml.
        game: String,
    },
    /// Kill every process lootrs started.
    Killall {
        /// Also kill any wine/Proton process of this user, even ones lootrs didn't start.
        #[arg(long)]
        everything: bool,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Create or update a profile's prefix.
    Init {
        #[arg(short, long)]
        profile: String,
    },
    /// Run winetricks in a profile's prefix.
    Tricks {
        #[arg(short, long)]
        profile: String,
        /// Winetricks verbs; none opens the winetricks GUI (wine only).
        #[arg(last = true)]
        verbs: Vec<String>,
    },
    /// Run an executable once with a profile, e.g. an installer or `winecfg`.
    Execute {
        #[arg(short, long)]
        profile: String,
        /// Executable path, or a wine program name like `winecfg`.
        target: PathBuf,
        /// Stay attached: stream output and exit with the program's exit code.
        #[arg(long)]
        wait: bool,
        /// Verbose runner logging for this run.
        #[arg(long)]
        debug: bool,
        /// Arguments passed to the executable.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// List games.
    List {
        #[arg(long)]
        json: bool,
    },
    /// List profiles.
    Profiles {
        #[arg(long)]
        json: bool,
    },
    /// Show running games and sessions.
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Validate games and profiles without launching anything.
    Check {
        /// Check only this game.
        game: Option<String>,
        /// Check only this profile.
        #[arg(short, long, conflicts_with = "game")]
        profile: Option<String>,
    },
    /// Print a game's latest run log.
    Logs {
        /// Game name from games.toml.
        game: String,
        /// Print the log file path instead of its contents.
        #[arg(long)]
        path: bool,
    },
}

impl Cli {
    fn log_level(&self) -> Option<LogLevel> {
        match (self.log_level, self.verbose, self.quiet) {
            (Some(level), ..) => Some(level),
            (None, _, true) => Some(LogLevel::Error),
            (None, 0, false) => None,
            (None, 1, false) => Some(LogLevel::Debug),
            (None, _, false) => Some(LogLevel::Trace),
        }
    }
}

impl From<Cli> for CliOverrides {
    fn from(cli: Cli) -> Self {
        let log_level = cli.log_level();
        let action = cli.command.map_or(Action::Tui, Action::from);

        Self {
            config_dir: cli.config_dir,
            log_dir: cli.log_dir,
            log_level,
            action,
        }
    }
}

impl From<Command> for Action {
    fn from(command: Command) -> Self {
        match command {
            Command::Run {
                game,
                wait,
                debug,
                args,
            } => Self::Run {
                game,
                wait,
                debug,
                args,
            },
            Command::Stop { game } => Self::Stop { game },
            Command::Killall { everything, yes } => Self::Killall { everything, yes },
            Command::Init { profile } => Self::Init { profile },
            Command::Tricks { profile, verbs } => Self::Tricks { profile, verbs },
            Command::Execute {
                profile,
                target,
                wait,
                debug,
                args,
            } => Self::Execute {
                profile,
                target,
                wait,
                debug,
                args,
            },
            Command::List { json } => Self::List { json },
            Command::Profiles { json } => Self::Profiles { json },
            Command::Status { json } => Self::Status { json },
            Command::Check { game, profile } => Self::Check {
                target: match (game, profile) {
                    (Some(game), _) => CheckTarget::Game(game),
                    (None, Some(profile)) => CheckTarget::Profile(profile),
                    (None, None) => CheckTarget::All,
                },
            },
            Command::Logs { game, path } => Self::Logs {
                game,
                path_only: path,
            },
        }
    }
}
