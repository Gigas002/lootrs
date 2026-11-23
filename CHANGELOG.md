# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- `umu_folders` in `config.toml`: umu's folders (`UMU_FOLDERS_PATH`), so umu
  profiles run on a Steam Linux Runtime installed elsewhere (sruman's, say)
  instead of the one umu downloads.

### Changed

- libgameprofiles comes from crates.io (`0.1`) instead of its git master
  branch; dependencies updated.
- The profile model, its validation and profile loading and saving moved to
  the [libgameprofiles](https://github.com/Gigas002/libgameprofiles) crate,
  shared with gaki and sruman. `liblootrs::model` re-exports its types;
  `Profile::validate` is now libgameprofiles' file check, and lootrs' launch
  checks are `model::validate_profile`. `Runner::is_available` is
  `model::runner_available`; `store::StoreError` is libgameprofiles'
  `store::Error`.
- `lootrs check` warns about unknown keys in profile files, and about
  `[proton]` and `steam` on runners other than `proton` (they were kept
  silently).
- Profiles with `runner = "slr"` (sruman's Steam Linux Runtime) load but are
  refused at launch and in `init`, and the TUI won't edit them, like `proton`
  profiles. An `[slr]` section on any other runner is ignored with a warning.
- `native` games no longer get DXVK and VKD3D-Proton `shader_cache`
  variables.
- A `prefix` that is or contains the home directory is an error, as are NUL
  characters in arguments, environment values and wrappers.
- Saving a profile writes through a symlinked profile file instead of
  replacing the link, and keeps the file's permissions.
- `config.toml` and `games.toml` keys lootrs doesn't know are reported:
  every command warns about `config.toml` ones, and `check` and launches
  about a game's. They used to be ignored silently.
- Run logs are `0600` and their runner-log directories `0700`: they hold the
  environment lootrs set.
- `chrono` was replaced with `jiff` (RustSec lists `chrono` as unmaintained).

### Fixed

- Launching one game several times at once started it several times; the
  launches after the first now fail with exit code 4.
- Runs starting at the same moment could fail with "failed to write the run
  log".
- With several runs in the same second, log retention deleted newer logs
  before older ones.
- Removing DXVK/VKD3D from a prefix trusted the paths in
  `<prefix>/.lootrs.toml`, so an edited state file could make lootrs delete
  or rename files outside the prefix.
- `run` started the first of several games with the same name; it reports
  the duplicate now.
- Prefix lock files had names that could differ between lootrs builds.
- Run durations were wrong when the clock changed during a run.

## [0.1.0] - 2026-11-09

- Initial release
