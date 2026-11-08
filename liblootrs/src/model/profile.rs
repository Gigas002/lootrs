use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{Issue, Issues, Subject, ValidationContext};

/// A runner profile from `profiles/<name>.toml`.
///
/// Settings that don't apply to the selected runner are kept but ignored,
/// so switching `runner` back and forth never loses data.
///
/// `runner = "proton"`, `[proton]` and `steam` describe a launch through
/// Steam's Proton, for clients that start games through Steam. They are part
/// of the schema so such profiles load, but lootrs itself can't launch a
/// `proton` profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Must equal the file stem.
    pub name: String,
    pub runner: Runner,
    /// Wine prefix; required for wine and umu. For `proton` it is the
    /// `STEAM_COMPAT_DATA_PATH`, and optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<PathBuf>,
    /// Inserted between the runner binary and the executable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runner_args: Vec<String>,
    /// Prepended to the whole command, e.g. `["mangohud"]`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrappers: Vec<String>,
    /// Verbose runner logging.
    #[serde(default, skip_serializing_if = "is_false")]
    pub debug: bool,
    /// Wine only: DXVK directory with `x64/` (and optionally `x32/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dxvk: Option<PathBuf>,
    /// Wine only: VKD3D-Proton directory with `x64/` (and optionally `x86/`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vkd3d: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Merged into `WINEDLLOVERRIDES`, e.g. `{ winhttp = "n,b" }`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dll_overrides: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wine: Option<WineSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub umu: Option<UmuSection>,
    /// The `proton` runner's build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<ProtonSection>,
    /// The `proton` runner's Steam integration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam: Option<SteamIntegration>,
    #[serde(default, skip_serializing_if = "Anticheat::is_off")]
    pub anticheat: Anticheat,
    /// Where each game's compiled shaders go. Unset leaves it to the client
    /// (lootrs sets nothing then).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shader_cache: Option<ShaderCache>,
}

/// `shader_cache`: `"off"`, or a directory that holds one folder per game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShaderCache {
    /// Set nothing: DXVK, VKD3D-Proton and the drivers use their own
    /// defaults.
    Off,
    /// Each game's caches go in `<dir>/<game>`.
    Dir(PathBuf),
}

impl ShaderCache {
    pub fn parse(text: &str) -> Self {
        if text.trim().eq_ignore_ascii_case("off") {
            Self::Off
        } else {
            Self::Dir(PathBuf::from(text))
        }
    }

    pub fn as_text(&self) -> String {
        match self {
            Self::Off => "off".to_owned(),
            Self::Dir(dir) => dir.display().to_string(),
        }
    }

    /// The variables that put one game's shader caches in `dir`, with the
    /// layout Proton uses for its local shader cache: DXVK, VKD3D-Proton,
    /// Mesa, NVIDIA, and AMD's own Vulkan driver.
    pub fn env(dir: &Path) -> Vec<(&'static str, PathBuf)> {
        vec![
            ("DXVK_STATE_CACHE_PATH", dir.join("DXVK_state_cache")),
            ("VKD3D_SHADER_CACHE_PATH", dir.join("VKD3D_shader_cache")),
            ("MESA_SHADER_CACHE_DIR", dir.to_path_buf()),
            ("__GL_SHADER_DISK_CACHE_PATH", dir.join("nvidiav1")),
            ("AMD_VK_PIPELINE_CACHE_PATH", dir.join("AMDv1")),
        ]
    }

    /// Create the directories [`Self::env`] names: not every driver creates
    /// its own.
    pub fn create_dirs(dir: &Path) -> std::io::Result<()> {
        for (_, path) in Self::env(dir) {
            std::fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

impl Serialize for ShaderCache {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_text())
    }
}

impl<'de> Deserialize<'de> for ShaderCache {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.trim().is_empty() {
            return Err(serde::de::Error::custom(
                "`shader_cache` is `off` or a directory",
            ));
        }
        Ok(Self::parse(&text))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runner {
    Wine,
    Umu,
    /// Run the executable as is.
    Native,
    /// Steam's Proton (`proton run`), launched through Steam; lootrs can't
    /// launch it.
    Proton,
}

/// `[wine]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WineSection {
    /// Wine binary, or a wine build directory containing `bin/wine`.
    /// Default: `wine` from `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// `[umu]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UmuSection {
    /// Default: `umu-run` from `PATH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub umu_run: Option<PathBuf>,
    /// Proton build directory, passed as `PROTONPATH`. Required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proton: Option<PathBuf>,
    /// Default `GAMEID`; a game's own value wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,
    /// `STORE` for protonfixes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
}

/// `[proton]` section, for the `proton` runner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtonSection {
    /// A Steam-installed build name (`experimental`, `9.0`, ...) or the path
    /// of a Proton directory. `None` is the default build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

impl ProtonSection {
    /// Whether `build` names a directory rather than a Steam build.
    pub fn build_is_path(&self) -> bool {
        self.build
            .as_deref()
            .is_some_and(|build| build.starts_with(['/', '~']) || build.contains('/'))
    }
}

/// `steam`: whether a Proton launch talks to the host Steam client.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SteamIntegration {
    /// Bridge to the host Steam client when it is running.
    #[default]
    Auto,
    /// Always bridge; start the host Steam client when needed. Needed for
    /// Steamworks DRM and Family-Shared games.
    Host,
    /// Never bridge: the game sees no Steam client.
    None,
}

impl SteamIntegration {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Host => "host",
            Self::None => "none",
        }
    }
}

impl fmt::Display for SteamIntegration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `[anticheat]` section; both off by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anticheat {
    #[serde(default)]
    pub battleye: bool,
    #[serde(default)]
    pub eac: bool,
}

impl Anticheat {
    pub fn is_off(&self) -> bool {
        !self.battleye && !self.eac
    }
}

impl Runner {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wine => "wine",
            Self::Umu => "umu",
            Self::Native => "native",
            Self::Proton => "proton",
        }
    }

    /// Whether this build of lootrs can launch the runner (Cargo features
    /// `wine`/`umu`). `proton` never is: it runs through Steam.
    pub fn is_available(self) -> bool {
        match self {
            Self::Wine => cfg!(feature = "wine"),
            Self::Umu => cfg!(feature = "umu"),
            Self::Native => true,
            Self::Proton => false,
        }
    }

    fn uses_prefix(self) -> bool {
        matches!(self, Self::Wine | Self::Umu)
    }
}

impl fmt::Display for Runner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Profile {
    /// A profile with only `name` and `runner` set.
    pub fn new(name: impl Into<String>, runner: Runner) -> Self {
        Self {
            name: name.into(),
            runner,
            prefix: None,
            runner_args: Vec::new(),
            wrappers: Vec::new(),
            debug: false,
            dxvk: None,
            vkd3d: None,
            env: BTreeMap::new(),
            dll_overrides: BTreeMap::new(),
            wine: None,
            umu: None,
            proton: None,
            steam: None,
            anticheat: Anticheat::default(),
            shader_cache: None,
        }
    }

    pub fn validate(&self, ctx: &ValidationContext) -> Vec<Issue> {
        let mut issues = Issues::new(Subject::Profile(self.name.clone()));
        let runner = self.runner;
        let ignored = |field: &str| format!("`{field}` is ignored for runner `{runner}`");
        issues.name(&self.name);

        if runner == Runner::Proton {
            issues.error(crate::launch::PROTON_UNSUPPORTED);
            // The other fields mean what the Steam launch makes of them.
            return issues.finish();
        }

        if !runner.is_available() {
            issues.error(format!(
                "runner `{runner}` is not available: lootrs was built without the `{runner}` feature"
            ));
        }

        if let Some(ShaderCache::Dir(dir)) = &self.shader_cache {
            issues.absolute("shader_cache", dir, ctx.home);
        }

        match (&self.prefix, runner.uses_prefix()) {
            (Some(prefix), true) => {
                issues.absolute("prefix", prefix, ctx.home);
            }
            (None, true) => issues.error(format!("`prefix` is required for runner `{runner}`")),
            (Some(_), false) => issues.warn(ignored("prefix")),
            (None, false) => {}
        }

        for (field, path) in [("dxvk", &self.dxvk), ("vkd3d", &self.vkd3d)] {
            let Some(path) = path else { continue };
            match runner {
                Runner::Wine => {
                    issues.absolute(field, path, ctx.home);
                }
                Runner::Umu => issues.warn(format!(
                    "{}: Proton ships its own; point `[umu].proton` at another Proton build instead",
                    ignored(field)
                )),
                Runner::Native | Runner::Proton => issues.warn(ignored(field)),
            }
        }

        if let Some(wine) = &self.wine {
            if runner != Runner::Wine {
                issues.warn(ignored("[wine]"));
            } else if let Some(path) = &wine.path {
                issues.absolute("wine.path", path, ctx.home);
            }
        }

        match (&self.umu, runner) {
            (Some(umu), Runner::Umu) => {
                if let Some(path) = &umu.umu_run {
                    issues.absolute("umu.umu_run", path, ctx.home);
                }
                match &umu.proton {
                    Some(proton) => {
                        issues.absolute("umu.proton", proton, ctx.home);
                    }
                    None => issues.error(PROTON_REQUIRED),
                }
            }
            (None, Runner::Umu) => issues.error(PROTON_REQUIRED),
            (Some(_), _) => issues.warn(ignored("[umu]")),
            (None, _) => {}
        }

        if runner == Runner::Native && !self.runner_args.is_empty() {
            issues.warn(ignored("runner_args"));
        }

        if runner == Runner::Native {
            if !self.dll_overrides.is_empty() {
                issues.warn(ignored("dll_overrides"));
            }
        } else {
            for (dll, mode) in &self.dll_overrides {
                if let Err(message) = validate_dll_override(dll, mode) {
                    issues.error(message);
                }
            }
        }

        let anticheat = self.anticheat;
        if runner == Runner::Native {
            if !anticheat.is_off() {
                issues.warn(ignored("[anticheat]"));
            }
        } else {
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

        if self
            .wrappers
            .iter()
            .any(|wrapper| wrapper.trim().is_empty())
        {
            issues.error("`wrappers` must not contain empty entries");
        }

        issues.env(&self.env);
        issues.finish()
    }
}

const PROTON_REQUIRED: &str = "`[umu].proton` is required: an absolute path to a Proton build \
     (without it umu would download Proton)";

/// Accepts wine's override modes: comma-separated `n`/`b`/`native`/`builtin`, or empty to disable.
fn validate_dll_override(dll: &str, mode: &str) -> Result<(), String> {
    if dll.is_empty() || dll.contains(['=', ',', ';', ' ']) {
        return Err(format!(
            "`dll_overrides` key `{dll}` is not a valid DLL name"
        ));
    }
    if mode.is_empty() {
        return Ok(());
    }
    if mode
        .split(',')
        .all(|part| matches!(part, "n" | "b" | "native" | "builtin"))
    {
        Ok(())
    } else {
        Err(format!(
            "`dll_overrides.{dll}` = `{mode}`: use `n`, `b`, `n,b`, `b,n` or `` (disabled)"
        ))
    }
}

fn is_false(value: &bool) -> bool {
    !value
}
