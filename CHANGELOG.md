# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Changed

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

## [0.1.0] - 2026-11-09

- Initial release
