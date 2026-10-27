//! Command handlers. Each writes its output to `out` so tests can capture it.

use std::{
    collections::BTreeSet,
    io::{self, BufRead, Write},
};

use liblootrs::{
    model::{Issue, Severity, Subject, ValidationContext},
    process::{self, Outcome, Owner, Process},
    runlog::{self, RunKind},
    store::{self, Catalog},
};

use super::AppError;
use crate::{
    settings::{CheckTarget, Settings},
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

/// How many process names `status` shows per owner.
const STATUS_COMMANDS: usize = 4;

/// `lootrs status`: running games and sessions.
pub fn status(processes: &[Process], out: &mut impl Write) -> Result<(), AppError> {
    let running = process::running(processes);
    if running.is_empty() {
        writeln!(out, "nothing running")?;
        return Ok(());
    }

    let header = ["NAME", "PROCS", "COMMANDS"].map(String::from).to_vec();
    let rows = running.iter().map(|(owner, group)| {
        let mut commands: Vec<&str> = Vec::new();
        for process in group.iter().filter(|p| !p.is_wine_infrastructure()) {
            if !commands.contains(&process.comm.as_str()) {
                commands.push(&process.comm);
            }
        }
        let mut shown = commands
            .iter()
            .take(STATUS_COMMANDS)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        if commands.len() > STATUS_COMMANDS {
            shown.push_str(", …");
        }
        vec![owner.to_string(), group.len().to_string(), shown]
    });

    write!(out, "{}", table(std::iter::once(header).chain(rows)))?;
    Ok(())
}

/// `lootrs stop`: terminates a game's processes.
pub fn stop(settings: &Settings, game: &str, out: &mut impl Write) -> Result<(), AppError> {
    let owner = Owner::Game(game.to_owned());
    if !process::is_running(&process::scan(), &owner) {
        return Err(AppError::NotRunning(game.to_owned()));
    }

    let outcome = process::terminate(
        process::scan,
        |processes| process::stop_targets(processes, &owner),
        settings.stop_timeout,
    );
    tracing::info!(?outcome, "stopped `{game}`");
    writeln!(out, "stopped `{game}` ({})", describe(outcome))?;
    Ok(())
}

/// `lootrs killall`: everything lootrs started; with `everything`, any
/// wine/Proton process of this user too (asks first unless `yes`).
///
/// `kill` performs the termination, so the selection and prompt are testable
/// without killing anything.
pub fn killall(
    processes: &[Process],
    everything: bool,
    yes: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
    kill: impl FnOnce() -> Outcome,
) -> Result<(), AppError> {
    let targets = process::killall_targets(processes, everything);
    if targets.is_empty() {
        writeln!(out, "nothing to kill")?;
        return Ok(());
    }

    if everything && !yes {
        let names: BTreeSet<&str> = processes
            .iter()
            .filter(|p| targets.contains(&p.pid))
            .map(|p| p.comm.as_str())
            .collect();
        writeln!(
            out,
            "This kills {} including processes lootrs did not start:\n  {}",
            plural(targets.len(), "process"),
            names.into_iter().collect::<Vec<_>>().join(", ")
        )?;
        write!(out, "Continue? [y/N] ")?;
        out.flush()?;
        let mut answer = String::new();
        input.read_line(&mut answer)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            return Err(AppError::Cancelled);
        }
    }

    let outcome = kill();
    tracing::info!(?outcome, everything, "killall");
    writeln!(out, "stopped {}", describe(outcome))?;
    Ok(())
}

fn describe(outcome: Outcome) -> String {
    let mut text = plural(outcome.signaled, "process");
    if outcome.killed > 0 {
        text += &format!(", {} force-killed", outcome.killed);
    }
    text
}

/// `lootrs logs`: the latest run log of a game, or its path.
pub fn logs(
    settings: &Settings,
    game: &str,
    path_only: bool,
    out: &mut impl Write,
) -> Result<(), AppError> {
    let path = runlog::latest(&settings.logging.dir, RunKind::Game(game))
        .ok_or_else(|| AppError::NoLogs(game.to_owned()))?;
    if path_only {
        writeln!(out, "{}", path.display())?;
    } else {
        let mut file = std::fs::File::open(&path)?;
        io::copy(&mut file, out)?;
    }
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
