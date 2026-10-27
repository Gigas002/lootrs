use std::{collections::BTreeMap, ffi::OsStr, path::Path};

use super::*;

#[test]
fn os_release_pretty_name() {
    let content = "NAME=\"CachyOS\"\nPRETTY_NAME=\"CachyOS Linux\"\nID=cachyos\n";

    assert_eq!(parse_os_release(content).as_deref(), Some("CachyOS Linux"));
    assert_eq!(parse_os_release("ID=arch\n"), None);
}

#[test]
fn quoting() {
    assert_eq!(shell_quote(OsStr::new("/usr/bin/wine")), "/usr/bin/wine");
    assert_eq!(shell_quote(OsStr::new("")), "''");
    assert_eq!(shell_quote(OsStr::new("a b")), "'a b'");
    assert_eq!(shell_quote(OsStr::new("it's")), r"'it'\''s'");
}

#[test]
fn command_line_joins_quoted() {
    let argv = ["wine", "C:\\Games\\x.exe", "-a"].map(OsStr::new);

    assert_eq!(command_line(argv), r"wine 'C:\Games\x.exe' -a");
}

#[test]
fn env_lines_are_sorted_and_quoted() {
    let env = BTreeMap::from([
        ("B".to_owned(), "two words".into()),
        ("A".to_owned(), "1".into()),
    ]);

    assert_eq!(env_lines(&env), "A=1\nB='two words'\n");
}

#[test]
fn runner_version_reads_first_line() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake-wine");
    write_script(&script, "#!/bin/sh\necho\necho wine-11.0\n");

    assert_eq!(runner_version(&script).as_deref(), Some("wine-11.0"));
    assert_eq!(runner_version(Path::new("/nonexistent/wine")), None);
}

#[test]
fn proton_version_from_version_file() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(proton_version(dir.path()), None);

    std::fs::write(
        dir.path().join("version"),
        "1792545198 cachyos-11.0-native\n",
    )
    .unwrap();
    assert_eq!(
        proton_version(dir.path()).as_deref(),
        Some("cachyos-11.0-native")
    );
}

/// Writes an executable script from a child process: a fork in this
/// multi-threaded test binary that inherited a write handle to it would make
/// running it fail with "Text file busy".
fn write_script(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let source = path.with_extension("src");
    std::fs::write(&source, content).unwrap();
    let status = std::process::Command::new("install")
        .args(["-m", "755"])
        .arg(&source)
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn collect_does_not_panic() {
    let info = SystemInfo::collect();

    assert!(info.kernel.is_some());
}
