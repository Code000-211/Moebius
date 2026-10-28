# Set up agents

## When you want this

You want the default workflow to have a Coder, and perhaps a Reviewer, or you want another agent for a purpose of your
own: a stronger model for hard cards, a reviewer that is stricter than the coder, an agent to chat with. An agent is a
named setup that a run uses: the model, how it thinks, its instructions, its skills and extensions, and a token budget.

Agents are kept in one of two places. In tables of `moebius.toml`, the simple way, which the steps below use. Or in a
harness configuration repository, a git repository of their own, where an edit is a commit and agents can be changed
through the API (see [In a harness configuration repository](#in-a-harness-configuration-repository)). MCP servers
are given to agents from the repository only. The keys of `moebius.toml` are
listed exactly in [`docs/reference/configuration.md`](../reference/configuration.md), those of the repository in
[`docs/reference/harness-configuration.md`](../reference/harness-configuration.md).

## The Moebius agent

The Moebius agent is bundled and always there. It knows Moebius from this guide and the references, which are its skill,
and builds what a person asks for: a column script, a rule, an agent, an extension. It works in a chat, in the
repository the chat is in (what belongs to one repository goes in its `.moebius/`), tries a column script with
`moebius check-script` and leaves what it built on the chat's branch. When the chat ends, that work is offered as a
change that a person merges (see [Chat with an agent](#chat-with-an-agent)). It never commits or pushes by itself and it
does not merge.

It is the default of a chat: `POST /v1/chats` without `agent` starts a chat with it. It needs a model. Without
configuration it takes the Coder's model and protocol; with an `[agents.Moebius]` table it uses that table's:

```toml
[agents.Moebius]
model = "my-hosted-model"
protocol = "anthropic"
thinking = "high"
instructions = "Write scripts in the style of the repository's other scripts."
```

The keys are those of any agent below. Its `instructions` are added after the built-in ones, and its `skills` after the
`moebius` skill, which it always has. With neither `[agents.Moebius]` nor `[agents.Coder]`, a chat with it is refused
with status 422 and a message that says which to configure. The skill is written to `<data_dir>/skills/moebius/` at
every start, so a change to the guide arrives with the server.

## The Grilling partner

The Grilling partner is bundled too: a chat with `agent` set to `Grilling-partner` in a repository questions an idea until the
work is clear and ends in issues you confirm, or in a research report (see [Grilling](grilling.md)). Like the Moebius
agent it takes the Coder's model unless `[agents.Grilling-partner]` gives it one, and its `instructions` are added after the
built-in ones.

## The steps

1. Add an `[agents.<name>]` table to `moebius.toml`. The name is made of letters, digits, `-` and `_`. Only `model` is
   required.
2. Make sure the router has an upstream for the agent's `protocol` (`openai` by default, or `anthropic`). The agent's
   model name is sent to that upstream as it is written.
3. Restart the server. The file is read once, at start; a change to it does nothing until then. A mistake (an unknown
   key, a bad name) stops the server with a message that names the key.
4. Use the agent: the default workflow starts `Coder` and, when it exists, `Reviewer`; a script can start any agent
   with `moebius.start_run`; a chat can start with any agent.

| Key | What it does |
|---|---|
| `model` | The model's name, given to Pi-Bolt as `--model`. |
| `protocol` | `openai` or `anthropic`: which router endpoint the harness uses. |
| `thinking` | Given to Pi-Bolt as `--thinking`, such as `low`, `medium` or `high`. Pi-Bolt checks the value. |
| `instructions` | Text given to Pi-Bolt as `--append-system-prompt`. |
| `skills` | Paths, each given to Pi-Bolt as `--skill`. |
| `extensions` | Paths, each given to Pi-Bolt as `--extension` before the bridge. See [Extensions](extensions.md). |
| `token_budget` | The most tokens (input plus output) the card's runs may use, the teacher's included. Over it the router answers HTTP 429 and the default scripts mark the card stuck. |

A relative path in `skills` or `extensions` is relative to the directory that holds `moebius.toml`. Moebius does not
check that the paths exist.

## In a harness configuration repository

The server can keep its agents and rules in a git repository instead of `moebius.toml`. That is one place for them with a
history, and the desktop API can change them: every edit is a commit. With `[harness]` set, `[agents.*]` and `rules_file`
in `moebius.toml` are errors, so the repository is the only source.

1. Make a git repository with the layout below and push it somewhere the server can reach. A GitHub repository needs the
   GitHub App to have write access to it, if edits through the API are wanted.
2. Name it in `moebius.toml`, as shown below.
3. Start the server. It clones the repository to `<data_dir>/harness` and reads the agents from its default branch. Every
   30 seconds it fetches and reads them again when something changed, so a commit pushed by hand arrives without a
   restart.

```toml
[harness]
url = "https://github.com/acme/harness-configuration"
```

An agent is two files, `agents/<name>.toml` for the settings and `agents/<name>.md` for the instructions, which is
optional. The name is made of letters, digits, `-` and `_`.

<!-- agent-file -->
```toml
model = "my-local-model"
thinking = "medium"
token_budget = 2000000
skills = ["house-style"]
extensions = ["extensions/log-tool-calls.ts"]
```

An agent can be given MCP servers, which are described in `mcp.toml` of the repository and named in the agent's `mcp`
key. A server is a command (`command`, `args`, `env`) or a remote `url` (`headers`), and takes secrets from the server's
environment with `${NAME}` for the names listed in `mcp_env` of `[harness]`:

<!-- skip -->
```toml
[docs]
command = "docs-server"
env = { API_KEY = "${DOCS_TOKEN}" }
```

With `mcp_env = ["DOCS_TOKEN"]` in `moebius.toml` and `mcp = ["docs"]` in the agent's file, its runs get the server's
tools as `mcp__docs__<tool>`, checked by the rules like any tool call. A run is given exactly its agent's skills and
servers. The agent can read what a server is given, so give it only what the agent may see; see the
[reference](../reference/harness-configuration.md#mcptoml).

`agents/Coder.md`:

```markdown
Keep changes small. Run the repository's tests before you stop.
```

A Reviewer on a hosted model needs only its settings, since the instructions are optional:

<!-- agent-file -->
```toml
model = "my-hosted-model"
protocol = "anthropic"
thinking = "high"
```

`skills` names directories under `skills/` that hold a `SKILL.md` (here `skills/house-style/SKILL.md`), `extensions` names
files in the repository, and `mcp` names servers in `mcp.toml`. Unknown keys, and names that are not in the repository, are
errors. The repository's `rules.md` holds the server's rules, in the format of [Rules](rules.md).

An agent whose files are broken is left out and the others go on: `GET /v1/agents` lists it with an `error` and an empty
`model`, the event `harness_error` reports it, and a run that asks for it fails with the error. Fix the file and commit;
the next refresh reads it again.

Change an agent through the API instead of by hand: the edit is checked, committed as `Moebius` and pushed to the
repository. If the push is refused, nothing changes and the request fails with git's message.

```sh
curl -s -H "Authorization: Bearer $TOKEN" $API/v1/agents/Coder
curl -s -X PUT -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"settings": {"model": "my-local-model", "thinking": "high"}, "instructions": "Keep changes small."}' $API/v1/agents/Coder
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" $API/v1/agents/Strong
curl -s -H "Authorization: Bearer $TOKEN" $API/v1/skills
curl -s -H "Authorization: Bearer $TOKEN" $API/v1/mcp-servers
```

The answer of `PUT` carries `commit`, the short id of the commit. `GET /v1/skills` and `GET /v1/mcp-servers` list what an
agent can name, for an editor (the desktop app's Agents screen uses them); they show no secret and are empty without
`[harness]`. The desktop app edits agents this way.

The agent is used from the next run on. The exact files, keys and errors are in the
[reference](../reference/harness-configuration.md).

Without `[harness]` the three routes of an agent answer 422: the agents are in `moebius.toml`.

## Examples

### A Coder and a Reviewer on a local model

```toml
[[router.upstreams]]
protocol = "openai"
base_url = "http://127.0.0.1:8080"

[agents.Coder]
model = "my-local-model"
thinking = "medium"
instructions = "Keep changes small. Run the repository's tests before you stop."
token_budget = 2000000

[agents.Reviewer]
model = "my-local-model"
thinking = "high"
instructions = "Be strict about missing tests."
token_budget = 500000
```

### An agent on a hosted model

The key is read from an environment variable, never from the file. If the variable is not set when the server starts,
the server does not start.

```toml
[[router.upstreams]]
protocol = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"

[agents.Strong]
model = "my-hosted-model"
protocol = "anthropic"
thinking = "high"
```

### Skills and extensions

A skill is what Pi-Bolt documents as one: a directory with a `SKILL.md` that tells the agent how to do a kind of work,
which it loads when the task calls for it. An extension is TypeScript that runs inside the Pi process.

```toml
[agents.Coder]
model = "my-local-model"
skills = ["skills/house-style"]
extensions = ["extensions/log-tool-calls.ts"]
```

### Use a stronger agent for large cards

The default `code.luau` always starts `Coder`. This version starts `Strong` for a card whose title contains `[large]`
when that agent exists, and `Coder` otherwise. The token budget is the chosen agent's.

<!-- script: code.luau; test: agent-by-title -->
```luau
local function agent_for(card)
  if string.find(card.title, "[large]", 1, true) ~= nil and moebius.has_agent("Strong") then
    return "Strong"
  end
  return "Coder"
end

function on_card(card)
  local run = card.run
  local agent = agent_for(card)
  local budget = moebius.agent_budget(agent)
  if run ~= nil and (run.status == "starting" or run.status == "running") then
    moebius.wait("coding")
  elseif run ~= nil and run.purpose == "code" and run.status == "succeeded" and card.note == nil then
    moebius.move("In review")
  elseif card.stopped then
    moebius.stuck("stopped by a person")
  elseif card.rounds >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  elseif budget ~= nil and card.tokens >= budget then
    moebius.stuck("the token budget of " .. budget .. " is spent")
  elseif moebius.day_budget ~= nil and moebius.tokens_today() >= moebius.day_budget then
    moebius.wait("the daily token budget is spent")
  else
    moebius.start_run(agent, "code", card.note)
    if card.rounds == 0 then
      moebius.wait("coding")
    else
      moebius.wait("coding, round " .. (card.rounds + 1))
    end
  end
end
```

Commit it as `.moebius/code.luau` (see [Column scripts](column-scripts.md)). A card is stuck when its tokens reach the
budget of the agent chosen at that moment.

## What the default workflow expects of the agents

### The Coder

The agent named `Coder` must exist, or every card stops in `In progress` with a `board_error` saying
`no agent named Coder`. A round is a `code` run in a worktree of its own. Its prompt is the issue (its number, title
and body), then: make the change, run what checks the repository has, stop when the issue is done, and Moebius commits what is
left in the worktree. After a send-back it ends with `Notes from the last round:` and the reason.

Moebius does the commit. A `code` run whose worktree has nothing uncommitted when the agent stops has failed with
`the agent changed nothing`, even if the agent made a commit of its own. So do not tell the Coder to commit.

### The Reviewer

The agent named `Reviewer` is optional. With it, every change is reviewed before it is offered; without it changes are
offered unreviewed. A review run is told to read the change on its branch, not to edit files, to write its findings and
to end with one last line, `VERDICT: approve` or `VERDICT: changes`.

The run's final answer, which is the text of the last assistant message, is read like this:

- The last line decides. `VERDICT: approve` (the word in any case) approves; the lines before it are the findings.
- `VERDICT: changes` sends the card back to the Coder, and the findings are its note. A `changes` without findings
  becomes `The review asked for changes but gave no findings.`
- Anything else, including no verdict line or text after it, is `changes` with the finding `The review gave no verdict.`
  Work that was not reviewed is not approved.

If you give the Reviewer `instructions`, keep them from adding closing remarks after the verdict line.

### Any other agent

A column script starts it by name with `moebius.start_run(agent, purpose, note)`. The purpose is any string. Only
`code` and `review` are counted by the default scripts, and a purpose other than `review` is prompted like `code`.

## Chat with an agent

A chat is a run without a card: you write, the agent answers, and you can go on, steer and stop. Any agent in
`moebius.toml` can be chatted with, and the Moebius agent, which a chat without `agent` is with.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"agent": "Coder", "message": "Explain how the router chooses an upstream.", "repository": 1}' $API/v1/chats
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"message": "Write a ready.luau that starts a card only when nothing blocks it.", "repository": 1}' $API/v1/chats
```

With `repository` the agent works in a new worktree on the branch `moebius/chat-<run id>` and what it leaves is committed
there when the chat ends. If the branch then has commits that the default branch does not, they are offered as a change
without a card: an inbox item of kind `ready_to_merge`, the event `change_offered` and, on GitHub, a pull request titled
as the chat is. A person merges it with `POST /v1/changes/{id}/merge`. A chat that changed nothing offers nothing.
Without `repository`, the agent works in an empty directory and nothing is offered. The answer is the run; its `id` is the
chat's id. Read what the agent says as run events and send the next message:

```sh
curl -s -H "Authorization: Bearer $TOKEN" "$API/v1/runs/$CHAT/events?after=0"
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"message": "Now the budgets."}' $API/v1/chats/$CHAT/messages
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{}' $API/v1/chats/$CHAT/end
```

A chat does not count against `run_limit`; the daily budget applies to it and its `token_budget` counts the chat's own
tokens. Every tool call in a chat is checked by the built-in rules and the server's rules. See the
[desktop API](desktop-api.md) for the rest.

## How to try an agent

Start a chat with it and read its events. A first message that makes it call a tool shows the whole path: model request
through the router, tool call through the bridge (a `tool_check` event), tool result. The agent's answer is in the data of the event
of kind `agent_end`, which is Pi's own record.

## What can go wrong and how you see it

| What happens | How you see it |
|---|---|
| The server does not start. | The message names the key: an unknown key, a bad agent name, an upstream whose `api_key_env` variable is not set, `[agents.*]` together with `[harness]`, or a harness repository that cannot be cloned. |
| An agent of the harness repository is missing from the chats. | It is listed by `GET /v1/agents` with an `error`, and the event `harness_error` carried it. |
| `no agent named X` | In a column script, a `board_error` event; in `POST /v1/chats`, status 422. The name is case sensitive. |
| Model requests fail. | The router answers 502 when it has no upstream for the agent's `protocol`, or cannot reach it. See the [router reference](../reference/router.md). |
| Model requests are refused with 429. | The card's `token_budget` or the day's budget is spent. The message says which. |
| A run fails at once. | The run is `failed` and its events hold a `moebius_error` with the reason, for example that Pi-Bolt could not be started (`pi_bolt.program`). A run's standard error is in `<data_dir>/runs/<id>/stderr.log`. |
| A skill or extension does not load. | Moebius only passes the path. Look in `<data_dir>/runs/<id>/stderr.log` and in Pi-Bolt's documentation. |
| The Reviewer never approves. | Its final answer has text after the verdict line, or no verdict line. Read the run's `summary` (`GET /v1/runs/{id}`). |
