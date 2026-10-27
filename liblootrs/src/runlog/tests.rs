use std::io::Write;

use super::*;

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn layout_per_kind() {
    let logs = Path::new("/logs");

    assert_eq!(RunKind::Game("wow").dir(logs), Path::new("/logs/games/wow"));
    assert_eq!(
        RunKind::Execute("default").dir(logs),
        Path::new("/logs/execute/default")
    );
    assert_eq!(RunKind::Tricks("p").dir(logs), Path::new("/logs/tricks/p"));
    assert_eq!(RunKind::Init("p").dir(logs), Path::new("/logs/init/p"));
}

#[test]
fn create_writes_log_artifacts_and_latest() {
    let logs = tempfile::tempdir().unwrap();
    let kind = RunKind::Game("wow");

    let mut run = create(logs.path(), kind, "20261027-211403", 10).unwrap();
    writeln!(run.file, "hello").unwrap();

    let dir = kind.dir(logs.path());
    assert_eq!(run.path, dir.join("20261027-211403.log"));
    assert!(run.artifacts.is_dir());
    assert_eq!(latest(logs.path(), kind), Some(run.path.clone()));
    assert_eq!(
        std::fs::read_to_string(dir.join(LATEST)).unwrap(),
        "hello\n"
    );
}

#[test]
fn same_stamp_gets_suffix() {
    let logs = tempfile::tempdir().unwrap();
    let kind = RunKind::Game("wow");

    let first = create(logs.path(), kind, "s", 10).unwrap();
    let second = create(logs.path(), kind, "s", 10).unwrap();

    assert!(first.path.ends_with("s.log"));
    assert!(second.path.ends_with("s-2.log"));
    assert_eq!(latest(logs.path(), kind), Some(second.path));
}

#[test]
fn retention_keeps_newest_logs_and_their_artifacts() {
    let logs = tempfile::tempdir().unwrap();
    let kind = RunKind::Game("wow");

    for stamp in ["20260101", "20260102", "20260103", "20260104"] {
        let run = create(logs.path(), kind, stamp, 2).unwrap();
        std::fs::write(run.artifacts.join("dxvk.log"), "x").unwrap();
    }

    assert_eq!(
        names(&kind.dir(logs.path())),
        [
            "20260103",
            "20260103.log",
            "20260104",
            "20260104.log",
            "latest.log"
        ]
    );
}

#[test]
fn latest_missing() {
    let logs = tempfile::tempdir().unwrap();

    assert_eq!(latest(logs.path(), RunKind::Game("nope")), None);
}

#[test]
fn empty_artifacts_are_removed() {
    let logs = tempfile::tempdir().unwrap();
    let empty = create(logs.path(), RunKind::Game("a"), "1", 10).unwrap();
    let full = create(logs.path(), RunKind::Game("b"), "1", 10).unwrap();
    std::fs::write(full.artifacts.join("dxvk.log"), "x").unwrap();

    remove_if_empty(&empty.artifacts);
    remove_if_empty(&full.artifacts);

    assert!(!empty.artifacts.exists());
    assert!(full.artifacts.is_dir());
}

#[test]
fn tail_returns_last_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("run.log");
    let content: String = (1..=50).map(|i| format!("line {i}\n")).collect();
    std::fs::write(&path, content).unwrap();

    assert_eq!(tail(&path, 2).unwrap(), ["line 49", "line 50"]);
    assert_eq!(tail(&path, 100).unwrap().len(), 50);
    assert!(tail(&dir.path().join("missing.log"), 5).is_err());
}

#[test]
fn tail_of_a_big_file_drops_the_cut_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.log");
    let line = "x".repeat(1000);
    let content: String = (0..400).map(|i| format!("{i} {line}\n")).collect();
    std::fs::write(&path, content).unwrap();

    let lines = tail(&path, 1000).unwrap();

    assert!(lines.len() < 400);
    assert!(lines.iter().all(|l| l.ends_with(&line)));
    assert!(lines.last().unwrap().starts_with("399 "));
}
