//! Games and profiles as stored in `games.toml` and `profiles/*.toml`, plus validation.
//!
//! Validation never fails fast: it returns every [`Issue`] it finds, so `check`
//! and the TUI can show them all at once.

use std::path::Path;

mod game;

pub use game::Game;
pub use libgameprofiles::{
    Anticheat, Issue, Issues, Profile, ProtonSection, Runner, Severity, ShaderCache, SlrSection,
    SteamIntegration, Subject, UmuSection, WineSection, validate_name,
};

#[cfg(test)]
mod tests;

/// Values from the global settings that validation depends on.
#[derive(Debug, Clone, Copy)]
pub struct ValidationContext<'a> {
    /// Used to expand `~` in paths.
    pub home: &'a Path,
    /// Whether `battleye_runtime` is set in `config.toml`.
    pub battleye_runtime: bool,
    /// Whether `eac_runtime` is set in `config.toml`.
    pub eac_runtime: bool,
}

/// Whether this build of lootrs can launch `runner` (Cargo features `wine`
/// and `umu`). `proton` never is: it runs through Steam.
pub fn runner_available(runner: Runner) -> bool {
    match runner {
        Runner::Wine => cfg!(feature = "wine"),
        Runner::Umu => cfg!(feature = "umu"),
        Runner::Native => true,
        Runner::Proton => false,
    }
}

/// What's wrong with `profile` for a lootrs launch: the file's own issues
/// ([`Profile::validate`]) plus what lootrs needs to run it.
pub fn validate_profile(profile: &Profile, ctx: &ValidationContext) -> Vec<Issue> {
    let mut issues = Issues::new(Subject::Profile(profile.name.clone()));
    issues.extend(profile.validate(ctx.home));
    let runner = profile.runner;

    if runner == Runner::Proton {
        issues.error(crate::launch::PROTON_UNSUPPORTED);
        return issues.finish();
    }
    if !runner_available(runner) {
        issues.error(format!(
            "runner `{runner}` is not available: lootrs was built without the `{runner}` feature"
        ));
    }
    if runner == Runner::Native && profile.slr.is_some() {
        issues.error(SLR_UNSUPPORTED);
    }
    if runner.is_windows() && profile.prefix.is_none() {
        issues.error(format!("`prefix` is required for runner `{runner}`"));
    }
    if runner == Runner::Umu && profile.umu.as_ref().is_none_or(|umu| umu.proton.is_none()) {
        issues.error(PROTON_REQUIRED);
    }

    let anticheat = profile.anticheat;
    if runner.is_windows() {
        if anticheat.battleye && !ctx.battleye_runtime {
            issues.error("`anticheat.battleye` needs `battleye_runtime` in config.toml");
        }
        if anticheat.eac && !ctx.eac_runtime {
            issues.error("`anticheat.eac` needs `eac_runtime` in config.toml");
        }
        if runner == Runner::Wine && !anticheat.is_off() {
            issues.warn("anti-cheat runtimes only work with Proton-based wine builds");
        }
    }
    issues.finish()
}

const PROTON_REQUIRED: &str = "`[umu].proton` is required: an absolute path to a Proton build \
     (without it umu would download Proton)";

/// lootrs runs `native` games directly, so it can't keep what `[slr]` asks
/// for (a runtime container, a private home).
pub(crate) const SLR_UNSUPPORTED: &str =
    "`[slr]` profiles run in a Steam Linux Runtime: use sruman; lootrs can't launch them";
