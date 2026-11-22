//! Reads and writes `games.toml` and `profiles/*.toml` in the config directory.
//!
//! Loading is forgiving: a broken file is reported in [`Catalog::load_errors`]
//! and everything else still loads. Writes are atomic (temp file + rename).
//! Profiles are loaded and written by `libgameprofiles::store`.

use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use libgameprofiles::store as profiles;

use crate::model::{self, Game, Issue, Profile, Severity, Subject, ValidationContext};

#[cfg(test)]
mod tests;

pub const GAMES_FILE: &str = "games.toml";
pub const PROFILES_DIR: &str = "profiles";

/// Why a file couldn't be loaded or written: libgameprofiles' store error,
/// used for `games.toml` too.
pub use libgameprofiles::store::Error as StoreError;

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
    /// Keys in the profile files that the format doesn't know.
    pub load_warnings: Vec<Issue>,
}

impl Catalog {
    /// Loads `games.toml` and every `profiles/*.toml`; missing files mean empty lists.
    pub fn load(config_dir: &Path) -> Self {
        let mut catalog = Self::default();

        match load_games(config_dir) {
            Ok(games) => catalog.games = games,
            Err(err) => catalog.load_errors.push(err),
        }

        let loaded = profiles::load_dir(&profiles_dir(config_dir));
        catalog.profiles = loaded.profiles;
        catalog.load_errors.extend(loaded.errors);
        catalog.load_warnings = loaded.warnings;

        catalog
    }

    pub fn game(&self, name: &str) -> Option<&Game> {
        self.games.iter().find(|game| game.name == name)
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.name == name)
    }

    /// Files that failed to load (errors) and unknown keys (warnings).
    pub fn load_issues(&self) -> impl Iterator<Item = Issue> + '_ {
        self.load_errors
            .iter()
            .map(StoreError::to_issue)
            .chain(self.load_warnings.iter().cloned())
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
            issues.extend(model::validate_profile(profile, ctx));
        }

        issues
    }

    /// Validates one game and its profile; `None` if there is no such game.
    pub fn validate_game(&self, name: &str, ctx: &ValidationContext) -> Option<Vec<Issue>> {
        let game = self.game(name)?;
        let profile = self.profile(&game.profile);
        let mut issues = game.validate(profile, ctx);
        if let Some(profile) = profile {
            issues.extend(model::validate_profile(profile, ctx));
        }
        Some(issues)
    }

    /// Validates one profile; `None` if there is no such profile.
    pub fn validate_profile(&self, name: &str, ctx: &ValidationContext) -> Option<Vec<Issue>> {
        self.profile(name)
            .map(|profile| model::validate_profile(profile, ctx))
    }
}

pub fn games_path(config_dir: &Path) -> PathBuf {
    config_dir.join(GAMES_FILE)
}

pub fn profiles_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(PROFILES_DIR)
}

pub fn profile_path(config_dir: &Path, name: &str) -> PathBuf {
    profiles::path(&profiles_dir(config_dir), name)
}

/// Loads `games.toml`; a missing file is an empty list.
pub fn load_games(config_dir: &Path) -> Result<Vec<Game>, StoreError> {
    let path = games_path(config_dir);
    let Some(content) = read_optional(&path)? else {
        return Ok(Vec::new());
    };
    toml::from_str::<GamesFile>(&content)
        .map(|file| file.games)
        .map_err(|source| StoreError::Parse {
            path,
            source: Box::new(source),
        })
}

/// Loads one profile file and checks that its `name` matches the file stem.
pub fn load_profile(path: &Path) -> Result<Profile, StoreError> {
    profiles::load(path).map(|(profile, _)| profile)
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
    profiles::save(&profiles_dir(config_dir), profile).map(|_| ())
}

/// Deletes `profiles/<name>.toml`. The prefix directory is never touched.
pub fn remove_profile(config_dir: &Path, name: &str) -> Result<(), StoreError> {
    profiles::remove(&profiles_dir(config_dir), name)
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

fn write_atomic(path: &Path, content: &str) -> Result<(), StoreError> {
    profiles::write_atomic(path, content)
}
