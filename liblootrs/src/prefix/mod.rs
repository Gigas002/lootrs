//! Wine prefix setup: creation, and DXVK/VKD3D-Proton linking.
//!
//! lootrs records what it linked in `<prefix>/.lootrs.toml`. [`ensure_wine`]
//! compares that with the profile on every launch and fixes any difference:
//!
//! - `x64/*.dll` are symlinked into `system32`, `x32/` or `x86/` into `syswow64`;
//!   wine's own files are kept as `*.dll.orig` and restored when the override goes.
//! - Symlinks, not copies, so updating the files in your DXVK directory takes
//!   effect without re-linking.

use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    ffi::OsString,
    fs::File,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::model::{Profile, Runner};
#[cfg(feature = "wine")]
use crate::paths;

#[cfg(test)]
mod tests;

pub const STATE_FILE: &str = ".lootrs.toml";

/// Where each architecture directory of a DXVK/VKD3D release goes.
const LINK_DIRS: [(&str, &str); 3] = [
    ("x64", "drive_c/windows/system32"),
    ("x32", "drive_c/windows/syswow64"),
    ("x86", "drive_c/windows/syswow64"),
];

/// What lootrs has set up in a prefix.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrefixState {
    /// lootrs version that last wrote the state.
    #[serde(default)]
    pub lootrs: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dxvk: Option<Linked>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vkd3d: Option<Linked>,
}

/// One linked component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Linked {
    pub source: PathBuf,
    /// Symlinks lootrs created, relative to the prefix.
    pub links: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Dxvk,
    Vkd3d,
}

impl Component {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dxvk => "DXVK",
            Self::Vkd3d => "VKD3D-Proton",
        }
    }
}

/// Everything [`ensure_wine`] needs; paths are absolute.
#[derive(Debug, Clone)]
pub struct WineSetup<'a> {
    pub prefix: &'a Path,
    pub wine: &'a Path,
    pub wineserver: Option<&'a Path>,
    /// Env for `wineboot` (prefix vars, plus lootrs tags).
    pub env: &'a BTreeMap<String, OsString>,
    pub dxvk: Option<&'a Path>,
    pub vkd3d: Option<&'a Path>,
    /// Directory for per-prefix lock files.
    pub lock_dir: &'a Path,
}

/// What [`ensure_wine`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub created: bool,
    /// One line per change, e.g. "linked DXVK from /opt/dxvk (5 DLLs)".
    pub changes: Vec<String>,
    /// Output of `wineboot` when the prefix was created.
    pub output: String,
    /// The state after setup.
    pub state: PrefixState,
}

#[derive(Debug, thiserror::Error)]
pub enum PrefixError {
    #[error("{context}: {source}")]
    Io { context: String, source: io::Error },

    #[error("creating the prefix failed (wineboot {status}):\n{output}")]
    Create { status: String, output: String },

    #[error("{} directory {} has no x64/*.dll", component.name(), dir.display())]
    NoDlls { component: Component, dir: PathBuf },

    #[error("prefix {} is in use; stop the games running in it before changing DXVK/VKD3D", prefix.display())]
    Busy { prefix: PathBuf },

    #[error("{0}")]
    Runner(String),
}

/// Inputs for [`ensure`] that come from settings rather than the profile.
#[derive(Debug, Clone, Copy)]
pub struct EnsureContext<'a> {
    pub home: &'a Path,
    pub search_path: &'a [PathBuf],
    pub lock_dir: &'a Path,
    /// lootrs tags for the processes started during setup.
    pub tags: &'a BTreeMap<String, OsString>,
}

/// Sets up the profile's prefix. `Ok(None)` for runners lootrs doesn't
/// prepare: native has no prefix, and umu creates its own on launch.
pub fn ensure(
    profile: &Profile,
    ctx: &EnsureContext,
    in_use: impl Fn(&Path) -> bool,
) -> Result<Option<Report>, PrefixError> {
    match profile.runner {
        Runner::Native | Runner::Umu => Ok(None),
        #[cfg(feature = "wine")]
        Runner::Wine => {
            use crate::runner::wine;

            let prefix = profile
                .prefix
                .as_deref()
                .map(|prefix| paths::expand_tilde(prefix, ctx.home))
                .ok_or_else(|| {
                    PrefixError::Runner("`prefix` is required for runner `wine`".into())
                })?;
            let wine = wine::resolve_wine(profile, ctx.home, ctx.search_path)
                .map_err(PrefixError::Runner)?;
            let wineserver = wine::resolve_wineserver(&wine, ctx.search_path);
            let expand =
                |path: &Option<PathBuf>| path.as_deref().map(|p| paths::expand_tilde(p, ctx.home));
            let (dxvk, vkd3d) = (expand(&profile.dxvk), expand(&profile.vkd3d));

            let mut env = wine::base_env(&prefix, profile.debug);
            env.extend(ctx.tags.clone());
            ensure_wine(
                &WineSetup {
                    prefix: &prefix,
                    wine: &wine,
                    wineserver: wineserver.as_deref(),
                    env: &env,
                    dxvk: dxvk.as_deref(),
                    vkd3d: vkd3d.as_deref(),
                    lock_dir: ctx.lock_dir,
                },
                in_use,
            )
            .map(Some)
        }
        #[cfg(not(feature = "wine"))]
        Runner::Wine => {
            let _ = (ctx, in_use);
            Err(PrefixError::Runner(
                "runner `wine` is not available: lootrs was built without the `wine` feature"
                    .into(),
            ))
        }
    }
}

fn io_err(context: impl Into<String>) -> impl FnOnce(io::Error) -> PrefixError {
    let context = context.into();
    move |source| PrefixError::Io { context, source }
}

/// Creates the prefix if needed and brings DXVK/VKD3D links in line with the
/// profile. `in_use` tells whether games are running in the prefix; links are
/// never changed under them.
pub fn ensure_wine(
    setup: &WineSetup,
    in_use: impl Fn(&Path) -> bool,
) -> Result<Report, PrefixError> {
    let prefix = setup.prefix;
    let _lock = lock(setup.lock_dir, prefix)?;
    let mut report = Report::default();

    if !prefix.join("system.reg").is_file() {
        report.output = create(setup)?;
        report.created = true;
    }

    let mut state = read_state(prefix);
    let mut changed = report.created;
    for (component, desired) in [
        (Component::Dxvk, setup.dxvk),
        (Component::Vkd3d, setup.vkd3d),
    ] {
        let wanted = desired
            .map(|source| plan_links(prefix, component, source))
            .transpose()?;
        let slot = match component {
            Component::Dxvk => &mut state.dxvk,
            Component::Vkd3d => &mut state.vkd3d,
        };
        if up_to_date(prefix, slot.as_ref(), desired, wanted.as_deref()) {
            continue;
        }
        if in_use(prefix) {
            return Err(PrefixError::Busy {
                prefix: prefix.to_path_buf(),
            });
        }

        if let Some(old) = slot.take() {
            unlink(prefix, &old)?;
            report.changes.push(format!(
                "removed {} ({})",
                component.name(),
                old.source.display()
            ));
        }
        if let (Some(source), Some(links)) = (desired, wanted) {
            link(prefix, &links)?;
            report.changes.push(format!(
                "linked {} from {} ({} DLLs)",
                component.name(),
                source.display(),
                links.len()
            ));
            *slot = Some(Linked {
                source: source.to_path_buf(),
                links: links.into_iter().map(|(link, _)| link).collect(),
            });
        }
        changed = true;
    }

    if changed {
        state.lootrs = env!("CARGO_PKG_VERSION").to_owned();
        write_state(prefix, &state)?;
    }
    report.state = state;
    Ok(report)
}

/// The state file, or an empty state if there is none (or it is unreadable).
pub fn read_state(prefix: &Path) -> PrefixState {
    std::fs::read_to_string(prefix.join(STATE_FILE))
        .ok()
        .and_then(|content| toml::from_str(&content).ok())
        .unwrap_or_default()
}

/// DLL names (without `.dll`) in `<source>/x64`, for `WINEDLLOVERRIDES`.
pub fn dll_names(source: &Path) -> Vec<String> {
    dlls(&source.join("x64"))
        .into_iter()
        .filter_map(|path| Some(path.file_stem()?.to_str()?.to_ascii_lowercase()))
        .collect()
}

/// Runs `wine wineboot -u`, then waits for wineserver so the registry is on disk.
fn create(setup: &WineSetup) -> Result<String, PrefixError> {
    let prefix = setup.prefix;
    std::fs::create_dir_all(prefix).map_err(io_err(format!("creating {}", prefix.display())))?;

    let wineboot = run(Command::new(setup.wine)
        .args(["wineboot", "-u"])
        .envs(setup.env))
    .map_err(io_err(format!("running {} wineboot", setup.wine.display())))?;
    let mut output = text(&wineboot);
    if let Some(wineserver) = setup.wineserver {
        // Best effort: without it the registry may still be flushing.
        if let Ok(wait) = run(Command::new(wineserver).arg("-w").envs(setup.env)) {
            output += &text(&wait);
        }
    }

    if wineboot.status.success() && prefix.join("system.reg").is_file() {
        Ok(output)
    } else {
        Err(PrefixError::Create {
            status: wineboot.status.to_string(),
            output,
        })
    }
}

fn run(command: &mut Command) -> io::Result<Output> {
    command.stdin(Stdio::null()).output()
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr)
}

/// `(link relative to prefix, absolute target)` for every DLL in `source`.
fn plan_links(
    prefix: &Path,
    component: Component,
    source: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, PrefixError> {
    let mut links = Vec::new();
    for (arch, dir) in LINK_DIRS {
        if !prefix.join(dir).is_dir() {
            continue;
        }
        for dll in dlls(&source.join(arch)) {
            let name = dll.file_name().expect("dll path has a file name");
            links.push((Path::new(dir).join(name), dll));
        }
    }
    if !links
        .iter()
        .any(|(link, _)| link.starts_with(LINK_DIRS[0].1))
    {
        return Err(PrefixError::NoDlls {
            component,
            dir: source.to_path_buf(),
        });
    }
    Ok(links)
}

/// `*.dll` files in `dir`, sorted; empty if `dir` doesn't exist.
fn dlls(dir: &Path) -> Vec<PathBuf> {
    let mut dlls: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
                && path.is_file()
        })
        .collect();
    dlls.sort();
    dlls
}

/// Whether the recorded links match the wanted ones and still point at them.
fn up_to_date(
    prefix: &Path,
    recorded: Option<&Linked>,
    desired: Option<&Path>,
    wanted: Option<&[(PathBuf, PathBuf)]>,
) -> bool {
    match (recorded, desired, wanted) {
        (None, None, _) => true,
        (Some(recorded), Some(source), Some(wanted)) => {
            recorded.source == source
                && recorded.links.len() == wanted.len()
                && wanted.iter().all(|(link, target)| {
                    recorded.links.contains(link)
                        && std::fs::read_link(prefix.join(link)).is_ok_and(|t| &t == target)
                })
        }
        _ => false,
    }
}

fn orig(path: &Path) -> PathBuf {
    let mut orig = path.as_os_str().to_owned();
    orig.push(".orig");
    orig.into()
}

/// Replaces each file with a symlink, keeping wine's original as `.orig`.
fn link(prefix: &Path, links: &[(PathBuf, PathBuf)]) -> Result<(), PrefixError> {
    for (link, target) in links {
        let path = prefix.join(link);
        let context = || format!("linking {}", path.display());
        if path.is_symlink() {
            std::fs::remove_file(&path).map_err(io_err(context()))?;
        } else if path.exists() {
            let backup = orig(&path);
            if backup.exists() {
                std::fs::remove_file(&path).map_err(io_err(context()))?;
            } else {
                std::fs::rename(&path, &backup).map_err(io_err(context()))?;
            }
        }
        std::os::unix::fs::symlink(target, &path).map_err(io_err(context()))?;
    }
    Ok(())
}

/// Removes lootrs's symlinks and restores wine's originals.
fn unlink(prefix: &Path, linked: &Linked) -> Result<(), PrefixError> {
    for link in &linked.links {
        let path = prefix.join(link);
        let context = || format!("restoring {}", path.display());
        if path.is_symlink() {
            std::fs::remove_file(&path).map_err(io_err(context()))?;
        }
        let backup = orig(&path);
        if backup.exists() && !path.exists() {
            std::fs::rename(&backup, &path).map_err(io_err(context()))?;
        }
    }
    Ok(())
}

fn write_state(prefix: &Path, state: &PrefixState) -> Result<(), PrefixError> {
    let path = prefix.join(STATE_FILE);
    let content = toml::to_string(state).map_err(|err| PrefixError::Io {
        context: format!("writing {}", path.display()),
        source: io::Error::other(err),
    })?;
    let tmp = prefix.join(format!("{STATE_FILE}.tmp"));
    std::fs::write(&tmp, content)
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(io_err(format!("writing {}", path.display())))
}

/// Exclusive lock per prefix, held until the returned file is dropped.
fn lock(lock_dir: &Path, prefix: &Path) -> Result<File, PrefixError> {
    let mut hasher = DefaultHasher::new();
    prefix.hash(&mut hasher);
    let path = lock_dir.join(format!("prefix-{:016x}.lock", hasher.finish()));
    let context = || format!("locking {}", path.display());

    std::fs::create_dir_all(lock_dir).map_err(io_err(context()))?;
    let file = File::create(&path).map_err(io_err(context()))?;
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
        .map_err(|err| io_err(context())(err.into()))?;
    Ok(file)
}
