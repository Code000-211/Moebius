# Architecture

This is how Moebius is put together as built today: the processes, how a card travels from an issue to a merged change,
what the store holds, which code runs on which thread, and what each module is for. Read [`GLOSSARY.md`](../GLOSSARY.md)
for the words and [`spec.md`](spec.md) for what the first version includes. The exact contracts are in
[`reference/`](reference/).

## Processes

```
                  desktop app / curl
                         │  HTTP + WebSocket, bearer token      (api.rs, listen = 127.0.0.1:7171)
                         ▼
 ┌──────────────────────────────────────────────── moebius serve ───────────────────────────────┐
 │  board engine ── runs Luau column scripts per card, every tick                  (board.rs)   │
 │  core: App ── store (SQLite + blobs), bus, repos (git), forges, decisions   (app.rs, ...)   │
 │  poll ── GitHub issues and pull requests, every poll_seconds                    (poll.rs)    │
 │  router + check ── model endpoints and tool-call check, one listener (127.0.0.1:7172)        │
 └──────────────────────────────────────┬──────────────────────────────▲──────────────────────┘
                                        │ spawns                       │ model requests, /moebius/check
                                        ▼                              │ (the run's token)
                              pi-bolt --mode rpc  ── bridge extension ─┘
                              (one process per run, in the run's worktree)
```

- The server (`moebius serve`) is one process. It holds the core, the board engine, the poll, the desktop API and the
  router. `crates/moebius/src/main.rs` binds the two listeners and runs `Server::serve`.
- A Pi run is a `pi-bolt --mode rpc` process started per run in a git worktree. Moebius writes one JSON line (the
  prompt) to its standard input and reads JSON lines from its standard output. The bridge extension inside it asks the
  server about every tool call.
- The router is part of the server. Each Pi process has it as its only model provider and holds a per-run token. The
  router forwards requests to the upstreams in the configuration and counts the tokens.
- The desktop app (`crates/moebius-desktop`) is not built yet. Any HTTP client can stand in for it.

## The life of a card

1. **An issue arrives.** Without a forge: `POST /v1/repositories/{id}/issues` (`api::create_issue`) calls
   `App::create_issue`, which stores a card in `Backlog` (`Store::add_card`) and publishes `card_added`. On GitHub: the
   loop in `Server::serve` calls `poll::poll_github` every `github.poll_seconds`. That fetches the clone
   (`App::refresh`), lists issues with `GitHub::issues` (conditional on the ETag), and `App::sync_issues` adds or updates
   the cards and their "blocked by #n" dependencies (`Store::set_dependencies`).
2. **The board ticks.** The loop in `Server::serve` calls `Server::tick` on a blocking thread every `board.tick_ms`.
   For each card it calls `Engine::settle`, which calls `Engine::tick` until a script stops moving the card.
   `Engine::tick` finds the script for the card's column: the repository's `.moebius/<script>` read with
   `git show` on its default branch, else the built-in one from `workflow/` (`Engine::script`). `Engine::run_script`
   runs it in a fresh sandboxed Luau state with `card_table` and the `moebius` table, and applies the effects.
3. **Backlog to Ready.** `backlog.luau` moves a trusted card on (`App::is_trusted`). Others get an `approve_issue` inbox
   item (`moebius.ask`) until a person calls `POST /v1/cards/{id}/approve` (`App::approve_issue`), which records a
   judgement by `person` and moves the card to `Ready`.
4. **Ready to In progress.** `ready.luau` waits for blockers and for a free run slot, then moves the card.
5. **A run starts.** `code.luau` calls `moebius.start_run("Coder", "code")`, which calls `PiBolt::start` (the `Runs`
   trait). It stores the run (`Store::add_run`), makes a worktree on branch `moebius/run-<id>` from the default branch
   (`Repos::add_worktree`), issues the run's token (`Router::issue_token`, carrying the agent's budget), writes the run's
   agent directory and the bridge to `<data_dir>/runs/<id>/`, spawns Pi-Bolt (`PiBolt::launch`), sets the run `running`,
   publishes `run_started` and sends the prompt (`prompt_for`).
6. **Pi works.** A thread per run (`Reader::read`) reads Pi's records: each is stored (`Store::append_event`) and
   published as `run_event`. Pi asks the model through the router (`Router::forward`), which counts the tokens and may
   refuse with 429.
7. **Every tool call is checked.** The bridge posts to `/moebius/check` (`check::check`). On a blocking thread,
   `check::ask` builds the `tool_call` question (`tool_call_question`, with the applicable rules from
   `rules::applicable`), and `Decisions::decide` asks the `guard` plugin through the `Host` on a thread of its own with
   a time limit. `Guard::call` tests the built-in patterns, then the prose rules with the teacher model (`Teacher::judge`,
   which calls the router with a token for this run). The answer is stored as one judgement, as a `tool_check` run
   event, and returned. A refusal goes back to the model as the tool's result and the run goes on. The result of each
   tool is cleaned of secrets through `/moebius/result` (`check::result`).
8. **The run ends.** At `agent_settled`, `Reader::end` closes Pi's standard input, revokes the token, commits what the
   agent left (`commit`: `git add --all` and a commit by Moebius), sets the status (`succeeded`, `failed` when nothing
   changed or anything went wrong, `stopped` when aborted), removes the worktree (the branch stays) and publishes
   `run_ended`.
9. **The change is offered.** On a later tick `code.luau` sees `card.run.status == "succeeded"` and calls
   `moebius.offer_change`, which calls `App::offer_change`. The forge offers the branch (`NoForge`: nothing to do;
   `GitHubForge::offer`: push, then open a pull request). The change is stored (`Store::add_change`), a `ready_to_merge`
   inbox item added, `change_offered` published and the card moved to `Ready to merge`. A run that failed or was stopped
   gets another `Coder` run until `board.round_limit`, then the card is stuck in the inbox.
10. **GitHub reports.** `poll::poll_pull_requests` reads each open change's pull request: checks and review are recorded
    (`App::record_change_status`, `change_updated`), a merged pull request calls `App::change_merged`, a closed one
    calls `App::change_closed`.
11. **A person merges.** `POST /v1/changes/{id}/merge` calls `App::merge_change`: it records a judgement by `person`,
    the forge merges (`Repos::merge`, a `--no-ff` merge, or `GitHub::merge_pull_request`), and `App::change_merged`
    marks the change merged, resolves the inbox, publishes `change_merged` and moves the card to `Done`.
12. **Done.** `done.luau` calls `moebius.record_outcome("merged")`, so every judgement made for the card gets an outcome.

At start, `Server::new` ends the runs a previous server left going (`end_runs_of_a_previous_server`): they are marked
`failed` and their worktrees removed, so the board starts them again.

## The store

SQLite in `<data_dir>/moebius.sqlite` (WAL mode) with large content in `<data_dir>/blobs`. The schema is migrated at
start; its version is `PRAGMA user_version`.

| Record | Holds |
|---|---|
| `repositories` | Name, clone path, forge (`none` or `github`), default branch, GitHub name and trusted logins. |
| `cards` | One per issue: number (unique per repository), title, body, author, column. |
| `dependencies` | For a card, the issue numbers it is blocked by. |
| `changes` | A branch offered for merge: card, state (`open`, `merged`, `closed`), pull request number, checks, review. |
| `runs` | One per harness run: card, agent, harness, purpose, status, worktree, start and end. |
| `events` | Everything a run emitted, numbered per run, as JSON. |
| `judgements` | One per decision: kind, context (blob ids), options, choice, reason, decider, run, card, outcome. |
| `inbox_items` | What needs a person: kind, card, title, data, resolved or not. |
| `usage` | Tokens per model request: run, model, input, output, cached. |
| blobs | Content-addressed by SHA-256, deduplicated, with secrets removed before writing (`blobs::redact`). |

## Threads and async boundaries

The server runs on a multi-threaded tokio runtime.

- HTTP handlers are async and never block. Anything that touches the store or git runs in `tokio::task::spawn_blocking`:
  the desktop API (`api::blocking`), the check endpoint, the board tick and the poll's store and git calls
  (`poll::blocking`).
- Forge methods block. `GitHubForge` waits for the async GitHub client with `Handle::block_on`, so it must be called from a
  blocking thread, never from an async task.
- Column scripts run on the blocking thread of the tick. A Luau state is made and dropped per script run, and is never
  shared between threads.
- Every plugin call runs on a thread of its own, started by the `Host` for that call, with a time limit. The teacher uses a
  blocking HTTP client there.
- Each Pi run has one reader thread (`pibolt.rs`), and `PiBolt::stop` starts a short thread that kills Pi when it does not
  settle in time.
- The store is one SQLite connection behind a mutex. Calls are short and take it one at a time.
- The event bus is a tokio broadcast channel of 1024 events; a slow listener loses the oldest and is told with `lagged`.

## Modules

| Module | What it is for |
|---|---|
| `crates/moebius/src/main.rs` | The `moebius` binary: `init` writes the example configuration, `serve` starts the server. |
| `crates/moebius-plugin-api/src/lib.rs` | The `Plugin` trait, the call and decision types, the `Host` that calls plugins with a time limit and fails closed. |
| `crates/moebius-desktop/src/main.rs` | A placeholder for the desktop app. |
| `api.rs` | The desktop API: routes, token check, the event WebSocket. |
| `app.rs` | The core put together; the operations on cards, changes and issues, each published on the bus. |
| `blobs.rs` | The content-addressed blob store and secret redaction. |
| `board.rs` | The board engine: the Luau sandbox, the `moebius` table and the default workflow. |
| `bus.rs` | The event bus behind the event stream. |
| `check.rs` | The tool-call check endpoints `/moebius/check` and `/moebius/result`. |
| `config.rs` | `moebius.toml`: parsing, defaults, validation. |
| `decide.rs` | Decision points: routing a question to a decider, failing closed, recording the judgement. |
| `forge.rs` | The `Forge` trait with the forges `none` and `github`. |
| `github.rs` | The GitHub App client: JWT, installation tokens, issues with ETags, pull requests, checks, reviews, merge. |
| `guard.rs` | The `guard` decider: built-in pattern rules and the teacher model. |
| `pibolt.rs` | The Pi-Bolt harness: starting, reading and stopping runs. |
| `poll.rs` | The poll of GitHub: issues, pull requests, checks, reviews. |
| `repos.rs` | Clones, worktrees, pushes and merges through the `git` command line. |
| `router.rs` | The router: model endpoints, run tokens, usage, budgets. |
| `rules.rs` | The `rules.md` format and where rules come from. |
| `server.rs` | Wires everything from the configuration and runs the loops. |
| `store.rs` | The SQLite records and their migrations. |
| `workflow/*.luau` | The six built-in column scripts. |
| `bridge/moebius-bridge.ts` | The Pi extension loaded into every run. |
