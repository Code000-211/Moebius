# The documentation of Moebius

Everything written about Moebius, by who reads it. Each page has one job; when two pages would say the same thing, one
links to the other.

## To use Moebius

- [`README.md`](../README.md): what Moebius is, and a quick start.
- [Running Moebius](guide/running-moebius.md): installing, configuring and running the server and the app.
- [`GLOSSARY.md`](../GLOSSARY.md): the words Moebius uses, such as card, run, change and decision point.
- [The release notes](releases/): what each release holds, such as [0.1.0](releases/v0.1.0.md), [0.1.1](releases/v0.1.1.md), [0.1.2](releases/v0.1.2.md), [0.1.3](releases/v0.1.3.md), [0.1.4](releases/v0.1.4.md), [0.1.5](releases/v0.1.5.md), [0.1.6](releases/v0.1.6.md), [0.1.7](releases/v0.1.7.md), [0.1.8](releases/v0.1.8.md), [0.1.9](releases/v0.1.9.md), [0.2.0](releases/v0.2.0.md), [0.2.1](releases/v0.2.1.md), [0.3.0](releases/v0.3.0.md), [0.4.0](releases/v0.4.0.md), [0.5.0](releases/v0.5.0.md), [0.6.0](releases/v0.6.0.md), [0.7.0](releases/v0.7.0.md) and [0.8.0](releases/v0.8.0.md).

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
  [training export](reference/training-export.md), [decision model](reference/decision-model.md) and [packages](reference/packages.md). Tests check that
  every route, script function, `card` field, configuration key and event kind is on its page.
- [`examples/`](../examples/README.md): small examples to copy, run in CI.

## To work on Moebius itself

- [`CONTRIBUTING.md`](../CONTRIBUTING.md): how to work in the repository, where new code goes, the checks, and what a
  commit holds.
- [`CLAUDE.md`](../CLAUDE.md), also as [`AGENTS.md`](../AGENTS.md): the same rules, written for coding agents.
- [`architecture.md`](architecture.md): how a card moves through the code, the store, the threads, and the map of
  every module. A test checks that the map lists every module.
- The README of each crate: [`moebius-core`](../crates/moebius-core/README.md) (the server),
  [`moebius`](../crates/moebius/README.md) (the binary and command line) and [`moebius-plugin-api`](../crates/moebius-plugin-api/README.md).
- [`design-web.md`](design-web.md): the standard the web app is built to — voice, type, colour, components, interaction.
- [`web/README.md`](../web/README.md): the web app, its layout, scripts and licenses.

## Why it is the way it is

- [`spec.md`](spec.md): what the first version does and how it is built.
- [`adr/`](adr/): the decisions behind its shape, one record each. Where the spec and a record disagree, the record
  wins. [0013](adr/0013-the-web-app-is-the-front-end.md) says which front end is the one to build on and [0017](adr/0017-the-desktop-app-is-removed.md) that the desktop app is gone, and [0014](adr/0014-a-launcher-plugin-wraps-how-a-harness-starts.md) why a sandbox is a plugin of the launcher kind, [0015](adr/0015-packages-come-from-registries.md) why plugins and other packages come from registries, and [0016](adr/0016-the-web-app-has-its-own-design-standard.md) why the web app has a design standard of its own.
- [`plan/`](plan/): the issues the first version was built from, milestone by milestone
  ([M0 and M1](plan/issues-m0-m1.md), [M2 to M9](plan/issues-m2-m9.md)). Later work is in the repository's GitHub
  issues.
