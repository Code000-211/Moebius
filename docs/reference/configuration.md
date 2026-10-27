# Configuration

The server reads one TOML file, `moebius.toml`, when it starts. This page is for whoever runs a Moebius server: it lists
every key with its type, default, meaning and validation. The code is in `crates/moebius-core/src/config.rs`.

## The file

- `moebius --config <file> serve` reads the file; the default is `moebius.toml` in the current directory.
  `moebius init` writes a commented example with every default to that path and refuses to overwrite an existing file.
- A file that does not exist means every default. An empty file is valid.
- Every key is optional unless it says required. Unknown keys and unknown tables are errors, so a typo is found at start.
- In TOML, keys outside any table must come before the first `[table]` header. `rules_file` is such a key.
- A bad file stops the server with a message that names the key.

Keys of a table are written with the table's name, such as `board.tick_ms`. A key of an array table is written
`router.upstreams.protocol`. A key of an agent is written `agents.<name>.model`, where `<name>` is the agent's name.

## Paths

A relative path is resolved against the directory that holds the configuration file, made absolute when the file is
read. Exceptions:

- `token_file` is resolved against `data_dir`.
- `pi_bolt.program` is not resolved at all: a bare name such as `pi-bolt` is looked up on the `PATH`, and a relative
  path with a slash is relative to the server's working directory. Use an absolute path.
- `agents.<name>.skills` and `agents.<name>.extensions` are passed to the harness as written. Use absolute paths.

An absolute path is used as it is.

## Top-level keys

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `data_dir` | path | `.moebius` | Where Moebius keeps its files: `moebius.sqlite` (the store), `blobs/`, `repos/<name>` (the clones), `worktrees/<name>/run-<id>`, `runs/<id>/` (a run's agent directory, bridge, sessions and `stderr.log`) and by default `token`. Created if missing. |
| `listen` | string | `127.0.0.1:7171` | The address of the desktop API, as `ip:port` (a host name is not accepted). Keep it on a private network. |
| `token_file` | path | `token` | The file with the token the desktop app sends. Created with a random token (64 hex digits, readable by the owner only) when missing or empty. |
| `run_limit` | integer | `3` | How many runs may go at once across all boards. At least 1, else the file is refused. Scripts read it as `moebius.run_limit`. |
| `retro_at` | string | `23:30` | When the daily Retro runs, as `HH:MM` with two digits each (hour 00 to 23, minute 00 to 59) in the server's local time. Read and checked, but nothing uses it yet. |
| `rules_file` | path | none | The server's rules, see [rules](rules.md). Without it only the built-in rules and the repositories' rules apply. A missing file at run time means no rules. |

## `[trial_window]`

The daily stretch in which trials, replays and autoresearch may run without asking. Read and checked, but nothing uses it
yet.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `trial_window.start` | string | `00:00` | Start, as `HH:MM` as for `retro_at`. |
| `trial_window.end` | string | `06:00` | End, as `HH:MM`. |

## `[board]`

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `board.tick_ms` | integer | `2000` | Milliseconds between two runs of the column scripts over all cards. A value below 50 is raised to 50. |
| `board.round_limit` | integer | `5` | How many `code` rounds a card gets before the default `code.luau` marks it stuck. Scripts read it as `moebius.round_limit`. |

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
| `router.upstreams.api_key_env` | string | none | The name of the environment variable that holds the provider's key. The key is never in the file. If the variable is not set when the server starts, the server does not start. Without this key, no key is sent (a local server). The key is sent as `Authorization: Bearer` (`openai`) or `x-api-key` (`anthropic`). |

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

One table per agent. The default workflow starts an agent named `Coder`, so a server that runs it needs `[agents.Coder]`.
A name is made of letters, digits, `-` and `_` and must not be empty, else the file is refused.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `agents.<name>.model` | string | required | The model's name, sent to the router and given to Pi-Bolt as `--model`. |
| `agents.<name>.protocol` | string | `openai` | `openai` or `anthropic`: which router endpoint the harness uses. |
| `agents.<name>.thinking` | string | none | Given to Pi-Bolt as `--thinking`, such as `low`, `medium` or `high`. Pi-Bolt checks the value. |
| `agents.<name>.instructions` | string | none | Given to Pi-Bolt as `--append-system-prompt`. |
| `agents.<name>.skills` | array of paths | `[]` | Each given to Pi-Bolt as `--skill`. |
| `agents.<name>.extensions` | array of paths | `[]` | Each given to Pi-Bolt as `--extension`, after the bridge, which is always loaded first. |
| `agents.<name>.token_budget` | integer | none | The most tokens (input plus output, summed over all runs of the card, teacher included) a card may use. A model request over it is answered with HTTP 429 `budget_exceeded`. Without it there is no limit. |

## Environment variables

- The variables named by `router.upstreams.api_key_env`. Nothing else is read from the environment by the server.
- `MOEBIUS_PI_BOLT` is read only by the end-to-end tests in `crates/moebius-core/tests`; the server ignores it.
- The server sets `PI_CODING_AGENT_DIR`, `MOEBIUS_CHECK_URL` and `MOEBIUS_RUN_TOKEN` for each Pi-Bolt process, see the
  [bridge protocol](bridge-protocol.md).

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
