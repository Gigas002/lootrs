use super::*;

/// A temp dir with a fake prefix (wine's DLLs present), a DXVK and a VKD3D release.
struct Fixture {
    dir: tempfile::TempDir,
    env: BTreeMap<String, OsString>,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let fixture = Self {
            dir,
            env: BTreeMap::new(),
        };
        fixture.write("pfx/system.reg", "");
        for dll in ["d3d9", "d3d11", "dxgi", "d3d12"] {
            fixture.write(&format!("pfx/drive_c/windows/system32/{dll}.dll"), "wine64");
            fixture.write(&format!("pfx/drive_c/windows/syswow64/{dll}.dll"), "wine32");
        }
        for dll in ["d3d9", "d3d11", "dxgi"] {
            fixture.write(&format!("dxvk-a/x64/{dll}.dll"), "dxvk-a 64");
            fixture.write(&format!("dxvk-a/x32/{dll}.dll"), "dxvk-a 32");
            fixture.write(&format!("dxvk-b/x64/{dll}.dll"), "dxvk-b 64");
        }
        fixture.write("vkd3d/x64/d3d12.dll", "vkd3d 64");
        fixture.write("vkd3d/x64/d3d12core.dll", "vkd3d 64");
        fixture.write("vkd3d/x86/d3d12.dll", "vkd3d 32");
        fixture
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.path(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.path(relative)).unwrap()
    }

    fn ensure(
        &self,
        dxvk: Option<&str>,
        vkd3d: Option<&str>,
        in_use: bool,
    ) -> Result<Report, PrefixError> {
        let prefix = self.path("pfx");
        let wine = self.path("bin/wine");
        let lock_dir = self.path("locks");
        let dxvk = dxvk.map(|d| self.path(d));
        let vkd3d = vkd3d.map(|d| self.path(d));
        ensure_wine(
            &WineSetup {
                prefix: &prefix,
                wine: &wine,
                wineserver: None,
                env: &self.env,
                dxvk: dxvk.as_deref(),
                vkd3d: vkd3d.as_deref(),
                lock_dir: &lock_dir,
            },
            |_| in_use,
        )
    }
}

const SYS32: &str = "pfx/drive_c/windows/system32";
const WOW64: &str = "pfx/drive_c/windows/syswow64";

#[test]
fn existing_prefix_without_components_is_untouched() {
    let fx = Fixture::new();

    let report = fx.ensure(None, None, false).unwrap();

    assert!(!report.created);
    assert!(report.changes.is_empty());
    assert!(!fx.path("pfx").join(STATE_FILE).exists());
}

#[test]
fn links_dxvk_and_vkd3d_with_backups() {
    let fx = Fixture::new();

    let report = fx.ensure(Some("dxvk-a"), Some("vkd3d"), false).unwrap();

    assert_eq!(report.changes.len(), 2);
    assert!(report.changes[0].starts_with("linked DXVK from "));
    assert!(report.changes[0].ends_with("(6 DLLs)"));
    assert!(report.changes[1].ends_with("(3 DLLs)"));

    assert_eq!(fx.read(&format!("{SYS32}/d3d11.dll")), "dxvk-a 64");
    assert_eq!(fx.read(&format!("{WOW64}/d3d11.dll")), "dxvk-a 32");
    assert_eq!(fx.read(&format!("{SYS32}/d3d11.dll.orig")), "wine64");
    assert_eq!(fx.read(&format!("{SYS32}/d3d12core.dll")), "vkd3d 64");
    assert!(!fx.path(&format!("{SYS32}/d3d12core.dll.orig")).exists());
    assert_eq!(fx.read(&format!("{WOW64}/d3d12.dll")), "vkd3d 32");

    let state = read_state(&fx.path("pfx"));
    assert_eq!(state, report.state);
    assert_eq!(state.lootrs, env!("CARGO_PKG_VERSION"));
    assert_eq!(state.dxvk.unwrap().source, fx.path("dxvk-a"));
}

#[test]
fn second_run_is_a_no_op() {
    let fx = Fixture::new();
    fx.ensure(Some("dxvk-a"), None, false).unwrap();

    let report = fx.ensure(Some("dxvk-a"), None, true).unwrap();

    assert!(report.changes.is_empty());
}

#[test]
fn switching_versions_relinks_and_keeps_original_backup() {
    let fx = Fixture::new();
    fx.ensure(Some("dxvk-a"), None, false).unwrap();

    let report = fx.ensure(Some("dxvk-b"), None, false).unwrap();

    assert_eq!(report.changes.len(), 2);
    assert!(report.changes[0].starts_with("removed DXVK"));
    assert_eq!(fx.read(&format!("{SYS32}/d3d11.dll")), "dxvk-b 64");
    assert_eq!(fx.read(&format!("{SYS32}/d3d11.dll.orig")), "wine64");
    // dxvk-b has no x32: wine's 32-bit DLL is back.
    assert_eq!(fx.read(&format!("{WOW64}/d3d11.dll")), "wine32");
    assert!(!fx.path(&format!("{WOW64}/d3d11.dll.orig")).exists());
}

#[test]
fn removing_restores_wine_dlls() {
    let fx = Fixture::new();
    fx.ensure(Some("dxvk-a"), Some("vkd3d"), false).unwrap();

    let report = fx.ensure(None, None, false).unwrap();

    assert_eq!(report.changes.len(), 2);
    assert_eq!(fx.read(&format!("{SYS32}/d3d11.dll")), "wine64");
    assert!(!fx.path(&format!("{SYS32}/d3d11.dll.orig")).exists());
    assert!(!fx.path(&format!("{SYS32}/d3d12core.dll")).exists());
    assert_eq!(read_state(&fx.path("pfx")).dxvk, None);
}

#[test]
fn broken_link_is_repaired() {
    let fx = Fixture::new();
    fx.ensure(Some("dxvk-a"), None, false).unwrap();
    // e.g. a wine update replaced one of our links with its own file.
    let link = fx.path(&format!("{SYS32}/dxgi.dll"));
    std::fs::remove_file(&link).unwrap();
    std::fs::write(&link, "wine64 new").unwrap();

    let report = fx.ensure(Some("dxvk-a"), None, false).unwrap();

    assert!(!report.changes.is_empty());
    assert_eq!(fx.read(&format!("{SYS32}/dxgi.dll")), "dxvk-a 64");
    assert_eq!(fx.read(&format!("{SYS32}/dxgi.dll.orig")), "wine64");
}

#[test]
fn busy_prefix_is_not_changed() {
    let fx = Fixture::new();

    let err = fx.ensure(Some("dxvk-a"), None, true).unwrap_err();

    assert!(matches!(err, PrefixError::Busy { .. }));
    assert_eq!(fx.read(&format!("{SYS32}/d3d11.dll")), "wine64");
}

#[test]
fn component_without_x64_dlls_is_an_error() {
    let fx = Fixture::new();
    std::fs::create_dir_all(fx.path("empty/x64")).unwrap();

    let err = fx.ensure(Some("empty"), None, false).unwrap_err();

    assert!(matches!(
        err,
        PrefixError::NoDlls {
            component: Component::Dxvk,
            ..
        }
    ));
}

#[test]
fn dll_names_for_overrides() {
    let fx = Fixture::new();

    assert_eq!(dll_names(&fx.path("dxvk-a")), ["d3d11", "d3d9", "dxgi"]);
    assert_eq!(dll_names(&fx.path("vkd3d")), ["d3d12", "d3d12core"]);
    assert!(dll_names(&fx.path("missing")).is_empty());
}

fn fake_wine(fx: &Fixture, script: &str) {
    write_script(&fx.path("bin/wine"), script);
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
fn creates_missing_prefix_with_wineboot() {
    let fx = Fixture::new();
    std::fs::remove_dir_all(fx.path("pfx")).unwrap();
    fake_wine(
        &fx,
        "#!/bin/sh\n[ \"$1 $2\" = 'wineboot -u' ] || exit 9\necho \"booting $WINEPREFIX $LOOTRS_SESSION\"\ntouch \"$WINEPREFIX/system.reg\"\n",
    );
    let mut fx = fx;
    fx.env.insert("WINEPREFIX".into(), fx.path("pfx").into());
    fx.env.insert("LOOTRS_SESSION".into(), "init:p:1".into());

    let report = fx.ensure(None, None, false).unwrap();

    assert!(report.created);
    assert_eq!(
        report.output.trim(),
        format!("booting {} init:p:1", fx.path("pfx").display())
    );
    assert_eq!(
        read_state(&fx.path("pfx")).lootrs,
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn failed_wineboot_is_reported() {
    let fx = Fixture::new();
    std::fs::remove_dir_all(fx.path("pfx")).unwrap();
    fake_wine(&fx, "#!/bin/sh\necho 'no display' >&2\nexit 1\n");

    let err = fx.ensure(None, None, false).unwrap_err();

    assert!(
        matches!(&err, PrefixError::Create { output, .. } if output.contains("no display")),
        "{err:?}"
    );
}

fn ensure_umu_with(fx: &Fixture, script: &str) -> Result<Report, PrefixError> {
    write_script(&fx.path("bin/umu-run"), script);
    let env = BTreeMap::from([("WINEPREFIX".to_owned(), fx.path("pfx").into())]);
    ensure_umu(&UmuSetup {
        prefix: &fx.path("pfx"),
        umu_run: &fx.path("bin/umu-run"),
        env: &env,
        lock_dir: &fx.path("locks"),
    })
}

#[test]
fn umu_prefix_is_created_despite_proton_exit_code() {
    let fx = Fixture::new();
    std::fs::remove_dir_all(fx.path("pfx")).unwrap();

    // Proton fails to start the empty executable but has created the prefix.
    let report = ensure_umu_with(
        &fx,
        "#!/bin/sh\n[ \"$1\" = createprefix ] || exit 9\ntouch \"$WINEPREFIX/system.reg\"\necho 'File not found.'\nexit 1\n",
    )
    .unwrap();

    assert!(report.created);
    assert_eq!(report.output.trim(), "File not found.");
    assert!(report.changes.is_empty());
    assert!(!fx.path("pfx").join(STATE_FILE).exists());
}

#[test]
fn existing_umu_prefix_is_left_alone() {
    let fx = Fixture::new();

    let report = ensure_umu_with(&fx, "#!/bin/sh\nexit 9\n").unwrap();

    assert!(!report.created);
    assert!(report.output.is_empty());
}

#[test]
fn failed_createprefix_is_reported() {
    let fx = Fixture::new();
    std::fs::remove_dir_all(fx.path("pfx")).unwrap();

    let err = ensure_umu_with(&fx, "#!/bin/sh\necho 'no runtime' >&2\nexit 1\n").unwrap_err();

    assert!(
        matches!(
            &err,
            PrefixError::Create { command: "umu-run createprefix", output, .. } if output.contains("no runtime")
        ),
        "{err:?}"
    );
}
