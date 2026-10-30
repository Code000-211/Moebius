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
- `pi_bolt.program` and `claude_code.program` are resolved only when they contain a directory (`./bin/pi-bolt`,
  `tools/pi-bolt`); a bare name such as `pi-bolt` is looked up on the `PATH`. `plugins.path` is resolved the same way.

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

## `[regression]`

The regression gate of the trials (see [the regression set](harness-configuration.md#the-regression-set)). Besides the
cards whose errors it addresses, a proposal is tried on the regression set of each repository of its cards: the latest
`size` cards merged there, those whose change adds a reproduction test (`.moebius/reproduce`) first. It is positive only
when none of them fails where it passed before. The regression trials run one after the other on the proposal's thread,
like its other trials, and stop at the first that fails, so a proposal adds at most `size` runs for each repository of its
cards; one proposal is tried at a time, and only when the board has a run to spare.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `regression.size` | integer | `5` | How many merged cards of a repository the set holds, at least 0; 0 tries none. |
| `regression.gate` | boolean | `true` | Whether proposals are tried on the regression set at all. With `false`, a proposal is judged on its own cards only. |

## `[localize]`

The hint a `code` run is given: the files of the repository that its issue is likely to touch, ranked by the decision point
`localize` (see [the decision model](decision-model.md#localize)). The table is optional; a change needs a restart.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `localize.enabled` | boolean | `true` | Whether a run is given the hint. With `false` it starts without, and no judgement of `localize` is recorded. |
| `localize.files` | integer | `10` | How many files the hint names at most, from 1 to 10. |

A repository decides for itself in the file `.moebius/localize` on its default branch (as `.moebius/test-first` turns test
first on), over these keys: lines that are empty or start with `#` are skipped, `on` and `off` turn the hint on and off,
and `files <n>` (1 to 10) says how many files it names. A line that says anything else is ignored. The default branch is
the repository's workflow: a repository that wants no hint for its coding writes `off` there.

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
[column scripts](column-scripts.md). The same limit holds for the reproduction test of a repository that works test
first, on any forge. Both run with the server's environment cleared but for `PATH`, `HOME`, `USER`, `LANG` and
`TMPDIR`.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `checks.timeout_seconds` | integer | `1800` | How long the checks of one change, or one run of its reproduction test, may run. After that they are stopped, with everything they started, and count as failed. At least 1, else the file is refused. |

## `[router]`

The router is the only model provider the harnesses see, see [router](router.md). The check endpoints of the bridge are
served on the same listener.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `router.listen` | string | `127.0.0.1:7172` | The address of the router, as `ip:port`. It must be a loopback address (`127.0.0.1`, `::1`), else the file is refused. Port 0 picks a free port. |

### `[[router.upstreams]]`

Where the router sends requests, one table per upstream. A request goes to the first upstream whose protocol matches the
endpoint it came in on, and to the next of that protocol when one fails before it answers (see [router](router.md#choosing-the-upstream));
without a match the router answers 502. Without any upstream, every model request fails.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `router.upstreams.protocol` | string | required | `openai` or `anthropic`. |
| `router.upstreams.base_url` | string | required | The URL the endpoint's path is appended to, such as `http://127.0.0.1:8080` or `https://api.anthropic.com`. A trailing slash is ignored. |
| `router.upstreams.api_key_env` | string | none | The name of the environment variable that holds the provider's key. The key is never in the file. If the variable is not set when the server starts, the server does not start. Without this key and without `api_key_secret`, no key is sent (a local server). The key is sent as `Authorization: Bearer` (`openai`) or `x-api-key` (`anthropic`). |
| `router.upstreams.api_key_secret` | string | none | The name of a stored key (see [Secrets](#secrets)) that holds the provider's key, for a key typed into the desktop app. A name is made of letters, digits, `-` and `_`. If no key is stored under it when the server starts, the server does not start. Setting it together with `api_key_env` refuses the file. The key is sent as for `api_key_env`. |

#### `[router.upstreams.oauth]`

An upstream that logs in with OAuth 2.0 instead of a key, such as a provider's subscription. `preset` names a provider
that is built in and fills in every key that is not written; for any other provider the client id and the URLs are
written here. Setting `oauth` together with `api_key_env` or
`api_key_secret` refuses the file. The login is started from the Models page of the desktop app (or
`POST /v1/models/upstreams/{place}/login`, see the [desktop API](desktop-api.md)) after the person confirms that using a
subscription through a third-party tool may break the provider's terms; `moebius doctor` repeats the warning. A server
whose upstream is not logged in starts, and the router answers its requests with `502` (see [router](router.md#oauth-logins)).

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `router.upstreams.oauth.preset` | string | none | A built-in provider: `anthropic` or `openai` (the table below). It fills in `flow`, `client_id`, `token_url`, `authorize_url`, `scopes`, `redirect_port`, `redirect_path` and `secret` (`oauth-<preset>`), and decides what the router sends with the token (see [router](router.md#oauth-requests-per-provider)). A key that is written wins. An unknown name refuses the file. Without a preset `flow`, `client_id`, `token_url` and `secret` are required. |
| `router.upstreams.oauth.flow` | string | required without `preset` | `device`: the device authorization grant (RFC 8628), a code the person enters at the provider. `code`: an authorization code with PKCE (RFC 7636), the provider's page opened in the browser and the browser sent back to `http://127.0.0.1:<port><path>`. |
| `router.upstreams.oauth.client_id` | string | required without `preset` | The OAuth client id; not empty. |
| `router.upstreams.oauth.token_url` | string | required without `preset` | The `http` or `https` URL where codes and refresh tokens are exchanged for tokens. |
| `router.upstreams.oauth.device_url` | string | none | The device authorization URL; required by `device`. |
| `router.upstreams.oauth.authorize_url` | string | none | The provider's authorization page; required by `code`. |
| `router.upstreams.oauth.scopes` | array of strings | `[]` | The scopes asked for, sent space-separated as `scope`; none is sent when empty. |
| `router.upstreams.oauth.redirect_port` | integer | any free port | For `code`: the loopback port the browser is sent back to, for a provider that registers it. |
| `router.upstreams.oauth.redirect_path` | string | `/callback` | For `code`: the path the browser is sent back to; it starts with `/`. |
| `router.upstreams.oauth.secret` | string | required without `preset` | The name the refresh token is kept under in `secrets.toml` (see [Secrets](#secrets)); letters, digits, `-` and `_`. |

The presets, as Pi (commit `f09d01b85e0384647fcab6779d35ab9dabe04be3` of `badlogic/pi-mono`) does. The client ids are
those of Claude Code and of the Codex CLI, another app's: the provider may block an account that logs in with them, and
the Models page says so before the login. They are public values, not secrets. Write `protocol` and `base_url` of the
upstream as in the table (`GET /v1/models/providers` lists them).

| `preset` | Subscription | `protocol` and `base_url` | Login |
|---|---|---|---|
| `anthropic` | Claude Pro/Max | `anthropic`, `https://api.anthropic.com` | `code` at `https://claude.ai/oauth/authorize`, tokens at `https://platform.claude.com/v1/oauth/token` (JSON body), client id `9d1c250a-e61b-44d9-88ed-5944d1962f5e`, redirect `http://localhost:53692/callback` (another free port when that one is taken), the scopes `org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload`, and `code=true` on the authorization URL. |
| `openai` | ChatGPT Plus/Pro (Codex) | `openai`, `https://chatgpt.com/backend-api` | `code` at `https://auth.openai.com/oauth/authorize`, tokens at `https://auth.openai.com/oauth/token`, client id `app_EMoamEEZ73f0CkXaXp7hrann`, redirect `http://localhost:1455/auth/callback` (without a listener, so that only a pasted address finishes the login, when that port is taken), the scopes `openid profile email offline_access`, and `id_token_add_organizations=true`, `codex_cli_simplified_flow=true`, `originator=moebius`. Its requests are translated to the Codex Responses protocol the ChatGPT backend takes, and its models are the subscription's: see [router](router.md#oauth-requests-per-provider). |

```toml
[[router.upstreams]]
protocol = "anthropic"
base_url = "https://api.anthropic.com"
[router.upstreams.oauth]
preset = "anthropic"
```

### `[router.classes]`

A model class is a name, such as `fast`, `strong` or `local`, for an ordered list of models. An agent's `model`, the
teacher's `model` and a chat's model may name a class instead of a model; the router serves the class as a model name
(see [router](router.md#model-classes)): a request for it goes to the first of its models that has tokens left today,
and falls back to the next when an upstream fails before it answers. A class takes the place of a model of the same
name. The desktop app's model pickers offer the classes before the models. The decision model is not asked through
the router, so it cannot name a class.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `router.classes.<name>` | array | none | The models of the class `<name>` (letters, digits, `-` and `_`), first tried first, such as `router.classes.fast` or `router.classes.strong`. Each is a model's name as the upstream knows it, or a table `{ model = "...", day_tokens = N }` whose `day_tokens` (at least 1) is the most tokens (input plus output) that model may use in a day of the server's local time, counted over all its usage, whatever asked for it; a model at or above it is passed over. An empty list, a model without a name or a table with another key refuses the file. |

## `[pi_bolt]`

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `pi_bolt.program` | path | `pi-bolt` | The Pi-Bolt executable. See Paths. |

## `[claude_code]`

Claude Code's command line, which runs the agents whose `harness` is `claude-code` (see `agents.<name>.harness`).

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `claude_code.program` | path | `claude` | The Claude Code executable. Resolved as `pi_bolt.program` is (see Paths). Only started for an agent on `claude-code`, so a server without such an agent needs none. |

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

## `[[plugins]]`

A plugin in its own process: a program, in any language, that speaks the JSON Lines of the
[plugin API](plugin-api.md#plugins-in-their-own-process). Each entry is started when the server starts, in the directory
of this file, and the server waits for its manifest (at most 30 seconds) before it serves. A plugin that does not start,
exits, or writes a line that is not a reply is started again after a pause of 1 second, doubled each time up to a
minute; meanwhile every call into it is refused, so what it decides takes the fail-closed option. Its standard error,
and a line each time it stops, go to `<data_dir>/plugins/<name>.log` with secrets redacted. The Plugins page of the app
and `GET /v1/plugins` show its state. There are none by default.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `plugins.name` | string | required | The plugin's name: letters, digits, `-` and `_`, not `guard` or `decision-model`, and not the name of another entry, else the file is refused. The manifest the program sends must give the same name, else it counts as not started. |
| `plugins.path` | path | required | The program. Empty refuses the file. See [Paths](#paths). |
| `plugins.args` | array of strings | `[]` | The program's arguments. |
| `plugins.decides` | array of strings | `[]` | The kinds of decision the plugin answers, such as `start_card`. A kind named by two entries refuses the file. A kind has one decider, so naming `tool_call` replaces the guard, with the built-in rules and the teacher. |

## `[[runners]]`

A machine that runs the harness of a card's runs over SSH, instead of the server. An agent whose `runner` names it, or a
column script that passes its name to `moebius.start_run`, sends its runs there (see [running on another
machine](../guide/running-moebius.md#running-on-another-machine)); a run without a runner goes on the server. The
machine needs `sshd` with key login, `git`, `tar` and `sh`, and the harness (`pi-bolt` or `claude`); it needs no access
to the server's network. Runners are read when the server starts. There are none by default.

How a run goes there: the server opens a reverse tunnel (`ssh -N -R 0:<router>`) to the router's listener, which gives
the machine a loopback port that reaches the router, the tool-call check and the MCP gateway; pushes the run's commit
over SSH as the branch `moebius/run-<id>` to a bare clone `<working_dir>/repos/repo-<repository id>.git`, which the
machine keeps between runs; clones it to `<working_dir>/runs/<id>/work`; copies what the harness is given from the
server (its agent directory with `models.json` and `mcp.json`, the bridge, the skills and extensions) to
`<working_dir>/runs/<id>/files/`, at the paths the server has them under; and starts the harness there with one more
`ssh`, whose standard input and output are the harness's (so steering and stopping work as they do locally). Its
environment is the one a local run has, with the router's URL the tunnel's, and without `MOEBIUS`, `MOEBIUS_URL` and
`MOEBIUS_TOKEN`: the desktop API is not tunnelled, so the Moebius agent's own commands do not work there. When the run
ends what the agent left is committed there, fetched into the run's worktree on the server, and committed from there as
for any run; the run's directory on the machine and its branch in the clone are removed, and the tunnel is closed. A
connection to the machine that breaks ends the run as failed. Only the runs of a card (`code`, `review` and the other
purposes of column scripts, and trials) go to a runner; chats, the Retro, the Autoresearcher and the Grilling partner
always run on the server. `ssh` is started without a terminal and asks no question (`BatchMode`), so the machine's host
key must be in the server user's `known_hosts`.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `runners.name` | string | required | The name `agents.<name>.runner` and `moebius.start_run` use: letters, digits, `-` and `_`, and not the name of another entry, else the file is refused. |
| `runners.host` | string | required | The machine's host name or address. One word that does not start with `-`. |
| `runners.user` | string | none | The user to log in as; without one, the SSH configuration's. One word that does not start with `-`. |
| `runners.port` | integer | none | The SSH port; without one, the SSH configuration's. |
| `runners.key_file` | path | none | The private key to log in with (`ssh -i`, with `IdentitiesOnly`); without one, the SSH configuration's and the agent's. Resolved against the file's directory. |
| `runners.working_dir` | string | required | An absolute path on the machine, without quotes or line breaks, under which the clones and the runs' directories are kept. Created if missing. |
| `runners.max_runs` | integer | `1` | How many runs may go on the machine at once, at least 1. A run that would be one more is not started: `moebius.start_run` raises `the runner <name> is busy: <n> of <n> runs are going`, and no run is made. `GET /v1/runners` says how many go now. |
| `runners.bin_dir` | string | none | An absolute path on the machine where `pi-bolt` and `claude` are; without it they are looked up on the machine's `PATH` for a non-interactive login. |
| `runners.ssh` | path | `ssh` | The `ssh` program. See [Paths](#paths). |

## `[github]`

Moebius acts as its own GitHub App, with the App's own webhooks off; it polls, and a webhook set up on the repository only
wakes the poll (see [Webhooks](#webhooks)). The table is optional. Without it, repositories on
GitHub cannot be polled or merged.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `github.app_id` | integer | required | The App's id. |
| `github.private_key_file` | path | required | The App's private key (PEM). Read when the server starts; the server does not start if it cannot be read or used. |
| `github.api_url` | string | `https://api.github.com` | The API's base URL, for GitHub Enterprise or a test double. |
| `github.poll_seconds` | integer | `30` | Seconds between two polls of the repositories on GitHub. A value below 1 is raised to 1. |

## `[gitlab]`

Repositories on GitLab, gitlab.com or a GitLab hosted elsewhere, are reached with an access token: a personal, project
or group access token with the scope `api`. The server polls; a webhook only wakes the poll (see [Webhooks](#webhooks)). The table is optional. Without it,
repositories on GitLab cannot be polled or merged. The token is sent to the API as `PRIVATE-TOKEN`, and to git (as the
password of the user `oauth2`) only for remotes whose URL starts with `gitlab.url` followed by `/`.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `gitlab.url` | string | `https://gitlab.com` | The GitLab's web address; its API is `<url>/api/v4`. A trailing `/` is dropped. Not blank. |
| `gitlab.token_file` | path | none | The file holding the access token, relative to this file's directory. Read when the server starts; the server does not start if it cannot be read or is empty. |
| `gitlab.token_secret` | string | none | The name of the secret holding the access token (see [Secrets](#secrets)), instead of `token_file`. Exactly one of the two is set, else the file is refused; a secret that is not stored stops the server from starting. |
| `gitlab.poll_seconds` | integer | `30` | Seconds between two polls of the repositories on GitLab. A value below 1 is raised to 1. |

## `[forgejo]`

Repositories on Forgejo or Gitea, Codeberg or one hosted elsewhere, are reached with an access token with the scopes
`write:issue` and `write:repository`. The server polls; a webhook only wakes the poll (see [Webhooks](#webhooks)). The table is optional. Without it,
repositories on Forgejo cannot be polled or merged. The token is sent to the API (`<url>/api/v1`) as
`Authorization: token <token>`, and to git (as the user name, with the password `x-oauth-basic`) only for remotes whose
URL starts with `forgejo.url` followed by `/`. Forgejo tokens are 40 hexadecimal digits, the same shape as a commit id,
so the redaction cannot recognise one in text: keep the token in its file or secret.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `forgejo.url` | string | `https://codeberg.org` | The forge's web address; its API is `<url>/api/v1`. A trailing `/` is dropped. Not blank. |
| `forgejo.token_file` | path | none | The file holding the access token, relative to this file's directory. Read when the server starts; the server does not start if it cannot be read or is empty. |
| `forgejo.token_secret` | string | none | The name of the secret holding the access token (see [Secrets](#secrets)), instead of `token_file`. Exactly one of the two is set, else the file is refused; a secret that is not stored stops the server from starting. |
| `forgejo.poll_seconds` | integer | `30` | Seconds between two polls of the repositories on Forgejo. A value below 1 is raised to 1. |

## `[agents.<name>]`

One table per agent; an error with `[harness]`, whose repository holds the agents (see [harness configuration](harness-configuration.md)). The default workflow starts an agent named `Coder`, so a server that runs it needs `[agents.Coder]`. It also starts an agent named `Reviewer` when there is one, for both review axes unless agents named `Standards` and `Spec` exist; without a `Reviewer` changes are offered unreviewed.
A name is made of letters, digits, `-` and `_` and must not be empty, else the file is refused.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `agents.<name>.model` | string | required | The model's name, sent to the router and given to Pi-Bolt as `--model`. |
| `agents.<name>.protocol` | string | `openai` | `openai` or `anthropic`: which router endpoint the harness uses. |
| `agents.<name>.harness` | string | `pi-bolt` | The harness the agent runs on: `pi-bolt`, or `claude-code` for Claude Code's command line (see [Claude Code](#claude-code)). An agent on `claude-code` must have `protocol = "anthropic"` and no `extensions`, else the file is refused. A built-in agent without its own table runs on the Coder's harness. |
| `agents.<name>.runner` | string | none | The name of a `[[runners]]` entry the agent's runs go to, over SSH; without it they run on the server. A name that is no runner refuses the file. A column script can send a run elsewhere with the fourth argument of `moebius.start_run`. |
| `agents.<name>.thinking` | string | none | Given to Pi-Bolt as `--thinking`, such as `low`, `medium` or `high`. Pi-Bolt checks the value. Claude Code is not given it. |
| `agents.<name>.instructions` | string | none | Given to Pi-Bolt as `--append-system-prompt`. |
| `agents.<name>.skills` | array of paths | `[]` | Each given to Pi-Bolt as `--skill`. |
| `agents.<name>.extensions` | array of paths | `[]` | Each given to Pi-Bolt as `--extension`, before the bridge, which is always loaded last. Pi loads no extension or skill it discovers on its own. |
| `agents.<name>.token_budget` | integer | none | The most tokens (input plus output, summed over all runs of the card, teacher included) a card may use. A model request over it is answered with HTTP 429 `budget_exceeded`, and the default scripts mark a card that reached it stuck instead of starting a run. Tokens used before the card's latest retry do not count. Without it there is no limit. A chat has no card, so its budget is the tokens of the chat's own run. Scripts read it as `moebius.agent_budget(name)`. |
| `agents.<name>.engineering_skills` | boolean | `true` | Whether the agent is given the engineering skills Moebius brings for it, with the skill `moebius-autonomous` that says how to use them without a person to ask: for the `Coder` `tdd` and `diagnosing-bugs`, for the `Reviewer` `code-review`, for the `Grilling-partner` `grilling` and `to-tickets`. They come before the agent's own `skills`. `false` gives the agent none of them. An agent with another name has none either way. See [Engineering skills](harness-configuration.md#engineering-skills). |
| `agents.<name>.tool_output.hide_after_turns` | integer | `10` | The output of a tool call older than this many turns (model responses) is replaced, in the requests to the model, by a placeholder: `[<tool> <input>: output hidden: <n> lines]`. Errors are never hidden, and the session and the run's events keep every output. `0` keeps every output. The bridge does it, through Pi's `context` hook. |
| `agents.<name>.tool_output.max_lines` | integer | `200` | The most lines of one tool result the model reads. Above it (or above `max_bytes`), the result's head and tail are kept, half the budget each, around `[<n> lines left out; the full output is in .moebius/out/<k>.txt]`. `0` is no limit. |
| `agents.<name>.tool_output.max_bytes` | integer | `16384` | The most bytes of one tool result, as `max_lines`. A result whose lines are each too long keeps its first and last bytes and says how many bytes were left out. `0` is no limit. |
| `agents.<name>.tool_output.compress` | boolean | `true` | Whether escape codes and progress bars are removed from every result, runs of the same line folded into one with `(×<n>)`, and the output of well-known commands compressed: `cargo test`, `pytest`, `go test`, `jest` and `vitest` (also as `npm test`) to their failures and summary, cargo's builds to their errors and warnings with repeats folded. A compressor only removes lines it knows to be noise, so no error message and no failing test's output is lost. |

The `tool_output` table keeps what the model reads of tools small (Pi-Bolt already shows at most 2000 lines or 50 KB of
a `bash` or `read` result; these budgets are tighter). It applies to Pi-Bolt runs only: the bridge does it, and an agent
with `harness = "claude-code"` has no bridge, so its table is accepted and has no effect. When lines are left out, the whole output, without secrets, is in
`.moebius/out/<k>.txt` of the run's worktree, which the agent can read in ranges; that directory ignores itself in git,
so it is never part of the change, and it goes with the worktree. Each result made smaller is a `tool_output` run event
(see the [bridge protocol](bridge-protocol.md)), and `GET /v1/usage` adds up what they saved.

```toml
[agents.Coder]
model = "my-local-model"

[agents.Coder.tool_output]
hide_after_turns = 0      # keep every output in the context
max_lines = 400
compress = false
```

### `[agents.Moebius]`

The Moebius agent is always available: a chat without `agent` is with it, and a script can start it by name. It is
bundled, so it needs no table, but it needs a model:

- With `[agents.Moebius]`, its `model`, `protocol`, `thinking` and `token_budget` come from there.
- Without it, its model and protocol are the Coder's (`[agents.Coder]`); it has no thinking level or budget of its own.
- With neither, starting a chat with it fails with status 422 and a message that names both tables.

Its instructions are built in (who it is, that it works for the person in the chat and builds what they ask for with the
`moebius` skill); `agents.Moebius.instructions` is added after them. Its skills always begin with the `moebius` skill,
which is the [guide](../guide/README.md) and this reference, and the task skills `moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions`, `moebius-plugins` and `moebius-report`, each with the steps of one
kind of task, all written to `<data_dir>/skills/<name>/` when the server starts; `agents.Moebius.skills` adds more, and `agents.Moebius.extensions` adds extensions. The keys are the same as for
any agent. What a chat with it leaves in its repository is offered as a change when the chat ends, see the
[desktop API](desktop-api.md).

### `[agents.Grilling-partner]`

The Grilling partner is bundled like the Moebius agent and configured the same way: `[agents.Grilling-partner]` gives it a
model, protocol, thinking level, token budget, instructions (added after the built-in ones), skills and extensions;
without the table it takes the Coder's model and protocol, and with neither a chat with it is refused with status 422. It
is the agent of a grilling, see the [desktop API](desktop-api.md) and [Grilling](harness-configuration.md#grilling).


### Claude Code

An agent whose `harness` is `claude-code` runs as `claude -p --input-format stream-json --output-format stream-json
--verbose` in the run's worktree, with:

- `--model` the agent's model, `--append-system-prompt` its instructions, `--session-id` a new session (`--resume` it
  for a chat given another model), `--permission-mode bypassPermissions`, `--setting-sources user` and
  `--strict-mcp-config`, and `--mcp-config <data_dir>/runs/<id>/mcp.json` when the agent has MCP servers (written as for
  Pi-Bolt, readable by the server's user only);
- `ANTHROPIC_BASE_URL` the router and `ANTHROPIC_API_KEY` the run's token (`ANTHROPIC_AUTH_TOKEN`,
  `CLAUDE_CODE_USE_BEDROCK` and `CLAUDE_CODE_USE_VERTEX` removed), `CLAUDE_CONFIG_DIR` the run's own
  `<data_dir>/runs/<id>/claude/` with each of the agent's skills linked (copied on Windows) into `skills/`,
  `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, and `MOEBIUS`, `MOEBIUS_RUN_TOKEN` (and for the Moebius agent
  `MOEBIUS_URL`, `MOEBIUS_TOKEN`) as a Pi-Bolt run has them.

Claude Code has no bridge, so the router checks the tool calls of its model's answers instead (see
[the router](router.md#the-check-of-a-harness-without-a-bridge)). Its records are kept as the run events a Pi-Bolt run
has (see the [desktop API](desktop-api.md)). A chat on Claude Code has no terminal interface, keeps its process while it
waits, and ignores the reasoning level.
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
- `regression.size` and `regression.gate`: read when a proposal's trials start;
- `router.upstreams`, with their keys;
- `router.classes`, set as a whole table (`{"fast": ["a", {"model": "b", "day_tokens": 100}]}`);
- `agents.<name>`, on a server without `[harness]`.

Every other key is written to the file and used after the server restarts: `data_dir`, `listen`, `token_file`,
`rules_file`, `checks.timeout_seconds`, `router.listen`, `pi_bolt.program`, `harness.*`, `github.*`, `gitlab.*`,
`forgejo.*`, `teacher.*`, `decision_model.*` and `plugins` (an entry is addressed by its position, `plugins.0`). The answer of `PUT /v1/settings` and `GET /v1/settings` lists the ones that differ from what the
running server started with, and `POST /v1/server/restart` restarts it.

### Secrets

A key typed into the app is stored under a name in `secrets.toml` in `data_dir`, a TOML table of names and keys that only
its owner can read (mode 0600). `moebius.toml` names it with `api_key_secret`, the GitLab token with
`gitlab.token_secret` and the Forgejo token with `forgejo.token_secret`; the key itself is never in `moebius.toml`, in an answer of the API, in the store, in an event or
in a log. A key is stored with `PUT /v1/secrets/{name}` and removed with `DELETE /v1/secrets/{name}`; removing one that
`api_key_secret`, `gitlab.token_secret` or `forgejo.token_secret` still names makes the next start fail, as an unset `api_key_env` variable does.
The refresh token of an OAuth login is kept the same way, under `router.upstreams.oauth.secret`, and replaced when the
provider rotates it.

### Webhooks

A forge that can reach the server may send webhooks, which save the wait for the next poll. There is no key for them in
`moebius.toml`: a forge takes webhooks when a secret named `webhook-github`, `webhook-gitlab` or `webhook-forgejo` is
stored (for a forge that is set up above), and without it `POST /v1/webhooks/{forge}` answers `404`. The server makes the
secret (`POST /v1/webhooks/{forge}/secret`, the button of the Settings page, which also shows it to set on the forge), or a
person stores one with `PUT /v1/secrets/{name}`. A generated secret is `whsec_` and 64 hexadecimal digits, which
`blobs::redact` recognises; a secret of another shape is remembered by its value once it is used. It is never in
`moebius.toml`, and only a person can read it from the API.

On the forge, add a webhook for the repository (or the group) with the URL `<server address>/v1/webhooks/<forge>`, the
content type `application/json`, the events issues, pull requests, pushes, reviews and checks, and the secret:

| Forge | `<forge>` | The forge proves a delivery with |
|---|---|---|
| GitHub | `github` | `X-Hub-Signature-256: sha256=<hex>`, the HMAC-SHA256 of the body under the secret. |
| GitLab | `gitlab` | `X-Gitlab-Token`, the secret itself. |
| Forgejo, Gitea | `forgejo` | `X-Gitea-Signature`, the HMAC-SHA256 of the body as hexadecimal digits. |

The server reads nothing from a delivery as data. A verified one only makes the poll of that forge run at once, for the
repository the body names (`repository.full_name`, GitLab's `project.path_with_namespace`) or for all of them when it
names none; a delivery that is not verified is answered with `401` and changes nothing. Polling goes on at
`poll_seconds`, as the safety net for deliveries that do not arrive. See [ADR 0012](../adr/0012-forge-webhooks-wake-the-poll.md).

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
