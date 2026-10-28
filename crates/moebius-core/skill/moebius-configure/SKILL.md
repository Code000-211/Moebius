---
name: moebius-configure
description: Change how a Moebius server is set up - an agent (model, instructions, skills, MCP servers, extensions), the rules, a column's script in one repository, a setting of moebius.toml, a skill or an MCP server. Use it whenever the person asks to add, change or remove any of these, or to write an agent's instructions or the rules for them.
---

# Configure Moebius

Goal: make the one change the person asked for, keep everything else as it is, and read it back. Commands act at once
and are recorded as yours.

## Agents

`agent save` replaces the whole agent, so read it first and pass every setting you keep:

```sh
"$MOEBIUS" agent show Coder --json      # settings and instructions as they are
"$MOEBIUS" agent save Coder --model qwen-coder --protocol openai --thinking high --token-budget 2000000 \
  --skill house-style --mcp docs --extension extensions/log.ts --instructions coder.md
"$MOEBIUS" agent show Coder
```

Write the instructions to a file first (`coder.md`, outside the worktree's tracked files or delete it after) and pass
it with `--instructions`; `-` reads standard input. Keep `--thinking`, `--token-budget`, every `--skill`, `--mcp` and
`--extension` the agent had, unless the person asked to change them. These commands need a harness configuration
repository; without one the agents are the `agents` tables of `moebius.toml` (see Settings below). The built-in agents
(`Moebius`, `Grilling-partner`) keep their built-in instructions and skills; what you save is added after them.

## Rules

```sh
"$MOEBIUS" rules                 # the built-in patterns, the rules in prose, each repository's and the refusals
"$MOEBIUS" rules save rules.md   # replaces the whole server rules file
```

Start from the text `"$MOEBIUS" rules --json` gives (`prose.text`), add or sharpen rules, and save the whole file. Never
remove or loosen a rule to make something pass; if a rule is in the way, tell the person. One `## name` heading per
rule, a name without spaces, never twice. One repository's rules are `.moebius/rules.md` in its worktree.

## A column's script in one repository

```sh
"$MOEBIUS" api GET /v1/repositories/1/scripts/Ready               # the script that runs now, built in or its own
"$MOEBIUS" check-script ready.luau --column Ready                  # check before saving
"$MOEBIUS" api PUT /v1/repositories/1/scripts/Ready --body '{"text": "...", "reason": "..."}'
"$MOEBIUS" api DELETE /v1/repositories/1/scripts/In%20progress     # back to the built-in script
```

Build the body with `jq -n --rawfile text ready.luau '{text: $text}' > body.json` and pass `--body body.json`. The server
checks the script again and refuses it with the check's message. How to write the script: the `moebius-workflows` skill.

## Settings

```sh
"$MOEBIUS" settings                                   # every key, its value, (default), and what needs a restart
"$MOEBIUS" settings set board.round_limit=7 budgets.day_tokens=5000000
"$MOEBIUS" settings set --remove budgets.day_tokens
"$MOEBIUS" settings set 'agents.Reviewer={"model": "m", "thinking": "high"}'
```

A value is JSON when it parses as JSON, text otherwise. The answer says `restart_needed` and the keys; restart only
when the person agrees (`"$MOEBIUS" server restart`). The last line changes an agent of `moebius.toml`, for a server
without a harness configuration repository. Never put an API key in a setting: name the variable (`api_key_env`) or
the stored key (`api_key_secret`) and leave storing it to the person.

## Skills and MCP servers

Both live in the harness configuration repository and are named in an agent's `skills` and `mcp`.

```sh
"$MOEBIUS" api GET /v1/skills
"$MOEBIUS" api PUT /v1/skills/house-style --body '{"description": "Write plainly. Use it for every change to prose.", "text": "# House style\n..."}'
"$MOEBIUS" api GET /v1/mcp-servers
"$MOEBIUS" api PUT /v1/mcp-servers/docs --body '{"command": "docs-server", "args": ["--stdio"], "env": {"API_KEY": "${DOCS_TOKEN}"}}'
```

The description says when to use the skill, in one line. An MCP server takes secrets only as `${NAME}` for a name in
`harness.mcp_env`. Then give it to an agent with `agent save ... --skill house-style --mcp docs`. Built-in skills
(`moebius` and the `moebius-` task skills) are read only.

In a chat in the harness configuration repository, write the files instead (`agents/<name>.toml` and `.md`,
`skills/<name>/SKILL.md`, `mcp.toml`, `rules.md`); the person merges them as a change.

## Try it

Read every change back (`agent show`, `rules`, `settings`, `api GET`) and quote what the server now says. A new agent,
skill or server is used from the next run: start a short chat with the agent to show it.

## What can go wrong

- `422` names the field or line that is wrong; `409` for a skill or server says the server has no harness configuration
  repository, or that an agent still names what you delete.
- `agent save` without the settings the agent had removes them: always start from `agent show`.
- A setting in `restart_keys` does nothing until the server restarts.

## Details

In the `moebius` skill: `guide/agents.md`, `reference/harness-configuration.md`, `guide/rules.md`, `reference/rules.md`,
`reference/configuration.md` (every key), `reference/desktop-api.md` (the bodies) and `reference/cli.md`.
