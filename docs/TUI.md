# TUI

Run `lootrs` without a subcommand. Two tabs, games and profiles; `?` lists every key.

- Games: `⏎` run, `D` run with debug logging, `s` stop, `l` log pane, `L` open the log in `$PAGER`.
- Profiles: `i` create/update the prefix, `t` winetricks, `x` execute a program.
- Both: `a` add, `e` edit, `d` delete, `K` kill everything lootrs started, `R` reload files, `q` quit (running games keep running).

Games and profiles can be edited in the TUI forms or by hand in the TOML files. Hand edits are picked up on restart (or `R`). A TUI save rewrites the whole file, so comments in it are lost; annotated samples live in [`examples/`](../examples/). If a file changed on disk since lootrs loaded it, the TUI refuses to overwrite it.
