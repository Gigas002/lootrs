//! Profiles for the `proton` runner (Steam's Proton) load and round-trip.
//! lootrs can't launch them, so validating one gives exactly one error; the
//! `native` fixture validates cleanly.

use std::path::Path;

use liblootrs::{
    model::{ProtonSection, Runner, Severity, SteamIntegration, ValidationContext},
    store,
};

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/profiles")
}

#[test]
fn fixture_profiles_load() {
    let ctx = ValidationContext {
        home: Path::new("/home/user"),
        battleye_runtime: false,
        eac_runtime: false,
    };
    let mut names = Vec::new();
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        let profile = store::load_profile(&path).unwrap();
        let issues = profile.validate(&ctx);
        if profile.runner == Runner::Proton {
            assert_eq!(issues.len(), 1, "{issues:#?}");
            assert_eq!(issues[0].severity, Severity::Error);
            assert!(
                issues[0].message.contains("runs through Steam"),
                "{}",
                issues[0]
            );
        } else {
            assert!(issues.is_empty(), "{issues:#?}");
        }
        names.push(profile.name);
    }
    names.sort();
    assert_eq!(names, ["native", "proton", "proton-cachyos"]);
}

#[test]
fn proton_fields_round_trip() {
    let profile = store::load_profile(&fixtures().join("proton.toml")).unwrap();
    assert_eq!(
        profile.proton,
        Some(ProtonSection {
            build: Some("experimental".into())
        })
    );
    assert_eq!(profile.steam, Some(SteamIntegration::Auto));
    assert!(!profile.proton.as_ref().unwrap().build_is_path());

    let text = toml::to_string(&profile).unwrap();
    let back: liblootrs::model::Profile = toml::from_str(&text).unwrap();
    assert_eq!(back, profile);

    let cachyos = store::load_profile(&fixtures().join("proton-cachyos.toml")).unwrap();
    assert!(cachyos.proton.unwrap().build_is_path());
    assert_eq!(cachyos.steam, None);
}
