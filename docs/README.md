# The documentation of Moebius

Everything written about Moebius, by who reads it. Each page has one job; when two pages would say the same thing, one
links to the other.

## To use Moebius

- [`README.md`](../README.md): what Moebius is, and a quick start.
- [Running Moebius](guide/running-moebius.md): installing, configuring and running the server and the app.
- [`GLOSSARY.md`](../GLOSSARY.md): the words Moebius uses, such as card, run, change and decision point.
- [The release notes](releases/): what each release holds, such as [0.1.0](releases/v0.1.0.md), [0.1.1](releases/v0.1.1.md) and [0.1.2](releases/v0.1.2.md).

## To build on Moebius

These pages are for people and agents who change how Moebius behaves without changing its code. The Moebius agent
reads them too: they are its `moebius` skill.

- [The guide](guide/README.md): one task per page, such as column scripts, workflows, rules, agents, extensions,
  plugins, the command line and the desktop API. Every example in it is run by a test.
- [The reference](reference/): the exact contracts, one page each:
  [column scripts](reference/column-scripts.md), [rules](reference/rules.md),
  [configuration](reference/configuration.md), [command line](reference/cli.md),
  [desktop API](reference/desktop-api.md), [router](reference/router.md), [plugin API](reference/plugin-api.md),
  [bridge protocol](reference/bridge-protocol.md), [harness configuration](reference/harness-configuration.md),
  [training export](reference/training-export.md) and [decision model](reference/decision-model.md). Tests check that
  every route, script function, `card` field, configuration key and event kind is on its page.
- [`examples/`](../examples/README.md): small examples to copy, run in CI.

## To work on Moebius itself

- [`CONTRIBUTING.md`](../CONTRIBUTING.md): how to work in the repository, where new code goes, the checks, and what a
  commit holds.
- [`CLAUDE.md`](../CLAUDE.md), also as [`AGENTS.md`](../AGENTS.md): the same rules, written for coding agents.
- [`architecture.md`](architecture.md): how a card moves through the code, the store, the threads, and the map of
  every module. A test checks that the map lists every module.
- The README of each crate: [`moebius-core`](../crates/moebius-core/README.md) (the server),
  [`moebius-desktop`](../crates/moebius-desktop/README.md) (the app), [`moebius`](../crates/moebius/README.md) (the
  binary and command line) and [`moebius-plugin-api`](../crates/moebius-plugin-api/README.md).
- [`design.md`](design.md) and the mockups in [`design/`](design/): what the desktop app looks like.

## Why it is the way it is

- [`spec.md`](spec.md): what the first version does and how it is built.
- [`adr/`](adr/): the decisions behind its shape, one record each. Where the spec and a record disagree, the record
  wins.
- [`plan/`](plan/): the issues the first version was built from, milestone by milestone
  ([M0 and M1](plan/issues-m0-m1.md), [M2 to M9](plan/issues-m2-m9.md)). Later work is in the repository's GitHub
  issues.
