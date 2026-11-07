# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- The profile schema has a `proton` runner (Steam's Proton) with `[proton].build` and `steam` (`auto`/`host`/`none`), for `liblootrs` clients that launch through Steam. lootrs keeps these fields but can't launch a `proton` profile; `check` and launches report it, and the TUI refuses to edit it.
