# Plugin API

A plugin adds to or replaces a part of Moebius. This page is for anyone who writes a plugin: it describes the Rust trait
every plugin implements, the calls the core makes, what happens when a plugin is slow or wrong, and the JSON Lines
messages that plugins in their own process will use later. The code is in `crates/moebius-plugin-api/src/lib.rs`; a
working decider is in
[`crates/moebius-plugin-api/examples/allow_reads.rs`](../../crates/moebius-plugin-api/examples/allow_reads.rs).

Today every plugin is compiled into the `moebius` binary. Plugins in their own process are not supported yet: the
messages below exist so that they can be, but nothing starts or talks to such a process. The plugins that go
through this API today are the `guard` decider (`crates/moebius-core/src/decide/guard.rs`) and, when a `[decision_model]` is
configured, the `decision-model` decider (`crates/moebius-core/src/decide/model.rs`, see
[the decision model](decision-model.md)).

## The trait

```rust
pub trait Plugin: Send + Sync + 'static {
    fn manifest(&self) -> Manifest;
    fn call(&self, call: Call) -> Result<serde_json::Value, PluginError>;
}
```

`manifest` says who the plugin is. `call` answers one call. A plugin may be called from several threads at once, so it
must not rely on being called one at a time.

### `Manifest`

| Field | Type | Meaning |
|---|---|---|
| `name` | string | The plugin's name; the host and the routes refer to it by this name. A plugin added with the name of another replaces it. |
| `version` | string | The plugin's version, for people. |
| `kinds` | array of `Kind` | What the plugin is. A call whose kind is not in this list is refused with `WrongKind` before the plugin sees it. |

### `Kind`

Serialised in lower case with underscores, as in `"decider"`.

| Kind | Meant for | Used by the core today |
|---|---|---|
| `forge` | Issues and changes: GitHub, plain git, later Codeberg and GitLab. | No. The forges are a Rust trait in `forge.rs`. |
| `harness` | Drives a coding-agent harness such as Pi-Bolt. | No. `pibolt.rs` is called directly. |
| `runner` | Where harnesses run: local worktrees, later GPU machines. | No. |
| `decider` | Answers decision points. | Yes: `guard` decides `tool_call`. |
| `router` | The model provider the harnesses use. | No. `router.rs` is part of the core. |
| `view` | Describes a screen of the desktop app as data. | No. |

Only the `decider` kind has methods defined today.

### `Call`

One call into a plugin.

| Field | Type | Meaning |
|---|---|---|
| `kind` | `Kind` | Which kind of plugin the call is for. |
| `method` | string | The method of that kind. The only method defined is `decide` of `decider`. |
| `params` | JSON value | The method's parameters. Missing means `null`. |

`Call::new(kind, method, params)` builds one from any serialisable `params`.

### The method `decide` of a decider

The parameters are a `DecideRequest`:

| Field | Type | Meaning |
|---|---|---|
| `kind` | string | The kind of decision, such as `tool_call`. |
| `context` | map of string to string | Everything the decision is about, in full. Secrets are replaced by `[redacted:<kind>]` before the plugin sees them. |
| `options` | array of strings | The choices. The plugin must pick one of them. |

The result is a `Decision`:

| Field | Type | Meaning |
|---|---|---|
| `choice` | string | One of `options`, exactly. Anything else is refused as `BadReply`. |
| `reason` | string | Why, in a sentence. It is recorded in the judgement and, for a refused tool call, shown to the model. |
| `by` | string or missing | Which part of the plugin decided, when it has several. The core records the decider as `<plugin>/<by>`, and as `<plugin>` when `by` is missing. The guard uses `patterns` and `teacher`. |

```json
{"kind": "tool_call", "context": {"tool": "bash", "input": "{\"command\":\"ls\"}"}, "options": ["allow", "refuse"]}
```

```json
{"choice": "allow", "reason": "ls only reads", "by": "patterns"}
```

### `PluginError`

What a plugin returns when it cannot answer.

| Variant | Meaning |
|---|---|
| `BadParams(message)` | The parameters are not what the method takes. Displayed as `bad parameters: <message>`. |
| `Failed(message)` | The plugin failed, or does not know the method. Displayed as `<message>`. |

## The host and failing closed

The core never calls a plugin directly. It goes through a `Host`, built with `Host::new(limit)` and filled with
`host.add(plugin)`. The server's limit is 30 seconds (`PLUGIN_TIME_LIMIT_SECONDS`). `host.call(plugin, call)` and
`host.decide(plugin, &request)` return a `Result<_, Refusal>`.

The host runs every call on a thread of its own, named `plugin <name>`, and waits at most the limit. Every way a call can
go wrong is a `Refusal`, so the core never acts on a half-answered call:

| `Refusal` | When |
|---|---|
| `UnknownPlugin(name)` | No plugin is added under that name. |
| `WrongKind { plugin, kind }` | The call's kind is not in the plugin's manifest. |
| `TimedOut { plugin, millis }` | The plugin did not answer within the limit. Its thread keeps running, but its answer is never used. |
| `Panicked { plugin, message }` | The plugin panicked. |
| `Failed { plugin, error }` | The plugin returned a `PluginError`. |
| `BadReply { plugin, message }` | For `decide`: the reply is not a `Decision`, or its `choice` is not one of the options. |

A `Refusal` is serialised as `{"refusal": "<variant in snake_case>", "detail": ...}`, for example
`{"refusal":"unknown_plugin","detail":"x"}` and
`{"refusal":"timed_out","detail":{"plugin":"x","millis":30000}}`.

The core turns a refusal of a decider into the question's fail-closed option (below). A plugin that hangs, panics or
answers nonsense therefore blocks the thing it was asked about; it never lets it through.

## Decision points and routing

A decision point is a `Question` handed to `Decisions::decide` (`crates/moebius-core/src/decide/mod.rs`):

| Field | Meaning |
|---|---|
| `kind` | The kind of decision. It picks the decider. |
| `context` | Text by name; redacted before it is stored or sent to a decider. |
| `options` | The choices. |
| `fail_closed` | The option taken when the decider cannot answer. It must be the safe one. |
| `run_id`, `card_id` | The run and card the decision belongs to, if any. |

`Decisions::route(kind, plugin)` makes the decider plugin `plugin` answer questions of `kind`. `Decisions::decide`:

1. Redacts and stores the context as a blob.
2. Asks the routed plugin through the host. A kind with no route, or any `Refusal`, gives the `fail_closed` option with
   the reason `refused: <why>`, and the decision's `refused` flag is true.
3. Records exactly one judgement: kind, context, options, choice, reason, decider and the run and card.

A person's decisions (approve an issue, merge a change) are recorded the same way, with the decider `person`, through
`Decisions::record_person`.

### Registering a compiled-in plugin

In `Server::new` (`crates/moebius-core/src/server/mod.rs`), the host is built, the plugin added, and the route set before the
`App` is shared:

```rust
let mut host = Host::new(PLUGIN_TIME_LIMIT);
host.add(Guard::new(teacher));
let mut app = App::new(store.clone(), Arc::new(host), &config.data_dir);
app.decisions.route("tool_call", guard::NAME);
```

A new decider is added the same way: implement `Plugin` with `Kind::Decider`, add it to the host and route a decision
kind to it. A kind has one decider; routing a kind again replaces the route.

### The `tool_call` context

The kind `tool_call` is asked once for every tool call of every run, with the options `allow` and `refuse` and the
fail-closed option `refuse`. All context values are strings:

| Key | Present | Value |
|---|---|---|
| `tool` | always | The tool's name, such as `bash`, `write`, `edit`, `read`. |
| `input` | always | The call's arguments as JSON text, such as `{"command":"ls"}`. |
| `tool_call_id` | always | The harness's id for the call. |
| `run_id` | always | The run's id, as decimal text. |
| `card_id` | when the run has a card | The card's id, as decimal text. |
| `worktree` | when the run has a worktree | The absolute path of the run's worktree. |
| `card_title`, `card_body` | when the card is found | The card's title and body. |
| `rules` | unless `rules_error` is set | The rules that apply as Markdown (`## <name>` and the text), possibly empty. See [rules](rules.md). |
| `rules_error` | when the server's rules file exists but cannot be read | Why it cannot (a message). The guard refuses every call then. |

A decider for `tool_call` that refuses should say which rule it applied at the start of `reason`, followed by a colon,
as the guard does (`no-force-push: ...`).

## JSON Lines messages for plugins in their own process

This is the format the core will use to talk to a plugin that runs as its own process. It is not used yet. A line is one
JSON object, ends with a line feed, and holds no line feed inside. The core writes calls to the plugin's standard input
and reads replies from its standard output. Every message is an `Envelope` with a `type`:

```json
{"type":"call","id":7,"call":{"kind":"decider","method":"decide","params":{"context":{"tool":"bash"},"kind":"tool_call","options":["allow","refuse"]}}}
{"type":"reply","id":7,"result":{"choice":"allow","reason":"ok","by":null}}
{"type":"error","id":7,"message":"boom"}
```

- `call`: `id` (an unsigned integer chosen by the core) and a `Call`.
- `reply`: the `id` of the call and its `result` (for `decide`, a `Decision`).
- `error`: the `id` of the call and a `message`. It stands for a `PluginError`.

The host's time limit and fail-closed rules apply to such a plugin in the same way. The wrapper that would turn
envelopes into a `Plugin` does not exist yet, so details such as the plugin's start-up and its manifest exchange are not
defined.
