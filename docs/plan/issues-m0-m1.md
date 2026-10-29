# First issues: M0 and M1

Drafts for the first two milestones of [`docs/spec.md`](../spec.md), cut so that each is one card a harness can finish in one
change. They are created on GitHub only after a person confirms them.

## M0 · Skeleton

### 1. Cargo workspace and the `moebius` binary
Set up the Rust workspace: `moebius-core` (library), `moebius` (server binary), `moebius-plugin-api` (the plugin API crate)
and `moebius-desktop` (empty for now). CI runs `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test`.
**Done when** `cargo run -p moebius -- --version` prints the version and CI is green.

### 2. Server configuration
Read `moebius.toml` (data directory, listen address for the app, token file, global run limit, Retro and trial-window
times) with defaults for every key; `moebius init` writes a commented example.
**Done when** the server starts from defaults alone and from a full example, and a test covers both.
**Depends on** 1.

### 3. The store
SQLite schema and migrations for repositories, cards, changes, runs, events, judgements, inbox items and usage; a
content-addressed blob store on disk (SHA-256, deduplicated). Secrets are removed before a blob is written (a first set of
patterns: provider keys, GitHub tokens, private keys).
**Done when** records and blobs round-trip in tests and a blob written twice is stored once.
**Depends on** 1.

### 4. The plugin API, compiled-in form
The `Plugin` trait and the plugin kinds of the first version (forge, harness, runner, decider, router, view), with every
call made through the host with a time limit and a fail-closed result. The message types are serialisable, so that the same
API can later run over JSON Lines.
**Done when** a test plugin that hangs or panics is cut off by the host and the call counts as refused.
**Depends on** 1.

### 5. `decide` and judgements
The core's `decide(kind, context, options)` call: asks the configured decider for the kind, records a judgement with its
context blobs and decider, and returns the choice. A person's decision is recorded through the same path.
**Done when** every call leaves exactly one judgement, and a failing decider produces a refusal and a judgement saying so.
**Depends on** 3, 4.

### 6. Desktop API stub
WebSocket and HTTP on the configured private address, token authentication, a health call, a snapshot of boards and cards,
and an event stream.
**Done when** a test client authenticates, reads a snapshot and receives an event published by the core; a wrong token is
refused.
**Depends on** 2, 3.

## M1 · One card, end to end

### 7. Repositories and worktrees
Clone a repository into the data directory, fetch, create a worktree and branch per run, remove it when the run ends, and
merge a change locally for repositories without a forge.
**Done when** tests create and remove worktrees in a temporary repository and a local merge lands on the default branch.
**Depends on** 3.

### 8. Git forge (none)
Issues and changes of a repository without a forge live in the store; creating an issue puts a card in the backlog;
merging a change closes its card.
**Done when** an issue created through the desktop API appears as a card and a merged change moves it to done.
**Depends on** 5, 6, 7.

### 9. Board engine with Luau
Columns from a workflow definition, each with a Luau script run in a sandbox (mlua with Luau); the script API offers
`moebius.decide`, `moebius.start_run`, `moebius.move`, `moebius.wait` and limits; scripts are loaded from the server's
default workflow and from a repository's `.moebius/` on its default branch.
**Done when** the default workflow's Backlog, Ready, In progress, Ready to merge and Done scripts move a card in a test with
a fake harness, and a script cannot reach the file system or network.
**Depends on** 5, 7.

### 10. Router, pass-through
OpenAI- and Anthropic-compatible endpoints on localhost; per-run tokens; one upstream with an API key or a local server;
usage recorded per call; a card's token budget enforced.
**Done when** a recorded request goes upstream, its usage is stored against the run, and a request over budget is refused.
**Depends on** 3, 4.

### 11. Pi-Bolt harness plugin
Start `pi-bolt --mode rpc` in the run's worktree with the router as provider, the agent's model, thinking level,
instructions and skills; send the prompt; read events (split on LF only); store them as run events; end on `agent_settled`;
stop with `abort`.
**Done when** a run against a fake Pi speaking the RPC protocol is stored event by event, including text with U+2028 in it,
and can be stopped.
**Depends on** 7, 10.

### 12. First card through the workflow
Wire 8, 9, 10 and 11: an issue in a plain git repository is coded by the Coder agent, committed on its branch, offered as
a change and merged by a person through the desktop API.
**Done when** an end-to-end test with a real `pi-bolt` and a local model (or a recorded upstream) turns an issue into a
merged change.
**Depends on** 8, 9, 10, 11.
