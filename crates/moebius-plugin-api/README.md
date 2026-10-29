# moebius-plugin-api

The API for Moebius plugins:

- the `Plugin` trait;
- the types of a call and of a decision;
- the `Host`, which calls plugins with a time limit and fails closed.

A decider plugin is compiled into the `moebius` binary. Its contract is in
[`docs/reference/plugin-api.md`](../../docs/reference/plugin-api.md), and [the guide](../../docs/guide/plugins.md) shows
how to write one.

`examples/allow_reads.rs` is a decider that allows the tools that only read. CI runs it:

```sh
cargo run -p moebius-plugin-api --example allow_reads
```
