# Harness configuration

The harness configuration is a git repository that holds the agents, their skills, the server's rules and the MCP servers.
This page is for whoever sets it up or edits it, by hand or through the desktop API: it lists every file and key. The code
is in `crates/moebius-core/src/harness_repo.rs`.

## Using it

Name the repository in `moebius.toml`:

```toml
[harness]
url = "https://github.com/acme/harness-configuration"
```

- `url` is anything `git clone` takes. With `[github]` set, an `https://github.com/...` URL is cloned, fetched and pushed
  with the GitHub App's installation token, like the repositories Moebius works on; the App needs write access to the
  repository for edits. Any other URL, such as a local path, uses plain git.
- The server clones it to `<data_dir>/harness` when it starts. A clone that cannot be made stops the server; a clone that is
  there already is used as it is, so the server starts without the network.
- Every 30 seconds the server fetches `origin` and fast-forwards the clone's default branch (the branch that is checked
  out). When that changed anything, the agents are read again. Runs that are going keep the agent they started with.
- With `[harness]`, `[agents.*]` and `rules_file` in `moebius.toml` are errors that say to move them into the repository, so
  that there is one source of truth. Without `[harness]` everything is configured in `moebius.toml`, and the routes
  `GET`, `PUT` and `DELETE /v1/agents/{name}` answer `422`.
- Everything is read from the default branch. Other branches of the repository are not read.

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
`agents/Moebius.md` is added after its built-in instructions (see [configuration](configuration.md)).

## `agents/<name>.toml`

`<name>` is made of letters, digits, `-` and `_` and is not empty. Unknown keys are errors.

| Key | Type | Default | Meaning and validation |
|---|---|---|---|
| `model` | string | required | The model's name, sent to the router and given to Pi-Bolt as `--model`. Not blank. |
| `protocol` | string | `openai` | `openai` or `anthropic`: which router endpoint the harness uses. |
| `thinking` | string | none | Given to Pi-Bolt as `--thinking`. Pi-Bolt checks the value. |
| `token_budget` | integer | none | The most tokens a card may use with this agent, as `agents.<name>.token_budget` in [configuration](configuration.md). At least 1. |
| `skills` | array of strings | `[]` | Names of directories under `skills/` that hold a `SKILL.md`. Each is given to Pi-Bolt as `--skill`. A name that is not there is an error. |
| `extensions` | array of strings | `[]` | Paths of files in the repository, relative to its root and without `..`. Each is given to Pi-Bolt as `--extension`, before the bridge. A path that is not a file is an error. |
| `mcp` | array of strings | `[]` | Names of servers in `mcp.toml`. A name that is not there, or any name while `mcp.toml` cannot be read, is an error. The servers are not given to the agent yet. |

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
`SKILL.md` is not a skill. An agent that names a skill that is missing is left out with an error.

## `rules.md`

The server's rules, in the format of [rules](rules.md): one `## name` section per rule. It replaces `rules_file`, and has
the same precedence over a repository's `.moebius/rules.md`: the server's rules come first and a repository cannot
weaken them. Like `rules_file` it is read afresh for every tool call, from the clone's working tree, which is the
default branch. Without the file there are no server rules.

## `mcp.toml`

The MCP servers agents can name. Each server is a top-level table named for it, such as `[docs]`. Moebius reads only the
names, to check the `mcp` key of agents; what a server's table holds is not read yet.

```toml
[docs]
command = "docs-server"
```

## A broken agent

A file that cannot be used leaves only its agent out; the other agents are read and used. The agent is reported:

- by the event `harness_error`, `{"agent": name, "message": string}`, each time the repository is read (see the
  [desktop API](desktop-api.md));
- in `GET /v1/agents` as an entry with an `error` and an empty `model`, so that a client can show it and not offer it;
- by a run or a chat that asks for it, which fails with `the agent <name> cannot be used: <error>`.

The errors: a name that is not valid; `agents/<name>.md` without `agents/<name>.toml`; a `.toml` file that is not valid
TOML, has an unknown key or a value of the wrong type; a missing `model`; a skill, extension or MCP server that is not
there. An error in `agents/Moebius.toml` is reported the same way, and the Moebius agent then runs as if the file were
not there. A fetch that fails is reported as a `harness_error` without an `agent`, and the agents stay as they were.

## Edits through the API

These routes are in the [desktop API](desktop-api.md). No secret belongs in these files and none is kept in them:
model providers' keys are read from the environment by the router, so the files can be read and written through the API
as they are.

- `GET /v1/agents/{name}` answers `{"name", "settings", "instructions"}`, where `settings` has the keys of the table above
  (`protocol` always, `skills`, `extensions` and `mcp` always, `thinking` and `token_budget` when set) and `instructions`
  is the text of the `.md` file, empty without one.
- `PUT /v1/agents/{name}` takes `{"settings": {...}, "instructions": "..."}` (`instructions` may be left out) and
  is checked as a file read is. It writes `agents/<name>.toml` (the file is written again from the settings, so comments
  in it are lost) and `agents/<name>.md` (removed when the instructions are blank), commits them on the default branch as
  `Moebius <moebius@localhost>` with the message `Add the agent <name>` or `Update the agent <name>`, records the
  decision (kind `edit_agent`, decider `person`, the settings and instructions in its context), and pushes the branch to
  `origin`. An edit that changes nothing commits nothing and records nothing.
- `DELETE /v1/agents/{name}` removes both files the same way, with the message `Delete the agent <name>`.

If anything fails after the commit was made, including the push, the commit is undone (`git reset --hard` to the head
before it), so the clone never drifts from `origin`. The request then fails with the error. When someone else pushed to
`origin` meanwhile, the push is refused for that reason, and the next refresh brings their commits in.
