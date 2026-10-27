# lootrs

Minimal, lightweight TUI and CLI for running Windows games on Linux through wine, Proton (via [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher)) or natively. It covers a small part of what Lutris does: a games list, runner profiles, prefix setup, winetricks, logs and process control.

## Requirements

lootrs only runs what you installed. Depending on the runners you use:

| Runner   | You install                                                          | Notes                                                                                                                                                                                                                                                          |
| -------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `wine`   | a 64-bit wine build                                                  | From `PATH`, or `[wine].path` (a binary or a build directory).                                                                                                                                                                                                 |
|          | DXVK / VKD3D-Proton (optional)                                       | Release directories with `x64/` (and `x32/`/`x86/`); `dxvk`/`vkd3d` in the profile. lootrs symlinks them into the prefix and keeps wine's DLLs as `.orig`.                                                                                                     |
|          | winetricks (optional)                                                | From `PATH`, for `lootrs tricks`.                                                                                                                                                                                                                              |
| `umu`    | [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher) | `umu-run` from `PATH`, or `[umu].umu_run`.                                                                                                                                                                                                                     |
|          | a Proton build                                                       | `[umu].proton` must be its directory: GE-Proton in `~/.local/share/Steam/compatibilitytools.d/`, or a distro package such as `/usr/share/steam/compatibilitytools.d/proton-cachyos-native`. lootrs never passes a version name, so umu never downloads Proton. |
|          | the Steam Linux Runtime                                              | umu fetches it on its first run. After that lootrs sets `UMU_RUNTIME_UPDATE=0` unless `umu_runtime_update = true` in `config.toml`. `umu-run winetricks` uses the winetricks bundled with Proton.                                                              |
| `native` | nothing                                                              | The executable runs as is.                                                                                                                                                                                                                                     |

Anti-cheat runtimes (Proton BattlEye / EasyAntiCheat Runtime) are optional; set their directories in `config.toml` and enable them per profile.

Run `lootrs check` after editing files: it validates games and profiles and checks that runners, Proton builds, wrappers and anti-cheat runtimes exist.

## Files

| What                      | Default location                                               |
| ------------------------- | -------------------------------------------------------------- |
| Settings, games, profiles | `$XDG_CONFIG_HOME/lootrs/` (`~/.config/lootrs/`)               |
| Logs                      | `$XDG_STATE_HOME/lootrs/logs/` (`~/.local/state/lootrs/logs/`) |

See [`examples/config.toml`](examples/config.toml) for every setting, and [`examples/games.toml`](examples/games.toml) and [`examples/profiles/`](examples/profiles/) for annotated games and profiles. Profiles are named after their file: `profiles/default.toml` is profile `default`.

## Usage

```sh
# start TUI
lootrs
# or use as cli
lootrs run <game> [--wait] [-- args...]
lootrs stop <game>
lootrs killall [--everything]
lootrs init --profile <p>
lootrs tricks --profile <p> [-- verbs...]
lootrs execute --profile <p> <path> [--wait] [-- args...]
lootrs list | profiles | status [--json]
lootrs check [<game> | --profile <p>]
lootrs logs <game> [--path]
```

See [`docs/USAGE.md`](docs/USAGE.md) for command details, wrappers and logs, and [`docs/TUI.md`](docs/TUI.md) for the TUI.
