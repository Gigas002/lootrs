//! Games and profiles as stored in `games.toml` and `profiles/*.toml`, plus validation.
//!
//! Validation never fails fast: it returns every [`Issue`] it finds, so `check`
//! and the TUI can show them all at once.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

mod game;
mod profile;

pub use game::Game;
pub use profile::{Anticheat, Profile, Runner, UmuSection, WineSection};

use crate::paths;

#[cfg(test)]
mod tests;

/// Values from the global settings that validation depends on.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    /// Used to expand `~` in paths.
    pub home: &'a Path,
    /// Whether `battleye_runtime` is set in `config.toml`.
    pub battleye_runtime: bool,
    /// Whether `eac_runtime` is set in `config.toml`.
    pub eac_runtime: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// What an [`Issue`] is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subject {
    Game(String),
    Profile(String),
    /// A file that could not be loaded at all.
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub subject: Subject,
    pub message: String,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game(name) => write!(f, "game `{name}`"),
            Self::Profile(name) => write!(f, "profile `{name}`"),
            Self::File(path) => write!(f, "{}", path.display()),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}: {}", self.severity, self.subject, self.message)
    }
}

/// Checks a game or profile name: `[A-Za-z0-9._-]+`, not `.`/`..`, not starting with `-`.
///
/// Names are used as CLI arguments, file names and process tags.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name must not be empty".into());
    }
    if name == "." || name == ".." || name.starts_with('-') {
        return Err(format!("name `{name}` is not allowed"));
    }
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
    {
        return Err(format!(
            "name `{name}` contains `{c}`; use only letters, digits, `.`, `_` and `-`"
        ));
    }
    Ok(())
}

/// Collects issues for one subject.
struct Issues {
    subject: Subject,
    list: Vec<Issue>,
}

impl Issues {
    fn new(subject: Subject) -> Self {
        Self {
            subject,
            list: Vec::new(),
        }
    }

    fn push(&mut self, severity: Severity, message: impl Into<String>) {
        self.list.push(Issue {
            severity,
            subject: self.subject.clone(),
            message: message.into(),
        });
    }

    fn error(&mut self, message: impl Into<String>) {
        self.push(Severity::Error, message);
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.push(Severity::Warning, message);
    }

    fn name(&mut self, name: &str) {
        if let Err(message) = validate_name(name) {
            self.error(message);
        }
    }

    /// Errors unless `path` is absolute after `~` expansion; returns the expanded path.
    fn absolute(&mut self, field: &str, path: &Path, home: &Path) -> Option<PathBuf> {
        let expanded = paths::expand_tilde(path, home);
        if expanded.is_absolute() {
            Some(expanded)
        } else {
            self.error(format!(
                "`{field}` must be an absolute path (or start with `~`), got `{}`",
                path.display()
            ));
            None
        }
    }

    fn env(&mut self, env: &BTreeMap<String, String>) {
        for key in env.keys() {
            if key.is_empty() || key.contains(['=', '\0']) {
                self.error(format!("`env` key `{key}` is not a valid variable name"));
            }
        }
    }

    fn finish(self) -> Vec<Issue> {
        self.list
    }
}
