//! `config.toml` discovery, reading and deserialization.
//!
//! Every field is optional; precedence and defaults are applied in `settings`.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::Deserialize;

#[cfg(test)]
mod tests;

pub const FILE_NAME: &str = "config.toml";

/// Raw contents of `config.toml`. Unknown keys are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct FileConfig {
    pub log_level: Option<LogLevel>,
    pub log_dir: Option<PathBuf>,
    pub log_keep: Option<usize>,
    pub stop_timeout_secs: Option<u64>,
    pub prefix_root: Option<PathBuf>,
    pub umu_runtime_update: Option<bool>,
    pub battleye_runtime: Option<PathBuf>,
    pub eac_runtime: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    /// The name used in `config.toml` and `--log-level`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },

    #[error("invalid {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

/// Loads `config.toml` from `config_dir`; a missing file is `Ok(None)`.
pub fn load(config_dir: &Path) -> Result<Option<FileConfig>, ConfigError> {
    let path = config_dir.join(FILE_NAME);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(ConfigError::Io { path, source }),
    };

    parse(&content)
        .map(Some)
        .map_err(|source| ConfigError::Parse { path, source })
}

/// Parses `config.toml` contents.
pub fn parse(content: &str) -> Result<FileConfig, toml::de::Error> {
    toml::from_str(content)
}
