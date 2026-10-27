//! Diagnostics for run-log headers: system info, runner versions, shell quoting.

use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    path::Path,
    process::{Command, Stdio},
};

#[cfg(test)]
mod tests;

const VULKAN_ICD_DIRS: [&str; 2] = ["/usr/share/vulkan/icd.d", "/etc/vulkan/icd.d"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemInfo {
    pub kernel: Option<String>,
    /// `PRETTY_NAME` from `/etc/os-release`.
    pub distro: Option<String>,
    /// Whether `/dev/ntsync` exists.
    pub ntsync: bool,
    /// Vulkan ICD manifest file names.
    pub vulkan_icds: Vec<String>,
}

impl SystemInfo {
    pub fn collect() -> Self {
        let read = |path| std::fs::read_to_string(path).ok();
        Self {
            kernel: read("/proc/sys/kernel/osrelease").map(|s| s.trim().to_owned()),
            distro: read("/etc/os-release").and_then(|s| parse_os_release(&s)),
            ntsync: Path::new("/dev/ntsync").exists(),
            vulkan_icds: VULKAN_ICD_DIRS
                .iter()
                .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten())
                .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                .collect(),
        }
    }
}

/// `PRETTY_NAME` from os-release content, unquoted.
pub fn parse_os_release(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let value = line.strip_prefix("PRETTY_NAME=")?;
        Some(value.trim().trim_matches(['"', '\'']).to_owned())
    })
}

/// First non-empty line of `program --version`, if it runs.
pub fn runner_version(program: &Path) -> Option<String> {
    let output = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Quotes one argument for a POSIX shell, leaving safe words bare.
pub fn shell_quote(arg: &OsStr) -> String {
    let arg = arg.to_string_lossy();
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        arg.into_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// A copy-pasteable command line.
pub fn command_line<'a>(argv: impl IntoIterator<Item = &'a OsStr>) -> String {
    argv.into_iter()
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `KEY=value` lines, values shell-quoted.
pub fn env_lines(env: &BTreeMap<String, OsString>) -> String {
    env.iter()
        .map(|(key, value)| format!("{key}={}\n", shell_quote(value)))
        .collect()
}
