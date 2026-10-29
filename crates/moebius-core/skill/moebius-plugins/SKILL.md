---
name: moebius-plugins
description: Write a Moebius plugin, a decider in Rust that answers a decision point a column script asks with moebius.decide. Use it when a decision needs code - a lookup, a policy too large for Luau, a check against another system - or the person asks for a plugin, a decider or the plugin API.
---

# Decider plugins

Goal: a Rust type that answers one decision kind, tested with cargo, registered in the server and asked from a column
script. Today a plugin is compiled into the `moebius` binary, so this work happens in a checkout of Moebius itself, and
the person rebuilds and restarts the server to use it.

## Steps

1. Implement `moebius_plugin_api::Plugin`: `manifest()` returns its `name`, `version` and `kinds: vec![Kind::Decider]`;
   `call(call)` answers the method `decide`. Its `params` are a `DecideRequest` (`kind`, `context` of text to text with
   secrets already redacted, `options`); return a `Decision` (`choice`, exactly one of the options, a `reason` in a
   sentence, `by` optionally) as JSON. Return `PluginError::BadParams` for params that are not a `DecideRequest` and
   `PluginError::Failed` for anything you cannot answer. It may be called from several threads at once.
2. Test it through a `Host`, as the core calls it: `Host::new(Duration::from_secs(5))`, `host.add(MyPlugin)`, then
   `host.decide("my-plugin", &request)` and assert the choice. Cover every option and a kind it does not take.
3. Register it in `Server::new` (`crates/moebius-core/src/server/mod.rs`) next to the guard with `host.add(...)`, and route
   the kind after `App::new` with `app.decisions.route("start_card", "my-plugin")`. Never route `tool_call`: that
   removes the guard, the built-in rules and the teacher.
4. Ask it from a column script with `moebius.decide(kind, context, options, fail_closed)` (see the `moebius-workflows`
   skill).

## Example

The working decider is `crates/moebius-plugin-api/examples/allow_reads.rs`; start from it. Its heart:

```rust
impl Plugin for AllowReads {
    fn manifest(&self) -> Manifest {
        Manifest { name: "allow-reads".into(), version: "0.1.0".into(), kinds: vec![Kind::Decider] }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        if call.method != "decide" {
            return Err(PluginError::Failed(format!("no method {:?}", call.method)));
        }
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        let tool = request.context.get("tool").map_or("", String::as_str);
        let (choice, reason) = if READ_ONLY.contains(&tool) {
            ("allow", format!("{tool} only reads"))
        } else {
            ("refuse", format!("only reads are allowed, not {tool}"))
        };
        let decision = Decision { choice: choice.into(), reason, by: None };
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}
```

## Try it

```sh
cargo run -p moebius-plugin-api --example allow_reads
cargo test -p moebius-core
cargo clippy --workspace --all-targets -- -D warnings
```

Run the tests you wrote and read their output. Then say what the person has to do: merge, rebuild `moebius`, restart the
server (`"$MOEBIUS" server restart` only when they ask), and commit the column script that asks the kind.

## What can go wrong

The host turns every failure into a refusal, and the script gets its fail-closed option with a reason that starts
`refused:`: no route for the kind (`no decider answers`), more than 30 seconds, a panic, an error, a `choice` that is
not one of the options, or a kind not in the manifest. A script runs every tick, so a held card is decided again every
tick; keep `call` fast.

## Details

In the `moebius` skill: `guide/plugins.md` (the whole walk-through with a `start_card` decider) and
`reference/plugin-api.md` (every type and the host's guarantees).
