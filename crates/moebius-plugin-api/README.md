# moebius-plugin-api

The API for Moebius plugins:

- the `Plugin` trait;
- the types of a call and of a decision, and the JSON Lines `Envelope` a plugin in its own process speaks;
- the `Host`, which calls plugins with a time limit and fails closed;
- the launcher kind: the request, answer and leftovers types of a plugin that wraps how a harness process starts;
- `Process`, a plugin in its own program, started again with a pause when it stops, and `serve`, which runs a `Plugin`
  as such a program.

`src/lib.rs` holds the trait, the types and the host; `src/launcher.rs` the launcher kind (a plugin that wraps how a
harness process starts) and its messages; `src/process/` the plugin process (`mod.rs`), the thread that
looks after it (`supervise.rs`) and the plugin's side of the protocol (`serve.rs`). `testdata/plugin.py` is a plugin
process for the tests that behaves as its first argument says.

A decider plugin is compiled into the `moebius` binary, or named in `moebius.toml` as a program. Its contract is in
[`docs/reference/plugin-api.md`](../../docs/reference/plugin-api.md), and [the guide](../../docs/guide/plugins.md) shows
how to write one.

`examples/allow_reads.rs` is a decider that allows the tools that only read; `examples/allow_reads_process.rs` is the
same decider in its own process. CI runs both:

```sh
cargo run -p moebius-plugin-api --example allow_reads
cargo run -p moebius-plugin-api --example allow_reads_process
```

A launcher plugin wraps how a harness process is started, such as in a sandbox. `examples/wrap_launcher.rs` is one, and
`examples/wrap_launcher_process.rs` the same in its own process; CI runs both:

```sh
cargo run -p moebius-plugin-api --example wrap_launcher
cargo run -p moebius-plugin-api --example wrap_launcher_process
```
