# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- `shader_cache` in a profile: `"off"`, or a directory that gets one folder per game for its compiled shaders. lootrs points DXVK, VKD3D-Proton, Mesa, NVIDIA and AMDVLK there (the layout Proton uses for its local shader cache) and creates the folders; a variable in the profile's or game's `env` still wins. Unset sets nothing, as before. `ShaderCache::env` gives clients the same variable list.
- The profile schema has a `proton` runner (Steam's Proton) with `[proton].build` and `steam` (`auto`/`host`/`none`), for `liblootrs` clients that launch through Steam. lootrs keeps these fields but can't launch a `proton` profile; `check` and launches report it, and the TUI refuses to edit it.
