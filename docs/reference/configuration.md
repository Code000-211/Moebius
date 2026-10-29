# Configuration

The server reads one TOML file, `moebius.toml`, when it starts. This page is for whoever runs a Moebius server: it lists
every key with its type, default, meaning and validation. The code is in `crates/moebius-core/src/config/`.

## The file

- `moebius --config <file> serve` reads the file; the default is `moebius.toml` in the current directory.
  `moebius init` writes a commented example with every default to that path and refuses to overwrite an existing file.
  The example's one key that is not a default is `[harness] url`, which names the harness configuration repository
  `init` makes next to the file (see below).
- A file that does not exist means every default. An empty file is valid.
- Every key is optional unless it says required. Unknown keys and unknown tables are errors, so a typo is found at start.
- In TOML, keys outside any table must come before the first `[table]` header. `rules_file` is such a key.
- A bad file stops the server with a message that names the key.

Keys of a table are written with the table's name, such as `board.tick_ms`. A key of an array table is written
`router.upstreams.protocol`. A key of an agent is written `agents.<name>.model`, where `<name>` is the agent's name.

## The commands

`moebius [--config <file>] <command>`; the file is read by every command that needs it.

- `init` writes the commented example configuration. When a `pi-bolt` executable is on `PATH`, the file sets
  `[pi_bolt] program` to its absolute path, and `init` says so. It first makes `harness/` next to the file a new
  harness configuration repository (see [harness configuration](harness-configuration.md#a-repository-on-the-server)):
  a git repository on `main` with one commit holding an empty `rules.md`, an `mcp.toml` without servers and a
  `README.md`, and no agents. The file sets `[harness] url` to its absolute path, and the example's commented
  `[agents.*]` say that the agents are files there. `init` fails, and writes nothing, when `harness/` exists and is not
  empty.
- `serve` starts the server.
- `check-script <file> [--column <name>] [--title <text>] [--author <login>]` runs a column script once (see
  [column scripts](column-scripts.md)).
- `export --out <dir> [--kind <kind>]...` writes the training-set export of the store in `data_dir` to `<dir>`, all
  kinds or only the kinds given, and prints one line per file with its counts. It writes only to `<dir>`; it changes
  nothing in the store, and works while the server runs. It fails when `data_dir` has no store, or when a kind has no
  judgements. See [the training export](training-export.md).
- `api`, `board`, `inbox`, `chat`, `settings` and the other commands that operate a running server through the desktop
  API, with `--server`, `--token` and `--json`: see [the command line](cli.md).

## Paths

A relative path (including `agents.<name>.skills` and `extensions`) is resolved against the directory that holds the
configuration file, made absolute when the file is read. Exceptions:

- `token_file` is resolved against `data_dir`.
- `pi_bolt.program` is resolved only when it contains a directory (`./bin/pi-bolt`, `tools/pi-bolt`); a bare name such as
  `pi-bolt` is looked up on the `PATH`.

An absolute path is used as it is.

## Top-level keys

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `data_dir` | path | `.moebius` | Where Moebius keeps its files: `moebius.sqlite` (the store), `blobs/`, `repos/<name>` (the clones), `harness/` (the clone of the harness configuration repository), `worktrees/<name>/run-<id>` (`chat-<id>` for a chat), `chats/<id>` (the empty directory of a chat without a repository), `runs/<id>/` (a run's agent directory, bridge, sessions and `stderr.log`), `skills/<name>/` (the built-in skills, `moebius` and the task skills of the Moebius agent, written again at every start) and by default `token`. Created if missing. |
| `listen` | string | `127.0.0.1:7171` | The address of the desktop API, as `ip:port` (a host name is not accepted). Keep it on a private network. |
| `token_file` | path | `token` | The file with the token the desktop app sends. Created with a random token (64 hex digits, readable by the owner only) when missing or empty. |
| `run_limit` | integer | `3` | How many runs may go at once across all boards. At least 1, else the file is refused. Scripts read it as `moebius.run_limit`. |
| `retro_at` | string | `23:30` | When the daily Retro runs, as `HH:MM` with two digits each (hour 00 to 23, minute 00 to 59) in the server's local time. The Retro of a day starts when this time has passed and the day has none (the server looks once a minute, also when it starts), so a day whose time passed while the server was off is not caught up. See [harness configuration](harness-configuration.md#the-retro). |
| `rules_file` | path | `"rules.md"` | The server's rules, see [rules](rules.md), which the desktop app edits. A missing file means no server rules: only the built-in rules and the repositories' rules apply. An error together with `[harness]`: the repository's `rules.md` is the server's rules then. |

## `[harness]`

The harness configuration repository, see [harness configuration](harness-configuration.md). The table is optional.
Without it, agents and rules are configured here, and skills and MCP servers cannot be edited.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `harness.mcp_env` | array of strings | `[]` | The names of the environment variables of the server that `mcp.toml` of the repository may take values from, as `${NAME}`; any other name makes the server unusable. Each name is made of letters, digits and `_` and does not start with a digit, else the file is refused. What a server is given is visible to the runs of the agents that name it. |
| `harness.url` | string | required | The git URL of the repository, cloned to `<data_dir>/harness`, fetched every 30 seconds and pushed to by edits through the API. Not blank. With `[github]`, an `https://github.com/...` URL uses the App's token. With `[harness]`, `[agents.*]` and `rules_file` are errors: the agents are `agents/<name>.toml` and `agents/<name>.md` in the repository, the rules `rules.md`. |

### Setting it up from the app

A server without `[harness]` can make one: `POST /v1/harness/setup` (see the [desktop API](desktop-api.md)), the "Set
up" of the desktop app's Get started, Skills, MCP servers and Agents pages, which only a person can ask for. It makes
`harness/` next to `moebius.toml` a new repository, as `moebius init` does, with every `[agents.<name>]` as
`agents/<name>.toml` and `agents/<name>.md` and the file of `rules_file` (by default `rules.md` next to this file) as
`rules.md`. A skill an agent names by path is copied to `skills/<directory name>/`, and an extension to
`extensions/<file name>`. The file then gets `[harness] url` with the repository's absolute path, and loses
`[agents.*]` and `rules_file`; its other keys and comments stay. The server uses the repository after it restarts. The
old skill and extension files and the rules file are left where they were.

## `[trial_window]`

The daily stretch in which trials, replays and autoresearch may run without asking. Once a minute, inside the window, the
oldest open proposal is tried (see [the trials](harness-configuration.md#the-trials)), if no other is being tried, no
research is running and the board has a run to spare (`run_limit`); the replays follow (see `POST
/v1/judgements/{id}/replay`); and the oldest confirmed research starts (see
[autoresearch](harness-configuration.md#autoresearch)) when no proposal is being tried and no research is running. A trial that
is going on when the window ends finishes. The window may cross midnight (`22:00` to `04:30`); a window whose end is its
start is empty, so nothing starts.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `trial_window.start` | string | `00:00` | Start, as `HH:MM` as for `retro_at`. |
| `trial_window.end` | string | `06:00` | End, as `HH:MM`. |

## `[board]`

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `board.tick_ms` | integer | `2000` | Milliseconds between two runs of the column scripts over all cards. A value below 50 is raised to 50. |
| `board.round_limit` | integer | `5` | How many `code` rounds a card gets (counted from its latest retry) before the default `code.luau` marks it stuck. Scripts read it as `moebius.round_limit`. |

## `[budgets]`

Token budgets that are not an agent's own (see `agents.<name>.token_budget`). The table is optional.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `budgets.day_tokens` | integer | none | The most tokens (input plus output, all runs, teacher included) that may be used in a day of the server's local time. Once it is reached the router refuses model requests with HTTP 429 `budget_exceeded`, and the default scripts start no new run until the next day (a running run is not cut off). At least 1, else the file is refused. Without it there is no limit. Scripts read it as `moebius.day_budget`. |

## `[checks]`

A repository without a forge may name its own checks in `.moebius/checks` on its default branch, see
[column scripts](column-scripts.md).

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `checks.timeout_seconds` | integer | `1800` | How long the checks of one change may run. After that they are stopped, with everything they started, and count as failed. At least 1, else the file is refused. |

## `[router]`

The router is the only model provider the harnesses see, see [router](router.md). The check endpoints of the bridge are
served on the same listener.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `router.listen` | string | `127.0.0.1:7172` | The address of the router, as `ip:port`. It must be a loopback address (`127.0.0.1`, `::1`), else the file is refused. Port 0 picks a free port. |

### `[[router.upstreams]]`

Where the router sends requests, one table per upstream. A request goes to the first upstream whose protocol matches the
endpoint it came in on; without a match the router answers 502. Without any upstream, every model request fails.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `router.upstreams.protocol` | string | required | `openai` or `anthropic`. |
| `router.upstreams.base_url` | string | required | The URL the endpoint's path is appended to, such as `http://127.0.0.1:8080` or `https://api.anthropic.com`. A trailing slash is ignored. |
| `router.upstreams.api_key_env` | string | none | The name of the environment variable that holds the provider's key. The key is never in the file. If the variable is not set when the server starts, the server does not start. Without this key and without `api_key_secret`, no key is sent (a local server). The key is sent as `Authorization: Bearer` (`openai`) or `x-api-key` (`anthropic`). |
| `router.upstreams.api_key_secret` | string | none | The name of a stored key (see [Secrets](#secrets)) that holds the provider's key, for a key typed into the desktop app. A name is made of letters, digits, `-` and `_`. If no key is stored under it when the server starts, the server does not start. Setting it together with `api_key_env` refuses the file. The key is sent as for `api_key_env`. |

#### `[router.upstreams.oauth]`

An upstream that logs in with OAuth 2.0 instead of a key, such as a provider's subscription. Nothing of any provider is
built in: the client id and the URLs are the provider's, written here. Setting `oauth` together with `api_key_env` or
`api_key_secret` refuses the file. The login is started from the Models page of the desktop app (or
`POST /v1/models/upstreams/{place}/login`, see the [desktop API](desktop-api.md)) after the person confirms that using a
subscription through a third-party tool may break the provider's terms; `moebius doctor` repeats the warning. A server
whose upstream is not logged in starts, and the router answers its requests with `502` (see [router](router.md#oauth-logins)).

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `router.upstreams.oauth.flow` | string | required | `device`: the device authorization grant (RFC 8628), a code the person enters at the provider. `code`: an authorization code with PKCE (RFC 7636), the provider's page opened in the browser and the browser sent back to `http://127.0.0.1:<port><path>`. |
| `router.upstreams.oauth.client_id` | string | required | The OAuth client id; not empty. |
| `router.upstreams.oauth.token_url` | string | required | The `http` or `https` URL where codes and refresh tokens are exchanged for tokens. |
| `router.upstreams.oauth.device_url` | string | none | The device authorization URL; required by `device`. |
| `router.upstreams.oauth.authorize_url` | string | none | The provider's authorization page; required by `code`. |
| `router.upstreams.oauth.scopes` | array of strings | `[]` | The scopes asked for, sent space-separated as `scope`; none is sent when empty. |
| `router.upstreams.oauth.redirect_port` | integer | any free port | For `code`: the loopback port the browser is sent back to, for a provider that registers it. |
| `router.upstreams.oauth.redirect_path` | string | `/callback` | For `code`: the path the browser is sent back to; it starts with `/`. |
| `router.upstreams.oauth.secret` | string | required | The name the refresh token is kept under in `secrets.toml` (see [Secrets](#secrets)); letters, digits, `-` and `_`. |

## `[pi_bolt]`

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `pi_bolt.program` | path | `pi-bolt` | The Pi-Bolt executable. See Paths. |

## `[teacher]`

The teacher model checks the rules written in prose, see [rules](rules.md). The table is optional. Without it, a tool call
is refused whenever any prose rule applies.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `teacher.model` | string | required | The model's name, sent to the router. |
| `teacher.protocol` | string | `openai` | `openai` or `anthropic`: which router endpoint the teacher is asked through. The router needs an upstream for it. |
| `teacher.timeout_seconds` | integer | `20` | How long the teacher may take. From 1 to 29, else the file is refused: the plugin host cuts a decider off after 30 seconds. |

## `[decision_model]`

The decision model decides alongside the teacher and can take kinds of decision over, see
[the decision model](decision-model.md). The table is optional. Without it nothing is shadowed and nothing can be taken
over. An empty `url` or `model`, an unset variable named by `api_key_env`, or a key of the table that is not here refuses
the file or the start.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `decision_model.url` | string | required | The OpenAI-compatible base URL, such as `http://127.0.0.1:8081/v1`. The model is asked at `<url>/chat/completions`. |
| `decision_model.model` | string | required | The model's name, sent in the request. |
| `decision_model.api_key_env` | string | none | The name of the environment variable that holds the key, sent as a bearer token. The key is never read from the file, stored or logged. The server does not start when the variable is not set. |
| `decision_model.api_key_secret` | string | none | The name of a stored key (see [Secrets](#secrets)) that holds the key, instead of an environment variable. The server does not start when no key is stored under the name. Setting it together with `api_key_env` refuses the file. |
| `decision_model.timeout_seconds` | integer | `20` | How long the model may take. From 1 to 29, else the file is refused: the plugin host cuts a decider off after 30 seconds. |
| `decision_model.shadow` | array of strings | all kinds | The kinds to shadow. |
| `decision_model.agreement` | number | `0.95` | The share of shadows that must agree with the recorded choice for a takeover to be proposed. Above 0 and at most 1, else the file is refused. |
| `decision_model.min_judgements` | integer | `100` | How many shadows of a held-out judgement a kind needs before a takeover is proposed. At least 1, else the file is refused. |

## `[github]`

Moebius acts as its own GitHub App, with webhooks off; it polls. The table is optional. Without it, repositories on
GitHub cannot be polled or merged.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `github.app_id` | integer | required | The App's id. |
| `github.private_key_file` | path | required | The App's private key (PEM). Read when the server starts; the server does not start if it cannot be read or used. |
| `github.api_url` | string | `https://api.github.com` | The API's base URL, for GitHub Enterprise or a test double. |
| `github.poll_seconds` | integer | `30` | Seconds between two polls of the repositories on GitHub. A value below 1 is raised to 1. |

## `[agents.<name>]`

One table per agent; an error with `[harness]`, whose repository holds the agents (see [harness configuration](harness-configuration.md)). The default workflow starts an agent named `Coder`, so a server that runs it needs `[agents.Coder]`. It also starts an agent named `Reviewer` when there is one; without it changes are offered unreviewed.
A name is made of letters, digits, `-` and `_` and must not be empty, else the file is refused.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `agents.<name>.model` | string | required | The model's name, sent to the router and given to Pi-Bolt as `--model`. |
| `agents.<name>.protocol` | string | `openai` | `openai` or `anthropic`: which router endpoint the harness uses. |
| `agents.<name>.thinking` | string | none | Given to Pi-Bolt as `--thinking`, such as `low`, `medium` or `high`. Pi-Bolt checks the value. |
| `agents.<name>.instructions` | string | none | Given to Pi-Bolt as `--append-system-prompt`. |
| `agents.<name>.skills` | array of paths | `[]` | Each given to Pi-Bolt as `--skill`. |
| `agents.<name>.extensions` | array of paths | `[]` | Each given to Pi-Bolt as `--extension`, before the bridge, which is always loaded last. Pi loads no extension or skill it discovers on its own. |
| `agents.<name>.token_budget` | integer | none | The most tokens (input plus output, summed over all runs of the card, teacher included) a card may use. A model request over it is answered with HTTP 429 `budget_exceeded`, and the default scripts mark a card that reached it stuck instead of starting a run. Tokens used before the card's latest retry do not count. Without it there is no limit. A chat has no card, so its budget is the tokens of the chat's own run. Scripts read it as `moebius.agent_budget(name)`. |

### `[agents.Moebius]`

The Moebius agent is always available: a chat without `agent` is with it, and a script can start it by name. It is
bundled, so it needs no table, but it needs a model:

- With `[agents.Moebius]`, its `model`, `protocol`, `thinking` and `token_budget` come from there.
- Without it, its model and protocol are the Coder's (`[agents.Coder]`); it has no thinking level or budget of its own.
- With neither, starting a chat with it fails with status 422 and a message that names both tables.

Its instructions are built in (who it is, that it works for the person in the chat and builds what they ask for with the
`moebius` skill); `agents.Moebius.instructions` is added after them. Its skills always begin with the `moebius` skill,
which is the [guide](../guide/README.md) and this reference, and the task skills `moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions` and `moebius-plugins`, each with the steps of one
kind of task, all written to `<data_dir>/skills/<name>/` when the server starts; `agents.Moebius.skills` adds more, and `agents.Moebius.extensions` adds extensions. The keys are the same as for
any agent. What a chat with it leaves in its repository is offered as a change when the chat ends, see the
[desktop API](desktop-api.md).

### `[agents.Grilling-partner]`

The Grilling partner is bundled like the Moebius agent and configured the same way: `[agents.Grilling-partner]` gives it a
model, protocol, thinking level, token budget, instructions (added after the built-in ones), skills and extensions;
without the table it takes the Coder's model and protocol, and with neither a chat with it is refused with status 422. It
is the agent of a grilling, see the [desktop API](desktop-api.md) and [Grilling](harness-configuration.md#grilling).

## Editing the file from the app

The desktop app edits every key of this page through `PUT /v1/settings` (see the [desktop API](desktop-api.md)). The
server changes the text of the file in place, so comments and the order of the keys stay, refuses a result that
`moebius serve` would refuse (status 422, nothing written), writes the file in one step, and records the decision
(kind `edit_settings`). A key is addressed by the name this page gives it, `board.round_limit`, and a table by its
name (`teacher`), a `[[router.upstreams]]` table by its position (`router.upstreams.0`) and an agent by its name
(`agents.Coder`).

A change applies to the running server at once for these keys:

- `run_limit`, `board.round_limit` and `budgets.day_tokens`: the limits column scripts and the router read;
- `board.tick_ms`, `retro_at`, `trial_window.start` and `trial_window.end`: read by the board's loop and the schedule;
- `router.upstreams`, with their keys;
- `agents.<name>`, on a server without `[harness]`.

Every other key is written to the file and used after the server restarts: `data_dir`, `listen`, `token_file`,
`rules_file`, `checks.timeout_seconds`, `router.listen`, `pi_bolt.program`, `harness.*`, `github.*`, `teacher.*` and
`decision_model.*`. The answer of `PUT /v1/settings` and `GET /v1/settings` lists the ones that differ from what the
running server started with, and `POST /v1/server/restart` restarts it.

### Secrets

A key typed into the app is stored under a name in `secrets.toml` in `data_dir`, a TOML table of names and keys that only
its owner can read (mode 0600). `moebius.toml` names it with `api_key_secret`; the key itself is never in
`moebius.toml`, in an answer of the API, in the store, in an event or in a log. A key is stored with `PUT
/v1/secrets/{name}` and removed with `DELETE /v1/secrets/{name}`; removing one that `api_key_secret` still names makes
the next start fail, as an unset `api_key_env` variable does. The refresh token of an OAuth login is kept the same way,
under `router.upstreams.oauth.secret`, and replaced when the provider rotates it.

## Environment variables

- The variables named by `router.upstreams.api_key_env` and `decision_model.api_key_env`. Nothing else is read from the
  environment by the server.
- `MOEBIUS_PI_BOLT` is read only by the end-to-end tests in `crates/moebius-core/tests`; the server ignores it.
- The server sets `PI_CODING_AGENT_DIR`, `MOEBIUS_CHECK_URL` and `MOEBIUS_RUN_TOKEN` for each Pi-Bolt process, see the
  [bridge protocol](bridge-protocol.md). It also sets `MOEBIUS`, the path of the running `moebius` executable, so that
  an agent can run `"$MOEBIUS" check-script <file>` (see [column scripts](column-scripts.md)). That path is not a key of
  the file.

## A minimal file

A local OpenAI-compatible server upstream, the default workflow's Coder, and nothing else:

```toml
[[router.upstreams]]
protocol = "openai"
base_url = "http://127.0.0.1:8080"

[pi_bolt]
program = "/usr/local/bin/pi-bolt"

[agents.Coder]
model = "my-local-model"
token_budget = 2000000
```
