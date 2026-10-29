# Working on Moebius

Moebius is an orchestration layer that drives coding-agent harnesses (Pi-Bolt first) from a repository's issues to
changes ready to merge. Read `GLOSSARY.md` for the words, `docs/spec.md` for what is built, `docs/adr/` for why, and
`docs/plan/` for the work the first version was built from, issue by issue. `docs/README.md` is the map of all the
documentation; `CONTRIBUTING.md` says the same as this file for people.

## Layout

- `crates/moebius-core`: everything the server does. At the top of `src/`: `app/` ties the store, bus, repositories,
  forges and decisions together; `config`, `store`, `blobs` (and `blobs::redact`), `bus`, `router` (the model
  provider) and `platform` (what differs between Linux and Windows). The rest is grouped by part, one folder each:
  `server/` (the wiring and nightly schedule, `api`,
  `settings` for editing `moebius.toml` while it runs, `setup` for the Models, Plugins and Get started pages), `repos/`
  (git, `forge`, `github`, `gitlab`, `forgejo`, `poll`, `files`), `board/` (the engine and `scripts`), `decide/` (decision points,
  `check` for the bridge's tool-call check, `guard`, `rules`, `localize` for the files a code run is hinted, `model` for shadowing and takeovers), `harness/`
  (`pibolt`, `claude` for Claude Code, `pick` choosing per agent, `terminal`, `repo` for the harness configuration
  repository, `mcp`, `gateway` for the MCP gateway, `remote` for runners over SSH, `skill`, `grilling`, `lens` for the lint and type feedback after an agent's edits) and `improve/` (`retro`, `trial`, `own` for the
  Retro's proposals to Moebius's own configuration, `replay`, `research`, `export`, `lessons` for what the Retro learns from earlier cards). `docs/architecture.md` lists every
  module. Column scripts are in `workflow/*.luau`.
- `crates/moebius-plugin-api`: the plugin trait and the host that calls plugins with a time limit, failing closed.
- `crates/moebius`: the `moebius` binary (`init`, `serve`, `check-script`, `export`, and in `cli/` the commands that
  operate a running server through the desktop API: `api` for any route, a command for what is done often; the Moebius
  agent uses them as `"$MOEBIUS"`).
- `crates/moebius-core/skill/` and `src/harness/skill.rs`: the built-in skills. The `moebius` skill is `SKILL.md` plus
  every page of `docs/guide/` and `docs/reference/` (add a new page to the list in `harness/skill.rs`; a test fails if
  you forget); the Moebius agent's task skills are `skill/moebius-<task>/SKILL.md`, and every `"$MOEBIUS"` command and
  route they name is checked by a test.
- `crates/moebius-desktop`: the iced desktop app (built to `docs/design.md`). It talks to the server only through the
  desktop API and does not depend on `moebius-core` (a dev-dependency starts a real server in its tests). At the top:
  `app/` the state, messages and view, `client/` the API client and event stream, `ssh/` connecting over SSH (config,
  askpass, remote setup, tunnel), `settings.rs` the saved address and token, `theme.rs` every colour and size, `sidebar.rs` and `search.rs`. `screens/` has one module per page,
  `widgets/` the pieces screens share. `tests/snapshots/` holds the pictures its render tests compare against (delete
  one to store a new picture).
- `docs/guide/`: how to build on Moebius, one how-to per page (column scripts, workflows, rules, agents, plugins,
  extensions, the desktop API). Every `luau` block in it is a complete column script that `tests/guide.rs` runs; read it
  before you build anything for a user, and keep it in step with the code.
- `README.md` says what Moebius is and how to run it; `docs/architecture.md` says how a card moves and which module does
  what; `docs/reference/` holds one exact reference per contract; `examples/` holds the examples that run in CI (the
  decider plugin is `crates/moebius-plugin-api/examples/allow_reads.rs`).
- Each crate has a `README.md` with its layout.
- `AGENTS.md` is a copy of this file for agents that read that name. Edit `CLAUDE.md`, then copy it below the first
  line of `AGENTS.md`; a test fails if they differ.

## Where new code goes

- Into the folder of its part, never as a new module at the top of a crate's `src/` unless it ties the crate together.
  A new route goes into the module of `server/api/` for its part of the API, its client call into `client/calls/` of
  the app; a new kind of record into a module of `store/`; a new page into `screens/`; a piece two screens share into
  `widgets/`.
- A module that grows past about 600 lines, or mixes concerns, becomes a folder: `git mv x.rs x/mod.rs`, then cut it
  by concern. `mod.rs` keeps what the parts share and re-exports their public items, so callers do not change; each
  part takes its tests with it.
- Every module starts with a `//!` doc comment that says what it is for and how it fits in; a folder's `mod.rs` says
  what the folder holds. Every module has a row in the module map of `docs/architecture.md`, with its path from the
  crate's `src/`; a test fails until it has one, and another fails for a link in the documentation that leads nowhere.

## Checks

Run all three before every commit; CI runs the same:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The end-to-end test (`crates/moebius-core/tests/first_card.rs`) runs only when `MOEBIUS_PI_BOLT` names a `pi-bolt`
executable; CI installs one.
CI also runs the plugin examples: `cargo run -p moebius-plugin-api --example allow_reads`, and `allow_reads_process`
for the same decider in its own process.

CI builds and tests on Windows too. Check it with `cargo clippy --workspace --all-targets --target
x86_64-pc-windows-gnu -- -D warnings` (it needs the target and a MinGW C compiler, no Windows). Code goes through
`platform` instead of `std::os::unix`; a test that needs a Unix program (`sh`, `bash`, a fake that is a Python script
started by its `#!` line) is `#[cfg(unix)]`, with a comment that says why.

## Style

- Match the code around you: doc comments that say what a thing is for in plain sentences, names that read as words,
  little else in comments.
- Build only what the issue needs. No traits with one implementation unless a second is planned in the spec, no options
  nobody sets, no unused public API. Prefer a plain function to a new type.
- Every behaviour an issue asks for has a test; a bug fix has a test that fails without the fix. Tests must not be
  flaky: no tight time limits, no fixed ports (bind to port 0), no sleeps where a condition can be awaited.
- Talk to outside services through a fake in tests (an axum server on port 0, a local bare git repository).
- Errors: `thiserror` enums per module; a run never waits for a person; anything that fails while deciding fails
  closed.
- Secrets never reach the store or a log unredacted (`blobs::redact`).

## Documentation

Moebius is built to be extended by people and agents who have not read its code, so every contract is documented
exactly in `docs/reference/` (column scripts, `rules.md`, configuration, command line, plugin API, bridge protocol,
router, desktop API, harness configuration, training export, decision model) and shown in `examples/`. A change to a
contract updates its reference and examples in the same commit; tests (`crates/moebius-core/tests/reference.rs`) check
that every route, script function, `card` field, configuration key and event kind is in the reference. Each module
starts with a doc comment saying what it is for and how it fits in.

## Commits

One commit per issue, imperative subject, a body that says what and why, then `Closes #<n>` and the session's
attribution trailers. Never put a model name in a commit, code or comment.
