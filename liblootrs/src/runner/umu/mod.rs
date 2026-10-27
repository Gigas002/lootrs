//! umu runner: `umu-run <exe>` with `PROTONPATH` set to a user-installed Proton.
//!
//! `PROTONPATH` is always an absolute directory: unset or set to a token like
//! `GE-Proton`, umu downloads Proton, which lootrs never lets happen.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use super::RunnerCommand;
use crate::{
    launch::{self, DLL_OVERRIDES_VAR, LaunchContext},
    model::Profile,
    paths,
};

#[cfg(test)]
mod tests;

/// umu's own fallback when no `GAMEID` is set.
pub const DEFAULT_GAME_ID: &str = "umu-default";

pub fn command(
    profile: &Profile,
    prefix: Option<&Path>,
    game_id: Option<&str>,
    debug: bool,
    ctx: &LaunchContext,
) -> Result<RunnerCommand, Vec<String>> {
    let mut problems = Vec::new();
    if prefix.is_none() {
        problems.push("`prefix` is required for runner `umu`".to_owned());
    }
    let umu_run = resolve_umu_run(profile, ctx.home, ctx.search_path)
        .map_err(|problem| problems.push(problem))
        .ok();
    let proton = resolve_proton(profile, ctx.home)
        .map_err(|problem| problems.push(problem))
        .ok();
    let (Some(prefix), Some(umu_run), Some(proton)) = (prefix, umu_run, proton) else {
        return Err(problems);
    };

    let mut env = base_env(profile, prefix, &proton, game_id, ctx.umu_runtime_update);
    // Not `UMU_LOG`: at debug level umu dumps the whole inherited environment
    // (tokens included) into the output, and run logs are meant to be shared.
    if debug {
        env.insert("PROTON_LOG".into(), "1".into());
        env.insert("PROTON_LOG_DIR".into(), ctx.artifacts_dir.into());
    }
    // Proton manages DXVK/VKD3D itself; only the profile's own overrides apply.
    if !profile.dll_overrides.is_empty() {
        env.insert(
            DLL_OVERRIDES_VAR.into(),
            launch::format_dll_overrides(&profile.dll_overrides).into(),
        );
    }

    Ok(RunnerCommand {
        program: Some(umu_run),
        args: profile.runner_args.iter().map(OsString::from).collect(),
        env,
    })
}

/// `[umu].umu_run`, or `umu-run` from `PATH`.
pub fn resolve_umu_run(
    profile: &Profile,
    home: &Path,
    search_path: &[PathBuf],
) -> Result<PathBuf, String> {
    match profile.umu.as_ref().and_then(|umu| umu.umu_run.as_deref()) {
        Some(path) => {
            let path = paths::expand_tilde(path, home);
            if launch::is_executable(&path) {
                Ok(path)
            } else {
                Err(format!("umu-run not found at {}", path.display()))
            }
        }
        None => launch::find_program(Path::new("umu-run"), search_path).ok_or_else(|| {
            "`umu-run` not found in PATH; install umu-launcher or set `[umu].umu_run`".into()
        }),
    }
}

/// `[umu].proton`: an absolute Proton directory containing the `proton` script.
pub fn resolve_proton(profile: &Profile, home: &Path) -> Result<PathBuf, String> {
    let Some(proton) = profile.umu.as_ref().and_then(|umu| umu.proton.as_deref()) else {
        return Err("`[umu].proton` is required for runner `umu`".into());
    };
    let proton = paths::expand_tilde(proton, home);
    if !proton.is_absolute() {
        return Err(format!(
            "`[umu].proton` must be an absolute path: {}",
            proton.display()
        ));
    }
    if proton.join("proton").is_file() {
        Ok(proton)
    } else {
        Err(format!(
            "no Proton build at {} (expected a `proton` script there)",
            proton.display()
        ))
    }
}

/// Environment every umu command for this prefix gets. `game_id` (a game's own)
/// wins over the profile's `[umu].game_id`.
pub fn base_env(
    profile: &Profile,
    prefix: &Path,
    proton: &Path,
    game_id: Option<&str>,
    runtime_update: bool,
) -> BTreeMap<String, OsString> {
    let umu = profile.umu.as_ref();
    let game_id = game_id
        .or_else(|| umu.and_then(|umu| umu.game_id.as_deref()))
        .filter(|id| !id.is_empty())
        .unwrap_or(DEFAULT_GAME_ID);

    let mut env = BTreeMap::from([
        ("WINEPREFIX".into(), prefix.into()),
        ("PROTONPATH".into(), proton.into()),
        ("GAMEID".into(), game_id.into()),
    ]);
    if let Some(store) = umu
        .and_then(|umu| umu.store.as_deref())
        .filter(|store| !store.is_empty())
    {
        env.insert("STORE".into(), store.into());
    }
    if !runtime_update {
        env.insert("UMU_RUNTIME_UPDATE".into(), "0".into());
    }
    env
}
