# Set up agents

## When you want this

You want the default workflow to have a Coder, and perhaps a Reviewer, or you want another agent for a purpose of your
own: a stronger model for hard cards, a reviewer that is stricter than the coder, an agent to chat with. An agent is a
named setup that a run uses: the model, how it thinks, its instructions, its skills and extensions, and a token budget.

Today agents are tables in `moebius.toml`. The harness configuration repository, which will hold them in git with edits
as commits, and MCP servers for agents, are planned for milestone M7 and do not exist yet (see the
[README](README.md)). The keys are listed exactly in [`docs/reference/configuration.md`](../reference/configuration.md).

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
| `extensions` | Paths, each given to Pi-Bolt as `--extension` after the bridge. See [Extensions](extensions.md). |
| `token_budget` | The most tokens (input plus output) the card's runs may use, the teacher's included. Over it the router answers HTTP 429 and the default scripts mark the card stuck. |

A relative path in `skills` or `extensions` is relative to the directory that holds `moebius.toml`. Moebius does not
check that the paths exist.

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
`moebius.toml` can be chatted with. Today you name the agent; there is no default.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"agent": "Coder", "message": "Explain how the router chooses an upstream.", "repository": 1}' $API/v1/chats
```

With `repository` the agent works in a new worktree on the branch `moebius/chat-<run id>` and what it leaves is committed
there when the chat ends. Without it, the agent works in an empty directory. The answer is the run; its `id` is the
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
| The server does not start. | The message names the key: an unknown key, a bad agent name, an upstream whose `api_key_env` variable is not set. |
| `no agent named X` | In a column script, a `board_error` event; in `POST /v1/chats`, status 422. The name is case sensitive. |
| Model requests fail. | The router answers 502 when it has no upstream for the agent's `protocol`, or cannot reach it. See the [router reference](../reference/router.md). |
| Model requests are refused with 429. | The card's `token_budget` or the day's budget is spent. The message says which. |
| A run fails at once. | The run is `failed` and its events hold a `moebius_error` with the reason, for example that Pi-Bolt could not be started (`pi_bolt.program`). A run's standard error is in `<data_dir>/runs/<id>/stderr.log`. |
| A skill or extension does not load. | Moebius only passes the path. Look in `<data_dir>/runs/<id>/stderr.log` and in Pi-Bolt's documentation. |
| The Reviewer never approves. | Its final answer has text after the verdict line, or no verdict line. Read the run's `summary` (`GET /v1/runs/{id}`). |
