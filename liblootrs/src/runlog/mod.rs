//! Per-run log files: layout, retention and the `latest.log` link.
//!
//! ```text
//! <logs>/games/<game>/<stamp>.log     # one per run
//! <logs>/games/<game>/<stamp>/        # runner-side logs (DXVK, VKD3D, Proton)
//! <logs>/games/<game>/latest.log      # symlink to the newest run log
//! <logs>/{execute,tricks,init}/<profile>/...
//! ```

use std::{
    fs::File,
    io,
    path::{Path, PathBuf},
};

#[cfg(test)]
mod tests;

pub const LATEST: &str = "latest.log";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind<'a> {
    Game(&'a str),
    Execute(&'a str),
    Tricks(&'a str),
    Init(&'a str),
}

impl RunKind<'_> {
    /// Directory holding this game's or profile's run logs.
    pub fn dir(&self, logs_dir: &Path) -> PathBuf {
        let (group, name) = match self {
            Self::Game(name) => ("games", name),
            Self::Execute(name) => ("execute", name),
            Self::Tricks(name) => ("tricks", name),
            Self::Init(name) => ("init", name),
        };
        logs_dir.join(group).join(name)
    }
}

/// A freshly created run log.
#[derive(Debug)]
pub struct RunLog {
    pub file: File,
    pub path: PathBuf,
    /// Directory for runner-side logs; remove with [`remove_if_empty`] after the run.
    pub artifacts: PathBuf,
}

/// Creates `<stamp>.log` (adding `-2`, `-3`, ... on collisions), keeping at most
/// `keep` logs including the new one, and points `latest.log` at it.
pub fn create(logs_dir: &Path, kind: RunKind, stamp: &str, keep: usize) -> io::Result<RunLog> {
    let dir = kind.dir(logs_dir);
    std::fs::create_dir_all(&dir)?;
    prune(&dir, keep.saturating_sub(1))?;

    let (file, path, stem) = (1..)
        .map(|n| match n {
            1 => stamp.to_owned(),
            n => format!("{stamp}-{n}"),
        })
        .find_map(|stem| {
            let path = dir.join(format!("{stem}.log"));
            match File::create_new(&path) {
                Ok(file) => Some(Ok((file, path, stem))),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => None,
                Err(err) => Some(Err(err)),
            }
        })
        .expect("unbounded range")?;

    let artifacts = dir.join(&stem);
    std::fs::create_dir_all(&artifacts)?;
    link_latest(&dir, &path)?;

    Ok(RunLog {
        file,
        path,
        artifacts,
    })
}

/// The newest run log for `kind`, following `latest.log`.
pub fn latest(logs_dir: &Path, kind: RunKind) -> Option<PathBuf> {
    let link = kind.dir(logs_dir).join(LATEST);
    let target = std::fs::read_link(&link).ok()?;
    let path = link.parent()?.join(target);
    path.is_file().then_some(path)
}

/// Removes the artifacts directory if the runner wrote nothing into it.
pub fn remove_if_empty(dir: &Path) {
    // Fails harmlessly when the directory is not empty.
    let _ = std::fs::remove_dir(dir);
}

/// Deletes the oldest logs (and their artifacts) until at most `keep` remain.
fn prune(dir: &Path, keep: usize) -> io::Result<()> {
    let mut logs: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "log")
                && path.file_name().is_some_and(|name| name != LATEST)
                && !path.is_symlink()
        })
        .collect();
    // Stamps sort chronologically.
    logs.sort();

    let excess = logs.len().saturating_sub(keep);
    for log in &logs[..excess] {
        std::fs::remove_file(log)?;
        let artifacts = log.with_extension("");
        if artifacts.is_dir() {
            std::fs::remove_dir_all(artifacts)?;
        }
    }
    Ok(())
}

/// Atomically repoints `latest.log` (relative link) at `target`.
fn link_latest(dir: &Path, target: &Path) -> io::Result<()> {
    let name = target.file_name().expect("log file has a name");
    let tmp = dir.join(".latest.log.tmp");
    let _ = std::fs::remove_file(&tmp);
    std::os::unix::fs::symlink(name, &tmp)?;
    std::fs::rename(&tmp, dir.join(LATEST))
}
