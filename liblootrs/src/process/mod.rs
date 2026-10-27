//! Finding and stopping processes through `/proc`.
//!
//! Every process lootrs starts carries [`TAG_GAME`] or [`TAG_SESSION`] in its
//! environment, and wine/Proton children inherit it (including processes inside
//! umu's pressure-vessel container, which shares our PID namespace). That tag is
//! the single source of truth for what belongs to a game; there is no state file.

use std::{
    collections::BTreeMap,
    fmt,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use rustix::process::{Pid, Signal};

use crate::launch::{TAG_GAME, TAG_PREFIX, TAG_SESSION};

#[cfg(test)]
mod tests;

/// Wine's per-prefix background processes. They inherit the tag of whichever
/// game started the prefix first, so they don't count as "running" on their own
/// and are spared while another game still uses the prefix.
const WINE_INFRASTRUCTURE: [&str; 7] = [
    "wineserver",
    "services.exe",
    "winedevice.exe",
    "plugplay.exe",
    "svchost.exe",
    "explorer.exe",
    "rpcss.exe",
];

/// Executable names that mark a wine/Proton process for `killall --everything`.
const WINE_EXECUTABLES: [&str; 9] = [
    "wine",
    "wine64",
    "wine-preloader",
    "wine64-preloader",
    "wineserver",
    "srt-bwrap",
    "pv-adverb",
    "pressure-vessel-wrap",
    "reaper",
];

const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// What a tagged process belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Owner {
    Game(String),
    /// `execute:<profile>:<stamp>`, `tricks:...`, `init:...`.
    Session(String),
}

impl fmt::Display for Owner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game(name) => f.write_str(name),
            Self::Session(session) => f.write_str(session),
        }
    }
}

/// lootrs tags read from a process environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    pub owner: Option<Owner>,
    pub prefix: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub pid: u32,
    /// Kernel command name (at most 15 bytes).
    pub comm: String,
    pub exe: Option<PathBuf>,
    pub tags: Tags,
}

impl Process {
    pub fn is_wine_infrastructure(&self) -> bool {
        WINE_INFRASTRUCTURE.contains(&self.comm.as_str())
    }

    /// Heuristic for `killall --everything`: Windows executables and wine/Proton binaries.
    pub fn looks_like_wine(&self) -> bool {
        let exe_name = self
            .exe
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str());
        self.comm.ends_with(".exe")
            || self.comm == "umu-run"
            || WINE_EXECUTABLES.contains(&self.comm.as_str())
            || exe_name.is_some_and(|name| WINE_EXECUTABLES.contains(&name))
    }
}

/// Result of [`terminate`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Processes that were sent SIGTERM.
    pub signaled: usize,
    /// Processes still alive after the timeout and sent SIGKILL.
    pub killed: usize,
}

/// Processes of the current user, excluding this one.
pub fn scan() -> Vec<Process> {
    let uid = std::fs::metadata("/proc/self").map_or(u32::MAX, |meta| meta.uid());
    scan_dir(Path::new("/proc"), uid, std::process::id())
}

/// Scans a `/proc`-like directory for processes owned by `uid`.
pub fn scan_dir(proc_dir: &Path, uid: u32, self_pid: u32) -> Vec<Process> {
    let Ok(entries) = std::fs::read_dir(proc_dir) else {
        return Vec::new();
    };

    let mut processes: Vec<Process> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let pid: u32 = entry.file_name().to_str()?.parse().ok()?;
            if pid == self_pid || entry.metadata().ok()?.uid() != uid {
                return None;
            }
            let dir = entry.path();
            // Unreadable or vanished entries are skipped; processes come and go.
            let environ = std::fs::read(dir.join("environ")).ok()?;
            let comm = std::fs::read_to_string(dir.join("comm")).ok()?;
            Some(Process {
                pid,
                comm: comm.trim_end().to_owned(),
                exe: std::fs::read_link(dir.join("exe")).ok(),
                tags: parse_environ(&environ),
            })
        })
        .collect();
    processes.sort_by_key(|process| process.pid);
    processes
}

/// Extracts lootrs tags from a NUL-separated environment block.
pub fn parse_environ(environ: &[u8]) -> Tags {
    let mut tags = Tags::default();
    for entry in environ.split(|&b| b == 0) {
        let Some(eq) = entry.iter().position(|&b| b == b'=') else {
            continue;
        };
        let (key, value) = (&entry[..eq], &entry[eq + 1..]);
        let value = || String::from_utf8_lossy(value).into_owned();
        match key {
            k if k == TAG_GAME.as_bytes() => tags.owner = Some(Owner::Game(value())),
            k if k == TAG_SESSION.as_bytes() && tags.owner.is_none() => {
                tags.owner = Some(Owner::Session(value()));
            }
            k if k == TAG_PREFIX.as_bytes() => tags.prefix = Some(value().into()),
            _ => {}
        }
    }
    tags
}

/// Owners with at least one process that isn't wine infrastructure, with all their processes.
pub fn running(processes: &[Process]) -> BTreeMap<Owner, Vec<&Process>> {
    let mut groups: BTreeMap<Owner, Vec<&Process>> = BTreeMap::new();
    for process in processes {
        if let Some(owner) = &process.tags.owner {
            groups.entry(owner.clone()).or_default().push(process);
        }
    }
    groups.retain(|_, group| group.iter().any(|p| !p.is_wine_infrastructure()));
    groups
}

pub fn is_running(processes: &[Process], owner: &Owner) -> bool {
    processes
        .iter()
        .any(|p| p.tags.owner.as_ref() == Some(owner) && !p.is_wine_infrastructure())
}

/// PIDs to signal when stopping `owner`.
///
/// Its own processes, except wine infrastructure while another owner is still
/// active in the same prefix. When nobody else is, the prefix's infrastructure
/// goes too, whichever tag it carries.
pub fn stop_targets(processes: &[Process], owner: &Owner) -> Vec<u32> {
    let owns = |p: &Process| p.tags.owner.as_ref() == Some(owner);
    let prefixes: Vec<&Path> = processes
        .iter()
        .filter(|p| owns(p))
        .filter_map(|p| p.tags.prefix.as_deref())
        .collect();
    let in_prefix = |p: &Process| {
        p.tags
            .prefix
            .as_deref()
            .is_some_and(|prefix| prefixes.contains(&prefix))
    };
    let shared = processes
        .iter()
        .any(|p| !owns(p) && p.tags.owner.is_some() && in_prefix(p) && !p.is_wine_infrastructure());

    processes
        .iter()
        .filter(|p| {
            if shared {
                owns(p) && !p.is_wine_infrastructure()
            } else {
                owns(p) || (in_prefix(p) && p.is_wine_infrastructure())
            }
        })
        .map(|p| p.pid)
        .collect()
}

/// PIDs for `killall`: every tagged process, plus anything wine-like with `everything`.
pub fn killall_targets(processes: &[Process], everything: bool) -> Vec<u32> {
    processes
        .iter()
        .filter(|p| p.tags.owner.is_some() || (everything && p.looks_like_wine()))
        .map(|p| p.pid)
        .collect()
}

/// Sends SIGTERM to the processes `select` picks from each fresh `scan`, waits
/// up to `timeout` for them to exit, then sends SIGKILL to the rest.
///
/// Processes that appear while waiting (e.g. spawned by the dying game) are
/// signaled too. Wine helpers left behind after wineserver exited (a
/// `winedevice.exe` that catches SIGTERM, typically) are killed right away.
pub fn terminate(
    scan: impl Fn() -> Vec<Process>,
    select: impl Fn(&[Process]) -> Vec<u32>,
    timeout: Duration,
) -> Outcome {
    let mut signaled: Vec<u32> = Vec::new();
    let deadline = Instant::now() + timeout;
    let mut first_pass = true;

    loop {
        let processes = scan();
        let alive = select(&processes);
        for &pid in &alive {
            if !signaled.contains(&pid) {
                send(pid, Signal::TERM);
                signaled.push(pid);
            }
        }
        // Helpers only count as orphaned after they've had one poll to react to SIGTERM.
        let orphaned = !first_pass && orphaned_helpers(&processes, &alive);
        first_pass = false;
        if alive.is_empty() || Instant::now() >= deadline || orphaned {
            let mut killed = 0;
            for pid in alive {
                send(pid, Signal::KILL);
                killed += 1;
            }
            return Outcome {
                signaled: signaled.len(),
                killed,
            };
        }
        thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    }
}

/// Only wine infrastructure is left, and its wineserver is gone.
fn orphaned_helpers(processes: &[Process], alive: &[u32]) -> bool {
    alive.iter().all(|pid| {
        processes
            .iter()
            .find(|p| p.pid == *pid)
            .is_some_and(|p| p.is_wine_infrastructure() && p.comm != "wineserver")
    })
}

fn send(pid: u32, signal: Signal) {
    let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) else {
        return;
    };
    // The process may already be gone; nothing to do then.
    if let Err(err) = rustix::process::kill_process(pid, signal) {
        tracing::debug!(pid = pid.as_raw_nonzero().get(), "kill failed: {err}");
    }
}
