# Contributing to Moebius

Thank you for helping. Moebius is an early prototype, so feedback is as welcome as code: an
[issue](https://github.com/Code000-211/Moebius/issues) that says what confused you is a contribution. This page is for
those who change the code. [`CLAUDE.md`](CLAUDE.md) says the same things for coding agents.

## Before you start

Read [`GLOSSARY.md`](GLOSSARY.md) for the words and [`docs/architecture.md`](docs/architecture.md) for how a card moves
through the code. Each module starts with a doc comment that says what it is for. To change a contract, such as a route,
a script function or a configuration key, read its page in [`docs/reference/`](docs/reference/) first.

You need Linux, Rust 1.90 or newer, and git. The end-to-end tests also need [Pi-Bolt](https://github.com/opensec-git/Pi-Bolt)
installed as `pi-bolt`, named by `MOEBIUS_PI_BOLT`. Without it they are skipped.

## Where the code goes

The workspace has four crates:

| Crate | What it is |
|---|---|
| [`crates/moebius-core`](crates/moebius-core/README.md) | The server: everything Moebius does. |
| [`crates/moebius-desktop`](crates/moebius-desktop/README.md) | The desktop app. It talks to the server only through the desktop API. |
| [`crates/moebius`](crates/moebius/README.md) | The `moebius` binary and its command line. |
| [`crates/moebius-plugin-api`](crates/moebius-plugin-api/README.md) | The plugin trait and its host. |

Inside a crate's `src/`, the modules that tie it together are at the top, and the rest is grouped by part, one folder
each. In the server those folders are `server/`, `repos/`, `board/`, `decide/`, `harness/` and `improve/`. In the app
they are `screens/` and `widgets/`. The module map in [`docs/architecture.md`](docs/architecture.md#modules) lists
every module.

- **New code goes into the folder of its part.** A new screen goes into `screens/`. A piece that two screens share goes
  into `widgets/`. A new kind of record goes into a module of `store/`. A new route goes into the module of `server/api/`
  for its part of the API, and its client call into `client/calls/` of the app.
- **A module that grows past about 600 lines, or that mixes concerns, becomes a folder.** Its `mod.rs` holds what the
  parts share and re-exports their public items, so that callers do not change. Tests stay next to the code they test.
- **Every module starts with a `//!` doc comment.** It says in plain sentences what the module is for and how it fits
  in. A folder's `mod.rs` says what the folder holds.
- **Every module has a row in the module map.** The test `every_module_is_in_the_architecture_map` fails until it has
  one.

## Style

- Match the code around you. Doc comments say what a thing is for. Names read as words. Little else goes in comments.
- Build only what is needed. No traits with one implementation unless a second is planned, no options nobody sets, and
  no public API nobody uses. Prefer a plain function to a new type.
- Errors are a `thiserror` enum per module. A run never waits for a person. Anything that fails while deciding fails
  closed.
- Secrets never reach the store or a log unredacted (`blobs::redact`).

## Tests

- Every behaviour has a test. A bug fix has a test that fails without the fix.
- Tests must not be flaky: no tight time limits, no fixed ports (bind to port 0), and no sleeps where a condition can
  be awaited.
- Talk to outside services through a fake: an axum server on port 0, or a local bare git repository.
- The desktop app's render tests compare against the pictures in `crates/moebius-desktop/tests/snapshots/`. Delete a
  picture to store a new one, and look at it before you commit it.

## Documentation

Moebius is built to be extended by people and agents who have not read its code. A change to a contract updates its
page in [`docs/reference/`](docs/reference/) and its examples in the same commit. Tests check that every route, script
function, `card` field, configuration key and event kind is documented, that every module is in the module map, and
that every link in the documentation leads somewhere. [`docs/README.md`](docs/README.md) is the map of all the
documentation.

## Checks

Run what CI runs before every commit:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p moebius-plugin-api --example allow_reads
```

## Commits

One commit per issue:

- an imperative subject;
- a body that says what changed and why;
- `Closes #<n>`.

Never put a model name in a commit, code or a comment. Contributions are accepted under the
[Apache License, Version 2.0](LICENSE).
