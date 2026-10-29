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
- Agents, skills and MCP servers, set up in `moebius.toml` or in a harness configuration repository. In the repository,
  every edit through the API is a commit, and it holds the server's `rules.md` too.
- Plugins compiled into the `moebius` binary or running as their own process (`[[plugins]]` in `moebius.toml`), and the
  plugin API.
- The bridge and Pi extensions.
- The desktop API (HTTP and one WebSocket), the desktop app built on it, and the [command line](command-line.md),
  which reaches every route of it.
- Chats with any agent, in a repository, in the harness configuration repository or in an empty directory. The Moebius
  agent, the default agent of a chat, operates Moebius through the command line and builds scripts, rules, agents,
  skills and extensions from this guide; see [Agents](agents.md#the-moebius-agent).
- The nightly Retro, trials and replays, autoresearch, grilling, the training-set export and the decision model.
- [Automations](automations.md): chats that start on their own, on a schedule.

## The how-tos

| You want to | Page |
|---|---|
| Install a release, run the server as a service, reach it from another machine, upgrade and back up | [Running Moebius](running-moebius.md) |
| Change what a column does for one repository, and try the change | [Column scripts](column-scripts.md) |
| Know how a card moves from Backlog to Done and what a person can do on the way | [Workflows](workflows.md) |
| Forbid something to every run, or to the runs of one repository, and see a refusal | [Rules](rules.md) |
| Set up the Coder, the Reviewer or another agent, chat with one, or have the Moebius agent build something | [Agents](agents.md) |
| Have an agent look for ways to improve a number you can measure, and keep what helps | [Autoresearch](autoresearch.md) |
| Turn an idea into issues by being questioned about it | [Grilling](grilling.md) |
| Have an agent do recurring work on a schedule, such as triaging new issues every morning | [Automations](automations.md) |
| Answer your own decision points with Rust code | [Plugins](plugins.md) |
| Add behaviour inside the Pi process of a run | [Extensions](extensions.md) |
| Operate Moebius from a terminal or a script, or let the Moebius agent do it | [Command line](command-line.md) |
| Drive Moebius from a script with `curl` | [Desktop API](desktop-api.md) |

## Where to start

1. Run a server and a card through the default workflow: the quick start in [`README.md`](../../README.md).
2. Read [Workflows](workflows.md) to see what happened and where you can step in.
3. Change one thing with a [column script](column-scripts.md) or a [rule](rules.md).
