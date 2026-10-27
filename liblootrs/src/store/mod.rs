//! Reads and writes `games.toml` and `profiles/*.toml` in the config directory.
//!
//! Loading is forgiving: a broken file is reported in [`Catalog::load_errors`]
//! and everything else still loads. Writes are atomic (temp file + rename).

use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::model::{self, Game, Issue, Profile, Severity, Subject, ValidationContext};

#[cfg(test)]
mod tests;

pub const GAMES_FILE: &str = "games.toml";
pub const PROFILES_DIR: &str = "profiles";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },

    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },

    #[error("invalid {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("failed to serialize {}: {source}", path.display())]
    Serialize {
        path: PathBuf,
        source: toml::ser::Error,
    },

    #[error("{}: `name = \"{name}\"` must match the file name `{stem}`", path.display())]
    NameMismatch {
        path: PathBuf,
        name: String,
        stem: String,
    },

    #[error("invalid profile name: {0}")]
    InvalidName(String),
}

impl StoreError {
    /// The file this error is about, if any.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Read { path, .. }
            | Self::Write { path, .. }
            | Self::Parse { path, .. }
            | Self::Serialize { path, .. }
            | Self::NameMismatch { path, .. } => Some(path),
            Self::InvalidName(_) => None,
        }
    }

    /// The error without the file path, for when the path is shown separately.
    pub fn detail(&self) -> String {
        match self {
            Self::Read { source, .. } => format!("failed to read: {source}"),
            Self::Write { source, .. } => format!("failed to write: {source}"),
            Self::Parse { source, .. } => format!("invalid TOML: {source}"),
            Self::Serialize { source, .. } => format!("failed to serialize: {source}"),
            Self::NameMismatch { name, stem, .. } => {
                format!("`name = \"{name}\"` must match the file name `{stem}`")
            }
            Self::InvalidName(message) => message.clone(),
        }
    }
}

/// On-disk shape of `games.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct GamesFile {
    #[serde(default, rename = "game")]
    games: Vec<Game>,
}

/// Everything loaded from a config directory.
#[derive(Debug, Default)]
pub struct Catalog {
    pub games: Vec<Game>,
    /// Sorted by name.
    pub profiles: Vec<Profile>,
    /// Files that could not be loaded; their contents are missing from the lists.
    pub load_errors: Vec<StoreError>,
}

impl Catalog {
    /// Loads `games.toml` and every `profiles/*.toml`; missing files mean empty lists.
    pub fn load(config_dir: &Path) -> Self {
        let mut catalog = Self::default();

        match load_games(config_dir) {
            Ok(games) => catalog.games = games,
            Err(err) => catalog.load_errors.push(err),
        }

        match profile_paths(config_dir) {
            Ok(paths) => {
                for path in paths {
                    match load_profile(&path) {
                        Ok(profile) => catalog.profiles.push(profile),
                        Err(err) => catalog.load_errors.push(err),
                    }
                }
            }
            Err(err) => catalog.load_errors.push(err),
        }

        catalog.profiles.sort_by(|a, b| a.name.cmp(&b.name));
        catalog
    }

    pub fn game(&self, name: &str) -> Option<&Game> {
        self.games.iter().find(|game| game.name == name)
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    /// Files that failed to load, as error issues.
    pub fn load_issues(&self) -> impl Iterator<Item = Issue> + '_ {
        self.load_errors.iter().map(|err| Issue {
            severity: Severity::Error,
            subject: Subject::File(err.path().map(Path::to_path_buf).unwrap_or_default()),
            message: err.detail(),
        })
    }

    /// Validates everything, including load errors and duplicate game names.
    pub fn validate(&self, ctx: &ValidationContext) -> Vec<Issue> {
        let mut issues: Vec<Issue> = self.load_issues().collect();

        let mut seen = HashSet::new();
        for game in &self.games {
            if !seen.insert(game.name.as_str()) {
                issues.push(Issue {
                    severity: Severity::Error,
                    subject: Subject::Game(game.name.clone()),
                    message: "duplicate game name".into(),
                });
            }
            issues.extend(game.validate(self.profile(&game.profile), ctx));
        }

        for profile in &self.profiles {
            issues.extend(profile.validate(ctx));
        }

        issues
    }

    /// Validates one game and its profile; `None` if there is no such game.
    pub fn validate_game(&self, name: &str, ctx: &ValidationContext) -> Option<Vec<Issue>> {
        let game = self.game(name)?;
        let profile = self.profile(&game.profile);
        let mut issues = game.validate(profile, ctx);
        if let Some(profile) = profile {
            issues.extend(profile.validate(ctx));
        }
        Some(issues)
    }

    /// Validates one profile; `None` if there is no such profile.
    pub fn validate_profile(&self, name: &str, ctx: &ValidationContext) -> Option<Vec<Issue>> {
        self.profile(name).map(|profile| profile.validate(ctx))
    }
}

pub fn games_path(config_dir: &Path) -> PathBuf {
    config_dir.join(GAMES_FILE)
}

pub fn profiles_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(PROFILES_DIR)
}

pub fn profile_path(config_dir: &Path, name: &str) -> PathBuf {
    profiles_dir(config_dir).join(format!("{name}.toml"))
}

/// Loads `games.toml`; a missing file is an empty list.
pub fn load_games(config_dir: &Path) -> Result<Vec<Game>, StoreError> {
    let path = games_path(config_dir);
    let Some(content) = read_optional(&path)? else {
        return Ok(Vec::new());
    };
    toml::from_str::<GamesFile>(&content)
        .map(|file| file.games)
        .map_err(|source| StoreError::Parse { path, source })
}

/// Loads one profile file and checks that its `name` matches the file stem.
pub fn load_profile(path: &Path) -> Result<Profile, StoreError> {
    let content = std::fs::read_to_string(path).map_err(|source| StoreError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let profile: Profile = toml::from_str(&content).map_err(|source| StoreError::Parse {
        path: path.to_path_buf(),
        source,
    })?;

    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    if profile.name != stem {
        return Err(StoreError::NameMismatch {
            path: path.to_path_buf(),
            name: profile.name,
            stem: stem.into_owned(),
        });
    }
    Ok(profile)
}

/// Writes `games.toml`.
pub fn save_games(config_dir: &Path, games: &[Game]) -> Result<(), StoreError> {
    let path = games_path(config_dir);
    let file = GamesFile {
        games: games.to_vec(),
    };
    let content = toml::to_string_pretty(&file).map_err(|source| StoreError::Serialize {
        path: path.clone(),
        source,
    })?;
    write_atomic(&path, &content)
}

/// Writes `profiles/<name>.toml`.
pub fn save_profile(config_dir: &Path, profile: &Profile) -> Result<(), StoreError> {
    model::validate_name(&profile.name).map_err(StoreError::InvalidName)?;
    let path = profile_path(config_dir, &profile.name);
    let content = toml::to_string_pretty(profile).map_err(|source| StoreError::Serialize {
        path: path.clone(),
        source,
    })?;
    write_atomic(&path, &content)
}

/// Deletes `profiles/<name>.toml`. The prefix directory is never touched.
pub fn remove_profile(config_dir: &Path, name: &str) -> Result<(), StoreError> {
    model::validate_name(name).map_err(StoreError::InvalidName)?;
    let path = profile_path(config_dir, name);
    std::fs::remove_file(&path).map_err(|source| StoreError::Write { path, source })
}

/// `profiles/*.toml`, sorted; a missing directory is empty.
fn profile_paths(config_dir: &Path) -> Result<Vec<PathBuf>, StoreError> {
    let dir = profiles_dir(config_dir);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(StoreError::Read { path: dir, source }),
    };

    let mut paths = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| StoreError::Read {
                path: dir.clone(),
                source,
            })?
            .path();
        if path.extension().is_some_and(|ext| ext == "toml") && path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn read_optional(path: &Path) -> Result<Option<String>, StoreError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(StoreError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Writes to `<file>.tmp` next to `path`, then renames over it.
fn write_atomic(path: &Path, content: &str) -> Result<(), StoreError> {
    let write_err = |source| StoreError::Write {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(write_err)?;
    }

    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    std::fs::write(&tmp, content).map_err(write_err)?;
    std::fs::rename(&tmp, path).map_err(write_err)
}
