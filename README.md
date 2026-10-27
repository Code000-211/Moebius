# Moebius

Moebius is an orchestration layer that drives coding-agent harnesses (Pi-Bolt first) from a repository's issues to changes
that are ready to merge. It is one headless server: it holds a board per repository, runs a column script for every
card, and checks every tool call an agent makes against rules.

## Status

Milestones M0 to M3 are done (see [`docs/spec.md`](docs/spec.md) section 10). Today Moebius:

- starts a server with a SQLite store, a desktop API and a model router;
- takes issues from a plain git repository (through the API) or from GitHub (by polling), and puts them on a board;
- runs a Pi-Bolt agent per card in its own worktree, and offers the result as a change (a branch, or a GitHub pull request);
- merges the change when a person asks through the API;
- checks every tool call against built-in rules and rules written in prose, and refuses the ones that break a rule;
- records every decision as a judgement and every model request's tokens as usage.

Not built yet: the review and CI loop (M4), chat and the Moebius agent (M5), the desktop app (`moebius-desktop` is a
placeholder), self-improvement and the decision model.

## Quick start

You need Rust 1.88 or newer, git, and [Pi-Bolt](https://github.com/opensec-git/Pi-Bolt) installed as `pi-bolt`. You also need
an OpenAI-compatible model server, such as a local one on `http://127.0.0.1:8080`.

```sh
cargo build --release
./target/release/moebius init      # writes moebius.toml, a commented file with every default
```

Add an upstream, the Pi-Bolt program and the Coder agent to `moebius.toml` (see
[`docs/reference/configuration.md`](docs/reference/configuration.md)). The file can be this small:

```toml
[[router.upstreams]]
protocol = "openai"
base_url = "http://127.0.0.1:8080"

[pi_bolt]
program = "/usr/local/bin/pi-bolt"

[agents.Coder]
model = "my-local-model"
```

Start the server. It creates `.moebius/` next to the file, with the token the API needs in `.moebius/token`:

```sh
./target/release/moebius serve
```

In another shell, add a repository, create an issue and read the board:

```sh
TOKEN=$(cat .moebius/token)
API=http://127.0.0.1:7171
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "demo", "url": "/path/to/a/git/repository"}' $API/v1/repositories
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"title": "Add a greeting", "body": "Add hello.txt that says hello."}' $API/v1/repositories/1/issues
curl -s -H "Authorization: Bearer $TOKEN" $API/v1/snapshot
```

The card moves from Backlog through Ready and In progress. When the Coder run has succeeded, the snapshot shows a change in
`changes` and the card in `Ready to merge`. Merge it with the change's `id`:

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"reason": "looks good"}' $API/v1/changes/1/merge
```

The answer is the merge commit, and the card is in `Done`. For a repository on GitHub, see the `[github]` table and the
`forge` field of `POST /v1/repositories`.

## Where everything is

| Path | What |
|---|---|
| `crates/moebius-core/` | Everything the server does. `src/` has one module per part; `workflow/` the built-in column scripts; `bridge/` the Pi extension that checks tool calls. |
| `crates/moebius-plugin-api/` | The plugin trait and the host that calls plugins with a time limit, failing closed. |
| `crates/moebius/` | The `moebius` binary (`init`, `serve`). |
| `crates/moebius-desktop/` | The desktop app, not built yet. |
| `examples/` | A column script, rules, a decider plugin and a Pi extension that run in CI. |
| `docs/` | Documentation, below. |

## Documentation

- [`GLOSSARY.md`](GLOSSARY.md): the words Moebius uses.
- [`docs/spec.md`](docs/spec.md): what the first version does. [`docs/adr/`](docs/adr/): why it is shaped this way.
- [`docs/architecture.md`](docs/architecture.md): how a card moves through the system and which module does what.
- Reference, exact enough to build on without reading the code:
  [column scripts](docs/reference/column-scripts.md), [rules](docs/reference/rules.md),
  [configuration](docs/reference/configuration.md), [plugin API](docs/reference/plugin-api.md),
  [bridge protocol](docs/reference/bridge-protocol.md), [router](docs/reference/router.md),
  [desktop API](docs/reference/desktop-api.md).
- [`examples/README.md`](examples/README.md): the examples.
- [`docs/issues-m0-m1.md`](docs/issues-m0-m1.md), [`docs/issues-m2-m9.md`](docs/issues-m2-m9.md): the work, issue by issue.
- [`docs/design.md`](docs/design.md): the look of the desktop app.
- [`CLAUDE.md`](CLAUDE.md) (mirrored in [`AGENTS.md`](AGENTS.md)): how to work in the repository, for people and agents.

## Checks

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p moebius-plugin-api --example allow_reads
```

The end-to-end tests run only when `MOEBIUS_PI_BOLT` names a `pi-bolt` executable.
