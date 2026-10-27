use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use super::{Issue, Issues, Profile, Runner, Subject, ValidationContext};

/// One `[[game]]` entry in `games.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    /// Unique id, used by `lootrs run <name>`.
    pub name: String,
    pub exe: PathBuf,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Working directory; defaults to the executable's directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    /// umu only: `GAMEID` for protonfixes, overrides the profile's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    /// Merged over the profile's env.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl Game {
    /// Validates this game; `profile` is the profile it references, if it exists.
    ///
    /// A missing `exe` or `cwd` is only a warning, so games on unmounted drives still load.
    pub fn validate(&self, profile: Option<&Profile>, ctx: &ValidationContext) -> Vec<Issue> {
        let mut issues = Issues::new(Subject::Game(self.name.clone()));
        issues.name(&self.name);

        if profile.is_none() {
            issues.error(format!("profile `{}` does not exist", self.profile));
        }

        if let Some(exe) = issues.absolute("exe", &self.exe, ctx.home)
            && !exe.is_file()
        {
            issues.warn(format!("`exe` not found: {}", exe.display()));
        }

        if let Some(cwd) = &self.cwd
            && let Some(cwd) = issues.absolute("cwd", cwd, ctx.home)
            && !cwd.is_dir()
        {
            issues.warn(format!("`cwd` not found: {}", cwd.display()));
        }

        if self.game_id.is_some()
            && let Some(profile) = profile
            && profile.runner != Runner::Umu
        {
            issues.warn(format!(
                "`game_id` is ignored: profile `{}` uses runner `{}`",
                profile.name, profile.runner
            ));
        }

        issues.env(&self.env);
        issues.finish()
    }
}
