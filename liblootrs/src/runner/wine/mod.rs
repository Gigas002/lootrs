//! Wine runner: `wine <exe>` with `WINEPREFIX` and friends.

use std::{collections::BTreeMap, ffi::OsString, path::Path};

use super::RunnerCommand;
use crate::{
    launch::{self, DLL_OVERRIDES_VAR, LaunchContext},
    model::Profile,
    paths,
};

#[cfg(test)]
mod tests;

/// Keeps wine's error messages in run logs while dropping `fixme` noise.
pub const DEFAULT_WINEDEBUG: &str = "fixme-all";
pub const DEBUG_WINEDEBUG: &str = "+err,+warn,+loaddll,+seh";

pub fn command(
    profile: &Profile,
    prefix: Option<&Path>,
    debug: bool,
    ctx: &LaunchContext,
) -> Result<RunnerCommand, Vec<String>> {
    let mut problems = Vec::new();

    let wine = match profile.wine.as_ref().and_then(|wine| wine.path.as_deref()) {
        Some(path) => {
            let path = paths::expand_tilde(path, ctx.home);
            let binary = if path.is_dir() {
                path.join("bin/wine")
            } else {
                path
            };
            launch::is_executable(&binary)
                .then_some(binary.clone())
                .or_else(|| {
                    problems.push(format!("wine not found at {}", binary.display()));
                    None
                })
        }
        None => launch::find_program(Path::new("wine"), ctx.search_path).or_else(|| {
            problems.push("`wine` not found in PATH; install wine or set `[wine].path`".into());
            None
        }),
    };

    let Some(prefix) = prefix else {
        problems.push("`prefix` is required for runner `wine`".into());
        return Err(problems);
    };
    let Some(wine) = wine else {
        return Err(problems);
    };

    let mut env: BTreeMap<String, OsString> = BTreeMap::new();
    env.insert("WINEPREFIX".into(), prefix.into());
    env.insert("WINEARCH".into(), "win64".into());
    env.insert(
        "WINEDEBUG".into(),
        if debug {
            DEBUG_WINEDEBUG
        } else {
            DEFAULT_WINEDEBUG
        }
        .into(),
    );
    env.insert("DXVK_LOG_PATH".into(), ctx.artifacts_dir.into());
    env.insert(
        "VKD3D_LOG_FILE".into(),
        ctx.artifacts_dir.join("vkd3d.log").into(),
    );
    if debug {
        env.insert("DXVK_LOG_LEVEL".into(), "info".into());
        env.insert("VKD3D_DEBUG".into(), "warn".into());
    }
    if !profile.dll_overrides.is_empty() {
        env.insert(
            DLL_OVERRIDES_VAR.into(),
            launch::format_dll_overrides(&profile.dll_overrides).into(),
        );
    }

    Ok(RunnerCommand {
        program: Some(wine),
        args: profile.runner_args.iter().map(OsString::from).collect(),
        env,
    })
}
