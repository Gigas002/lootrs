//! Unified resolver: CLI > `config.toml` > built-in defaults.
//!
//! [`Settings`] is the only configuration type passed below `main`.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};

use liblootrs::paths::{self, AppDirs, PathsError, XdgDirs};

use crate::config::{self, ConfigError, LogLevel};

#[cfg(test)]
mod tests;

pub const DEFAULT_LOG_KEEP: usize = 10;
pub const DEFAULT_STOP_TIMEOUT_SECS: u64 = 5;

/// Values taken from the command line, already converted from `clap` types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliOverrides {
    pub config_dir: Option<PathBuf>,
    pub log_dir: Option<PathBuf>,
    pub log_level: Option<LogLevel>,
    pub action: Action,
}

/// What this invocation should do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Action {
    /// Open the TUI (only reachable with the `tui` feature).
    #[default]
    Tui,
    Run {
        game: String,
        wait: bool,
        debug: bool,
        args: Vec<String>,
    },
    Stop {
        game: String,
    },
    Killall {
        everything: bool,
        yes: bool,
    },
    Init {
        profile: String,
    },
    Tricks {
        profile: String,
        verbs: Vec<String>,
    },
    Execute {
        profile: String,
        target: PathBuf,
        wait: bool,
        debug: bool,
        args: Vec<String>,
    },
    List {
        json: bool,
    },
    Profiles {
        json: bool,
    },
    Status {
        json: bool,
    },
    Check {
        target: CheckTarget,
    },
    Logs {
        game: String,
        path_only: bool,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CheckTarget {
    /// Everything in `games.toml` and `profiles/`.
    #[default]
    All,
    Game(String),
    Profile(String),
}

impl Action {
    /// Subcommand name, for logs and messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Run { .. } => "run",
            Self::Stop { .. } => "stop",
            Self::Killall { .. } => "killall",
            Self::Init { .. } => "init",
            Self::Tricks { .. } => "tricks",
            Self::Execute { .. } => "execute",
            Self::List { .. } => "list",
            Self::Profiles { .. } => "profiles",
            Self::Status { .. } => "status",
            Self::Check { .. } => "check",
            Self::Logs { .. } => "logs",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub dirs: Dirs,
    pub logging: Logging,
    pub stop_timeout: Duration,
    pub umu_runtime_update: bool,
    pub anticheat: AnticheatRuntimes,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    pub home: PathBuf,
    /// Holds `config.toml`, `games.toml` and `profiles/`.
    pub config: PathBuf,
    pub runtime: PathBuf,
    pub prefix_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logging {
    pub level: LogLevel,
    pub dir: PathBuf,
    /// Run logs per game/profile, and daily app logs, to keep.
    pub keep: usize,
    /// Mirror the app log to stderr (off while the TUI owns the terminal).
    pub stderr: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnticheatRuntimes {
    pub battleye: Option<PathBuf>,
    pub eac: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(transparent)]
    Paths(#[from] PathsError),

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("failed to resolve {}: {source}", path.display())]
    Absolute {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("config.toml: `{key}` must be an absolute path (or start with `~`), got {}", path.display())]
    RelativePath { key: &'static str, path: PathBuf },

    #[error("config.toml: `log_keep` must be at least 1")]
    LogKeep,
}

impl Settings {
    /// Resolves settings from CLI values, `config.toml` and the environment
    /// (`lookup` is typically `std::env::var_os`).
    pub fn resolve<F>(cli: CliOverrides, lookup: F) -> Result<Self, SettingsError>
    where
        F: Fn(&str) -> Option<OsString>,
    {
        let xdg = XdgDirs::from_lookup(lookup)?;
        let defaults = AppDirs::new(&xdg);

        let config_dir = match cli.config_dir {
            Some(dir) => absolute(dir)?,
            None => defaults.config,
        };
        let file = config::load(&config_dir)?.unwrap_or_default();
        let from_config = |key, path| config_path(key, path, &xdg.home);

        let log_dir = match cli.log_dir {
            Some(dir) => absolute(dir)?,
            None => from_config("log_dir", file.log_dir)?.unwrap_or(defaults.logs),
        };
        let keep = file.log_keep.unwrap_or(DEFAULT_LOG_KEEP);
        if keep == 0 {
            return Err(SettingsError::LogKeep);
        }

        Ok(Self {
            dirs: Dirs {
                config: config_dir,
                runtime: defaults.runtime,
                prefix_root: from_config("prefix_root", file.prefix_root)?
                    .unwrap_or(defaults.prefixes),
                home: xdg.home.clone(),
            },
            logging: Logging {
                level: cli.log_level.or(file.log_level).unwrap_or_default(),
                dir: log_dir,
                keep,
                stderr: cli.action != Action::Tui,
            },
            stop_timeout: Duration::from_secs(
                file.stop_timeout_secs.unwrap_or(DEFAULT_STOP_TIMEOUT_SECS),
            ),
            umu_runtime_update: file.umu_runtime_update.unwrap_or(false),
            anticheat: AnticheatRuntimes {
                battleye: from_config("battleye_runtime", file.battleye_runtime)?,
                eac: from_config("eac_runtime", file.eac_runtime)?,
            },
            action: cli.action,
        })
    }
}

/// CLI paths are relative to the working directory.
fn absolute(path: PathBuf) -> Result<PathBuf, SettingsError> {
    std::path::absolute(&path).map_err(|source| SettingsError::Absolute { path, source })
}

/// Config paths expand `~` and must then be absolute; empty means unset.
fn config_path(
    key: &'static str,
    path: Option<PathBuf>,
    home: &Path,
) -> Result<Option<PathBuf>, SettingsError> {
    let Some(path) = path.filter(|path| !path.as_os_str().is_empty()) else {
        return Ok(None);
    };

    let expanded = paths::expand_tilde(&path, home);
    if expanded.is_absolute() {
        Ok(Some(expanded))
    } else {
        Err(SettingsError::RelativePath { key, path })
    }
}
