//! Wine runner: `wine <exe>` with `WINEPREFIX` and friends.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use super::RunnerCommand;
use crate::{
    launch::{self, DLL_OVERRIDES_VAR, LaunchContext},
    model::Profile,
    paths, prefix,
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
    let wine = resolve_wine(profile, ctx.home, ctx.search_path);
    let Some(prefix) = prefix else {
        let mut problems = vec!["`prefix` is required for runner `wine`".to_owned()];
        problems.extend(wine.err());
        return Err(problems);
    };
    let wine = wine.map_err(|problem| vec![problem])?;

    let mut env = base_env(prefix, debug);
    env.insert("DXVK_LOG_PATH".into(), ctx.artifacts_dir.into());
    env.insert(
        "VKD3D_LOG_FILE".into(),
        ctx.artifacts_dir.join("vkd3d.log").into(),
    );
    if debug {
        env.insert("DXVK_LOG_LEVEL".into(), "info".into());
        env.insert("VKD3D_DEBUG".into(), "warn".into());
    }

    let overrides = dll_overrides(profile, ctx.home);
    if !overrides.is_empty() {
        env.insert(
            DLL_OVERRIDES_VAR.into(),
            launch::format_dll_overrides(&overrides).into(),
        );
    }

    Ok(RunnerCommand {
        program: Some(wine),
        args: profile.runner_args.iter().map(OsString::from).collect(),
        env,
    })
}

/// The wine binary: `[wine].path` (a binary or a build dir with `bin/wine`), or `wine` from `PATH`.
pub fn resolve_wine(
    profile: &Profile,
    home: &Path,
    search_path: &[PathBuf],
) -> Result<PathBuf, String> {
    match profile.wine.as_ref().and_then(|wine| wine.path.as_deref()) {
        Some(path) => {
            let path = paths::expand_tilde(path, home);
            let binary = if path.is_dir() {
                path.join("bin/wine")
            } else {
                path
            };
            if launch::is_executable(&binary) {
                Ok(binary)
            } else {
                Err(format!("wine not found at {}", binary.display()))
            }
        }
        None => launch::find_program(Path::new("wine"), search_path)
            .ok_or_else(|| "`wine` not found in PATH; install wine or set `[wine].path`".into()),
    }
}

/// `wineserver` next to `wine`, falling back to `PATH`.
pub fn resolve_wineserver(wine: &Path, search_path: &[PathBuf]) -> Option<PathBuf> {
    let sibling = wine.with_file_name("wineserver");
    if launch::is_executable(&sibling) {
        Some(sibling)
    } else {
        launch::find_program(Path::new("wineserver"), search_path)
    }
}

/// Environment every wine command in this prefix gets.
pub fn base_env(prefix: &Path, debug: bool) -> BTreeMap<String, OsString> {
    BTreeMap::from([
        ("WINEPREFIX".into(), prefix.into()),
        ("WINEARCH".into(), "win64".into()),
        (
            "WINEDEBUG".into(),
            if debug {
                DEBUG_WINEDEBUG
            } else {
                DEFAULT_WINEDEBUG
            }
            .into(),
        ),
    ])
}

/// `n,b` for every DLL shipped by the profile's DXVK/VKD3D, then the profile's own overrides.
fn dll_overrides(profile: &Profile, home: &Path) -> BTreeMap<String, String> {
    let mut overrides = BTreeMap::new();
    for source in [&profile.dxvk, &profile.vkd3d].into_iter().flatten() {
        let source = paths::expand_tilde(source, home);
        for dll in prefix::dll_names(&source) {
            overrides.insert(dll, "n,b".to_owned());
        }
    }
    overrides.extend(profile.dll_overrides.clone());
    overrides
}
