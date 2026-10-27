//! Turns a game (or a one-off executable) plus its profile into a ready-to-spawn
//! [`LaunchSpec`]. Building is pure apart from filesystem lookups for preflight
//! checks; spawning happens in the binary.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fmt,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::{
    model::{Game, Profile, Runner},
    paths,
    runner::{self, RunnerCommand},
};

#[cfg(test)]
mod tests;

/// Set to the game name on every process lootrs starts for a game.
pub const TAG_GAME: &str = "LOOTRS_GAME";
/// Set on processes started by `execute`, `tricks` and `init`: `<kind>:<profile>:<stamp>`.
pub const TAG_SESSION: &str = "LOOTRS_SESSION";
/// Absolute prefix path, for wine and umu profiles.
pub const TAG_PREFIX: &str = "LOOTRS_PREFIX";
/// Merged key-wise instead of overwritten.
pub const DLL_OVERRIDES_VAR: &str = "WINEDLLOVERRIDES";

/// What to launch.
#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    Game(&'a Game),
    /// A one-off executable: a path, or a bare program name (`winecfg`, or a
    /// binary from `PATH` for native profiles).
    Execute {
        program: &'a Path,
    },
    /// `winetricks` in the profile's prefix; no verbs opens its GUI.
    Tricks {
        verbs: &'a [String],
    },
}

/// How the processes are tagged, see [`TAG_GAME`] and [`TAG_SESSION`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    Game(String),
    Session(String),
}

impl Tag {
    /// The environment variable carrying this tag.
    pub fn env(&self) -> (String, OsString) {
        match self {
            Self::Game(name) => (TAG_GAME.into(), name.into()),
            Self::Session(session) => (TAG_SESSION.into(), session.into()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LaunchContext<'a> {
    pub home: &'a Path,
    /// Directories from `PATH`, used to resolve runners and wrappers.
    pub search_path: &'a [PathBuf],
    /// Verbose runner logging (profile `debug` or `--debug`).
    pub debug: bool,
    /// Appended after the game's own arguments.
    pub extra_args: &'a [String],
    /// Per-run directory for runner-side logs (DXVK, VKD3D, Proton).
    pub artifacts_dir: &'a Path,
}

/// A fully resolved command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Variables set on top of the inherited environment.
    pub env: BTreeMap<String, OsString>,
    pub cwd: PathBuf,
    /// Runner binary, for version reporting (`None` for native).
    pub runner: Option<PathBuf>,
}

impl LaunchSpec {
    /// The full command line: program followed by arguments.
    pub fn argv(&self) -> impl Iterator<Item = &std::ffi::OsStr> {
        std::iter::once(self.program.as_os_str()).chain(self.args.iter().map(OsString::as_os_str))
    }
}

/// Every problem that prevents a launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchError {
    pub problems: Vec<String>,
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.problems.join("\n"))
    }
}

impl std::error::Error for LaunchError {}

/// Builds the command for `target` under `profile`, checking that everything it
/// needs exists. Collects all problems instead of stopping at the first.
pub fn prepare(
    target: Target,
    profile: &Profile,
    tag: &Tag,
    ctx: &LaunchContext,
) -> Result<LaunchSpec, LaunchError> {
    let mut problems = Vec::new();
    let home = ctx.home;

    let (exe, cwd) = match target {
        Target::Game(game) => {
            let exe = paths::expand_tilde(&game.exe, home);
            if !exe.is_file() {
                problems.push(format!("executable not found: {}", exe.display()));
            }
            let cwd = match &game.cwd {
                Some(cwd) => paths::expand_tilde(cwd, home),
                None => exe
                    .parent()
                    .map_or_else(|| home.to_path_buf(), Path::to_path_buf),
            };
            (exe, cwd)
        }
        Target::Execute { program } => {
            let exe = execute_program(program, profile.runner, ctx, &mut problems);
            let cwd = match exe.parent() {
                Some(dir) if exe.is_absolute() => dir.to_path_buf(),
                _ => home.to_path_buf(),
            };
            (exe, cwd)
        }
        Target::Tricks { .. } => {
            let winetricks = Path::new("winetricks");
            let exe = match profile.runner {
                Runner::Native => {
                    problems.push("`tricks` needs a wine or umu profile".into());
                    winetricks.to_path_buf()
                }
                _ => find_program(winetricks, ctx.search_path).unwrap_or_else(|| {
                    problems.push("`winetricks` not found in PATH".into());
                    winetricks.to_path_buf()
                }),
            };
            (exe, home.to_path_buf())
        }
    };
    if !cwd.is_dir() {
        problems.push(format!("working directory not found: {}", cwd.display()));
    }

    let debug = ctx.debug || profile.debug;
    let prefix = profile
        .prefix
        .as_deref()
        .filter(|_| profile.runner != Runner::Native)
        .map(|prefix| paths::expand_tilde(prefix, home));

    let command = match runner::command(profile, prefix.as_deref(), debug, ctx) {
        Ok(command) => Some(command),
        Err(mut runner_problems) => {
            problems.append(&mut runner_problems);
            None
        }
    };

    let tricks = matches!(target, Target::Tricks { .. });
    let wrappers = if tricks {
        Vec::new()
    } else {
        resolve_wrappers(&profile.wrappers, ctx.search_path, &mut problems)
    };

    let Some(RunnerCommand {
        program: runner_program,
        args: runner_args,
        env: runner_env,
    }) = command
    else {
        return Err(LaunchError { problems });
    };
    if !problems.is_empty() {
        return Err(LaunchError { problems });
    }

    // [runner args...] exe [game args...] [extra args...]
    // winetricks runs wine itself, found through `WINE`.
    let mut argv: Vec<OsString> = Vec::new();
    let runner_bin = runner_program.clone();
    let mut runner_env = runner_env;
    match (runner_program, tricks) {
        (Some(program), true) => {
            runner_env.insert("WINE".into(), program.into_os_string());
        }
        (Some(program), false) => {
            argv.push(program.into_os_string());
            argv.extend(runner_args);
        }
        (None, _) => {}
    }
    argv.push(exe.into_os_string());
    match target {
        Target::Game(game) => argv.extend(game.args.iter().map(OsString::from)),
        Target::Tricks { verbs } => argv.extend(verbs.iter().map(OsString::from)),
        Target::Execute { .. } => {}
    }
    argv.extend(ctx.extra_args.iter().map(OsString::from));

    // Wrappers go in front of everything.
    let mut full: Vec<OsString> = wrappers.into_iter().map(OsString::from).collect();
    full.append(&mut argv);
    let mut full = full.into_iter();
    let program = PathBuf::from(full.next().expect("argv has at least the executable"));

    let game_env = match target {
        Target::Game(game) => Some(&game.env),
        Target::Execute { .. } | Target::Tricks { .. } => None,
    };
    let mut env = merge_env(runner_env, &profile.env, game_env);
    let (key, value) = tag.env();
    env.insert(key, value);
    if let Some(prefix) = prefix {
        env.insert(TAG_PREFIX.into(), prefix.into_os_string());
    }

    Ok(LaunchSpec {
        program,
        args: full.collect(),
        env,
        cwd,
        runner: runner_bin,
    })
}

/// Resolves `execute`'s target: paths must exist; bare names go through `PATH`
/// for native, and straight to wine otherwise (`winecfg`, `regedit`, ...).
fn execute_program(
    program: &Path,
    runner: Runner,
    ctx: &LaunchContext,
    problems: &mut Vec<String>,
) -> PathBuf {
    let expanded = paths::expand_tilde(program, ctx.home);
    let is_path = expanded.components().count() > 1 || expanded.is_absolute();

    if is_path {
        if !expanded.is_file() {
            problems.push(format!("executable not found: {}", expanded.display()));
        }
        expanded
    } else if runner == Runner::Native {
        find_program(&expanded, ctx.search_path).unwrap_or_else(|| {
            problems.push(format!("`{}` not found in PATH", expanded.display()));
            expanded
        })
    } else {
        expanded
    }
}

/// Each wrapper entry is split on whitespace, so `"mangohud --dlsym"` works.
fn resolve_wrappers(
    wrappers: &[String],
    search_path: &[PathBuf],
    problems: &mut Vec<String>,
) -> Vec<String> {
    let mut argv = Vec::new();
    for wrapper in wrappers {
        let mut words = wrapper.split_whitespace();
        let Some(program) = words.next() else {
            continue;
        };
        match find_program(Path::new(program), search_path) {
            Some(resolved) => argv.push(resolved.to_string_lossy().into_owned()),
            None => problems.push(format!("wrapper `{program}` not found")),
        }
        argv.extend(words.map(str::to_owned));
    }
    argv
}

/// Layers runner env < profile env < game env. `WINEDLLOVERRIDES` is merged per DLL.
fn merge_env(
    runner_env: BTreeMap<String, OsString>,
    profile_env: &BTreeMap<String, String>,
    game_env: Option<&BTreeMap<String, String>>,
) -> BTreeMap<String, OsString> {
    let mut env = runner_env;
    for layer in std::iter::once(profile_env).chain(game_env) {
        for (key, value) in layer {
            if key == DLL_OVERRIDES_VAR {
                let base = env
                    .get(key)
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut merged = parse_dll_overrides(&base);
                merged.extend(parse_dll_overrides(value));
                env.insert(key.clone(), format_dll_overrides(&merged).into());
            } else {
                env.insert(key.clone(), value.into());
            }
        }
    }
    env
}

/// Parses wine's `a,b=n,b;c=` syntax into one entry per DLL.
pub fn parse_dll_overrides(value: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for entry in value.split(';') {
        let Some((dlls, mode)) = entry.split_once('=') else {
            continue;
        };
        for dll in dlls.split(',').map(str::trim).filter(|dll| !dll.is_empty()) {
            map.insert(dll.to_owned(), mode.trim().to_owned());
        }
    }
    map
}

pub fn format_dll_overrides(map: &BTreeMap<String, String>) -> String {
    map.iter()
        .map(|(dll, mode)| format!("{dll}={mode}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// Finds an executable: paths (containing `/`) are checked directly, bare names in `search_path`.
pub fn find_program(program: &Path, search_path: &[PathBuf]) -> Option<PathBuf> {
    if program.components().count() > 1 || program.is_absolute() {
        return is_executable(program).then(|| program.to_path_buf());
    }
    search_path
        .iter()
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

pub fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}
