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
    std::fs::write(&script, "#!/bin/sh\necho\necho wine-11.0\n").unwrap();
    make_executable(&script);

    assert_eq!(runner_version(&script).as_deref(), Some("wine-11.0"));
    assert_eq!(runner_version(Path::new("/nonexistent/wine")), None);
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn collect_does_not_panic() {
    let info = SystemInfo::collect();

    assert!(info.kernel.is_some());
}
