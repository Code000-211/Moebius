# The Moebius guide

This guide is for anyone who builds on Moebius: a person who wants a repository to behave differently, or an agent (the
Moebius agent, once it exists) that is asked to build such a thing and has only this guide to go by. It says how to do
each task, step by step, with an example that works.

## How the guide relates to the reference

- [`docs/reference/`](../reference/) holds the exact contracts: every `card` field, every `moebius` function, every
  configuration key, every route. Look things up there.
- This guide holds the tasks. Each page says when you want something, what to do, a complete example, how to try it and
  what can go wrong and how you see it. It links to the reference instead of repeating it.
- [`GLOSSARY.md`](../../GLOSSARY.md) says what the words mean. [`docs/architecture.md`](../architecture.md) says how a
  card moves through the code.

Every `luau` example in the guide is a complete column script, and a test runs it. Every `toml` example is parsed as a
`moebius.toml`, every rules example as a `rules.md`, and every `curl` command names a route that exists. The test is
`crates/moebius-core/tests/guide.rs`. Examples that the test checks in more detail say so in a comment above them.

## What exists today and what does not

Built and described here:

- Column scripts, the default workflow, and a repository's overrides in `.moebius/`.
- Rules: the built-in rules, rules in prose in a server file and in a repository, and the teacher model.
- Agents, set up in `moebius.toml`: model, protocol, thinking level, instructions, token budget, skills and extensions.
  Skills and extensions are paths to files on the server.
- Plugins compiled into the `moebius` binary, and the plugin API.
- The bridge and Pi extensions.
- The desktop API (HTTP and one WebSocket) and chats with any configured agent.

Planned, so that nothing here describes them beyond a line:

- The harness configuration repository (agents, instructions, skills, rules and MCP servers kept in a git repository of
  their own, with edits as commits), the skills directory and MCP servers. They come with milestone M7 (issues 39 and 40
  in [`docs/issues-m2-m9.md`](../issues-m2-m9.md)). Today there is no way to give an agent an MCP server.
- The Moebius agent, which builds scripts, rules, agents and extensions from this guide (issue 31). Today a chat can
  start with any agent in `moebius.toml`.
- The desktop app (`crates/moebius-desktop` is a placeholder). Everything it will do is the desktop API, which you can
  call yourself.
- Plugins in their own process.

## The how-tos

| You want to | Page |
|---|---|
| Change what a column does for one repository, and try the change | [Column scripts](column-scripts.md) |
| Know how a card moves from Backlog to Done and what a person can do on the way | [Workflows](workflows.md) |
| Forbid something to every run, or to the runs of one repository, and see a refusal | [Rules](rules.md) |
| Set up the Coder, the Reviewer or another agent, and chat with one | [Agents](agents.md) |
| Answer your own decision points with Rust code | [Plugins](plugins.md) |
| Add behaviour inside the Pi process of a run | [Extensions](extensions.md) |
| Drive Moebius from a script with `curl` | [Desktop API](desktop-api.md) |

## Where to start

1. Run a server and a card through the default workflow: the quick start in [`README.md`](../../README.md).
2. Read [Workflows](workflows.md) to see what happened and where you can step in.
3. Change one thing with a [column script](column-scripts.md) or a [rule](rules.md).
