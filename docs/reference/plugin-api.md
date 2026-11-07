# Plugin API

A plugin adds to or replaces a part of Moebius. This page is for anyone who writes a plugin: it describes the Rust trait
every plugin implements, the calls the core makes, what happens when a plugin is slow or wrong, and the JSON Lines
messages a plugin in its own process speaks. The code is in `crates/moebius-plugin-api/src/lib.rs` and
`crates/moebius-plugin-api/src/process/`; a working decider is in
[`crates/moebius-plugin-api/examples/allow_reads.rs`](../../crates/moebius-plugin-api/examples/allow_reads.rs), and the
same decider in its own process in
[`crates/moebius-plugin-api/examples/allow_reads_process.rs`](../../crates/moebius-plugin-api/examples/allow_reads_process.rs).
A launcher, the other kind the core uses, is in
[`wrap_launcher.rs`](../../crates/moebius-plugin-api/examples/wrap_launcher.rs) and, in its own process, in
[`wrap_launcher_process.rs`](../../crates/moebius-plugin-api/examples/wrap_launcher_process.rs); see
[Launchers](#launchers).

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
| `project_settings` | array of `PluginSetting` | What a person sets for each repository for this plugin (see [Plugin settings](#plugin-settings)). Optional: left out, or empty, the plugin has none. |
| `app_settings` | array of `PluginSetting` | What a person sets once for this plugin on the whole server (see [Plugin settings](#plugin-settings)). Optional, like `project_settings`. |
| `settings_ui` | string | The source of a JavaScript module that draws the plugin's settings in the web app in place of the fields (see [A page of its own](#a-page-of-its-own)). Optional: left out, the web app draws the fields. |

### Plugin settings

A plugin may ask a person for settings in two scopes, each declared in its manifest:

| Scope | Manifest field | Where the web app shows it | Kept by | Routes | A launcher gets it as |
|---|---|---|---|---|---|
| Each repository, such as the Dockerfile a sandbox builds its runs' image from | `project_settings` | The repository's page (`/projects/<id>`), under the plugin's name | the repository's [project settings](harness-configuration.md#project-settings), a file `plugins/<plugin>/<key>` each (a server without `[harness]` has none) | [`GET` and `PUT /v1/repositories/{id}/plugin-settings`](desktop-api.md#get-v1repositoriesidplugin-settings) | `project_settings` of the run's repository |
| The whole server, such as a registry every image comes from | `app_settings` | The Plugin settings page (`/plugin-settings`), under the plugin's name | plugin name and key | [`GET /v1/plugins/settings` and `PUT /v1/plugins/{plugin}/settings`](desktop-api.md#get-v1pluginssettings) | `app_settings` |

A launcher gets both in every [`LaunchRequest`](#the-method-launch); no other kind is handed them yet. Both scopes
declare their settings the same way, each a `PluginSetting`:

| Field | Type | Meaning |
|---|---|---|
| `key` | string | The name the value is kept and handed over under, unique among the plugin's settings of its scope. |
| `label` | string | What the field is called on the page. |
| `description` | string | What the setting does, shown under the field. Optional. |
| `input` | `"line"` or `"code"` | `line` (the default) is a field of one line, which refuses a line break; `code` is an editor with line numbers, for a file. |
| `language` | string | For `code`, the language the editor highlights: the web app knows `dockerfile`, `shell`, `lua`, `toml` and `yaml`, and shows any other as plain text. Optional. |

Every value is text of at most 256 KiB; a setting with no value is left out of what the plugin gets, so a plugin
treats a missing key as unset. Only a person changes the values, never an agent, since a launcher's settings say what a
run may do. They are stored and handed over as written, not redacted, so a setting must not ask for a key or a token.

```json
{"name": "box-run", "version": "0.1.0", "kinds": ["launcher"], "project_settings": [{"key": "dockerfile", "label": "Dockerfile", "description": "The image of the runs of this repository.", "input": "code", "language": "dockerfile"}], "app_settings": [{"key": "registry", "label": "Registry", "description": "Where base images come from."}]}
```

### A page of its own

A plugin may draw its settings itself, with presets, a preview or controls of its own, by giving the source of an ES
module in `settings_ui`, at most 256 KiB (a process plugin whose manifest has more is not started, with that reason in
its status). The web app reads it from
[`GET /v1/plugins/{plugin}/settings-ui`](desktop-api.md#get-v1pluginspluginsettings-ui) and draws it in the plugin's box
in both scopes, in place of the fields. The settings the manifest declares are still what is kept: the page only edits
their values. The box keeps its Save and Revert buttons and the note that values are stored as plain text, and the core
checks and records a save as for any plugin, so a key the plugin does not declare is refused.

**The module does not run in the web app.** It runs in a frame, `<iframe sandbox="allow-scripts">` over
`/plugin-frame.html`, drawn inline: no border, no scrollbar of its own, as tall as its content. The frame has no
`allow-same-origin`, so its origin is opaque: it has no cookies, no local storage, no access to the app's page or to the
session, and a call to the server's API from it carries no login. The server also answers the frame with a content
security policy that forbids it every network request (`connect-src 'none'`), every image but `data:` and every
form. The module can therefore draw and compute, and report values; it cannot read anything of the app or the server
beyond what it is handed, or send anything out. Everything it says is untrusted: the app believes only the messages
below, from the frame's own window (`event.source`), and takes nothing from a message beyond the declared values.

The module's default export (a function, or an `async` one) is called once in the frame with an element to draw in and a
host. It is drawn again from the saved values after a save or a revert.

| Host field | Type | Meaning |
|---|---|---|
| `plugin`, `version` | string | The plugin's name and version. |
| `scope` | `"app"` or `"project"` | The Plugin settings page, or a repository's page. |
| `repository` | number or null | The repository's id in the `project` scope. |
| `settings` | array | The settings the plugin declares in this scope, each with its `value`, as [`GET /v1/repositories/{id}/plugin-settings`](desktop-api.md#get-v1repositoriesidplugin-settings) lists them. |
| `values` | object | The saved values to start from, by key. |
| `change(values)` | function | Reports every value of the scope by key; a key left out is emptied by a save. |
| `codeEditor(element, {label, language, value, onChange})` | function | Draws an editor for a file of code in `element` (a plain text area in a monospaced font, `Tab` indents by two spaces, `language` is kept as `data-language` and not highlighted: the app's own editor cannot run in a frame without an origin); returns a function that takes it down. |

**The messages** between the app and the frame, by `postMessage`, are what a plugin's page may rely on only through the
host above, but a client that wants to host a page needs them. The app sends `{type: "init", css, theme, source, host}`
once the frame says `ready` (`css` is the app's stylesheets as text, `theme` is `"light"` or `"dark"`, `source` the
module, `host` the fields above but `change`/`codeEditor`) and `{type: "theme", theme}` whenever the app's theme changes.
The frame sends:

| Message | Meaning | What the app accepts |
|---|---|---|
| `{type: "ready"}` | The frame is loaded and waits for `init`. | Always. |
| `{type: "mounted"}` | The module's default export returned. | Always. |
| `{type: "failed", message}` | The module could not be loaded or threw while it drew. | The message cut to 500 characters. The box then draws the fields and says why. |
| `{type: "height", height}` | The height of the page's content in pixels, which sizes the frame. | A finite number of at least 0, at most 20,000. |
| `{type: "change", values}` | Reports the values, as `change(values)` does. | Only an object whose keys are all declared in this scope and whose values are all text of at most 256 KiB, with no line break in a `line` setting; any other message is dropped whole, as is one from a window that is not the frame's. |

A frame that has not said `mounted` or `failed` within 10 seconds, a module that cannot be read, and a page that fails
are all reported in the box, which draws the fields instead. The values are saved by the app, never by the page.

The page looks like the app: the app's stylesheets are in the frame, so its CSS variables (`--text`,
`--text-secondary`, `--field`, `--hairline`, `--font-sans`, `--font-mono` and the others of `web/src/styles/theme.css`)
and the classes the app uses are there, in the theme the app has now.

```js
export default function mount(element, host) {
  const preset = document.createElement("button");
  preset.type = "button";
  preset.textContent = "Start from Debian";
  const editor = document.createElement("div");
  element.append(preset, editor);
  let takeDown = () => {};
  const draw = (value) => {
    takeDown();
    takeDown = host.codeEditor(editor, {
      label: "Base Dockerfile",
      language: "dockerfile",
      value,
      onChange: (base) => host.change({ base }),
    });
  };
  preset.onclick = () => {
    host.change({ base: "FROM debian:stable-slim\n" });
    draw("FROM debian:stable-slim\n");
  };
  draw(host.values.base ?? "");
  return () => takeDown();
}
```

#### Security model of the settings

- A plugin's page is untrusted code from the plugin's author, run in the frame above. The route that serves it sends
  `X-Content-Type-Options: nosniff` and `Cache-Control: no-store`.
- The web app is served with a content security policy (`web/csp.json`): scripts, styles, fonts and images from its own
  address only (inline styles allowed, inline scripts and `eval` not), connections to its own address only, frames of
  its own address only, and no embedding of the app in another site. The frame gets the stricter policy of its own. The app's `connect-src` also names `ws://` and `wss://` of the address the
  page was asked at (its `Host`, written only when it is a plain hostname and port), since older browsers do not match
  those schemes with `'self'`. The frame can navigate itself, and so could send the values it was given elsewhere; they
  are the plugin's own settings, which the plugin already has on the server.
- Values are stored as plain text (a repository's in the git history of the harness configuration repository, the
  server's in its database), are shown to anyone who can open the page and are handed to the launcher as written. A
  setting is not a place for a key or a token: the page says so beside the fields, and the decision that records a
  save names the keys, never the values.
- Only a person saves settings; an agent token may read them (the Moebius agent) and is refused a save.

### `Kind`

Serialised in lower case with underscores, as in `"decider"`.

| Kind | Meant for | Used by the core today |
|---|---|---|
| `forge` | Issues and changes: GitHub, GitLab, Forgejo (Codeberg, Gitea), plain git. | No. The forges are a Rust trait in `forge.rs`. |
| `harness` | Drives a coding-agent harness such as Pi-Bolt. | No. `harness/pibolt/` is called directly. |
| `runner` | Where harnesses run: local worktrees, later GPU machines. | No. |
| `decider` | Answers decision points. | Yes: `guard` decides `tool_call`, and `lens` decides `lint_report` and `deep_check` (see [the lens decisions](decision-model.md#the-lens-decisions)). |
| `router` | The model provider the harnesses use. | No. `router/` is part of the core. |
| `view` | Describes a screen of the web app as data. | No. |
| `launcher` | Wraps how a harness process is started, such as in a sandbox. | Yes: Pi-Bolt asks the configured launcher before it starts Pi (see [Launchers](#launchers)). |

Only the `decider` and `launcher` kinds have methods defined today.

### `Call`

One call into a plugin.

| Field | Type | Meaning |
|---|---|---|
| `kind` | `Kind` | Which kind of plugin the call is for. |
| `method` | string | The method of that kind. The methods defined are `decide` of `decider` and `launch` and `leftovers` of `launcher`. |
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
let host = Arc::new(host);
let mut app = App::new(store.clone(), host.clone(), &config.data_dir);
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
| `builtin_refusal` | when the call breaks a built-in rule | The rule's name and what it says, tested on the call before the context was redacted and with Claude Code's tool names and keys (`Bash`, `Write`, `Edit`, `file_path`) read as `bash`, `write`, `edit` and `path`. The guard refuses with it. |
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

## Launchers

A launcher is a plugin of the kind `launcher` that may wrap how a harness process is started, such as inside a sandbox.
Several plugins may be launchers: the `[[plugins]]` entries with [`launches = true`](configuration.md#plugins), each for the agents of its `launch_agents` (every agent, if it names none). A run goes to the first entry in the file that takes its agent. The last launcher must take every agent (no `launch_agents`), so that no run is started unwrapped while a launcher is configured; a file whose last launcher names agents is refused. One launcher can thus sandbox the Moebius agent while another takes the rest. A launcher
is a `Plugin` like any other (the example `wrap_launcher.rs` is one compiled into a host); the server wires one only from
`[[plugins]]`. **Without a launcher the
harness starts exactly as before; the launcher code is not entered.** The reasons and the first launcher, a sandbox, are
in [ADR 0014](../adr/0014-a-launcher-plugin-wraps-how-a-harness-starts.md).

The types are in `crates/moebius-plugin-api/src/launcher.rs`; `host.launch(plugin, &request)` and
`host.leftovers(plugin, &request)` make the two calls. The manifest of a launcher lists `"launcher"` in `kinds`.

### The method `launch`

The core calls it just before it starts a harness process for a run (today Pi-Bolt: a card run, a chat's Pi, each time
a chat's Pi is started again). The parameters are a `LaunchRequest`:

| Field | Type | Meaning |
|---|---|---|
| `run` | object | `id` (integer), `purpose` (`code`, `review`, `chat`, `retro`, `trial`, `research` and the like), `agent` (the agent's name), `chat` (boolean: the process stays up between messages) and `card` (the card's id or `null`). |
| `program` | string | The harness's executable as the core would start it. |
| `args` | array of strings | Its arguments. |
| `env` | map of string to string | Its complete environment, the run token (`MOEBIUS_RUN_TOKEN`) included: not the server's, but the few variables every program needs and those Moebius sets. With a launcher, `TMPDIR`, `TEMP` and `TMP` name a directory of the run (`<data_dir>/runs/<id>/tmp`) instead of the shared one. |
| `dir` | string | The working directory: the run's worktree, or an empty directory for a chat without a repository. |
| `read_only` | array of strings | Paths the process needs to read: the lens and bridge extensions, the agent's own extensions and skills, and the run's output folder (`<data_dir>/runs/<id>/out`), where the server saves the whole outputs of tools that were cut short and the agent reads them. They may be files or directories. |
| `read_write` | array of strings | Paths it needs to read and write: the worktree, its agent directory (which holds the credential store of its harness), its sessions directory, and the run's temporary directory. |
| `urls` | object | `router`, `gateway` and `check`: the addresses the process must reach as the server reaches them, such as `http://127.0.0.1:7172`, `http://127.0.0.1:7172/mcp` and `http://127.0.0.1:7172` (the tool-call check the bridge asks, `MOEBIUS_CHECK_URL`). |
| `clone` | string or `null` | The clone of the repository the worktree belongs to, whose git directory the worktree's git uses; `null` without a repository. |
| `runner` | string or `null` | The name of the `[[runners]]` machine when the run goes there over SSH, else `null`. |
| `repository` | string or `null` | The name in Moebius of the repository the run works in; `null` for a chat in an empty directory and a run in the harness configuration repository. |
| `project_settings` | map of string to string | The launcher's own [project settings](#plugin-settings) for that repository, by key, those without a value left out; empty without a repository. |
| `app_settings` | map of string to string | The launcher's own [app settings](#plugin-settings), by key, those without a value left out. |

The paths and URLs name what exists when the process starts. The agent directory, the sessions directory and the
temporary directory (the last two for a run on this machine) are made before the call, and the agent directory is written
again if the answer names other addresses.

The result is a `LaunchAnswer`, told apart by `answer`:

```json
{"answer": "unchanged"}
```

```json
{
  "answer": "wrapped",
  "program": "box-run",
  "args": ["--profile", "/tmp/run-7.profile", "--", "pi-bolt", "--mode", "rpc"],
  "env": {"PATH": "/usr/bin", "MOEBIUS_RUN_TOKEN": "moebius-run-x"},
  "urls": {"router": "http://10.0.0.1:7172", "gateway": "http://10.0.0.1:7172/mcp", "check": "http://10.0.0.1:7172"},
  "stop": {"program": "box-run", "args": ["stop", "--run", "7"]},
  "cleanup": {"program": "box-run", "args": ["cleanup", "--run", "7"]},
  "remove": ["/tmp/run-7.profile"]
}
```

| Field of `wrapped` | Type | Meaning |
|---|---|---|
| `program`, `args` | string, array | What the core starts instead of the harness, in the same working directory, in its own process group, with standard input and output piped: the core speaks the harness's protocol (Pi's RPC) over them as it does without a launcher, so the wrapper must pass them through to the harness. Standard error goes to the run's log. `program` must not be empty; `args` defaults to empty. |
| `env` | map | The wrapped program's complete environment. Nothing of the server's is added. Default empty. |
| `urls` | object | The addresses as the harness sees them from inside, where they differ from the request's: `router` goes into `models.json` (the provider's `baseUrl`, with `/v1` for the OpenAI protocol), `gateway` into `mcp.json`, `check` into `MOEBIUS_CHECK_URL`. A missing one stays; a `router` without a `gateway` puts the gateway at `<router>/mcp`. Default none. The launcher puts every other variable it wants in `env`. |
| `stop` | command or `null` | A command that ends the whole wrapped process tree. |
| `cleanup` | command or `null` | A command to run once when the wrapped process has ended. |
| `remove` | array of strings | Files and directories the launcher created, which the core removes after `cleanup`. |
| `note` | string or `null` | Something to say about the launch, recorded as a `launcher_note` event of the run (secrets redacted). Default none. |

A command is `{"program": string, "args": [string], "env": {string: string}}` (`args` and `env` default to empty). The
core runs it in its own process group with standard streams closed, an environment cleared but for `PATH`, `HOME` and the
few variables every program needs plus `env`, and kills it after 30 seconds. A command that fails or is killed is noted
as a line in the run's `stderr.log` (`moebius: the launcher's stop command failed: ...`); it does not fail the run, since
the run has ended or is being ended.

What the core does with a wrapped launch:

| When | What |
|---|---|
| The run starts | It starts the wrapped program. |
| The process is ended: the run is stopped, killed (after a stop that is not obeyed), a chat's process is ended or restarted, the server restarts, or the process exits on its own | It runs `stop`, then kills the wrapper's own process group as it does without a launcher. This happens every time a process is ended, after a normal end too, and may happen more than once for one process (a stopped run runs it from the stop and again when its reader finishes), so `stop` must be idempotent and succeed when nothing is left. |
| The process has ended, for whatever reason, including a start that failed after the launcher answered (the answer was unusable, the core could not write the agent directory or the process could not be spawned) | It runs `cleanup` and removes `remove`, once. |

A chat's Pi that is ended while the chat waits and started again for the next message is a new launch each time, with
its own stop and cleanup. Those, and the launch itself, run while the core holds the chats' lock, so a slow launcher
delays the other chats' messages meanwhile; when the server restarts the stop and cleanup of the runs go side by side.

### The method `leftovers`

The core calls it when the server starts, after it has failed the runs the last server left going, and when a person
turns the launcher on while the server runs (then with the ids of the runs that exist; a launcher that is off at the
start is not called until then), and removes what the launcher names, so that a profile or a container of a run that died with the server does not stay. The parameters are a
`LeftoversRequest`: `running`, the ids of the runs that exist (empty at the server's start). The result is `Leftovers`:

| Field | Type | Meaning |
|---|---|---|
| `cleanup` | array of commands | Commands to run, such as one that deletes a sandbox. Default empty. |
| `remove` | array of strings | Files and directories to remove, after the commands. Default empty. |

A command of `cleanup` that fails is collected; the server then does not start and says which.

### Failing closed

A launcher is asked for every launch, and what it cannot do is never a reason to start the harness bare:

| What happens | The core |
|---|---|
| The launcher does not answer within the host's time limit (30 seconds) | Fails the run (`TimedOut`). |
| It returns an error, panics, is not running, is not a launcher (`WrongKind`), is unknown, or its reply is not a `LaunchAnswer` (`BadReply`), or a wrapped one without a `program` | Fails the run before the harness starts, with `the launcher <name> did not start the run: <why>` (secrets, run tokens included, redacted). |
| It wraps a run whose `runner` is set | Fails the run: a launcher on the server cannot wrap a harness on another machine. It answers `unchanged` for those, with a `note` saying so. In both cases a `cleanup` and `remove` it gave are run. |
| It cannot name its leftovers at the server's start | The server does not start. |
| It cannot name its leftovers when it is turned on | It stays off, and the turning on is refused with the reason. |
| It is turned off while a run it wrapped goes | The run keeps its wrap and is stopped and cleaned up by the commands the launcher gave it, which the core runs without a call. A later launch through the launchers the run holds (a chat's next message) is refused as `NotRunning` (`it is turned off`), so it fails and never starts unwrapped. |

For a run on a card, a failed start is a failed run like a harness that cannot be started, so the board's usual handling
applies. The one process outside this: Pi-Bolt's terminal interface for a chat (`GET /v1/chats/{id}/tui`) is a process
the person's terminal owns, so while a launcher is configured it is refused with an error saying so. Claude Code is not
asked yet, so while a launcher is configured a Claude Code run fails to start with an error saying so, instead of
starting outside the launcher.

### A launcher in its own process

A launcher in its own process is a `[[plugins]]` entry with `launches = true`, speaking the
[wire format](#the-wire-format) below with the kind `launcher`:

```json
< {"type":"manifest","manifest":{"name":"box-run","version":"0.1.0","kinds":["launcher"]}}
> {"type":"call","id":1,"call":{"kind":"launcher","method":"launch","params":{"run":{"id":7,"purpose":"code","agent":"Coder","chat":false,"card":3},"program":"pi-bolt","args":["--mode","rpc"],"env":{"PATH":"/usr/bin"},"dir":"/data/worktrees/run-7","read_only":["/data/runs/7/moebius-bridge.ts","/data/runs/7/out"],"read_write":["/data/worktrees/run-7","/data/runs/7/agent","/data/runs/7/sessions"],"urls":{"router":"http://127.0.0.1:7172","gateway":"http://127.0.0.1:7172/mcp","check":"http://127.0.0.1:7172"},"clone":"/data/repos/demo","runner":null,"repository":"demo","project_settings":{},"app_settings":{}}}}
< {"type":"reply","id":1,"result":{"answer":"unchanged"}}
> {"type":"call","id":2,"call":{"kind":"launcher","method":"leftovers","params":{"running":[]}}}
< {"type":"reply","id":2,"result":{"cleanup":[],"remove":["/tmp/run-3.profile"]}}
```

Because the core runs the stop and cleanup commands itself, the launcher process need not be running to end a run that
was started through it. `crates/moebius-plugin-api/examples/wrap_launcher_process.rs` is one in Rust; `serve` runs any
`Plugin` as such a program. A launcher sees the run's token and environment, so it is trusted as much as the server.

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

### Turning a plugin off

An entry with `enabled = false` ([`plugins.enabled`](configuration.md#turning-a-plugin-off-and-on)) is not started at the
server's start; a plugin that runs is stopped when a person turns it off, and started again when they turn it on, both
without a restart. The host keeps a plugin that is off, with the manifest it last sent (the server keeps a copy in
`<data_dir>/plugins/<name>.manifest.json`), so that it is listed as off and its settings stay on the pages; nothing is
routed to it, it is no launcher, and a call into it is refused as `NotRunning` (`it is turned off`) like any call that
cannot be answered. In Rust, `host.turn_off(name, manifest)` stops a process plugin and keeps it as off (false for one
compiled in, which is never turned off), `host.add_off(manifest)` adds one that is off, `host.is_off(name)` asks, and
`host.set_process(process)` turns it on by replacing it. The server builds its launchers again when a launcher is turned
off or on; a run that started keeps the launchers it started with.

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
