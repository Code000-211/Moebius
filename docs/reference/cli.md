# The command line

The `moebius` binary also operates a running server. Each command makes one request to the [desktop
API](desktop-api.md) and writes the answer; `api` calls any route. A person uses them in a terminal, and the Moebius
agent uses them in its chats as `"$MOEBIUS" board`. This page is for anyone who scripts Moebius or builds on it. The
code is in `crates/moebius/src/cli/`, one module per group of commands.

The commands `init`, `serve`, `export` and `check-script` work on files and do not connect to a server; see
[configuration](configuration.md), [training export](training-export.md) and [column scripts](column-scripts.md).

## Where the command connects

| Option | Environment | Meaning |
|---|---|---|
| `--server <url>` | `MOEBIUS_URL` | The server's address, such as `http://127.0.0.1:7171`. Without a scheme, `http://` is assumed. |
| `--token <token>` | `MOEBIUS_TOKEN` | The token the server accepts: the server's [token](desktop-api.md#authentication), or a run token of the Moebius agent. |
| `--ssh <host>` | | Reach the server of an SSH host through a tunnel opened for this command; conflicts with `--server`. |
| `--json` | | Write the API's JSON as it came (pretty printed) instead of plain text. |
| `--config <file>` | | The configuration file (default `moebius.toml`), read when nothing else names the server. |

The server is the first of these that exists:

0. `--ssh <host>`: one `ssh -L` to the host forwards a free port of this machine to `127.0.0.1:7171` there and, on the
   same login, reads the token from `~/.moebius/server/.moebius/token`, where the desktop app's setup left it. The tunnel
   closes when the command ends. `ssh` asks for a password on the terminal if it needs one; keys are recommended. The
   program is `ssh`, or `ssh_program` of `desktop.toml`.
1. `--server` (or `MOEBIUS_URL`). It must come with `--token` (or `MOEBIUS_TOKEN`), so that a token is never sent to a
   server it does not belong to.
   Before this, a tunnel that `moebius connect` holds open is used, if there is one (see below).
2. The address and token the desktop app saved, `<config dir>/moebius/desktop.toml` (`address` and `token`; on Linux
   `~/.config/moebius/desktop.toml`, on Windows `%APPDATA%\moebius\desktop.toml`). A `--token` alone replaces the saved token.
3. The `moebius.toml` of the current directory (or `--config`): its `listen` address (`0.0.0.0` is reached as
   `127.0.0.1`) and the content of its token file. The file must exist; the server must have started once so that the
   token exists. A `--token` alone replaces the file's token.

A run of the Moebius agent has `MOEBIUS_URL` and `MOEBIUS_TOKEN` set by the server (see [the bridge
protocol](bridge-protocol.md#the-environment-of-the-harness)); what it does is recorded as the agent's (see
[the desktop API](desktop-api.md#authentication)).

### `connect <host> [--port <n>] [--no-window] [--browser <path>]`

Opens the same tunnel as `--ssh` and keeps it open until Ctrl+C. It writes the tunnel's address and token to
`connected.toml` next to `desktop.toml` (mode 0600) and prints `Connected to <host>: http://127.0.0.1:<port>.` The
`moebius` commands run meanwhile, without `--server`, use it; a file whose port no longer answers is removed and
ignored. The server must have been set up on the host once, by the desktop app. `desktop.toml` may name another client
with `ssh_program`.

`--port` is the port the server listens on at the host (default 7171). Once `GET /v1/health` answers through the
forwarded port, `connect` asks the server for a one-time login code through the tunnel and opens the web app in a
window, as `open` does, on the local address of the tunnel (`http://127.0.0.1:<local port>/?login=<code>`). The code
is made here with the token read over the same login, so nothing has to run on the host but the server. `--no-window`
holds the tunnel only; `--browser` (or `MOEBIUS_BROWSER`) names the browser, as for `open`. Neither the token nor the
code is printed.

### `open [--browser <path>]`

Opens the web app of the server in a window of its own. It asks the server for a one-time login code (`POST
/v1/session/codes`, as `web` does, so the server found as for every other command, or by `--ssh`) and starts a
Chromium-based browser in app mode (no tabs, no address bar):

```
<browser> --app=<server>/?login=<code> --user-data-dir=<config dir>/moebius/window --no-first-run --no-default-browser-check
```

The profile folder is the window's own, so it keeps its own session cookie and never joins a browser that is already
running. The browser is, in this order: `--browser <path>` (or `MOEBIUS_BROWSER`; a name is looked up on `PATH`, and a
browser that does not exist is an error), the first of `chromium`, `chromium-browser`, `google-chrome`,
`google-chrome-stable`, `microsoft-edge`, `msedge` and `brave-browser` on `PATH`, then Edge or Chrome at their usual
places on Windows (`Program Files`, `Program Files (x86)`, `%LocalAppData%`) and the application bundles in
`/Applications` on macOS. When none is found the address is opened in the default browser (`xdg-open`, `open`, `cmd /c
start`), in a tab, and the command says so. The command ends as soon as the browser is started. It writes a sentence
that names the browser, never the address, because the code in it is a credential.

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
for `api`), `/v1/runs/...`, `/v1/judgements...`, `/v1/replays`, `DELETE /v1/replays/{id}`, `/v1/exports...`, `DELETE /v1/exports/{id}`, `/v1/skills`, `/v1/mcp-servers`, `/v1/repositories/{id}/scripts/{column}`, `/v1/repositories/{id}/settings`, `/v1/repositories/{id}/settings/{file}`,
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
| `chat offer <id> [--reason <text>]` | `POST /v1/chats/{id}/offer` | The change. |
| `chat end <id>` | `POST /v1/chats/{id}/end` | The run. |
| `chat delete <id> [--reason <why>]` | `DELETE /v1/chats/{id}` | The chat's run, as it was. Stops the chat if it is going, then deletes it with its events, changes, inbox items, worktree and branch; only a person may (an agent's token gets `403`). |
| `automation list` | `GET /v1/automations` | Table `id name schedule agent enabled next_firing` (`next_firing` in Unix seconds). |
| `automation save --name <n> --schedule <cron> --prompt <text\|-> [--agent <name>] [--repository <id> \| --harness] [--off] [--id <id>]` | `POST /v1/automations`, or `PUT /v1/automations/{id}` with `--id` | The automation. Without `--agent` its chats are with the Moebius agent; `--off` saves it switched off; `-` as the prompt reads standard input. Replaces the whole automation, as the route does. |
| `automation run <id>` | `POST /v1/automations/{id}/run` | The firing: `started` with its chat's `run_id`, or `skipped` or `failed` with the reason. |
| `automation delete <id>` | `DELETE /v1/automations/{id}` | `deleted the automation <id>`. |
| `runners` | `GET /v1/runners` | Table `name host user max_runs busy`: the `[[runners]]` of `moebius.toml`, with how many runs go on each now. |
| `agents` | `GET /v1/agents` | Table `name model protocol built_in error`. |
| `agent show <name>` | `GET /v1/agents/{name}` | The agent: settings as JSON, instructions below, and for a built-in agent its built-in instructions and skills (`built_in`). |
| `agent save <name> --model <m> [--protocol <p>] [--thinking <t>] [--token-budget <n>] [--harness <h>] [--runner <r>] [--skill <s>]... [--extension <f>]... [--mcp <m>]... [--hide-outputs-after <turns>] [--output-lines <n>] [--output-bytes <n>] [--no-compress] [--no-engineering-skills] [--engineering-skill <s>]... [--instructions <file\|->] [--template <agent>]` | `PUT /v1/agents/{name}` | The agent, with `commit`. Replaces the whole agent, as the route does; its `tool_output` takes the flags given, the defaults for the others, and its engineering skills are those of its name unless `--no-engineering-skills` (none) or `--engineering-skill <name>` (exactly the skills named, given again for more; `tdd`, `diagnosing-bugs`, `code-review`, `grilling`, `to-tickets` or `moebius-autonomous`) is given. `--model` is needed unless `--template` gives one. `--template <agent>` starts from an existing agent, the built-in `Coder` and `Reviewer` included (not `Moebius`, `Grilling-partner`, `Retro-analyst` or `Autoresearcher`): its settings and instructions are copied under the new name, a flag given changes what it copies (`--skill`, `--extension` and `--mcp` replace the lists, `--instructions` the text), and a built-in agent's instructions become the copy's own, the built-in ones followed by its own. The copy is not built in and takes over everything, so the engineering skills the source has (by default for its name or by its list) are written as an explicit list: a copy of the Coder has `tdd`, `diagnosing-bugs` and `moebius-autonomous` under any name, unless a flag says others. A name that exists is replaced, as for any `agent save`: look at `agents` first. |
| `agent delete <name>` | `DELETE /v1/agents/{name}` | `deleted the agent <name>`. |
| `rules` | `GET /v1/rules` | The built-in rules, the rules in prose, those of each repository and the refusals of the week. |
| `rules save <file\|->` | `PUT /v1/rules` | The rules page after the change, with `committed <id>`. |
| `settings` | `GET /v1/settings` | `file:`, then `<key> = <value>` for every key, `(default)` after a default, the stored key names and the keys that need a restart. |
| `settings set <KEY=VALUE>... [--remove <key>]...` | `PUT /v1/settings` | `restart_needed` and `restart_keys`. |
| `secrets set <name>` | `PUT /v1/secrets/{name}` | `stored the key <name>`. The key is read from standard input, one trailing new line removed; it is never an argument. |
| `pkg search [<text>]` | `GET /v1/packages/search?q=<text>` | Table `name kind version registry installed description`, then `problem  <registry>: <message>` for each registry that could not be read. |
| `pkg info <name>` | `GET /v1/packages/{name}` | The name, registry and installed version, the description, and a table `version compatible moebius kind`. |
| `pkg install <name>[@<version>] [--repository <name>] [--registry <name>]` | `POST /v1/packages` | `installed <name> <version>`, then `change <id> waits in the inbox to be merged` for a workflow, rules or skills package, the plugin's error if it does not run, and `restart the server for it to take effect` for a launcher. A plugin is for a person only. |
| `pkg update [<name>]` | `POST /v1/packages/{name}/update` | `updated <name> <version>` as above, or `unchanged`. Without a name, the one package the nightly check found an update for (`GET /v1/packages` first); with none, `no package has an update waiting`, with several, an error naming them. |
| `pkg remove <name>` | `DELETE /v1/packages/{name}` | `removed <name> <version>` as above. |
| `pkg list` | `GET /v1/packages` | Table `name version kind registry update_available`. |
| `repos` | `GET /v1/snapshot` | Table `id name forge default_branch github project`. |
| `repo add <name> <url> [--github <owner/repo> \| --gitlab <group/project> \| --forgejo <owner/repo>] [--trusted <login>]...` | `POST /v1/repositories` | The repository. `--github` selects the GitHub forge, `--gitlab` the GitLab forge with the project's path, `--forgejo` the Forgejo forge (Codeberg, Gitea) with the repository there. Without it, an absolute path that does not exist, or an empty folder, becomes a new repository. |
| `repo remove <id> [--reason <why>]` | `DELETE /v1/repositories/{id}` | The repository, as it last was. Removes it with its cards, changes and chats, and deletes its clone; `409` while one of its runs is still going. |
| `issue create <repository> <title> [--body <text>] [--parent <n>] [--label <label>]... [--blocked-by <n>]...` | `POST /v1/repositories/{id}/issues` | The card, and `warnings` when a step after the issue was created failed. `--parent` and `--blocked-by` are issue numbers of the repository, checked before the issue is created. |
| `issue list <repository> [--parent <n>] [--label <label>] [--state open\|closed] [--claimed \| --unclaimed] [--blocked \| --unblocked]` | `GET /v1/repositories/{id}/issues` | Table `id number state title labels claimed_by blocked_by`. `--blocked` keeps the issues an issue that is not done blocks. The frontier of a map is `--parent <map> --state open --unblocked --unclaimed`. |
| `issue show <card>` | `GET /v1/cards/{id}` | The issue as lines, then its sub-issues as a table. |
| `issue edit <card> [--title <text>] [--body <text>]` | `GET /v1/cards/{id}`, then `PUT /v1/cards/{id}` | The card. What is not given stays. |
| `issue close <card> [--reason <why>]` | `POST /v1/cards/{id}/close` | The card, now in `Done`. |
| `issue reopen <card> [--reason <why>]` | `POST /v1/cards/{id}/reopen` | The card, back in `Backlog`. |
| `issue claim <card> [--as <name>]` | `POST /v1/cards/{id}/claim` | The card. Fails (status 409) when somebody else claimed it; the name is the caller's when `--as` is left out. A claim is advisory: the name is free text, and nothing keeps anyone from working on the issue. |
| `issue release <card> [--as <name>]` | `DELETE /v1/cards/{id}/claim` | The card. A person releases any claim; the Moebius agent only its own, under the name it claimed with (`--as`). Status 403 for somebody else's. |
| `issue labels <card> [--add <label>]... [--remove <label>]...` | `GET /v1/cards/{id}`, then `PUT /v1/cards/{id}/labels` | The card, with its labels as they are now. |
| `issue parent <card> <number\|none>` | `PUT /v1/cards/{id}/parent` | The card. The number is of the parent issue, with or without `#`. |
| `issue blocker add <card> <number>` | `POST /v1/cards/{id}/blockers` | The numbers that block the issue. Refused (status 422) when that makes a cycle, a blocker that is an ancestor of the issue included. On a forge that keeps no blocking (GitLab without Premium) it writes the line `Blocked by #n` into the description. |
| `issue blocker remove <card> <number>` | `DELETE /v1/cards/{id}/blockers/{number}` | The numbers that still block it. A line of the description that names the blocker is taken out. |
| `issue comment <card> <text>` | `POST /v1/cards/{id}/comments` | The comment. |
| `issue comments <card>` | `GET /v1/cards/{id}/comments` | Table `at author body`. |
| `retro` | `GET /v1/retros` | Table `date status errors proposals`. |
| `retro <date>` | `GET /v1/retros/{date}` | The Retro: its errors and proposals. |
| `retro run [--date <date>]` | `POST /v1/retros` | The Retro, as above. |
| `proposals` | `GET /v1/proposals` | Table `id kind status branch title`. |
| `proposal merge <id> [--reason <text>]` | `POST /v1/proposals/{id}/merge` | The proposal. |
| `proposal dismiss <id> [--reason <text>]` | `POST /v1/proposals/{id}/dismiss` | The proposal. |
| `research` | `GET /v1/research` | Table `id status repository goal`. |
| `research <id>` | `GET /v1/research/{id}` | The research. |
| `research new --repository <id> --goal <text> --measure <command> --better <lower\|higher> (--experiments <n> \| --tokens <n>)` | `POST /v1/research` | The research, a draft. The budget is `--experiments` or `--tokens`; `--token-budget <n>` is the old name of `--tokens`. |
| `research confirm <id>`, `research start <id>`, `research stop <id>` | `POST /v1/research/{id}/confirm`, `/start`, `/stop` | The research. |
| `research delete <id> [--reason <why>]` | `DELETE /v1/research/{id}` | The research as it was. Stops it if it is running, then deletes it with its experiments, runs, changes and lever branches; a person only. |
| `grillings` | `GET /v1/grillings` | Table `id status repository reason`. |
| `grilling show <id>` | `GET /v1/grillings/{id}` | The grilling. |
| `grilling confirm <id> [--issues <file\|->]` | `POST /v1/grillings/{id}/confirm` | The grilling. Without `--issues` the proposed issues are read first (`GET /v1/grillings/{id}`) and confirmed as they are; the file is a JSON array of `{"title", "body", "blocked_by"}`. |
| `grilling dismiss <id>` | `POST /v1/grillings/{id}/dismiss` | The grilling. |
| `grilling delete <id> [--reason <why>]` | `DELETE /v1/grillings/{id}` | The grilling as it was. Deletes it with its chat; the issues it filed stay. A person only. |
| `usage [--days <n>]` | `GET /v1/usage?days=<n>` | The window's totals, then the agents and the models. |
| `doctor` | `GET /v1/start` | One line for each step of [the first start](desktop-api.md#get-v1start), `ok` or `missing` with what is missing, such as `missing  upstream: Add an upstream on the Models page.` After the steps, a line `warning  <text>` for each of the answer's `warnings`, such as an upstream that logs in with OAuth. The exit status is 1 while a step is missing (a warning does not change it), so that a script can wait for a ready server. |
| `web` | `POST /v1/session/codes` | The line `Open the web app: <url>`, where the address signs a browser in once with a one-time code that lasts 10 minutes. The server prints the same kind of line when it starts. To get a window instead of a line, use [`open`](#open---browser-path). |
| `decision-model` | `GET /v1/decision-model` | The model, its agreement per kind and its takeovers. |
| `takeovers` | `GET /v1/takeovers` | Table `id kind status agreement count`. |
| `takeover merge\|dismiss\|release <id> [--reason <text>]` | `POST /v1/takeovers/{id}/merge`, `/dismiss`, `/release` | The takeover. |
| `ext search [<text>...] [--type <type>]` | `GET /v1/extensions/search?q=&type=` | Table `name version kinds weekly_downloads description`: the packages of the pi.dev catalog. `--type` is `extension`, `skill`, `theme` or `prompt`. |
| `ext info <source>` | `GET /v1/extensions/info?source=` | The package a source names (`npm:<name>[@<version>]` or `git:<host>/<owner>/<repo>[@<ref>]`): its version, resources, dependencies, install scripts and whether it is installed. |
| `ext add <source> [--agent <name>]` | `POST /v1/extensions/packages` | The change that installs the package, with its source, integrity and files. With `--agent` the agent names the package (`pkg:<name>`) in its extensions and skills in the same change. |
| `ext update [<name>]` | `POST /v1/extensions/packages/{name}/update`, `POST /v1/extensions/update` | A change for each package with a newer version, with the version change, the changelog sections and the files; the packages that are current are named. |
| `ext remove <name>` | `DELETE /v1/extensions/packages/{name}` | The change that removes the package and its names in the agents. |
| `ext list` | `GET /v1/extensions/packages` | Table `name version source agents`. |
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

An item of the kind `extension_update` (`data.package`, `data.from`, `data.to`, `data.source`) takes no action: `ext update <data.package>` proposes the update, which resolves the item.

`--reason` is sent with the decision where the route takes one.

## What the Moebius agent can do with them

Its token is accepted by the desktop API like the server's, with two exceptions: `secrets set` (an agent must not set
keys) and the terminals (of a run and of the server) give `403`. Everything it decides through a command is recorded as a judgement whose decider
is `agent:Moebius` and whose run is the chat; see [the desktop API](desktop-api.md#authentication). The guard checks the
commands like any other `bash` call.
