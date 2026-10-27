# Write a plugin that answers your own decisions

## When you want this

A decision point is a place where Moebius asks "which of these options?" and records the answer as a judgement. A
column script can ask its own with `moebius.decide`, and a plugin answers it in Rust code: a check against an internal
system, a policy that is too much for Luau, a rule that needs a lookup. The plugin decides, the script acts on the answer.

Today a plugin is compiled into the `moebius` binary. Plugins that run as their own process are not supported yet: the
JSON Lines messages for them are defined in the [plugin API reference](../reference/plugin-api.md), but nothing starts
or talks to such a process. Of the plugin kinds, only `decider` has methods defined. That reference holds the exact
types; this page walks through writing one.

Do not route the kind `tool_call` to your plugin unless you mean to replace the guard. The guard is the plugin that
answers `tool_call` today, and it holds the built-in rules and the teacher. A kind has one decider, so routing
`tool_call` to yours removes them all. To forbid more tool calls, write a [rule](rules.md).

## The steps

1. Write a type that implements `Plugin`: a manifest and a `call` method. The example below answers `start_card`.
2. Add it to the host and route a decision kind to it, in `Server::new` in `crates/moebius-core/src/server.rs`.
3. Ask the kind from a column script with `moebius.decide`.
4. Rebuild the `moebius` binary and restart the server.

### 1. The plugin

`crates/moebius-plugin-api/examples/allow_reads.rs` is a complete working decider. Its manifest says who it is and that
it is a decider. A call whose kind is not in the manifest is refused by the host before the plugin sees it.

<!-- excerpt: crates/moebius-plugin-api/examples/allow_reads.rs -->
```rust
struct AllowReads;

impl Plugin for AllowReads {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "allow-reads".into(),
            version: "0.1.0".into(),
            kinds: vec![Kind::Decider],
        }
    }
```

`call` gets a `Call` with a `method` and `params`. The only method defined is `decide`. Its `params` are a
`DecideRequest`: the decision `kind`, a `context` (a map of names to text; secrets are replaced by `[redacted:<kind>]`
first) and the `options`. It returns a `Decision` as JSON: a `choice` that is one of the options exactly, a `reason`
in a sentence, and optionally `by`, which part of the plugin decided. The core records the decider as `<plugin>/<by>`,
or as `<plugin>` without `by`.

<!-- excerpt: crates/moebius-plugin-api/examples/allow_reads.rs -->
```rust
fn call(&self, call: Call) -> Result<Value, PluginError> {
    if call.method != "decide" {
        return Err(PluginError::Failed(format!("no method {:?}", call.method)));
    }
    let request: DecideRequest = serde_json::from_value(call.params)
        .map_err(|e| PluginError::BadParams(e.to_string()))?;
    if request.kind != "tool_call" {
        return Err(PluginError::Failed(format!(
            "this plugin decides tool calls, not {:?}",
            request.kind
        )));
    }
    let tool = request.context.get("tool").map_or("", String::as_str);
    let (choice, reason) = if READ_ONLY.contains(&tool) {
        ("allow", format!("{tool} only reads"))
    } else {
        ("refuse", format!("only reads are allowed, not {tool}"))
    };
    let decision = Decision {
        choice: choice.into(),
        reason,
        by: None,
    };
    serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
}
```

Run it with `cargo run -p moebius-plugin-api --example allow_reads`. It builds a `Host`, adds the plugin and asks three
questions the way the core does.

Our plugin decides `start_card`: a card whose title says `[wip]` is held. It sets `by`, so its judgements say which part
decided.

<!-- excerpt: crates/moebius-core/tests/guide.rs -->
```rust
struct StartGate;

impl Plugin for StartGate {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "start-gate".into(),
            version: "0.1.0".into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        if call.method != "decide" {
            return Err(PluginError::Failed(format!("no method {:?}", call.method)));
        }
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        let title = request.context.get("title").map_or("", String::as_str);
        let (choice, reason) = if title.contains("[wip]") {
            ("hold", "the title says [wip]")
        } else {
            ("start", "the title does not say [wip]")
        };
        let decision = Decision {
            choice: choice.into(),
            reason: reason.into(),
            by: Some("title".into()),
        };
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}
```

A plugin may be called from several threads at once, so it must not rely on being called one at a time. Return
`PluginError::BadParams` for parameters that are not what the method takes and `PluginError::Failed` for anything else
you cannot answer; both are refusals to the core.

### 2. Registering and routing it

The server builds the host, adds the guard and routes `tool_call` to it in `Server::new`:

<!-- excerpt: crates/moebius-core/src/server.rs -->
```rust
let mut host = Host::new(PLUGIN_TIME_LIMIT);
host.add(Guard::new(teacher));
let mut app = App::new(store.clone(), Arc::new(host), &config.data_dir);
app.checks_timeout = config.checks_timeout;
app.decisions.route("tool_call", guard::NAME);
```

Add your plugin to the host next to the guard, and route your kind after `App::new`, before `app` is wrapped in an `Arc`.
The guide's test does it like this:

<!-- excerpt: crates/moebius-core/tests/guide.rs -->
```rust
let mut host = Host::new(Duration::from_secs(5));
host.add(StartGate);
let mut app = App::new(store, Arc::new(host), &dir.path().join("data"));
app.decisions.route("start_card", "start-gate");
```

`route(kind, plugin)` takes the decision kind and the plugin's manifest name. A plugin added with the name of another
replaces it. Routing a kind again replaces the route.

### 3. Asking from a script

`moebius.decide(kind, context, options, fail_closed)` returns the choice and the reason. `context` is a table of text
to text, `options` an array of text, and `fail_closed` the option to take when nobody can answer. The script moves a
card on only when the plugin says `start`.

<!-- script: backlog.luau; test: decide-with-a-plugin -->
```luau
function on_card(card)
  local choice, reason = moebius.decide("start_card", { title = card.title }, { "start", "hold" }, "hold")
  if choice == "start" then
    moebius.move("Ready")
  else
    moebius.wait(reason)
  end
end
```

## What the host guarantees

The core never calls a plugin directly. Every call goes through a `Host` that runs it on a thread of its own, named
`plugin <name>`, and waits at most 30 seconds. Whatever goes wrong becomes a refusal, and the question's fail-closed
option is taken:

| What happens | The judgement says |
|---|---|
| No route for the kind (a script asked a kind nobody answers) | `refused: no decider answers "<kind>"`, decider `none` |
| The plugin takes longer than 30 seconds | `refused: <plugin> did not answer within 30000 ms`. Its thread keeps running, but its answer is never used. |
| The plugin panics | `refused: <plugin> panicked: <message>` |
| The plugin returns an error | `refused: <plugin> failed: <message>` |
| The reply is not a `Decision`, or its `choice` is not one of the options | `refused: <plugin> answered something that is not a valid reply: ...` |
| The call's kind is not in the manifest | `refused: <plugin> is not a Decider plugin` |

A plugin that hangs, panics or answers nonsense therefore blocks what it was asked about; it never lets it through.

## How to try it

Run a card through the script. The `Fixture` in `crates/moebius-core/tests/guide.rs` shows how to do it in a test with
no model: it adds `StartGate` to a `Host`, routes `start_card` to it, commits the script as an override and ticks a card.
On a running server, create two issues, one titled `[wip] Add a menu`, and read `GET /v1/snapshot`: the first card
moves on and the second waits in `Backlog` with `the title says [wip]`.

## What can go wrong and how you see it

| What happens | How you see it |
|---|---|
| The script gets the fail-closed option and a reason that starts `refused:`. | The reason is the card's waiting reason (`waiting` in the snapshot). The table above says what each reason means. |
| A judgement is recorded at every tick. | `moebius.decide` records one judgement per call, and a column script runs at every tick, so a card that is held is decided again two seconds later. Ask only where the card moves on after the answer, and expect one judgement per tick otherwise. They show up in the card's history, `GET /v1/cards/{id}/history`, as `decided`. |
| The board slows down. | A script's `moebius.decide` waits for the plugin, up to 30 seconds, and the board's tick waits for the script. Keep `call` fast. |
| The server does not know your plugin. | Nothing registers a plugin by itself. If you did not rebuild and restart with it added in `Server::new`, the kind has no route. |
| You routed `tool_call` and rules stopped working. | The guard no longer answers. See the warning at the top. |
