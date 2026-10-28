# Moebius

Moebius is an orchestration layer that drives coding-agent harnesses (Pi-Bolt first) from a repository's issues to changes
that are ready to merge. It is one headless server: it holds a board per repository, runs a column script for every
card, and checks every tool call an agent makes against rules.

## Status

Every milestone of the first version, M0 to M9, is built (see [`docs/spec.md`](docs/spec.md) section 10). Moebius:

- starts a server with a SQLite store, a desktop API and a model router that counts tokens and keeps budgets;
- takes issues from a plain git repository (through the API) or from GitHub (by polling), and puts them on a board;
- runs a Pi-Bolt agent per card in its own worktree, reviews its work, waits for CI, and offers the result as a change
  (a branch, or a GitHub pull request) to merge;
- checks every tool call against built-in rules and rules written in prose, and refuses the ones that break a rule;
- chats with agents: the bundled Moebius agent builds scripts, rules and agents from the guide, and the Grilling partner
  turns an idea into issues a person confirms;
- reads its agents, skills, MCP servers and rules from a harness configuration repository, where an edit in the app is
  a commit;
- improves itself at night: the Retro proposes changes to the harness configuration from the day's errors, trials try
  them on the cards they came from, refused calls are replayed in a throwaway copy, and autoresearch measures levers
  against a number a person wants to improve;
- records every decision as a judgement, exports them as a training set, and lets a decision model shadow a kind of
  decision and take it over once a person merges the proposal.

The desktop app has every screen of [`docs/design.md`](docs/design.md): projects (a repository's board, its files and an
editor that commits a file on the default branch), live view with a shell, inbox, chat,
fleet (every working run at a glance), agents, usage, Retro, experiments, decision model, and the setup: models and settings, which edit `moebius.toml`, and the read-only plugins.

## Quick start

You need git, [Pi-Bolt](https://github.com/opensec-git/Pi-Bolt) installed as `pi-bolt`, and an OpenAI-compatible model
server, such as a local one on `http://127.0.0.1:8080`.

Install the server from a [release](https://github.com/Code000-211/Moebius/releases): download the archive for your
machine (releases are for Linux x86_64; elsewhere, build from source) and `SHA256SUMS`, check it and unpack it. The page
[Running Moebius](docs/guide/running-moebius.md) has the commands, and how to run the server as a service.

```sh
VERSION=0.1.0
TARGET=x86_64-unknown-linux-gnu
ARCHIVE=moebius-$VERSION-$TARGET.tar.gz
grep " $ARCHIVE\$" SHA256SUMS | sha256sum -c -
tar -xzf "$ARCHIVE"
mkdir -p ~/.local/bin && cp "moebius-$VERSION-$TARGET/moebius" ~/.local/bin/
```

To build from source instead, you need Rust 1.90 or newer (the `rust-version` in `Cargo.toml`, which CI checks):

```sh
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

Write the configuration in a directory of its own:

```sh
mkdir -p ~/.config/moebius && cd ~/.config/moebius
moebius init      # writes moebius.toml, a commented file with every default
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
moebius serve
```

Connect the desktop app: a [release](https://github.com/Code000-211/Moebius/releases) has it for Linux; from source,
`cargo run -p moebius-desktop`. Give it the server's address (`http://127.0.0.1:7171`) and the content of
`.moebius/token`.

Add a repository, create an issue and read the board, in the app or with the `moebius` commands, which run in the
directory of `moebius.toml` and find the address and token there:

```sh
moebius repo add demo /path/to/a/git/repository
moebius issue create 1 "Add a greeting" --body "Add hello.txt that says hello."
moebius board
```

The card moves from Backlog through Ready and In progress. When the Coder run has succeeded, it is in `Ready to merge`
with a change; merge it in the app (the inbox shows it) or with `moebius inbox` and `moebius inbox act <id> merge`. The
card is then in `Done`. For a repository on GitHub, see the `[github]` table and `--github` of `repo add`; the
[command line](docs/guide/command-line.md) lists every command, and `moebius api` reaches any route of the
[desktop API](docs/reference/desktop-api.md).

## The desktop app

On the first start the app asks for the server's address (`http://127.0.0.1:7171` by default) and its token (the content
of the server's `token` file) and saves them in `<config dir>/moebius/desktop.toml` (on Linux
`~/.config/moebius/desktop.toml`), readable by you only. The token is not shown again in full. If the server cannot be
reached or does not accept the token, the app asks again. The address is plain HTTP, so keep the server on a private
network. The window needs a GPU or a software Vulkan/OpenGL driver; the app finds it when it starts.

**Search and run.** The field at the top of the sidebar (Ctrl+K, Cmd+K on macOS, focuses it from anywhere) lists, as you
type, the cards of every board by title words or `#number`, the projects by name, the chats, the agents and the pages that contain every word
you typed, at most eight entries. Up and Down choose, Enter or a click opens, Escape closes. The last entry, "Ask the
Moebius agent", starts a chat with what you typed as its first message.

## Where everything is

| Path | What |
|---|---|
| `crates/moebius-core/` | Everything the server does. `src/` has one module per part; `workflow/` the built-in column scripts; `bridge/` the Pi extension that checks tool calls. |
| `crates/moebius-plugin-api/` | The plugin trait and the host that calls plugins with a time limit, failing closed. |
| `crates/moebius/` | The `moebius` binary (`init`, `serve`, `check-script`, `export`, and the commands that operate a running server). |
| `crates/moebius-desktop/` | The desktop app (iced), built to `docs/design.md`; it uses only the desktop API. |
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
  [desktop API](docs/reference/desktop-api.md), [harness configuration](docs/reference/harness-configuration.md),
  [command line](docs/reference/cli.md), [training export](docs/reference/training-export.md),
  [decision model](docs/reference/decision-model.md).
- [`docs/guide/`](docs/guide/README.md): the guide, how to build on Moebius step by step (column scripts, workflows,
  rules, agents, plugins, extensions, the desktop API, grilling, autoresearch), with examples that tests run.
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
