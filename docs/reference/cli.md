# The command line

The `moebius` binary also operates a running server. Each command makes one request to the [desktop
API](desktop-api.md) and writes the answer; `api` calls any route. A person uses them in a terminal, and the Moebius
agent uses them in its chats as `"$MOEBIUS" board`. This page is for anyone who scripts Moebius or builds on it. The
code is in `crates/moebius/src/cli.rs`.

The commands `init`, `serve`, `export` and `check-script` work on files and do not connect to a server; see
[configuration](configuration.md), [training export](training-export.md) and [column scripts](column-scripts.md).

## Where the command connects

| Option | Environment | Meaning |
|---|---|---|
| `--server <url>` | `MOEBIUS_URL` | The server's address, such as `http://127.0.0.1:7171`. Without a scheme, `http://` is assumed. |
| `--token <token>` | `MOEBIUS_TOKEN` | The token the server accepts: the server's [token](desktop-api.md#authentication), or a run token of the Moebius agent. |
| `--json` | | Write the API's JSON as it came (pretty printed) instead of plain text. |
| `--config <file>` | | The configuration file (default `moebius.toml`), read when nothing else names the server. |

The server is the first of these that exists:

1. `--server` (or `MOEBIUS_URL`). It must come with `--token` (or `MOEBIUS_TOKEN`), so that a token is never sent to a
   server it does not belong to.
2. The address and token the desktop app saved, `<config dir>/moebius/desktop.toml` (`address` and `token`; on Linux
   `~/.config/moebius/desktop.toml`). A `--token` alone replaces the saved token.
3. The `moebius.toml` of the current directory (or `--config`): its `listen` address (`0.0.0.0` is reached as
   `127.0.0.1`) and the content of its token file. The file must exist; the server must have started once so that the
   token exists. A `--token` alone replaces the file's token.

A run of the Moebius agent has `MOEBIUS_URL` and `MOEBIUS_TOKEN` set by the server (see [the bridge
protocol](bridge-protocol.md#the-environment-of-the-harness)); what it does is recorded as the agent's (see
[the desktop API](desktop-api.md#authentication)).

## Output and exit codes

Plain text by default: a table with one line for a list, `key: value` lines for one object, and a sentence for a
command that has nothing to show (`done` when the answer is empty). With `--json` the answer is the API's JSON, so a
script reads fields from it. Errors go to standard error as `moebius: <message>`.

| Exit code | Meaning |
|---|---|
| `0` | The request succeeded. |
| `1` | The server cannot be found or reached, the token is not accepted, or the server answered an error status. The message is `the server answered <status>: <the server's message>` (the statuses are in [the desktop API](desktop-api.md#errors)); a command that looks an item up first says so (`no item 5 in the inbox`). |
| `2` | The command line is wrong. |

A value that says `-` in a table is empty (null).

## `api`

`moebius api <METHOD> <route> [--body <json|file|->]` sends `GET`, `POST`, `PUT` or `DELETE` to any route and writes the
answer: pretty JSON, or the text as it is when it is not JSON (an export file). `--body` is the JSON itself (when it
starts with `{` or `[`), the name of a file that holds it, or `-` for standard input. A `POST` or `PUT` without `--body`
sends `{}`. With no output a successful call writes nothing. `--json` changes nothing for `api`.

```sh
moebius api GET /v1/snapshot
moebius api POST /v1/cards/3/approve --body '{"reason": "fine"}'
echo '{"set": {"board.round_limit": 7}}' | moebius api PUT /v1/settings --body -
```

Every route is reachable this way, including those without a command: `/v1/health`, `/v1/events` (a WebSocket, not
for `api`), `/v1/runs/...`, `/v1/judgements...`, `/v1/replays`, `/v1/exports...`, `/v1/skills`, `/v1/mcp-servers`, `/v1/repositories/{id}/scripts/{column}`,
`/v1/models`, `/v1/plugins`, `/v1/proposals/{id}/trial`, `DELETE /v1/secrets/{name}`.

## Commands

A repository, a card, a chat or anything else is named by its id, as the answers show it, except `board`, which also
takes a repository's name.

| Command | Route | Plain output |
|---|---|---|
| `board [<repository>]` | `GET /v1/snapshot` | For each repository, `<name> (repository <id>)`, then each column with its cards as `<id>  #<number>  <title>`, followed by `[<agent> <status>]` of the latest run and `(waiting: <reason>)`. Fails with `no repository <x>` for an unknown one. |
| `card <id>` | `GET /v1/snapshot` | The card: number, title, repository, column, author, rounds and tokens, latest run, open change, why it waits, note, blockers and body. `no card <id>` if there is none. |
| `card <id> --history` | `GET /v1/cards/{id}/history` | Table `at kind data`. |
| `card <id> --runs` | `GET /v1/cards/{id}/runs` | Table `id agent purpose status summary`. |
| `inbox` | `GET /v1/snapshot` | Table `id kind card title` of the open items. |
| `inbox act <item> <action> [--reason <text>]` | see below | The answer's fields. |
| `chats` | `GET /v1/chats` | Table `id agent status repository title`. |
| `chat start <message> [--agent <name>] [--repository <id>] [--continues <id>]` | `POST /v1/chats` | The run. `-` as the message reads standard input. |
| `chat say <id> <message>` | `POST /v1/chats/{id}/messages` | The run. `-` reads standard input. |
| `chat end <id>` | `POST /v1/chats/{id}/end` | The run. |
| `agents` | `GET /v1/agents` | Table `name model protocol built_in error`. |
| `agent show <name>` | `GET /v1/agents/{name}` | The agent: settings as JSON, instructions below, and for a built-in agent its built-in instructions and skills (`built_in`). |
| `agent save <name> --model <m> [--protocol <p>] [--thinking <t>] [--token-budget <n>] [--skill <s>]... [--extension <f>]... [--mcp <m>]... [--instructions <file\|->]` | `PUT /v1/agents/{name}` | The agent, with `commit`. Replaces the whole agent, as the route does. |
| `agent delete <name>` | `DELETE /v1/agents/{name}` | `deleted the agent <name>`. |
| `rules` | `GET /v1/rules` | The built-in rules, the rules in prose, those of each repository and the refusals of the week. |
| `rules save <file\|->` | `PUT /v1/rules` | The rules page after the change, with `committed <id>`. |
| `settings` | `GET /v1/settings` | `file:`, then `<key> = <value>` for every key, `(default)` after a default, the stored key names and the keys that need a restart. |
| `settings set <KEY=VALUE>... [--remove <key>]...` | `PUT /v1/settings` | `restart_needed` and `restart_keys`. |
| `secrets set <name>` | `PUT /v1/secrets/{name}` | `stored the key <name>`. The key is read from standard input, one trailing new line removed; it is never an argument. |
| `repos` | `GET /v1/snapshot` | Table `id name forge default_branch github`. |
| `repo add <name> <url> [--github <owner/repo>] [--trusted <login>]...` | `POST /v1/repositories` | The repository. `--github` selects the GitHub forge. Without it, an absolute path that does not exist, or an empty folder, becomes a new repository. |
| `issue create <repository> <title> [--body <text>]` | `POST /v1/repositories/{id}/issues` | The card. |
| `retro` | `GET /v1/retros` | Table `date status errors proposals`. |
| `retro <date>` | `GET /v1/retros/{date}` | The Retro: its errors and proposals. |
| `retro run [--date <date>]` | `POST /v1/retros` | The Retro, as above. |
| `proposals` | `GET /v1/proposals` | Table `id status branch title`. |
| `proposal merge <id> [--reason <text>]` | `POST /v1/proposals/{id}/merge` | The proposal. |
| `proposal dismiss <id> [--reason <text>]` | `POST /v1/proposals/{id}/dismiss` | The proposal. |
| `research` | `GET /v1/research` | Table `id status repository goal`. |
| `research <id>` | `GET /v1/research/{id}` | The research. |
| `research new --repository <id> --goal <text> --measure <command> --better <lower\|higher> --token-budget <n>` | `POST /v1/research` | The research, a draft. |
| `research confirm <id>`, `research start <id>`, `research stop <id>` | `POST /v1/research/{id}/confirm`, `/start`, `/stop` | The research. |
| `grillings` | `GET /v1/grillings` | Table `id status repository reason`. |
| `grilling show <id>` | `GET /v1/grillings/{id}` | The grilling. |
| `grilling confirm <id> [--issues <file\|->]` | `POST /v1/grillings/{id}/confirm` | The grilling. Without `--issues` the proposed issues are read first (`GET /v1/grillings/{id}`) and confirmed as they are; the file is a JSON array of `{"title", "body", "blocked_by"}`. |
| `grilling dismiss <id>` | `POST /v1/grillings/{id}/dismiss` | The grilling. |
| `usage [--days <n>]` | `GET /v1/usage?days=<n>` | The window's totals, then the agents and the models. |
| `doctor` | `GET /v1/start` | One line for each step of [the first start](desktop-api.md#get-v1start), `ok` or `missing` with what is missing, such as `missing  upstream: Add an upstream on the Models page.` The exit status is 1 while a step is missing, so that a script can wait for a ready server. |
| `decision-model` | `GET /v1/decision-model` | The model, its agreement per kind and its takeovers. |
| `takeovers` | `GET /v1/takeovers` | Table `id kind status agreement count`. |
| `takeover merge\|dismiss\|release <id> [--reason <text>]` | `POST /v1/takeovers/{id}/merge`, `/dismiss`, `/release` | The takeover. |
| `server restart` | `POST /v1/server/restart` | `the server is restarting`. |

`settings set` takes each value as JSON when it parses as JSON (`board.round_limit=7`, `board.tick_ms=500`,
`harness.mcp_env='["A"]'`, `teacher='{"model":"m"}'`) and as text
otherwise (`teacher.model=qwen`). To force text that looks like JSON, quote it: `key='"007"'`.

### `inbox act`

`inbox act` looks the item up in the snapshot and does what the app's button of its kind does. The actions an item takes
depend on its kind; any other action is refused with the list of those it takes, and an item of another kind (added by
a column script with `moebius.ask`) takes none (use `api`).

| Item kind | Action | Route |
|---|---|---|
| `approve_issue` | `approve` | `POST /v1/cards/{card}/approve` |
| `stuck`, `change_closed` | `retry` | `POST /v1/cards/{card}/retry` |
| `ready_to_merge` | `merge`, `reject` | `POST /v1/changes/{data.change}/merge`, `/reject` |
| `proposal` | `merge`, `dismiss` | `POST /v1/proposals/{data.proposal}/merge`, `/dismiss` |
| `takeover` | `merge`, `dismiss` | `POST /v1/takeovers/{data.takeover}/merge`, `/dismiss` |
| `grilling_issues` | `confirm` (the proposed issues as they are), `dismiss` | `POST /v1/grillings/{data.grilling}/confirm`, `/dismiss` |
| `grilling_report` | `dismiss` | `POST /v1/grillings/{data.grilling}/dismiss` |

`--reason` is sent with the decision where the route takes one.

## What the Moebius agent can do with them

Its token is accepted by the desktop API like the server's, with two exceptions: `secrets set` (an agent must not set
keys) and the terminals (of a run and of the server) give `403`. Everything it decides through a command is recorded as a judgement whose decider
is `agent:Moebius` and whose run is the chat; see [the desktop API](desktop-api.md#authentication). The guard checks the
commands like any other `bash` call.
