//! Command handlers. Each writes its output to `out` so tests can capture it.

use std::io::Write;

use liblootrs::{
    model::{Issue, Severity, Subject, ValidationContext},
    store::{self, Catalog},
};

use super::AppError;
use crate::{
    settings::CheckTarget,
    utils::{plural, table},
};

#[cfg(test)]
mod tests;

/// `lootrs list`: games with their profile and runner.
pub fn list(catalog: &Catalog, out: &mut impl Write) -> Result<(), AppError> {
    if catalog.games.is_empty() {
        writeln!(out, "no games")?;
        return Ok(());
    }

    let header = ["NAME", "PROFILE", "RUNNER", "EXE"]
        .map(String::from)
        .to_vec();
    let rows = catalog.games.iter().map(|game| {
        let runner = catalog
            .profile(&game.profile)
            .map_or("?", |profile| profile.runner.as_str());
        vec![
            game.name.clone(),
            game.profile.clone(),
            runner.to_owned(),
            game.exe.display().to_string(),
        ]
    });

    write!(out, "{}", table(std::iter::once(header).chain(rows)))?;
    Ok(())
}

/// `lootrs profiles`: profiles with runner, prefix and how many games use them.
pub fn profiles(catalog: &Catalog, out: &mut impl Write) -> Result<(), AppError> {
    if catalog.profiles.is_empty() {
        writeln!(out, "no profiles")?;
        return Ok(());
    }

    let header = ["NAME", "RUNNER", "GAMES", "PREFIX"]
        .map(String::from)
        .to_vec();
    let rows = catalog.profiles.iter().map(|profile| {
        let games = catalog
            .games
            .iter()
            .filter(|game| game.profile == profile.name)
            .count();
        vec![
            profile.name.clone(),
            profile.runner.to_string(),
            games.to_string(),
            profile
                .prefix
                .as_deref()
                .map_or("-".into(), |prefix| prefix.display().to_string()),
        ]
    });

    write!(out, "{}", table(std::iter::once(header).chain(rows)))?;
    Ok(())
}

/// `lootrs check`: prints every issue; fails if any is an error.
pub fn check(
    catalog: &Catalog,
    ctx: &ValidationContext,
    target: &CheckTarget,
    out: &mut impl Write,
) -> Result<(), AppError> {
    let (issues, checked) = match target {
        CheckTarget::All => (
            catalog.validate(ctx),
            format!(
                "{} and {}",
                plural(catalog.games.len(), "game"),
                plural(catalog.profiles.len(), "profile")
            ),
        ),
        CheckTarget::Game(name) => (
            catalog
                .validate_game(name, ctx)
                .ok_or_else(|| AppError::UnknownGame(name.clone()))?,
            format!("game `{name}`"),
        ),
        CheckTarget::Profile(name) => (
            catalog
                .validate_profile(name, ctx)
                .ok_or_else(|| AppError::UnknownProfile(name.clone()))?,
            format!("profile `{name}`"),
        ),
    };

    // A game whose profile file failed to load shows "does not exist"; add the reason.
    let issues = match target {
        CheckTarget::Game(name) => {
            let profile = catalog.game(name).map(|game| game.profile.as_str());
            broken_profile_file(catalog, profile)
                .chain(issues)
                .collect()
        }
        _ => issues,
    };

    report(&issues, &checked, out)
}

/// Load errors for `profiles/<name>.toml` when profile `name` is missing.
fn broken_profile_file<'a>(
    catalog: &'a Catalog,
    profile: Option<&'a str>,
) -> impl Iterator<Item = Issue> + 'a {
    let missing = profile.filter(|name| catalog.profile(name).is_none());
    catalog
        .load_issues()
        .filter(move |issue| match (&issue.subject, missing) {
            (Subject::File(path), Some(name)) => {
                path.file_stem().is_some_and(|stem| stem == name)
                    && path
                        .parent()
                        .is_some_and(|dir| dir.ends_with(store::PROFILES_DIR))
            }
            _ => false,
        })
}

fn report(issues: &[Issue], checked: &str, out: &mut impl Write) -> Result<(), AppError> {
    for issue in issues {
        writeln!(out, "{issue}")?;
    }

    let errors = issues
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .count();
    let warnings = issues.len() - errors;
    let summary = match (errors, warnings) {
        (0, 0) => "no problems".to_owned(),
        _ => format!(
            "{}, {}",
            plural(errors, "error"),
            plural(warnings, "warning")
        ),
    };
    writeln!(out, "checked {checked}: {summary}")?;

    if errors > 0 {
        Err(AppError::CheckFailed(plural(errors, "error")))
    } else {
        Ok(())
    }
}
