# The Moebius guide

This guide is for anyone who builds on Moebius: a person who wants a repository to behave differently, or an agent (the
Moebius agent, whose skill this guide is) that is asked to build such a thing and has only this guide to go by. It says how to do
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
- Agents, set up in `moebius.toml` or in a harness configuration repository: model, protocol, thinking level,
  instructions, token budget, skills and extensions. In the repository, edits through the API are commits, and it holds
  the server's `rules.md` too.
- Plugins compiled into the `moebius` binary, and the plugin API.
- The bridge and Pi extensions.
- The desktop API (HTTP and one WebSocket) and chats with any configured agent.
- The Moebius agent, which builds scripts, rules, agents and extensions from this guide in a chat and offers them as a
  change. It is the default agent of a chat. See [Agents](agents.md#the-moebius-agent).

Planned, so that nothing here describes them beyond a line:

- MCP servers for agents. They come with milestone M7 (issue 40 in [`docs/issues-m2-m9.md`](../issues-m2-m9.md)). Today
  the harness configuration repository lists them in `mcp.toml` and agents can name them, but there is no way to give an
  agent an MCP server yet.
- The desktop app (`crates/moebius-desktop` is a placeholder). Everything it will do is the desktop API, which you can
  call yourself.
- Plugins in their own process.

## The how-tos

| You want to | Page |
|---|---|
| Change what a column does for one repository, and try the change | [Column scripts](column-scripts.md) |
| Know how a card moves from Backlog to Done and what a person can do on the way | [Workflows](workflows.md) |
| Forbid something to every run, or to the runs of one repository, and see a refusal | [Rules](rules.md) |
| Set up the Coder, the Reviewer or another agent, chat with one, or have the Moebius agent build something | [Agents](agents.md) |
| Have an agent look for ways to improve a number you can measure, and keep what helps | [Autoresearch](autoresearch.md) |
| Turn an idea into issues by being questioned about it | [Grilling](grilling.md) |
| Answer your own decision points with Rust code | [Plugins](plugins.md) |
| Add behaviour inside the Pi process of a run | [Extensions](extensions.md) |
| Drive Moebius from a script with `curl` | [Desktop API](desktop-api.md) |

## Where to start

1. Run a server and a card through the default workflow: the quick start in [`README.md`](../../README.md).
2. Read [Workflows](workflows.md) to see what happened and where you can step in.
3. Change one thing with a [column script](column-scripts.md) or a [rule](rules.md).
