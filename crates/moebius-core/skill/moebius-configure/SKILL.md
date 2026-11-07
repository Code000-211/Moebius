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
it with `--instructions`; `-` reads standard input. Keep `--thinking`, `--token-budget`, `--harness`, every `--skill`, `--mcp` and
`--extension` the agent had, and its `tool_output` (`--hide-outputs-after`, `--output-lines`, `--output-bytes`,
`--no-compress`; left out, each is its default) and `--no-engineering-skills` if it had `engineering_skills = false` (or one `--engineering-skill <name>` for each name of its `engineering_skills` list), unless the person asked to change them. `--harness claude-code` runs the agent on Claude
Code, which needs `--protocol anthropic` and no extensions. These commands need a harness configuration
repository; without one the agents are the `agents` tables of `moebius.toml` (see Settings below). The built-in agents
(`Moebius`, `Grilling-partner`, `Coder`, `Reviewer`) keep their built-in instructions and skills; what you save is added after
them. The Coder and the Reviewer exist without a file, with good instructions of their own (they work test first and
review read only), so to set up the agents of the default workflow give each a model and leave `--instructions` out, unless
the person wants their own words added: `"$MOEBIUS" agent save Coder --model qwen-coder`. The Coder needs a model (a card
fails without one); the Reviewer without a model takes the Coder's, and reviews begin once it is saved.

To make a new agent like an existing one (the Coder and the Reviewer included, not the other built-in agents), start from
it: `"$MOEBIUS" agent save Quick --template Coder --model qwen-coder` copies its settings and instructions under the new
name (a flag you give changes what is copied; a built-in agent's instructions become the copy's own). Check
`"$MOEBIUS" agents` first: a name that exists is replaced.

## Rules

```sh
"$MOEBIUS" rules                 # the built-in patterns, the rules in prose, each repository's and the refusals
"$MOEBIUS" rules save rules.md   # replaces the whole server rules file
```

Start from the text `"$MOEBIUS" rules --json` gives (`prose.text`), add or sharpen rules, and save the whole file. Never
remove or loosen a rule to make something pass; if a rule is in the way, tell the person. One `## name` heading per
rule, a name without spaces, never twice. One repository's rules are the `rules.md` of its settings (`repos/<repository name>/` of the harness
configuration repository); save the whole file as a commit there (the server checks it first and answers 422 with the
wrong line):

```sh
"$MOEBIUS" api GET /v1/repositories/1/settings        # every file of the settings, its text, and any leftover .moebius/ files
"$MOEBIUS" api PUT /v1/repositories/1/settings/rules.md --body '{"text": "## small-diffs\nKeep changes small.\n", "reason": "..."}'
```

The other files of the settings are saved the same way: `review.md` (notes for the Standards review),
`checks` (the checks of a repository without a forge), `test-first` (an empty text turns test
first on, `DELETE` turns it off) and `localize`. A repository's own `.moebius/` folder is not read; if `settings` lists
`leftover` files, offer to move them to the settings.

## A column's script in one repository

```sh
"$MOEBIUS" api GET /v1/repositories/1/scripts/Not%20started               # the script that runs now, built in or its own
"$MOEBIUS" check-script ready.luau --column "Not started"                  # check before saving
"$MOEBIUS" api PUT /v1/repositories/1/scripts/Not%20started --body '{"text": "...", "reason": "..."}'
"$MOEBIUS" api DELETE /v1/repositories/1/scripts/In%20progress     # back to the built-in script
```

Build the body with `jq -n --rawfile text ready.luau '{text: $text}' > body.json` and pass `--body body.json`. The server
checks the script again and refuses it with the check's message. How to write the script: the `moebius-workflows` skill.

## A file of a repository

A project's own settings (`rules.md`, `review.md`, `checks`, `test-first`, `localize` and the column
scripts) are not files of the repository: change them with the settings and scripts routes above, never with
`/files/`, and never in a `.moebius/` folder, which is not read.

The settings that plugins ask of each repository are project settings too (`plugins/<plugin>/<key>`). Read them with
`"$MOEBIUS" api GET /v1/repositories/1/plugin-settings`, and the server's with `GET /v1/plugins/settings`. Only a person
saves them (the `PUT` routes refuse you with `403`), so give the person the text and say which page and field it goes
in. Never put such a value in a file of the repository.

Turning a plugin off or on (`enabled` of its `[[plugins]]` entry) is a person's too, and takes effect at once, without a
restart: you are refused with `403`, so tell the person to use the switch on the Plugins page or `"$MOEBIUS" plugin
disable <name>` (`enable` to turn it on; `--unwrap-runs` to confirm turning off the last launcher, after which runs start
unwrapped; `--allow-unchecked` to confirm turning off `guard`, after which tool calls are no longer checked). The built-in deciders `guard` and `localize` can be turned off and on the same way. A plugin that is off keeps its entry and its settings and is not started or asked.

When the person asks you to change a file of a repository right on its default branch (often the file open in the
Editor of the project page, which your context names with the cursor's line), read and write it through the server:

```sh
"$MOEBIUS" api GET "/v1/repositories/1/tree?path=src"             # a directory, directories first
"$MOEBIUS" api GET /v1/repositories/1/files/src/main.rs           # its text, or binary / too_large
"$MOEBIUS" api PUT /v1/repositories/1/files/src/main.rs --body body.json
```

Build the body with `jq -n --rawfile text main.rs '{text: $text, message: "Say why", reason: "..."}' > body.json`. Each
save is a commit on the default branch (pushed for a repository on GitHub) and a decision `edit_file` by you. For a
change that should be reviewed, create an issue instead.

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

GitHub is reached with a personal access token or with a GitHub App, never both: `github.token_file` or
`github.token_secret` (the name of a stored secret) for the token, `github.app_id` with `github.private_key_file` for
the App. The token is the simple way; with it Moebius acts as the token's user, so its pull requests and comments carry
that user's name, and GitHub does not let that user approve their own pull request. Never ask for the token's value or
write it into a setting: the person stores it (`moebius secrets set github`) or puts it in a file, and you set only the
name or the path. A fine-grained token needs the repository permissions Metadata read, Contents, Issues and Pull
requests read and write, and Checks read; a classic one the scope `repo`. The server does not start when GitHub refuses
the token (`docs/reference/configuration.md`, `[github]`).

## Skills and MCP servers

Both live in the harness configuration repository and are named in an agent's `skills` and `mcp`. A server without
one cannot have them. Only the person can set one up: the "Set up" button of the Skills, MCP servers or Agents page of
the app (`POST /v1/harness/setup`, which refuses you with `403`) moves the agents and rules of `moebius.toml` into a new
repository and restarts the server. Tell the person that, and do not try to make one yourself.

```sh
"$MOEBIUS" api GET /v1/skills
"$MOEBIUS" api PUT /v1/skills/house-style --body '{"description": "Write plainly. Use it for every change to prose.", "text": "# House style\n..."}'
"$MOEBIUS" api GET /v1/mcp-servers
"$MOEBIUS" api PUT /v1/mcp-servers/docs --body '{"command": "docs-server", "args": ["--stdio"], "env": {"API_KEY": "${DOCS_TOKEN}"}}'
```

The description says when to use the skill, in one line. An MCP server takes secrets only as `${NAME}` for a name in
`harness.mcp_env`. Then give it to an agent with `agent save ... --skill house-style --mcp docs`. Built-in skills
(`moebius` and the `moebius-` task skills) are read only.

### Importing

When the person gives a zip file (uploaded with `POST /v1/uploads`; its `path` is on the server) or a link to a
repository, fetch it into a new directory of your working directory and read it there; files are written only inside
the worktree:

```sh
unzip -q /srv/moebius/uploads/3f2a.zip -d import    # an uploaded zip
git clone --depth 1 https://github.com/acme/skills import
"$MOEBIUS" api GET /v1/skills                       # what exists already
"$MOEBIUS" api PUT /v1/skills/pdf-forms --body body.json
"$MOEBIUS" api GET /v1/skills/pdf-forms
```

Build each body from the source: for a skill, `jq -n --arg d "<description>" --rawfile text body.md '{description: $d,
text: $text}'` with the text after the `SKILL.md` frontmatter. Then write each other text file of the skill with
`jq -Rs '{text: .}' < scripts/fill.py > body.json` and `"$MOEBIUS" api PUT /v1/skills/pdf-forms/files/scripts/fill.py
--body body.json` (after the skill, which must exist first); name a file that is not text, which is added in a chat in
the harness configuration repository. For an MCP server, read its README, package.json
or pyproject.toml and any `mcp.json` example for the command, arguments and environment, or the URL and headers, and
put each key as `${NAME}` with `NAME` in `harness.mcp_env` (`"$MOEBIUS" settings set` it, keeping the names there).
Never replace an existing skill or server without asking, never use a built-in skill name, never write a key. End with
what was imported under which names, and what the person must do: keys to store (Settings page, or the server's
environment for `${NAME}`), programs to install, agents to give it to.

In a chat in the harness configuration repository, write the files instead (`agents/<name>.toml` and `.md`,
`skills/<name>/SKILL.md`, `mcp.toml`, `rules.md`); the person merges them as a change.

## Try it

Read every change back (`agent show`, `rules`, `settings`, `api GET`) and quote what the server now says. A new agent,
skill or server is used from the next run: start a short chat with the agent to show it.

## What can go wrong

- `422` names the field or line that is wrong; `409` for a skill or server says the server has no harness configuration
  repository (the person sets one up, see above), or that an agent still names what you delete.
- `agent save` without the settings the agent had removes them: always start from `agent show`.
- A setting in `restart_keys` does nothing until the server restarts.

## Details

In the `moebius` skill: `guide/agents.md`, `reference/harness-configuration.md`, `guide/rules.md`, `reference/rules.md`,
`reference/configuration.md` (every key), `reference/desktop-api.md` (the bodies) and `reference/cli.md`.
