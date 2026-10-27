//! Runner-specific parts of a launch: the runner binary, its arguments and env.

use std::{collections::BTreeMap, ffi::OsString, path::Path, path::PathBuf};

use crate::{
    launch::LaunchContext,
    model::{Profile, Runner},
};

mod native;
#[cfg(feature = "umu")]
pub mod umu;
#[cfg(feature = "wine")]
pub mod wine;

#[cfg(test)]
mod tests;

/// What a runner contributes to a launch.
#[derive(Debug, Default)]
pub struct RunnerCommand {
    /// Runner binary placed before the executable; `None` runs it directly.
    pub program: Option<PathBuf>,
    pub args: Vec<OsString>,
    pub env: BTreeMap<String, OsString>,
}

/// `game_id` is the game's own umu `GAMEID`, if any.
pub fn command(
    profile: &Profile,
    prefix: Option<&Path>,
    game_id: Option<&str>,
    debug: bool,
    ctx: &LaunchContext,
) -> Result<RunnerCommand, Vec<String>> {
    let command: Result<RunnerCommand, _> = match profile.runner {
        Runner::Native => return Ok(native::command()),
        #[cfg(feature = "wine")]
        Runner::Wine => wine::command(profile, prefix, debug, ctx),
        #[cfg(feature = "umu")]
        Runner::Umu => umu::command(profile, prefix, game_id, debug, ctx),
        #[allow(unreachable_patterns)]
        runner => {
            let _ = (prefix, game_id, debug);
            Err(vec![format!(
                "runner `{runner}` is not available: lootrs was built without the `{runner}` feature"
            )])
        }
    };
    match (command, anticheat_env(profile, ctx)) {
        (Ok(mut command), Ok(env)) => {
            command.env.extend(env);
            Ok(command)
        }
        (Ok(_), Err(problems)) => Err(problems),
        (Err(mut problems), anticheat) => {
            problems.extend(anticheat.err().into_iter().flatten());
            Err(problems)
        }
    }
}

/// `PROTON_BATTLEYE_RUNTIME`/`PROTON_EAC_RUNTIME` for the enabled anti-cheat runtimes.
fn anticheat_env(
    profile: &Profile,
    ctx: &LaunchContext,
) -> Result<BTreeMap<String, OsString>, Vec<String>> {
    let mut env = BTreeMap::new();
    let mut problems = Vec::new();
    for (enabled, runtime, var, key) in [
        (
            profile.anticheat.battleye,
            ctx.battleye_runtime,
            "PROTON_BATTLEYE_RUNTIME",
            "battleye_runtime",
        ),
        (
            profile.anticheat.eac,
            ctx.eac_runtime,
            "PROTON_EAC_RUNTIME",
            "eac_runtime",
        ),
    ] {
        if !enabled {
            continue;
        }
        match runtime {
            Some(dir) if dir.is_dir() => {
                env.insert(var.to_owned(), dir.into());
            }
            Some(dir) => problems.push(format!("`{key}` directory not found: {}", dir.display())),
            None => problems.push(format!("`{key}` is not set in config.toml")),
        }
    }
    if problems.is_empty() {
        Ok(env)
    } else {
        Err(problems)
    }
}
