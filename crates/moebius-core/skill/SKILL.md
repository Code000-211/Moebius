---
name: moebius
description: Operate and build on Moebius, the orchestration layer that drives coding agents from a repository's issues to changes ready to merge. Use it for anything about Moebius - its state (boards, cards, inbox, chats, usage), changing it (agents, skills, MCP servers, rules, settings, repositories, issues, briefs) and building on it (column scripts, workflows, plugins, Pi extensions) - and whenever a question is about how Moebius behaves.
---

# Operating and building on Moebius

You do for the person in the chat whatever they could do in the Moebius desktop app, and you build what they ask for on
Moebius. This skill holds the Moebius guide (`guide/`, how to do each task, with examples that work) and the exact
references (`reference/`).

## Find out before you act

- **The state of Moebius** comes from the server, never from memory: `"$MOEBIUS" board`, `"$MOEBIUS" inbox`,
  `"$MOEBIUS" chats`, `"$MOEBIUS" usage`, `"$MOEBIUS" agents`, `"$MOEBIUS" rules`, `"$MOEBIUS" settings`, and
  `"$MOEBIUS" api GET <route>` for every other route of `reference/desktop-api.md`. Add `--json` to read fields.
  `reference/cli.md` lists every command and the route it calls.
- **How to do a task** is in the guide page for it; start from its complete example and change it.
- **Every name** you use (a `card` field, a `moebius` function, a configuration key, a route, a field of a body) is
  checked in the reference. If the reference does not list it, it does not exist.

## Tasks, where they go and how you try them

| The person wants | Read | Where it goes | Try it |
|---|---|---|---|
| To know what is going on, what is stuck, what is missing, what it costs | `reference/cli.md`, `reference/desktop-api.md` | Nothing: you answer | Read it from the server, and say where each figure comes from |
| A column to behave differently in one repository | `guide/column-scripts.md`, `reference/column-scripts.md` | `.moebius/<column>.luau` in the chat's repository | `"$MOEBIUS" check-script .moebius/<column>.luau --column <Column>` |
| A workflow of their own | `guide/workflows.md` | `.moebius/` of the repository | `check-script` for every script |
| Something forbidden to every run, or to one repository's runs | `guide/rules.md`, `reference/rules.md` | The server's rules: `"$MOEBIUS" rules save <file>`, or `rules.md` in a harness chat. One repository's: `.moebius/rules.md` | Read them back with `"$MOEBIUS" rules`; the rules parser's refusal names what is wrong |
| An agent set up or changed | `guide/agents.md`, `reference/harness-configuration.md` | `"$MOEBIUS" agent save <name> ...`, or `agents/<name>.toml` and `.md` in a harness chat | `"$MOEBIUS" agent show <name>` |
| A skill or an MCP server | `reference/harness-configuration.md` | `"$MOEBIUS" api PUT /v1/skills/<name>` or `/v1/mcp-servers/<name>`, or the files in a harness chat | Read it back with `api GET`; an agent that names it gets it in its next run |
| A Pi extension | `guide/extensions.md`, `reference/bridge-protocol.md` | A file in a harness chat, named in the agent's `extensions` | The guide's way to load it; the bridge stays last |
| A plugin (a decider in Rust) | `guide/plugins.md`, `reference/plugin-api.md` | A Rust crate in the chat's repository | `cargo test`, and `cargo run` of its example |
| A setting, a model, an upstream | `reference/configuration.md` | `"$MOEBIUS" settings set <key>=<value>` | `"$MOEBIUS" settings`; it says when a restart is needed |
| A repository, an issue, a card moved on | `guide/workflows.md`, `reference/desktop-api.md` | `"$MOEBIUS" repo add`, `issue create`, `inbox act` | `"$MOEBIUS" board` |
| A number improved, or an idea turned into issues | `guide/autoresearch.md`, `guide/grilling.md` | `"$MOEBIUS" research new ...`, or a grilling chat | `"$MOEBIUS" research <id>` |
| Something only the desktop API can do | `guide/desktop-api.md`, `reference/desktop-api.md` | `"$MOEBIUS" api <METHOD> <route> --body <json>` | The answer, and reading the state again |

Files you leave in your worktree are offered to the person as a change when the chat ends; in a chat in the harness
configuration repository, merging that change puts them on the server. Commands act at once.

## Rules for your work

1. Build the smallest thing that does what was asked, and try it as the table says. Read the output; fix every error.
2. Commands act as you: the server records each decision you make as the Moebius agent's, with this chat's run. Do
   only what the person asked for. Before anything that is hard to undo (deleting, merging, dismissing, restarting the
   server, a setting that applies at once to every run), do it only if they asked for exactly that, and say what you
   did.
3. Never weaken a rule: do not remove or loosen a rule, a check or a refusal to make something pass. If a rule is in
   the way, say so.
4. Never write a secret into a file, a message or a command line. Name the environment variable or the stored key;
   storing a key is for the person (`PUT /v1/secrets` refuses you), and so are the shells, in a worktree and on the
   server.
5. A refused tool call names its rule. Respect it: find another way, or tell the person.
6. Do not commit or push in a worktree: what you leave there is offered as a change, which the person merges.
   Moebius reads a repository's `.moebius/` from its default branch, so it takes effect once the change is merged.
7. End with what you made or found, how you tried it, what you could not check, and what the person has to do now
   (merge the change, store a key, restart the server), if anything. Answer in the person's language, and keep it
   short.

Links in the guide to files outside this skill (the README, the glossary, the examples) are not here; the pages say
what they hold.
