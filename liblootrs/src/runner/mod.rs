//! Runner-specific parts of a launch: the runner binary, its arguments and env.

use std::{collections::BTreeMap, ffi::OsString, path::Path, path::PathBuf};

use crate::{
    launch::LaunchContext,
    model::{Profile, Runner},
};

mod native;
#[cfg(feature = "wine")]
pub mod wine;

/// What a runner contributes to a launch.
#[derive(Debug, Default)]
pub struct RunnerCommand {
    /// Runner binary placed before the executable; `None` runs it directly.
    pub program: Option<PathBuf>,
    pub args: Vec<OsString>,
    pub env: BTreeMap<String, OsString>,
}

pub fn command(
    profile: &Profile,
    prefix: Option<&Path>,
    debug: bool,
    ctx: &LaunchContext,
) -> Result<RunnerCommand, Vec<String>> {
    match profile.runner {
        Runner::Native => Ok(native::command()),
        #[cfg(feature = "wine")]
        Runner::Wine => wine::command(profile, prefix, debug, ctx),
        #[cfg(feature = "umu")]
        Runner::Umu => Err(vec![
            "runner `umu` is not supported yet (planned for M5)".into(),
        ]),
        #[allow(unreachable_patterns)]
        runner => {
            let _ = (prefix, debug, ctx);
            Err(vec![format!(
                "runner `{runner}` is not available: lootrs was built without the `{runner}` feature"
            )])
        }
    }
}
