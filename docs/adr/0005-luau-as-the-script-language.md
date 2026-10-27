# Luau is the script language

Scripts behind columns are written in Luau, run inside the Moebius process. Luau is a Lua dialect with optional types, a
sandbox built in and a fast interpreter in a small runtime, and its Lua syntax is familiar from Neovim and Redis. Scripts reach
the world only through a small Moebius API (start a run, move a card, comment, wait, check a limit); they may run shell
commands only where the configuration explicitly allows it.

## Considered Options

- Lua or LuaJIT: faster still, but its sandbox would have to be built and maintained by hand.
- Rhai: written in Rust and safe by design, but markedly slower and little known.
- JavaScript on QuickJS: familiar to Pi users, but slower than Luau with a larger runtime.
- Scripts as process plugins: any language, but a process per event is too heavy for behaviour that runs on every move.
