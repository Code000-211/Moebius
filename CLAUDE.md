# Working on Moebius

Moebius is an orchestration layer that drives coding-agent harnesses (Pi-Bolt first) from a repository's issues to
changes ready to merge. Read `GLOSSARY.md` for the words, `docs/spec.md` for what is built, `docs/adr/` for why, and
`docs/issues-m0-m1.md` / `docs/issues-m2-m9.md` for the work, issue by issue.

## Layout

- `crates/moebius-core`: everything the server does. `app.rs` ties the store, bus, repositories, forges and decisions
  together; `server.rs` wires the API, router, harness and board engine; one module per part (`store`, `blobs`, `repos`,
  `forge`, `github` (the API client), `poll` (GitHub without webhooks), `decide`, `board`, `router`, `pibolt`, `api`,
  `config`). Column scripts are in `workflow/*.luau`.
- `crates/moebius-plugin-api`: the plugin trait and the host that calls plugins with a time limit, failing closed.
- `crates/moebius`: the `moebius` binary (`init`, `serve`).
- `crates/moebius-desktop`: the iced desktop app (built to `docs/design.md`).
- `docs/guide/`: how to build on Moebius, one how-to per page (column scripts, workflows, rules, agents, plugins,
  extensions, the desktop API). Every `luau` block in it is a complete column script that `tests/guide.rs` runs; read it
  before you build anything for a user, and keep it in step with the code.
- `README.md` says what Moebius is and how to run it; `docs/architecture.md` says how a card moves and which module does
  what; `docs/reference/` holds one exact reference per contract; `examples/` holds the examples that run in CI (the
  decider plugin is `crates/moebius-plugin-api/examples/allow_reads.rs`).
- `AGENTS.md` is a copy of this file for agents that read that name. Edit `CLAUDE.md`, then copy it below the first line of
  `AGENTS.md`; a test fails if they differ.

## Checks

Run all three before every commit; CI runs the same:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The end-to-end test (`crates/moebius-core/tests/first_card.rs`) runs only when `MOEBIUS_PI_BOLT` names a `pi-bolt`
executable; CI installs one.
CI also runs the plugin example: `cargo run -p moebius-plugin-api --example allow_reads`.

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
exactly in `docs/reference/` (column scripts, `rules.md`, configuration, plugin API, bridge protocol, router, desktop API) and
shown in `examples/`. A change to a contract updates its reference and examples in the same commit; tests
(`crates/moebius-core/tests/reference.rs`) check that every route, script function, `card` field, configuration key and
event kind is in the reference. Each module starts with a doc comment saying what it is for and how it fits in.

## Commits

One commit per issue, imperative subject, a body that says what and why, then `Closes #<n>` and the session's
attribution trailers. Never put a model name in a commit, code or comment.
