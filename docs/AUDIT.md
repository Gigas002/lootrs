# Audit

A whole-application audit of liblootrs and lootrs (2026-11-22) in six parts:
security (§2), bugs (§3), performance (§4), usability (§5), code quality,
tests and project hygiene (§6), and what was left open (§7). §8 is the
checklist to re-run it.

Each finding says what was found, how serious it is, and what was done:
**fixed** (with the commit), **accepted** (with the reason), or **OK** (checked,
nothing to do). Every fix is its own commit with a regression test. Each test
failed on the code before its fix, and every finding marked fixed was also
reproduced with the real binary first.

Severity: **high** (code execution, or lootrs writing outside its own
directories and the user's prefixes), **medium** (a crash, hang, lost data or
wrong state in normal use, behaviour that contradicts the docs), **low**
(hardening, polish, or needs an unusual setup).

## 1. Method

Every source file of both crates was read in full. Findings were then
reproduced against a debug build of master (f678549) in throwaway homes under a
scratch directory, never the user's own config, prefixes or games:

- **Fake games**: shell scripts as native games, to drive `run`, `--wait`,
  `status`, `stop`, the detached supervisor and run logs, including several
  launches at once.
- **Real wine**: `init` created a fresh prefix with the system wine 11.22,
  `execute` ran `cmd /c echo` in it, and a detached `run` of `cmd.exe /c ping`
  went through `status` and `stop`. No display was available to these
  processes, and `mscoree`/`mshtml` were disabled, so nothing could open a
  window.
- **The TUI** ran in a detached tmux session: help, run, the running-state
  column, a refused second run, the log pane, stop, the profile form (with an
  invalid name, then a valid one), and quitting with a game running.
- **Hostile files**: a planted `<prefix>/.lootrs.toml`, typos in
  `config.toml` and `games.toml`.
- **Static checks**: the gates, `cargo deny check`, `typos`,
  `cargo llvm-cov`, and `cargo clippy -W clippy::pedantic` (read for real
  problems, not applied wholesale).

umu was not run live: its first run downloads the Steam Linux Runtime. Its
command building is covered by the unit tests and the fake `umu-run` in
`tests/cli.rs`.

## 2. Security

lootrs runs what the user's own files name: games, runners, wrappers, Proton
builds. Those files are trusted. Untrusted input is what other programs can
write: the prefix (anything running in it can write there, Windows programs
included), `/proc` entries of the user's other processes, and runner output.

| Area                    | Finding                                                                                                                                                                                                                                                                                                                                                                              | Status                  |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------- |
| Prefix state file       | When DXVK/VKD3D was removed, lootrs deleted the symlinks listed in `<prefix>/.lootrs.toml` and renamed their `.orig` files back, for any path. A planted `links = ["../outside/victim"]` made `lootrs init` delete a symlink and overwrite a file outside the prefix. Only `drive_c/windows/{system32,syswow64}/<name>.dll` entries are acted on now; others are logged and skipped. | Fixed, medium (22338d3) |
| Run log permissions     | Run logs hold the environment lootrs set, `env` tokens included (USAGE.md says so), yet were `0644`. Logs are `0600` and runner-log directories `0700` now.                                                                                                                                                                                                                          | Fixed, low (fc7a581)    |
| Lock files              | Created with the default mode; now `0600`.                                                                                                                                                                                                                                                                                                                                           | Fixed, low (6a2a9b7)    |
| Names in paths          | Game and profile names become log, lock and shader-cache directories. Profile names are checked by libgameprofiles, game names by `Game::validate`, and both checks run before any launch, `init` or save; the TUI refuses `../evil` with a message.                                                                                                                                 | OK                      |
| Process control         | `stop`, `killall` and the prefix-in-use check only consider the user's own processes (uid of `/proc/<pid>`), selected by the `LOOTRS_*` tags in their environment.                                                                                                                                                                                                                   | OK                      |
| Tags are inherited      | Everything a game starts inherits its tag: a browser opened by a launcher counts as part of the game, `status` shows it, and `stop` ends it. This is how wine's and Proton's helpers are found at all; documented in USAGE.md.                                                                                                                                                       | Accepted (docs)         |
| `unsafe`, shell         | No `unsafe` in either crate. No shell is involved: wrappers are split on whitespace and run directly; the pager is `$PAGER` split the same way.                                                                                                                                                                                                                                      | OK                      |
| Secrets in debug output | `--debug` doesn't set `UMU_LOG=debug`, which would dump the whole environment.                                                                                                                                                                                                                                                                                                       | OK                      |
| Dependencies            | `cargo deny check` failed: RustSec marks `chrono` unmaintained. Replaced with `jiff` (§6). Advisories, bans, licenses and sources pass.                                                                                                                                                                                                                                              | Fixed (13896d6)         |

## 3. Bugs

| Area                      | Finding                                                                                                                                                                                                                                                                                                                                         | Severity | Status          |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- | --------------- |
| Game started twice        | "Already running" was checked, then the game spawned later (after prefix setup), so launches at the same moment all passed: 4 parallel `lootrs run race` started 4 copies. A per-game lock is now held from the check until the game's first process exists; the test sees exactly one start and three exit-code-4 refusals.                    | medium   | Fixed (86e3cfa) |
| Runs starting together    | `latest.log` was repointed through a temporary link with a fixed name, so concurrent runs removed each other's: 2 of 6 parallel runs failed with "failed to write the run log". Two `execute` sessions of one profile could hit it in normal use. The temporary name is unique per process and call.                                            | medium   | Fixed (483769e) |
| Retention order           | Runs in the same second are `<stamp>.log`, `<stamp>-2.log`, …; sorted by name, `-2` came before the plain name and `-10` before `-2`, so pruning deleted newer logs (3 runs, `log_keep = 2`: the second run's log went). A freed name was also reused, making a new log look old. Logs sort by stamp then number, and numbers keep counting up. | low      | Fixed (f83acd2) |
| Duplicate game names      | `check` reported them, but `run` started the first match without a word. `validate_game`, used by `run` and `check <game>`, reports them now.                                                                                                                                                                                                   | low      | Fixed (0f1be91) |
| Lock names between builds | Prefix locks were named with `DefaultHasher`, whose output may change between Rust releases, so two lootrs builds (a TUI and a CLI build, say) could lock the same prefix under different names. Now FNV-1a over the normalized path, pinned by a test.                                                                                         | low      | Fixed (6a2a9b7) |
| Run duration              | Taken from wall-clock time, so a clock change during a run gave a wrong or zero duration. Now measured with a monotonic clock.                                                                                                                                                                                                                  | low      | Fixed (13896d6) |
| Real wine                 | Prefix creation (11.6 s for a fresh prefix, wine 11.22), `execute` (2.6 s for `cmd /c echo`), detached `run`, `status`, `stop` (0.13 s; the prefix's leftover helpers went too, no wine process left).                                                                                                                                          | —        | OK              |
| Concurrent prefix setup   | Setup holds an exclusive per-prefix lock; links aren't changed while a game runs in the prefix (exit 4).                                                                                                                                                                                                                                        | —        | OK              |

## 4. Performance

Release build, 30 runs each, median (max), 358 processes on the machine:

| Command       | Time         |
| ------------- | ------------ |
| `--help`      | 0.8 ms (1.1) |
| `list`        | 2.0 ms (2.3) |
| `profiles`    | 2.0 ms (4.2) |
| `check`       | 2.1 ms (2.6) |
| `status`      | 4.6 ms (5.4) |
| `list --json` | 4.6 ms (4.9) |

`status` and `list --json` read every process's environment from `/proc`;
that is the whole difference. A detached `run` returns in 0.03 s, `stop` in
0.13 s. The release binary is 3.7 MiB. Nothing here needed work.

## 5. Usability

| Area           | Finding                                                                                                                                                                                                                                                        | Status                     |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| Typos ignored  | Unknown keys in `config.toml` (`log_kep = 2`) and `games.toml` (`argz = [...]`) were dropped silently, so the setting quietly had no effect. Every command now warns about `config.toml` keys; `check` and launches warn about a game's keys, as for profiles. | Fixed (0f1be91, 0a7f8bac0) |
| Error messages | Validation errors name the file, the game or profile, and the field; launch failures list every problem at once.                                                                                                                                               | OK                         |
| TUI            | Help, run, running state, refusals, log pane, forms with validation and quitting all behave as TUI.md describes.                                                                                                                                               | OK                         |

## 6. Code quality, tests and project hygiene

| Area         | Finding                                                                                                                                                     | Status          |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------- |
| Dependencies | `chrono` → `jiff` (active, MIT/Unlicense, already used in abar).                                                                                            | Fixed (13896d6) |
| Manifests    | Cargo warned that `homepage` repeated `repository`; removed. The build is warning-free.                                                                     | Fixed (c225536) |
| Tests        | 241 → 254 with all features: launch race, concurrent log creation, retention order, file modes, state-file escapes, lock names, unknown keys, time formats. | Fixed           |
| Coverage     | 90.0% lines. Lowest: the TUI's terminal loop and widgets (`ui/mod.rs` 58%, `ui/prompt.rs` 0%), exercised by hand in tmux (§1).                              | OK              |
| Pedantic     | `clippy::pedantic` reports `must_use`, `# Errors` docs and style lints only; no bugs among them. Not applied.                                               | Accepted        |
| Gates        | fmt, typos, clippy `-D warnings` and tests at three feature levels, docs, `cargo deny`: all pass, on every audit commit separately (§8).                    | OK              |

## 7. Left open

- **Launch overhead from `--version`**: each run log header runs the runner
  with `--version`, without a timeout. A runner that hangs on it would hang
  the launch. Real wine and umu answer at once; accepted.
- **Prefix lock released before spawn**: setup and the in-use check are
  locked, but the lock is dropped before the game spawns, so a second game
  setting up the same prefix in that gap could relink DXVK just before the
  first starts. It needs two different games of one prefix launched within
  milliseconds; accepted.
- **umu live**: not run (it would download the Steam Linux Runtime); see §1.
- **Release**: nothing was pushed or tagged.

## 8. Re-running the audit

```sh
# Gates (docs/ARCHITECTURE.md §8)
cargo fmt --all -- --check
typos
cargo deny check
for f in --no-default-features "" --all-features; do
  cargo clippy --workspace --all-targets $f -- -D warnings
  cargo test --workspace $f
done
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# Coverage (system LLVM tools on Arch)
LLVM_COV=$(command -v llvm-cov) LLVM_PROFDATA=$(command -v llvm-profdata) \
  cargo llvm-cov --workspace --all-features --summary-only
```

Each audit commit on its own (from a scratch worktree):

```sh
for c in $(git rev-list --reverse f678549..HEAD); do
  git checkout -q "$c" && cargo fmt --all -- --check &&
    cargo clippy --workspace --all-targets --all-features -- -D warnings &&
    cargo test --workspace --all-features || echo "FAILED: $c"
done
```

Live checks use a throwaway `HOME` (and `XDG_*` unset) so nothing touches the
real configuration; for wine, also unset `DISPLAY`/`WAYLAND_DISPLAY` and set
`WINEDLLOVERRIDES="mscoree,mshtml="`.
