# Harness configuration

The harness configuration is a git repository that holds the agents, their skills, the server's rules and the MCP servers.
This page is for whoever sets it up or edits it, by hand or through the desktop API: it lists every file and key. The code
is in `crates/moebius-core/src/harness/repo/`.

## Using it

Name the repository in `moebius.toml`:

```toml
[harness]
url = "https://github.com/acme/harness-configuration"
mcp_env = ["DOCS_TOKEN"]
```

- `url` is anything `git clone` takes. With `[github]` set, an `https://github.com/...` URL is cloned, fetched and pushed
  with the GitHub App's installation token, like the repositories Moebius works on; the App needs write access to the
  repository for edits. Any other URL, such as a local path, uses plain git.
- `mcp_env` (array of names, default none) lists the environment variables of the server that `mcp.toml` may take values
  from (see [`mcp.toml`](#mcptoml)). Each is a name of letters, digits and `_` that does not start with a digit, else
  the file is refused.
- The server clones it to `<data_dir>/harness` when it starts. A clone that cannot be made stops the server; a clone that is
  there already is used as it is, so the server starts without the network.
- Every 30 seconds the server fetches `origin` and fast-forwards the clone's default branch (the branch that is checked
  out). When that changed anything, the agents are read again. Runs that are going keep the agent they started with.
- With `[harness]`, `[agents.*]` and `rules_file` in `moebius.toml` are errors that say to move them into the repository, so
  that there is one source of truth. Without `[harness]` everything is configured in `moebius.toml`, and the routes
  `GET`, `PUT` and `DELETE /v1/agents/{name}` answer `422`.
- Everything is read from the default branch. Other branches of the repository are not read.

## A repository on the server

`moebius init` and the desktop app's "Set up" (`POST /v1/harness/setup`, see the [desktop API](desktop-api.md) and
[configuration](configuration.md#setting-it-up-from-the-app)) make the repository `harness/` next to `moebius.toml` and
set `[harness] url` to its absolute path. It is a git repository on `main` with one commit, "Set up the harness
configuration repository", holding:

- `agents/<name>.toml` and `agents/<name>.md` for each `[agents.<name>]` the file had (none after `init`): the model,
  protocol, thinking level and token budget as they were, the instructions as the `.md`, and no MCP servers;
- `skills/<name>/`: a copy of each skill directory an agent named by path (a path to its `SKILL.md` names the directory),
  without a `.git` in it, named by the directory and listed by that name in the agent's `skills`. The name must be a valid
  skill name and not that of a built-in skill, and two different directories with the same name are refused: rename one
  first;
- `extensions/<file name>`: a copy of each extension file an agent named, listed by that path in its `extensions`; two
  different files with the same name are refused;
- `rules.md`: a copy of the file of `rules_file` (by default `rules.md` next to `moebius.toml`), empty when there was none;
- `mcp.toml` with no servers, and a `README.md` that says what the repository is.

The agents are read back as the server reads them before the commit; one that would not work stops the setup. A setup
that fails leaves the directory as it was. The repository is set with `receive.denyCurrentBranch=updateInstead`, so that
the server's clone can push the app's edits to the branch that is checked out and its files follow; a push is refused
while the working tree has changes that are not committed. Edit it by hand like any repository: commit on `main`, and
the server fetches the commit within 30 seconds.

## Layout

```
agents/<name>.toml     the settings of an agent
agents/<name>.md       the instructions of an agent (optional)
skills/<name>/SKILL.md a skill (a directory with this file)
rules.md               the server's rules
mcp.toml               the MCP servers
extensions/...         any files an agent's extensions name
```

Nothing else is read. The Moebius agent stays built in: `agents/Moebius.toml` configures it as `[agents.Moebius]` does, and
`agents/Moebius.md` is added after its built-in instructions (see [configuration](configuration.md)). So does the
`Grilling-partner` (see [Grilling](#grilling)).

## `agents/<name>.toml`

`<name>` is made of letters, digits, `-` and `_` and is not empty. Unknown keys are errors.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `model` | string | required | The model's name, sent to the router and given to Pi-Bolt as `--model`. Not blank. |
| `protocol` | string | `openai` | `openai` or `anthropic`: which router endpoint the harness uses. |
| `runner` | string | none | The name of a `[[runners]]` entry of `moebius.toml` (see [configuration](configuration.md#runners)) that the agent's runs go to over SSH; without it they run on the server. The name is checked when a run starts: a run of an agent whose runner is gone is not made, and says so. |
| `harness` | string | `pi-bolt` | `pi-bolt`, or `claude-code` to run the agent on Claude Code (see [configuration](configuration.md#claude-code)), which needs `protocol = "anthropic"` and no `extensions`, else the agent is an error. Written only when it is not `pi-bolt`. |
| `thinking` | string | none | Given to Pi-Bolt as `--thinking`. Pi-Bolt checks the value. |
| `token_budget` | integer | none | The most tokens a card may use with this agent, as `agents.<name>.token_budget` in [configuration](configuration.md). At least 1. |
| `skills` | array of strings | `[]` | Names of directories under `skills/` that hold a `SKILL.md`. Each is given to Pi-Bolt as `--skill`. A name that is not there is an error. |
| `extensions` | array of strings | `[]` | Paths of files in the repository, relative to its root and without `..`. Each is given to Pi-Bolt as `--extension`, before the bridge. A path that is not a file is an error. |
| `tool_output` | table | the defaults | `hide_after_turns`, `max_lines`, `max_bytes` and `compress`, as `agents.<name>.tool_output` in [configuration](configuration.md); a key left out has its default. Pi-Bolt runs only. |
| `engineering_skills` | boolean | `true` | Whether the agent is given the engineering skills Moebius brings for it (see [Engineering skills](#engineering-skills)); written only when it is `false`. Only the `Coder`, the `Reviewer` and the `Grilling-partner` have any. |
| `mcp` | array of strings | `[]` | Names of servers in `mcp.toml`. A run of the agent is given exactly these servers. A name that is not there, a server that cannot be used (see [`mcp.toml`](#mcptoml)), or any name while `mcp.toml` cannot be read, is an error. |

The instructions are not a key: they are the text of `agents/<name>.md`, given to Pi-Bolt as `--append-system-prompt`.
A file that is empty or only blank is no instructions.

```toml
model = "my-local-model"
protocol = "openai"
thinking = "medium"
token_budget = 2000000
skills = ["house-style"]
extensions = ["extensions/log-tool-calls.ts"]
mcp = ["docs"]
```

## `skills/<name>/SKILL.md`

A skill as Pi-Bolt documents it: a directory with a `SKILL.md` and whatever else the skill uses. A directory without a
`SKILL.md` is not a skill. An agent that names a skill that is missing is left out with an error. A run is given exactly
its agent's skills, in the order of the `skills` key, and no skill Pi-Bolt would find on its own. The Moebius agent has
its built-in skills first, the `moebius` skill and the task skills `moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions` and `moebius-plugins`, then those of
`agents/Moebius.toml`. The other built-in agents have the `moebius` skill first. A directory of `skills/` with the name
of a built-in skill is ignored, except for the engineering skills below.

### Engineering skills

Moebius brings five skills from [Matt Pocock's skills](https://github.com/mattpocock/skills) (MIT license), copied
unchanged at a pinned commit (`crates/moebius-core/skill/vendor/mattpocock/` in the source, with the license), and the
small skill `moebius-autonomous`. The skills are written for work with a person: the adapter says what to do where one
asks the user (decide, and give the decision and its reason in the run's summary), that the card's issue is the spec and
the tracker, and that the setup skill is never run. They are written to `<data_dir>/skills/<name>/` with the other
built-in skills when the server starts, and given to a run before the agent's own `skills`:

| Agent | Skills |
|---|---|
| `Coder` | `tdd`, `diagnosing-bugs`, `moebius-autonomous` |
| `Reviewer` | `code-review`, `moebius-autonomous` |
| `Grilling-partner` | `grilling`, `to-tickets`, `moebius-autonomous` (in front of the `moebius` skill) |

An agent with any other name has none. This holds on both harnesses (Claude Code gets them as skills of its run's
configuration directory). `to-tickets` asks not to be offered to the model on its own, so the Grilling partner is told in
its instructions where its `SKILL.md` is.

- **Turn them off** for an agent with `engineering_skills = false` in its `agents/<name>.toml` (or its
  `[agents.<name>]` table); the app has the setting on the agent's page, and `agent save` has `--no-engineering-skills`.
  It turns off the five skills and the adapter together; the `moebius` skill stays on the Grilling partner.
- **Replace one** with a skill of your own: a directory `skills/<name>/` of this repository with a `SKILL.md` and the
  name of the skill (`tdd`, `diagnosing-bugs`, `code-review`, `grilling` or `to-tickets`) takes its place for every agent that has it, with all its files. These
  names, unlike `moebius` and the `moebius-` skills, are not reserved. `moebius-autonomous` cannot be replaced.
- **Update them from upstream**: the Skills page shows each one as built in with the source and the commit it was copied
  at, and "Update from upstream" asks the Moebius agent to import that skill from the repository with the Skills import,
  which saves it as a skill of this repository with the same name (so it replaces the built-in copy, as above). The
  copies that ship with Moebius move only with a new release.

`GET /v1/skills` and `GET /v1/skills/{name}` give these skills `"source": {"url", "commit"}`.

## `rules.md`

The server's rules, in the format of [rules](rules.md): one `## name` section per rule. It replaces `rules_file`, and has
the same precedence over a repository's `.moebius/rules.md`: the server's rules come first and a repository cannot
weaken them. Like `rules_file` it is read afresh for every tool call, from the clone's working tree, which is the
default branch. Without the file there are no server rules.

## `mcp.toml`

The MCP servers agents can name. Each server is a top-level table named for it, such as `[docs]`. A name is made of
letters, digits, `-` and `_`. A server is one of two kinds, and a table that is neither or both is an error:

| Key | Type | Meaning |
|---|---|---|
| `command` | string | A stdio server: the executable, not a shell command line. Not blank. |
| `args` | array of strings | Its arguments. Only with `command`. Default none. |
| `env` | table of strings | Its environment, added to the Moebius server's, which starts it. Only with `command`. Default none. |
| `url` | string | A remote server (streamable HTTP). |
| `headers` | table of strings | Sent with every request, such as `Authorization`. Only with `url`. Default none. |

Unknown keys are errors. Every string value may contain `${NAME}`, which is replaced by the value of the environment
variable `NAME` of the Moebius server, when the run starts. Only the names in `mcp_env` of `[harness]` in
`moebius.toml` may be used (see [configuration](configuration.md)); any other name is an error for the server, so that
a repository cannot read secrets such as a provider's key. A name that is listed but not set in the server's
environment fails a run of an agent that uses the server (`cannot write the agent directory: ...`). Never write a
secret into the repository: keep it in the server's environment and take it with `${NAME}`.

```toml
[docs]
command = "docs-server"
args = ["--root", "/srv/docs"]
env = { API_KEY = "${DOCS_TOKEN}" }

[search]
url = "https://search.example.com/mcp"
headers = { Authorization = "Bearer ${SEARCH_TOKEN}" }
```

A server that cannot be used leaves out the agents that name it, with the error, like a missing skill. The other
servers and agents are not affected.

### What a run is given

A run reaches its servers only through the MCP gateway, Moebius's one MCP server for every harness (see the
[bridge protocol](bridge-protocol.md#the-mcp-gateway)). A run of an agent that names servers has the file
`<run-dir>/agent/mcp.json` in Pi's format with one server, `moebius`: the gateway's URL (`<router>/mcp`) and the run's
token as `Authorization: Bearer <token>`. The servers themselves, with the values of `${NAME}` filled in, are given to
the gateway for the run's token, so the run sees neither the servers' commands nor their values. Pi-Bolt is started
with `--extension builtin:mcp` before the agent's own extensions (the bridge stays last), because Pi's MCP support is
an extension that `--no-extensions` turns off. The gateway has `"exposure": "direct"`, so its tools are declared to the
model like Pi's own tools.

At the run's first request the gateway starts every stdio server as a child process of the Moebius server, in the run's
worktree, and connects to every remote one. It lists their tools as `<server>__<tool>`, so Pi names them
`mcp__moebius__<server>__<tool>`. Every call is checked twice: by the bridge under Pi's name, like any tool call, and by
the gateway under the name `mcp__<server>__<tool>`, so that a harness without a bridge is checked too. A refused call
never reaches the server. A server that cannot be started or reached leaves out its tools and is reported as an
`mcp_error` run event; the other servers still serve. The servers of a run are stopped when the run ends. A run of an
agent without servers has no `mcp.json` and no MCP support, and no agent is given a server of another agent.

`mcp.json` and `models.json` (which both hold the run's token) can be read only by the server's user (mode `0600`).
A stdio server runs with the Moebius server's environment and its own `env`, and works in the run's worktree, so it can
read and change what the agent works on: give a run only servers you trust.

## `.moebius/lint.toml`

Not part of the harness configuration repository: a file of the *project* an agent works on, committed to it like
`.moebius/checks`. It tells the [lens](bridge-protocol.md#the-lens) what to run after the agent's edits in the run's
worktree. Without the file, or with no `[[check]]` in it, the lens detects the common tools from the repository's files.

```toml
budget_ms = 1500            # all per-edit checks of one edit together; a check over it is skipped and said
max_lines = 10              # the most findings an edit's result or the end-of-turn message carries
deep_timeout_seconds = 120  # each end-of-turn command
syntax = true               # the built-in syntax checks (JSON, Python, shell); false turns them off
enabled = true              # false turns the lens off for this repository

[[check]]
glob = "**/*.py"                                  # or globs = ["*.py", "*.pyi"]
edit = "ruff check --output-format concise {file}"   # after each edit, on the one file
deep = "pyright {files}"                           # once, when the agent is done, on the changed files
format = "lines"                                   # how the output is read
pattern = "^(?<file>.+?):(?<line>\\d+):(?<column>\\d+): (?<message>.*)$"   # optional, for any other output
severity = "warning"                               # for a finding that does not say
```

`syntax` (not in the example) is a command like `edit` that runs as the syntax check of the files of the glob, instead of
the built-in one. The keys are all optional except that a `[[check]]` needs a `glob` or `globs` and at least one of `edit`, `deep` and
`syntax`. An unknown key, an empty command or a `pattern` that is not a regular expression makes the whole file unusable:
the lens then does nothing in that run except to store a `lens_check` event that says why.

- **Globs** are matched against the path from the worktree: `*` is any characters but `/`, `**` any characters, `**/` any
  folders, `?` one character, `{a,b}` either. A glob without a `/` names files at any depth, so `*.py` is every Python file.
- **Commands** are split into words as a shell would (`'single'` and `"double"` quotes), and run without a shell, in the
  worktree, so there is no `|`, `&&` or `$VAR`; a program that is in `node_modules/.bin` is found there. In a command,
  `{file}` is the edited file; for `deep`, an argument that is exactly `{files}` becomes the changed files that match the
  glob, and `{dirs}` their folders (`./pkg`), once each. The program's exit status is not read, only its output.
- **Output** is read by `format`: `lines` (the default) finds `<file>:<line>[:<column>]: <message>`, one finding per line,
  and ignores other lines; `files` takes each line that is only a file name as "not formatted" (`gofmt -l`) and also reads
  `lines`; `diff-in` reads `Diff in <file>:<line>:` (`rustfmt --check`); `github` reads `::error file=..,line=..,col=..,
  title=..::message` (`biome --reporter=github`). A `pattern` replaces the line format of `lines` and `files`: a regular
  expression with the named groups `file`, `line`, `column`, `message` and `severity`. Only findings in the file that was
  edited count after an edit. A finding is an error when its text says error, a warning when it says warning, and has the
  check's `severity` (default `warning`) otherwise. Two commands that print differently need two `[[check]]` entries.
- **Without `[[check]]`** the lens uses what the files suggest: `ruff check` for Python when the repository has a
  `pyproject.toml`, `setup.py`, `requirements.txt` or `ruff.toml`, and `pyright` at the end when it is configured;
  `rustfmt --check` and `cargo check --message-format short` with a `Cargo.toml` (the edition from it); `gofmt -l` and
  `go vet` with a `go.mod`; `biome lint` with a `biome.json`, or `oxlint` with an `.oxlintrc.json`; `tsc --noEmit
  --incremental` with a `tsconfig.json`; `shellcheck` for shell scripts. A tool that is not installed is skipped.

## A broken agent

A file that cannot be used leaves only its agent out; the other agents are read and used. The agent is reported:

- by the event `harness_error`, `{"agent": name, "message": string}`, each time the repository is read (see the
  [desktop API](desktop-api.md));
- in `GET /v1/agents` as an entry with an `error` and an empty `model`, so that a client can show it and not offer it;
- by a run or a chat that asks for it, which fails with `the agent <name> cannot be used: <error>`.

The errors: a name that is not valid; `agents/<name>.md` without `agents/<name>.toml`; a `.toml` file that is not valid
TOML, has an unknown key or a value of the wrong type; a missing `model`; a skill, extension or MCP server that is not
there or cannot be used (see [`mcp.toml`](#mcptoml)). An error in `agents/Moebius.toml` is reported the same way, and the Moebius agent then runs as if the file were
not there. A fetch that fails is reported as a `harness_error` without an `agent`, and the agents stay as they were.

## Edits through the API

These routes are in the [desktop API](desktop-api.md). No secret belongs in these files and none is kept in them:
model providers' keys are read from the environment by the router, so the files can be read and written through the API
as they are.

- `GET /v1/agents/{name}` answers `{"name", "settings", "instructions"}`, where `settings` has the keys of the table above
  (`protocol` always, `skills`, `extensions` and `mcp` always, `thinking` and `token_budget` when set, `tool_output` when it is not the defaults, `engineering_skills` when it is `false`) and `instructions`
  is the text of the `.md` file, empty without one.
- `PUT /v1/agents/{name}` takes `{"settings": {...}, "instructions": "..."}` (`instructions` may be left out) and
  is checked as a file read is. It writes `agents/<name>.toml` (the file is written again from the settings, so comments
  in it are lost) and `agents/<name>.md` (removed when the instructions are blank), commits them on the default branch as
  `Moebius <moebius@localhost>` with the message `Add the agent <name>` or `Update the agent <name>`, pushes the branch
  to `origin`, and then records the decision (kind `edit_agent`, decider `person`, the settings and instructions in its
  context). An edit that changes nothing commits nothing and records nothing.
- `DELETE /v1/agents/{name}` removes both files the same way, with the message `Delete the agent <name>`.
- `PUT /v1/skills/{name}` takes `{"description": "...", "text": "..."}` and writes `skills/<name>/SKILL.md`: the
  frontmatter `name` and `description` (a double-quoted line), then the text. The skill's other files are kept. The
  message is `Add the skill <name>` or `Update the skill <name>`, the decision kind `edit_skill`. `DELETE` removes the
  directory (`Delete the skill <name>`). The built-in skills (`moebius`, the task skills and `moebius-autonomous`) cannot be changed or
  deleted; the engineering skills cannot while the repository has no skill of the same name, and then they are the
  repository's own.
- `PUT /v1/skills/{name}/files/{*path}` takes `{"text": "..."}` and writes one of the skill's other files, such as
  `scripts/fill.py`, with its directories (`Add <path> of the skill <name>` or `Update <path> of the skill <name>`,
  the decision kind `edit_skill`); `DELETE` removes it and the directories it leaves empty (`Delete <path> of the
  skill <name>`). `SKILL.md` is written only with the skill, and a path through a symbolic link is refused.
- `PUT /v1/mcp-servers/{name}` takes the keys of a server of [`mcp.toml`](#mcptoml) and rewrites its table in the file,
  checked as reading checks it (so a `${NAME}` outside `mcp_env` is refused). The table is rewritten in place, or added
  at the end; the other tables and the comments of the file are kept, but comments inside the rewritten table are lost.
  The message is `Add the MCP server <name>` or `Update the MCP server <name>`, the decision kind `edit_mcp_server`.
  `DELETE` removes the table (`Delete the MCP server <name>`).
- A skill or an MCP server that an agent names is not deleted (`409`, naming the agents).
- `PUT /v1/rules` takes `{"text": "..."}`, writes it to `rules.md` and commits and pushes it the same way, with the
  message `Update the rules` and the decision kind `edit_rules`.

If anything fails after the commit was made, including the push, the commit is undone (`git reset --hard` to the head
before it), so the clone never drifts from `origin`, and no decision is recorded. The request then fails with the
error. When someone else pushed to
`origin` meanwhile, the push is refused for that reason, and the next refresh brings their commits in.

## Changing it in a chat

A chat can work in the harness configuration repository, so that an extension, a skill, an MCP server, an agent or the
server's rules are built as files and merged like any change. `POST /v1/chats` with `harness: true` (refused with `422`
without `[harness]`, and together with `repository`) starts the chat in a worktree on the new branch
`moebius/chat-<run id>` of the clone's default branch, made as a chat's worktree in a repository is. The run has
`harness_repository` true. A chat that continues an ended chat of this kind (`continues`) works there too, unless it names a
`repository`.

When the chat ends or is stopped, what it left is committed on that branch like any chat's work. If the branch has commits
beyond the default branch, it is checked the way a branch of a Retro proposal is (see [The Retro](#the-retro)):

- it changes only harness configuration files: `agents/`, `skills/`, `extensions/`, `rules.md` and `mcp.toml`;
- every agent that can be read on the default branch can still be read with the branch's files (a skill an agent names
  must exist in `skills/`, an extension in `extensions/` or wherever its path points, an MCP server in `mcp.toml`).

A branch that passes is pushed to `origin` and offered as a change of the harness configuration repository: a `Change`
with `repository_id` null, `harness_repository` true, no card and no pull request, in the snapshot's `harness_changes`,
with a `ready_to_merge` inbox item and the event `change_offered`. A branch that fails is not offered: the chat's run gets
a `moebius_error` event saying why, and the branch stays in the clone for the person.

`POST /v1/changes/{id}/merge` merges the branch into the clone's default branch with a merge commit (`Merge
moebius/chat-<run id>`), pushes it to `origin` and reads the agents again, so the next run of an agent has the skills and
extensions that came with it; this is the merge of a trial's proposal. A branch that does not merge cleanly is a `409`
and changes nothing, as does a push that `origin` refuses. The decision is recorded as the merge of any change is (kind
`merge`, with the repository named `harness`).

## The Retro

Once a day (`retro_at`, see [configuration](configuration.md)) Moebius collects the day's errors without a model into a
report (see `POST /v1/retros` in the [desktop API](desktop-api.md)). When the day had errors and `[harness]` is
configured, the built-in agent `Retro-analyst` turns the report into proposals. Agent names allow no spaces, so it is
`Retro-analyst`, not "Retro analyst".

- The agent is built in like the Moebius agent: its instructions are built in, it has the `moebius` skill (which holds
  these reference pages), and its model comes from `agents/Retro-analyst.toml` or else from the Coder. `agents/Retro-analyst.md`
  is added after the built-in instructions; `agents/Retro-analyst.toml` may set its model, skills and the rest.
- Its run has the purpose `retro` and no card. It works in a fresh detached worktree of the clone at the default branch
  (`<data_dir>/worktrees/harness/retro-<run id>`), which is removed when the run ends. It is told the report as JSON. It
  never waits for a person.
- It reads the raw traces too, not only the report's summary of each error: before it starts, the events of each run the
  report names are written to `.retro/run-<id>.events.jsonl` of its worktree (one JSON object a line: `seq`, `kind`,
  `data`, `at`), and what a run of a card changed, from the commit it started at to its branch, to `.retro/run-<id>.diff`
  while the branch is there. Every secret is redacted; at most the report's first 20 runs are written, each file at most
  200 000 bytes (an events file then ends with a `moebius_cut` line saying how many events are left out). Git leaves
  `.retro/` out (the clone's `info/exclude`), and the analyst is told never to commit it.
- Each proposal is a branch `proposal/<date>-<short-name>` made from the default branch, with one commit whose message is
  the proposal: the subject is its title, the body says what changes and which errors it addresses, and the last line is
  `Cards: 48, 50`, the `card` of each error of the report that it addresses (numbers that are not ids are ignored). It
  changes only harness configuration files (`agents/`, `skills/`, `extensions/`, `rules.md`, `mcp.toml`).
- When the run has succeeded, Moebius lists the `proposal/<date>-*` branches of the clone. A branch that changes
  nothing, changes another file, or breaks an agent that could be used on the default branch is left out, with a warning
  in the Retro. The others are pushed to `origin` and stored as proposals (`GET /v1/proposals`), with the agents whose files
  they change and the cards of the `Cards:` line. Nothing is merged without a person's decision, after the trials.

### Proposals for Moebius itself

Some errors are better avoided by Moebius's own configuration than by the harness configuration. The analyst may also
propose a change to `moebius.toml` or to a repository's own rules and column scripts. It states one the same way, as a
branch `proposal/<date>-<short-name>` with one commit whose message is the proposal, but the commit adds only files under
`moebius/`, which is not harness configuration:

- **A change to `moebius.toml`** adds only `moebius/settings.json`: a JSON object `{"set": {"<key>": value}, "remove":
  ["<key>"]}` with dotted keys, as `PUT /v1/settings` takes it (see the [desktop API](desktop-api.md)). It may name only
  `run_limit`, `retro_at`, `trial_window.start`, `trial_window.end`, `board.round_limit`, `budgets.day_tokens`,
  `checks.timeout_seconds`, `teacher.model`, `teacher.protocol` and `teacher.timeout_seconds`; the analyst is told
  their values now. The file must still be a valid [configuration](configuration.md) with the change.
- **A change to a repository's `.moebius/`** adds only files `moebius/repositories/<repository>/.moebius/<file>`, each
  the whole new text of that file, all for one repository. The files are `rules.md` and the column scripts
  (`backlog.luau`, `ready.luau`, `code.luau`, `review.luau`, `merge.luau`, `done.luau`); rules must be fit to save (see
  [rules](rules.md)) and a script must pass `moebius check-script`. The analyst is told the files that the repositories
  of the report's cards have now.

When the run has succeeded, these branches are read first and then deleted from the clone; they are never pushed. A
branch that breaks these conditions, or changes nothing, is left out with a warning in the Retro; a branch that mixes
files under `moebius/` with harness configuration is left out as one that changes another file. Each other one is
stored as a proposal of the kind `settings` or `repository` with a diff of what it changes. Trials do not apply to
them: each is `untried` and in the inbox at once, as its diff with the Retro's reason (the body of its commit message).
Nothing is applied without a person:

- Merging a `settings` proposal saves the change to `moebius.toml` through the same code as `PUT /v1/settings`; the
  live keys take effect at once, and when a key needs a restart the server restarts.
- Merging a `repository` proposal commits its files on the branch `moebius/proposal-<id>` of the repository, from its
  default branch, and offers it as a change through the repository's forge (a pull request on GitHub). The change is in
  the inbox, ready to merge, and merges as any change does.
- Dismissing one changes nothing.

### The trials

A stored proposal is `open`. Inside `[trial_window]` (see [configuration](configuration.md)), or when a person asks
(`POST /v1/proposals/{id}/trial`), it is tried, one proposal at a time, on each card of its `Cards:` line, one after the
other. A proposal with no cards, a card that is gone, a card the report has no error of, and a card without a code run
cannot be tried: that trial is `skipped`, with the reason. A trial that cannot be made (the agent cannot be used on the
proposal's branch, the run cannot start) is `failed`.

- **The run.** A run with the purpose `trial` and the card's id, by the agent that made the card's error in the report
  (the first error of the card that names an agent; the Coder otherwise), given the prompt a `code` run of the card
  gets. It works in a fresh worktree of the card's repository at the repository's default branch, on a branch of its own,
  `moebius/trial-<proposal id>-card-<card id>`. The agent's definition (settings, instructions, skills, extensions and MCP
  servers) is read from the proposal's branch, in a throwaway worktree of the harness clone, not from the default branch.
  Like a `code` run it fails when it changes nothing. A trial run is no part of the card's own history: the card's runs,
  rounds and tokens do not count it, and the next Retro's report leaves its errors out.
- **The checks.** When the run succeeded and the repository has no forge, its own checks (`.moebius/checks` on the
  default branch) run on what the run made.
- **The comparison** is with the card's first code run, the one its errors happened in. A trial is **positive** when all of
  these hold, else **negative**, with the reasons that do not:
  - the run succeeded;
  - none of the addressed errors happened in the run: the kinds of the card's errors in the report, looked for in the run
    the way the report looks for them (failed and refused tool calls, a failed or stopped run, steering, and failed
    checks);
  - the checks passed, or failed before as well (the card was once sent back because they failed), or there are none;
  - the run used no more than 1.5 times the tokens of the first code run.

  A trial is one round, and the card needed at least one, so rounds never make a trial negative.
- **Afterwards** the trial's branch and worktrees are removed; the trial is stored (`GET /v1/proposals`) with the
  measurements and the reason. Unless these trials made it negative, the proposal is then tried on the regression set
  (below). The proposal becomes `positive` when every trial of both was positive, `negative` when some was
  negative, and else `untried`. A `positive` or `untried` proposal gets an inbox item of the kind `proposal`; a `negative`
  one gets none, and stays on the Retro page.
- **Deciding.** `POST /v1/proposals/{id}/merge` merges the branch into the default branch of the clone with a merge commit,
  pushes it to `origin` and reads the agents again; `POST /v1/proposals/{id}/dismiss` leaves it. Both record the person's
  decision (kind `merge_proposal`) and resolve the inbox item. A merge that conflicts changes nothing.
- A server that starts finds the proposals the last server was trying and opens them again, without their trials, and ends
  the trial runs it left going as it ends other runs.

### The regression set

A proposal that fixes its own cards may still break what worked before, and changes to skills and instructions often
help nothing or harm, so each is measured on past work as well. The regression set of a repository is a fixed sample of
the cards merged there: the latest `regression.size` (default 5; see [configuration](configuration.md#regression)), chosen
among the latest 50 merged changes, the cards whose change adds or changes a reproduction test (`.moebius/reproduce`)
first, latest first among equals. It changes only as new cards merge. A card is in it only when its first code run
recorded the commit it started at; the proposal's own cards are left out.

- **The runs.** After its own trials, a proposal is tried on the regression set of each repository its cards belong to,
  one card after the other, as above: a `trial` run of the card's task by the agent of its last code run, with the
  definition from the proposal's branch, in a fresh worktree at the commit the card's first code run started at (the
  repository as it was when the card was worked on), on `moebius/trial-<proposal id>-card-<card id>`, then the checks.
- **The comparison** is with the card's code runs together, which passed: the card was merged. A regression trial is
  positive when the run succeeded, the checks passed (or there are none), and it used no more than 1.5 times their
  tokens; else negative. It is one round, never more than the card needed.
- **The bound.** A proposal adds at most `regression.size` runs for each repository of its cards. They run on the
  proposal's own thread, one at a time, like its other trials (so one proposal is tried at a time, and the schedule starts
  one only inside the trial window and when the board has a run to spare); the first regression card that fails ends the
  rest, which are stored as `skipped`. A proposal whose own trials are negative is not tried on it.
- Each regression trial is stored with `regression` true. A proposal that fixes its cards but makes a regression card
  fail is `negative` and not offered; the inbox item of one that is offered lists both results. `regression.gate =
  false` turns all of this off.

### The replays

Inside `[trial_window]`, when no proposal is being tried, a sample of refused tool calls is run again in a throwaway
copy of the worktree, so that a refusal can later be weighed by what the call would have done. `POST
/v1/judgements/{id}/replay` replays one at any hour.

- **Which calls.** `tool_call` judgements that refused the call, have no replay yet, are at most a week
  old, belong to a run whose repository still exists, and are **reversible**. At most 20 are replayed per night, the
  oldest first, one after the other; none runs beside a trial (the schedule asks the trials first, and stops when a
  proposal is `trying`). Calls that cannot be replayed are never picked and keep their outcome. A refusal on a card that is
  done already has the card's outcome (such as `merged`); its replay replaces it, since for a refusal what the call would
  have done says more, and the card still tells how it ended.
- **Reversible** means everything the call does stays inside the run's worktree: `read`, `write`, `edit`, `ls`, `grep`
  and `find` on paths inside it; and `bash` whose commands, read as the guard's patterns read them (nested shells, `cd`,
  redirects, `tee`), name only paths inside it. Not reversible: another tool; a path outside the worktree or `~`
  (`/dev/null` and the standard streams excepted); `git push`, `fetch`, `pull`, `clone`, `remote`, `ls-remote`,
  `submodule`; a URL; `curl`, `wget`, `ssh`, `scp`, `rsync`, `sftp`, `ftp`, `nc` and the like; `sudo`, `su`, `kill`,
  `pkill`; package managers and `cargo`; `env`, `printenv`, `export` and any `$` (variable or substitution);
  interpreters such as `python` and `node`; a shell nested too deeply to follow. This reads the command line, it is not a
  sandbox, so it errs on the side of not reversible.
- **The copy** is a directory `<data_dir>/replays/<judgement id>` holding the files of the run's worktree if it still
  exists (not `.git`, and no symbolic links), else a `git archive` of the card's change branch, else of the repository's
  default branch. `read`, `write` and `edit` are applied by Moebius (an edit's text must occur exactly once); `ls` is
  listed; `grep`, `find` and `bash -c` run in the copy with an empty environment (`PATH` only, `HOME` the copy), no input
  and 60 seconds, after which the process group is killed. The copy is removed afterwards; the worktree is never touched.
- **What is stored** (`GET /v1/replays`): the exit code (or that it timed out), the output (secrets removed, last 4 KB)
  and the files created, changed and deleted, found by comparing a listing of the copy with sizes and SHA-256 hashes
  before and after. Directories are not listed, so an empty one is not noticed.
- **The outcome** of the judgement is one sentence: `replayed: <ended>; <files>`. `<ended>` is `exit <code>`, `timed
  out` or `ended by a signal`. `<files>` is `created N files`, `changed N files` and `deleted N files` (those that are
  not zero, in that order, joined by `, `; `1 file` for one) or `no file changed`. For example `replayed: exit 0;
  deleted 3 files, changed 1 file`. A replay that could not be made (the copy or the checkout failed) reads `replayed: not
  run; <why>` and is stored too, so it is not tried again. The event `replayed` carries the sentence.

## Autoresearch

A research (see `POST /v1/research` in the [desktop API](desktop-api.md)) is driven by the built-in agent
`Autoresearcher`. Like `Retro-analyst` it is built in: its instructions are built in, it has the `moebius` skill, its model
comes from `agents/Autoresearcher.toml` or else from the Coder, and `agents/Autoresearcher.md` is added after the built-in
instructions. It works with or without a harness configuration repository (without one, from `[agents.Autoresearcher]`).

- Each round is a run with the purpose `research` and no card, in a fresh detached worktree of the researched repository at
  its default branch (`<data_dir>/worktrees/<repository>/research-<run id>`), which is removed when the run ends. It is
  told the goal, the measure, the direction, the baseline and every experiment so far with its result. It never waits for
  a person. The runs of a research are no part of the Retro's report.
- Each lever is a branch `research/<research id>-<short-name>` made from the default branch, with one commit whose message
  says what the lever is. A branch without a commit beyond the default branch is ignored. A branch the research has an
  experiment for is not tried again, so a round that leaves no new branch ends the research.
- Each experiment runs the measure and, if the measurement succeeded, the repository's own checks (see [the
  trials](#the-trials)), each in a fresh worktree of the branch. The branches stay in the clone, kept or not.

## Grilling

A grilling (see `POST /v1/grillings/{id}/confirm` in the [desktop API](desktop-api.md)) is a chat with the built-in agent
`Grilling-partner`, which grills a person's idea until the work is clear. Like the Moebius agent it is built in: its
instructions are built in, it has the `moebius` skill and the engineering skills `grilling` and `to-tickets` with `moebius-autonomous` (see [Engineering skills](#engineering-skills)), and its model comes from `agents/Grilling-partner.toml` (or
`[agents.Grilling-partner]`) or else from the Coder. `agents/Grilling-partner.md` is added after the built-in instructions;
`agents/Grilling-partner.toml` may set its model, skills and the rest. It is listed by `GET /v1/agents` after the Moebius
agent, and a chat with it needs a repository.

- The agent asks one question at a time and challenges assumptions. It works in the chat's worktree, a worktree of the
  repository, so it can read the code. It never waits for the person in any other way than by asking in the chat.
- It ends by writing exactly one of two files in the worktree. It never commits them and never pushes:
  - `.moebius/grilling/issues.json`, when the idea can be built: a JSON array of small issues, each doable in one card.

    ```json
    [
      {"title": "Add the export button", "body": "A button on the report page that downloads a CSV.", "blocked_by": []},
      {"title": "Write the CSV", "body": "The columns are the report's.", "blocked_by": [0]}
    ]
    ```

    `title` is a non-blank string, `body` a string (may be left out) and `blocked_by` an array of indexes of issues of the
    same array that must be done first (may be left out). An index out of range, the issue's own index or a circle makes the
    file unusable.
  - `.moebius/grilling/report.md`, when the idea needs research rather than issues: a research report in Markdown.
- A grilling does not offer its worktree as a change: nothing is committed on its branch and no `ready_to_merge` item is
  made, whatever the agent left outside `.moebius/grilling/`.
- If both files exist, `issues.json` decides. A grilling that leaves nothing usable is stored as `empty`, with the reason,
  and makes no inbox item.
