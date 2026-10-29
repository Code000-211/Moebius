# Plugin API

A plugin adds to or replaces a part of Moebius. This page is for anyone who writes a plugin: it describes the Rust trait
every plugin implements, the calls the core makes, what happens when a plugin is slow or wrong, and the JSON Lines
messages a plugin in its own process speaks. The code is in `crates/moebius-plugin-api/src/lib.rs` and
`crates/moebius-plugin-api/src/process/`; a working decider is in
[`crates/moebius-plugin-api/examples/allow_reads.rs`](../../crates/moebius-plugin-api/examples/allow_reads.rs), and the
same decider in its own process in
[`crates/moebius-plugin-api/examples/allow_reads_process.rs`](../../crates/moebius-plugin-api/examples/allow_reads_process.rs).

A plugin is either compiled into the `moebius` binary, or a program of its own, in any language, that `moebius.toml`
names under [`[[plugins]]`](configuration.md#plugins) and that speaks [JSON Lines](#plugins-in-their-own-process) on
its standard input and output. Both get the same calls, the same time limit and the same refusals. The compiled-in
plugins are the `guard` decider (`crates/moebius-core/src/decide/guard/`) and, when a `[decision_model]` is configured,
the `decision-model` decider (`crates/moebius-core/src/decide/model/`, see [the decision model](decision-model.md)).

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
| `forge` | Issues and changes: GitHub, GitLab, Forgejo (Codeberg, Gitea), plain git. | No. The forges are a Rust trait in `forge.rs`. |
| `harness` | Drives a coding-agent harness such as Pi-Bolt. | No. `harness/pibolt/` is called directly. |
| `runner` | Where harnesses run: local worktrees, later GPU machines. | No. |
| `decider` | Answers decision points. | Yes: `guard` decides `tool_call`. |
| `router` | The model provider the harnesses use. | No. `router/` is part of the core. |
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
| `options` | array of strings | The choices. The plugin must pick one of them. A kind whose answer is no choice among a few, such as the ranked list of files of `localize`, has none, and the plugin then answers with any text as `choice`. |

The result is a `Decision`:

| Field | Type | Meaning |
|---|---|---|
| `choice` | string | One of `options`, exactly. Anything else is refused as `BadReply`, unless the request has no options. |
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

`host.add_process(process)` adds a plugin in its own process (below), and `host.status(name)` says how one is doing.
The host runs every call on a thread of its own, named `plugin <name>`, and waits at most the limit. Every way a call can
go wrong is a `Refusal`, so the core never acts on a half-answered call:

| `Refusal` | When |
|---|---|
| `UnknownPlugin(name)` | No plugin is added under that name. |
| `WrongKind { plugin, kind }` | The call's kind is not in the plugin's manifest. |
| `TimedOut { plugin, millis }` | The plugin did not answer within the limit. Its thread keeps running, but its answer is never used. |
| `Panicked { plugin, message }` | The plugin panicked. |
| `Failed { plugin, error }` | The plugin returned a `PluginError`. |
| `BadReply { plugin, message }` | For `decide`: the reply is not a `Decision`, or its `choice` is not one of the options. For a plugin in its own process also: it wrote a line that is not a reply while the call waited. |
| `NotRunning { plugin, message }` | A plugin in its own process is not running (it is starting, or waits to be started again after it stopped), or it stopped before it answered. `message` says why, such as `it exited (exit status: 1)`. |

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

### The `localize` context

The kind `localize` is asked once at the start of every `code` run of a card, with no options and the fail-closed answer
`[]`, which gives the run no hint. A plugin of a repository's own that decides it (`decides = ["localize"]` in
`[[plugins]]`) replaces the built-in decider. All context values are strings: `title` and `body` (the card's issue),
`repository` (its name), `card` (its id), `files` (how many files to name at most, 1 to 10) and `candidates` (the files
whose text best matches the issue, one path per line). The `choice` is a JSON list of at most `files` objects
`{"path": string, "reason": string}`, best first, each path of a file of the repository once; see
[the decision model](decision-model.md#localize).

## Plugins in their own process

A plugin in its own process is a program that `moebius.toml` names (see [`[[plugins]]`](configuration.md#plugins)):

```toml
[[plugins]]
name = "allow-reads"
path = "plugins/allow-reads"
args = ["--plugin"]
decides = ["start_card"]
```

The server starts it with `args`, in the directory of `moebius.toml`, with the server's environment, when it starts.
It is added to the host under `name` and the kinds in `decides` are routed to it. In Rust, `Process::start(spec, limit,
log)` does the same, and `host.add_process(process)` adds it.

### The wire format

Messages are JSON Lines: a line is one JSON object in UTF-8, ends with a line feed (`\n`), and holds no line feed
inside. The server writes to the plugin's standard input and reads its standard output. Every message is an `Envelope`,
told apart by its `type`:

| `type` | Written by | Fields |
|---|---|---|
| `manifest` | the plugin, as its first line | `manifest`: a [`Manifest`](#manifest). |
| `call` | the server | `id`: an unsigned integer, unique among the calls to this process; `call`: a [`Call`](#call). |
| `reply` | the plugin | `id`: the call's; `result`: the method's result (for `decide`, a [`Decision`](#the-method-decide-of-a-decider)). |
| `error` | the plugin | `id`: the call's; `message`: why it cannot answer. It is refused as `Failed { error: Failed(message) }`. |

A whole exchange, the plugin's lines marked `<` and the server's `>`:

```json
< {"type":"manifest","manifest":{"name":"allow-reads","version":"0.1.0","kinds":["decider"]}}
> {"type":"call","id":1,"call":{"kind":"decider","method":"decide","params":{"kind":"tool_call","context":{"tool":"read"},"options":["allow","refuse"]}}}
> {"type":"call","id":2,"call":{"kind":"decider","method":"decide","params":{"kind":"merge","context":{"tool":"read"},"options":["allow","refuse"]}}}
< {"type":"reply","id":1,"result":{"choice":"allow","reason":"read only reads","by":null}}
< {"type":"error","id":2,"message":"this plugin decides tool calls, not \"merge\""}
```

The rules:

1. The plugin's first line is its manifest, within the host's time limit (30 seconds in the server). Its `name` must be
   the configured name. A first line that is not a manifest, another name, no line in time, or an exit before it counts
   as a failed start.
2. The server may send a call before the reply to an earlier one, from several threads at once. The plugin answers each
   `id` exactly once, in any order; it may answer one at a time.
3. A reply that comes after the time limit, or with an `id` no call waits for, is dropped.
4. Any other line on standard output (not JSON, not an `Envelope`, a second `manifest`, a `call`) is a fault: the
   process is killed, every call waiting on it is refused with `BadReply`, and it is started again.
5. When its standard input closes, the plugin exits. The server closes it, and kills the process, when it stops.
6. Standard error is free text. Each line goes to `<data_dir>/plugins/<name>.log`, after a time stamp and with secrets
   redacted.

### Failing closed, and starting again

Every way a plugin process can go wrong is a refusal, never an allow:

| What happens | The call is refused with |
|---|---|
| It does not answer within the time limit | `TimedOut`, as for a compiled-in plugin. The process keeps running. |
| It exits or crashes while calls wait | `NotRunning` with `it exited (<status>)`, for each of them. |
| It writes a line that is not a reply | `BadReply` with `it wrote a line that is not a reply: <the line>`, for each call waiting. |
| It answers with `error` | `Failed` with the message. |
| Its `result` is not a `Decision`, or picks no option | `BadReply`, as for a compiled-in plugin. |
| It is starting, or waits to be started again | `NotRunning` with why it stopped the last time. |

A process that stopped (it exited, was killed for a bad line, or did not start) is started again after a pause: 1 second
the first time, doubled each time up to 60 seconds, and back to 1 second after it ran for at least 60 seconds. Each stop
is written to its log as `moebius: <name> stopped: <why>; it is started again in <n> s`. `host.status(name)` gives a
`ProcessStatus`: the `program`, the `state` (`starting`, `running`, `restarting` or `stopped`), the number of
`restarts` and the last `error`. The Plugins page and `GET /v1/plugins` show it.

### A plugin process in Rust

`moebius_plugin_api::serve(&plugin, input, output)` runs any `Plugin` as a process: it writes the manifest, then
answers each call one at a time until `input` ends. A line that is not a call is skipped with a line on standard error.
`allow_reads_process.rs` serves `AllowReads` when started with `--plugin`:

```rust
serve(&AllowReads, std::io::stdin().lock(), std::io::stdout().lock())
```

A plugin in another language only has to follow the wire format above.
