# Usage

## Commands

`run` and `execute` detach by default: the game keeps running after the terminal closes. `--wait` stays attached, shows the output and exits with the game's exit code. `--debug` turns on verbose wine/Proton/DXVK logging for that run.

Exit codes: `0` ok, `1` runtime failure, `2` invalid input or configuration, `3` not running (`stop`), `4` already running or prefix in use.

`--json` prints machine-readable output: `list` has every game field plus `runner` and `running`, `profiles` has each profile file's settings plus the `games` using it, and `status` lists `{name, kind, pids, commands}` for each running game (`kind = "game"`) or `execute`/`tricks`/`init` session (`kind = "session"`).

## Wrappers: MangoHud, GameMode and friends

`wrappers` in a profile go in front of the whole command, for wine, umu and native alike. Wrapping `umu-run` is the same as Steam's `mangohud %command%`. Each entry is split on whitespace.

```toml
wrappers = ["mangohud"]                      # Vulkan (DXVK/VKD3D) games
wrappers = ["mangohud --dlsym"]              # OpenGL games
wrappers = ["gamemoderun", "mangohud"]       # both
wrappers = ["obs-gamecapture"]               # OBS game capture
wrappers = ["prime-run"]                     # discrete GPU on hybrid laptops
```

Without a wrapper, MangoHud's Vulkan layer can be enabled through the environment, per profile or per game:

```toml
env = { MANGOHUD = "1", MANGOHUD_CONFIGFILE = "/home/me/.config/MangoHud/wow.conf" }
```

## Logs

| Log                             | Where (under `~/.local/state/lootrs/logs/`)                   |
| ------------------------------- | ------------------------------------------------------------- |
| lootrs itself                   | `lootrs.<date>.log`                                           |
| one per game run                | `games/<game>/<timestamp>.log`, `latest.log` links the newest |
| `execute`, `tricks`, `init`     | `execute/<profile>/`, `tricks/<profile>/`, `init/<profile>/`  |
| Proton/DXVK/VKD3D logs of a run | a directory next to that run's log                            |

Each run log starts with the command line, the environment lootrs set, runner and Proton versions, prefix setup and system details (kernel, distro, ntsync, Vulkan drivers), then the timestamped output and the exit status. The last `log_keep` (default 10) are kept per game or profile. `lootrs logs <game>` prints the latest one.

Values are not redacted: if you put tokens in a profile's `env`, they end up in the log, so check before sharing one. lootrs doesn't set `UMU_LOG=debug` for `--debug`, because umu then dumps your whole environment into the output.
