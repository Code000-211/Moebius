# Architecture

This is how Moebius is put together as built today: the processes, how a card travels from an issue to a merged change,
what the store holds, which code runs on which thread, and what each module is for. Read [`GLOSSARY.md`](../GLOSSARY.md)
for the words and [`spec.md`](spec.md) for what the first version includes. The exact contracts are in
[`reference/`](reference/).

## Processes

```
                  desktop app / curl
                         │  HTTP + WebSocket, bearer token      (server/api/, listen = 127.0.0.1:7171)
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
- The desktop app (`crates/moebius-desktop`) is an iced app that talks to the server only through the desktop API
  (HTTP and the event WebSocket) and does not depend on `moebius-core`. Any HTTP client can stand in for it.

## The life of a card

1. **An issue arrives.** Without a forge: `POST /v1/repositories/{id}/issues` (`api::create_issue`) calls
   `App::create_issue`, which stores a card in `Backlog` (`Store::add_card`) and publishes `card_added`. On GitHub: the
   loop in `Server::serve` calls `poll::poll_github` every `github.poll_seconds`. That fetches the clone
   (`App::refresh`), lists issues with `GitHub::issues` (conditional on the ETag), and `App::sync_issues` adds or updates
   the cards and their "blocked by #n" dependencies (`Store::set_dependencies`). A repository without a forge has no
   poll; the loop in `Server::serve` calls `Server::refresh_plain_repositories` every 30 seconds, which refreshes it
   (`NoForge::refresh` fetches `origin` and fast-forwards the default branch when it can) and publishes a failure as
   `forge_error`.
2. **The board ticks.** The loop in `Server::serve` calls `Server::tick` on a blocking thread every `board.tick_ms`.
   For each card it calls `Engine::settle`, which calls `Engine::tick` until a script stops moving the card.
   `Engine::tick` finds the script for the card's column: the repository's `.moebius/<script>` read with
   `git show` on its default branch, else the built-in one from `workflow/` (`Engine::script`). `Engine::run_script`
   runs it in a fresh sandboxed Luau state with `card_table` and the `moebius` table, and applies the effects.
3. **Backlog to Ready.** `backlog.luau` moves a trusted card on (`App::is_trusted`), and one made from a confirmed grilling (`card.confirmed`). Others get an `approve_issue` inbox
   item (`moebius.ask`) until a person calls `POST /v1/cards/{id}/approve` (`App::approve_issue`), which records a
   judgement by `person` and moves the card to `Ready`.
4. **Ready to In progress.** `ready.luau` waits for blockers and for a free run slot, then moves the card.
5. **A run starts.** `code.luau` calls `moebius.start_run("Coder", "code", note)`, which calls `PiBolt::start` (the
   `Runs` trait). It stores the run (`Store::add_run`), makes a worktree on branch `moebius/run-<id>` from the branch of the
   card's latest successful `code` run (`Store::code_branch`), or from the default branch when it has none
   (`Repos::add_worktree`), issues the run's token (`Router::issue_token`, carrying the agent's budget), writes the run's
   agent directory and the bridge to `<data_dir>/runs/<id>/`, spawns Pi-Bolt (`PiBolt::launch`), sets the run `running`,
   publishes `run_started` and sends the prompt (`prompt_for`: the issue, the instructions for the run's purpose, and
   the note if there is one).
6. **Pi works.** A thread per run (`Reader::read`) reads Pi's records: each is stored and published as `run_event` with
   its `seq` (`App::record_run_event`, which `check::ask` and `App::steer_run` use too). Pi asks the model through the router (`Router::forward`), which counts the tokens and may
   refuse with 429.
7. **Every tool call is checked.** The bridge posts to `/moebius/check` (`check::check`). On a blocking thread,
   `check::ask` builds the `tool_call` question (`tool_call_question`, with the applicable rules from
   `rules::applicable`, or the `rules_error` when the server's rules file cannot be read), and `Decisions::decide` asks the `guard` plugin through the `Host` on a thread of its own with
   a time limit. `Guard::call` tests the built-in patterns, then the prose rules with the teacher model (`Teacher::judge`,
   which calls the router with a token for this run; once a takeover of `tool_call` is merged, `DecisionModel::ask` answers the prose rules instead). The answer is stored as one judgement, as a `tool_check` run
   event, and returned. A refusal goes back to the model as the tool's result and the run goes on. The result of each
   tool is cleaned of secrets through `/moebius/result` (`check::result`).
8. **The run ends.** At `agent_settled`, `Reader::end` closes Pi's standard input, revokes the token, commits what the
   agent left (`Reader::commit`: `git add --all` and a commit by Moebius; for `code` runs, and for chats, see 12b), sets the status (`succeeded`,
   `failed` when a `code` run changed nothing or anything went wrong, `stopped` when aborted), removes the worktree
   (the branch stays) and publishes `run_ended`. The text of the agent's final answer, reported in Pi's `agent_end`
   event, was stored as the run's summary when that event came (`Store::set_run_summary`).
9. **The change is published.** On a later tick `code.luau` sees the succeeded `code` run (and no `card.note`) and moves
   the card to `In review`. `review.luau` calls `moebius.publish_change`, which calls `App::publish_change`: the branch
   `moebius/card-<number>` is pointed at the latest successful `code` run's commit (`Repos::point_branch`; each round's
   branch descends from the last, so the branch only moves forward) and the forge publishes it (`Forge::publish`:
   `NoForge` does nothing; `GitHubForge` pushes and, the first time, opens the pull request). The change is stored
   (`Store::add_change`, later `Store::set_change_head`) with the published commit and time, and `change_updated` is
   published. A card has one change, one branch and one pull request for all its rounds.
10. **The change is checked.** On GitHub the poll records the check runs of the pull request's head commit
    (`poll::poll_pull_requests`, `App::record_change_status`), with the head they are for and the failing ones. Without
    a forge, `moebius.run_checks` calls `App::run_checks`: if `.moebius/checks` exists on the default branch, a thread
    runs it with `sh -c` in a throwaway worktree of the change's commit (`App::check_change`, `execute`) with the time
    limit `checks.timeout_seconds`, and records `success` or `failure` with the end of the output, redacted
    (`Store::set_local_checks`). `App::checks_of` gives scripts `card.change.checks`: checks for another commit than the
    published one do not count, and on GitHub the change is `pending` for a grace period after the publish and then
    `none`. `App::conflict_of` asks `git merge-tree` whether the change still merges into the default branch
    (`Repos::conflicts`).
11. **The change is sent back or reviewed.** `review.luau` waits while the checks are `pending`. Failing checks or a
    conflict move the card back to `In progress` with a note (`moebius.move(column, note)`, stored on the card by
    `Engine::tick` as `cards.note`); `code.luau` starts the next `Coder` round on top of the previous branch with the
    note in its prompt (`prompt_for`), and `Store::set_run_status` clears the note when a `code` run succeeds. Then the
    `Reviewer`, if configured, starts a run (purpose `review`) in a worktree on the code run's branch; the run commits
    nothing. Its summary ends with `VERDICT: approve` or `VERDICT: changes` (`board::verdict`, which treats a missing
    verdict as `changes`), and the scripts see it as `card.review`. `changes` sends the card back with the findings as
    the note.
12. **The change is offered.** When the checks and the review allow it, `review.luau` calls `moebius.offer_change`, which
    calls `App::offer_change`: it publishes the change if need be, adds a `ready_to_merge` inbox item, publishes
    `change_offered` and moves the card to `Ready to merge`. While the card waits there, `merge.luau` sends it back to
    `In progress` with the conflict if the default branch moved and the change no longer merges; leaving `Ready to
    merge` resolves the inbox item. A `code` or `review` run that failed gets another run until `board.round_limit`; the
    card is put in the inbox as stuck (`moebius.stuck`) at that limit, when it has used the agent's `token_budget` (the
    scripts compare `card.tokens` with `moebius.agent_budget`) and when a person stopped its run. While the daily
    `budgets.day_tokens` is spent `ready.luau`, `code.luau` and `review.luau` wait, and the router refuses requests.
12a. **A person decides what is stuck or offered.** Each is a route of the desktop API and a judgement by `person`:
    `POST /v1/cards/{id}/retry` (`App::retry_card`) resolves the stuck item (or a `change_closed` item, which also sends the card back to `In progress` with a note) and sets `cards.retried_after` to the latest
    run, so `card.rounds`, `card.reviews` and `card.tokens` (and the router's budget) count only later runs;
    `POST /v1/changes/{id}/reject` (`App::reject_change`) resolves the `ready_to_merge` item and moves the card back to
    `In progress` with the reason as its note, the change staying open; `POST /v1/runs/{id}/steer` and
    `POST /v1/runs/{id}/stop` (`App::steer_run`, `App::stop_run`) go through the harness (`Runs::steer`, `Runs::stop`,
    which `ApiState` holds), a steer also leaving a `steered` run event. A stopped run ends `stopped` and
    `card.stopped` makes the scripts mark the card stuck instead of starting another run.
12b. **A person chats.** A chat is a run without a card and with purpose `chat`. `POST /v1/chats` (`App::start_chat`)
    calls `PiBolt::start_chat`: the run (`Store::add_run` with no card, and `Store::set_run_repository` when it works
    in a repository) gets a worktree on `moebius/chat-<id>` from the repository's base ref (`Repos::add_chat_worktree`),
    or an empty directory `<data_dir>/chats/<id>`, and a token whose budget is the agent's `token_budget` for the run
    alone. The first message is stored as a `said` run event and sent to Pi as the prompt, without an issue prompt around
    it. The same `Reader` reads a chat, with one difference: at `agent_settled` the chat does not end but becomes
    `waiting` (`running` again at the next `agent_start`). `POST /v1/chats/{id}/messages` (`App::say_in_chat`) stores a
    `said` event and sends a `prompt` with `streamingBehavior: "followUp"` (`Runs::say`), which Pi starts or queues
    depending on whether the agent is working. Steer and stop are the run's own routes; a stop of a chat also closes Pi's
    standard input, since a waiting chat has no turn to abort. `POST /v1/chats/{id}/end` (`App::end_chat`,
    `Runs::end`) closes Pi's standard input. When Pi has exited the chat is committed on its branch (nothing changed is
    no failure), the worktree or directory is removed, and the run is `succeeded` (`stopped` after a stop). Before the status
    is set, a chat in a repository whose branch has commits beyond the default branch is offered as a change without a card
    (`App::offer_chat_change`: the forge publishes the branch, a change with a null card and a `ready_to_merge` inbox
    item are stored, `change_offered` is published); merging it is the same `App::merge_change`. `PiBolt::agent` makes
    the agent of a run; for `Moebius`, the bundled agent, it adds the built-in instructions, the `moebius` skill and the
    task skills to the Moebius or Coder configuration. Every run has `$MOEBIUS`, the `moebius` executable, for `check-script`. Chats are not
    counted by `moebius.runs_active()`, so they do not use the board's run limit; the daily budget applies to them.
    `GET /v1/chats` (`App::chats`) lists them, newest first, each with the files its worktree changes
    (`App::changed_files`). `POST /v1/chats/{id}/offer` (`App::offer_chat_changes`) offers the work without ending the
    chat: refused while the chat is `running` (`App::say_in_chat` marks it so before it hands the message on, under the
    same lock), it commits the worktree (`repos::commit_all`) and offers the branch as the end does
    (`App::offer_chat_branch`, shared with `offer_chat_change`); a branch offered before updates its open change
    (`Store::latest_change_on_branch`, `set_change_head`) instead of making another. Once a minute `server/mod.rs` calls
    `PiBolt::pause_idle_chats`, which ends the Pi of each chat that has waited `pibolt::IDLE` (30 minutes, from its
    last `agent_settled`) the way the terminal interface does, keeping the chat `waiting` among the paused ones;
    `Runs::say` starts its Pi again with `--continue` before the message, and an end or stop finishes it at once.
    A chat can work in the harness configuration repository (`harness: true`; the run's `harness_repository`): its worktree
    is a chat worktree of the harness clone (`HarnessRepo::add_chat_worktree`), and what it leaves is checked as a Retro
    proposal is (`HarnessRepo::check_chat_branch`, which shares `check_branch` with `proposed`) before `App::offer_chat_change`
    stores a change with a null repository (`Store::add_harness_change`; `harness_repository` set). A branch that fails the
    check is not offered and the chat gets a `moebius_error`. `App::merge_change` merges such a change with
    `HarnessRepo::merge_branch`, which `merge_proposal` uses too.
12c. **The Retro.** Once a minute `server/mod.rs` asks `retro::on_schedule`, which starts today's Retro when `retro_at` has passed
    and the `retros` table has no row for the day (`retro::due`); `POST /v1/retros` calls `retro::start` too. `retro::report`
    reads the day's errors from the store, one `Store` function per source (failed and refused tool calls, failed and stopped
    runs, steering, cards sent back from review), into the stored report. With errors and a harness repository,
    `Runs::start_retro` (`PiBolt::start_retro`) starts a `retro` run of the built-in `Retro-analyst` in a detached worktree of
    the harness clone; a thread waits for the run, then `HarnessRepo::proposed` checks, pushes and returns the
    `proposal/<date>-*` branches, which are stored in `proposals` with the cards of their `Cards:` line, and `retro_done`
    is published.
12d. **The trials.** The same minute, `server/mod.rs` asks `trial::on_schedule`, which inside `[trial_window]` (`trial::in_window`)
    starts the oldest `open` proposal when no other is `trying` and the board has a run to spare; `POST
    /v1/proposals/{id}/trial` calls `trial::start` at any hour. A thread tries the proposal on each card: the agent is read
    from the proposal's branch in a throwaway worktree (`HarnessRepo::agent_on_branch`), `Runs::start_trial`
    (`PiBolt::start_trial`) starts a `trial` run of it on the card's task on a branch of its own, `App::local_checks` runs the
    repository's checks on the result, and `trial::compare` sets the measurements against the card's first code run. The
    trial is stored in `trials`, its branch and worktrees are removed (`trial_done`), and the proposal becomes `positive`,
    `negative` or `untried` (`proposal_status`). A positive or untried one gets a `proposal` inbox item; `trial::merge`
    (`HarnessRepo::merge_proposal`) or `trial::dismiss` ends it. `Store::runs_for_card` and the Retro's sources leave trial
    runs out. A server start opens the `trying` proposals again.
12e. **The replays.** After the trials, `server/mod.rs` asks `replay::on_schedule`, which inside `[trial_window]` calls
    `replay::run_due` while the night's 20 are not used up: `replay::due` takes the oldest refused `tool_call` judgements without
    an outcome or replay (`Store::replay_candidates`) whose call `guard::reversible` accepts, and each is run by
    `replay::replay_judgement` in a copy of the worktree (or a `git archive`) under `<data_dir>/replays`, never while a proposal
    is `trying`. The result is stored in `replays`, becomes the judgement's outcome (`Store::set_outcome`) and is published
    (`replayed`). `POST /v1/judgements/{id}/replay` calls `replay::replay` at any hour.
12f. **Autoresearch.** `POST /v1/research` stores a brief (`research`, `experiments` and `research_runs` tables); `confirm`
    records a `confirm_brief` judgement by a person. The same minute, `server/mod.rs` asks `research::on_schedule`, which inside
    `[trial_window]` starts the oldest `confirmed` research when no proposal is `trying` and none is `running`;
    `POST /v1/research/{id}/start` calls `research::start` at any hour. A thread measures the baseline (`research::measure`:
    the research's command through `replay::run_apart` in a detached worktree, the last number on standard output), then
    loops: if `research::spent` says the tokens of the research's runs have reached the budget it ends `spent`;
    otherwise `Runs::start_research` (`PiBolt::start_research`) starts a `research` run of the built-in `Autoresearcher`
    in a detached worktree of the repository, which leaves `research/<id>-<name>` branches. Each new branch with a commit is
    measured, checked by `App::local_checks`, judged by `research::judge` (better than the baseline by more than 1 %, checks
    passing or none), stored in `experiments` (`experiment_done`) and, when kept, offered by `App::offer_branch_change`, the
    function that also offers a chat's work. A round without new branches ends the research `done`; a failing baseline ends
    it `failed`; `research::stop` ends it `stopped` and stops its run. Every status change is `research_status`. A server
    start sends `running` research back to `confirmed`. `trial::on_schedule` waits while a research runs.
12g. **Grilling.** A grilling is a chat (12b) with the built-in agent `Grilling-partner` (`pibolt::GRILLING_AGENT`);
    `App::start_chat` refuses it without a repository. `Reader::end` treats it differently from other chats: nothing is
    committed, and before the worktree is removed `grilling::conclude` reads `.moebius/grilling/issues.json` or `report.md`
    (`grilling::read_result`), stores a row of `grillings` (`proposed`, `reported` or `empty`), adds the inbox item
    `grilling_issues` or `grilling_report` and publishes `grilling_ended`; `offer_chat_change` is not called.
    `POST /v1/grillings/{id}/confirm` calls `grilling::confirm`: `grilling::creation_order` puts blockers first, and
    `App::create_confirmed_issue` makes each card (on GitHub after `Forge::create_issue`, which opens the issue with a
    `Blocked by` line, through `GitHub::create_issue`) with `Store::add_confirmed_card`, which stores the card as
    `confirmed` together with its dependencies. `backlog.luau` moves such a card to Ready (`card.confirmed`); a
    `confirm_grilling` judgement by `person` is recorded and the inbox item resolved.
13. **GitHub reports.** `poll::poll_pull_requests` reads each open change's pull request: checks and review are recorded
    (`change_updated`), a merged pull request calls `App::change_merged`, a closed one calls `App::change_closed`.
14. **A person merges.** `POST /v1/changes/{id}/merge` calls `App::merge_change`: it records a judgement by `person`,
    the forge merges (`Repos::merge`, a `--no-ff` merge, or `GitHub::merge_pull_request`), and `App::change_merged`
    marks the change merged, resolves the inbox, publishes `change_merged` and moves the card to `Done`.
15. **Done.** `done.luau` calls `moebius.record_outcome("merged")`, so every judgement made for the card gets an outcome.

At start, `Server::new` ends the runs a previous server left going (`end_runs_of_a_previous_server`): they are marked
`failed` and their worktrees (a chat's empty directory too) removed, so the board starts them again.

## The store

SQLite in `<data_dir>/moebius.sqlite` (WAL mode) with large content in `<data_dir>/blobs`. The schema is migrated at
start; its version is `PRAGMA user_version`.

| Record | Holds |
|---|---|
| `repositories` | Name, clone path, forge (`none` or `github`), default branch, GitHub name and trusted logins. |
| `cards` | One per issue: number (unique per repository), title, body, author, column, note (why it went back to coding). |
| `card_moves` | Each move of a card (`Store::move_card`, called by `App::move_card`): from, to, the card's note then, time. Moves before this table existed are not recorded. |
| `dependencies` | For a card, the issue numbers it is blocked by. |
| `changes` | A card's or chat's branch `moebius/card-<number>`: repository (none, with `harness_repository` set, for the harness configuration repository), card, state (`open`, `merged`, `closed`), pull request number, the published commit and time, checks with the commit they are for, failing checks, output of the repository's own checks, review. |
| `runs` | One per harness run: card (none for a chat), agent, harness, purpose, status, worktree, repository (a chat's) or whether it works in the harness configuration repository, start and end, summary (the agent's final answer). |
| `events` | Everything a run emitted, numbered per run, as JSON. |
| `judgements` | One per decision: kind, context (blob ids), options, choice, reason, decider, run, card, outcome. |
| `shadows` | One per judgement the decision model was asked about: judgement, kind, the model's choice (null when it failed), reason, failed. |
| `takeovers` | A proposal that the decision model take a kind over: kind, agreement, the number of shadows, status (`open`, `merged`, `dismissed`, `released`). |
| `replays` | One per replayed refusal: judgement, run, exit code, timed out, output, files created, changed and deleted. |
| `inbox_items` | What needs a person: kind, card, title, data, resolved or not. |
| `usage` | Tokens per model request: run, model, input, output, cached, duration. |
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
- A repository's own checks run on a thread of their own per change (`App::run_checks`), so the board is not blocked.
- Each Pi run has one reader thread (`pibolt.rs`), and `Runs::stop` (`PiBolt`) starts a short thread that kills Pi when it does not
  settle in time.
- The store is one SQLite connection behind a mutex. Calls are short and take it one at a time.
- The event bus is a tokio broadcast channel of 1024 events; a slow listener loses the oldest and is told with `lagged`.

## Known limits

- The server runs on Unix (Linux, macOS): local checks run in their own process group so that a check that runs too
  long is stopped with everything it started. The desktop app does not share this limit.
- Times in the store are whole seconds. A card's history keeps entries from the same second in the order of their kinds
  (moves, runs, decisions, inbox items), not necessarily the order they happened in.

## Modules

Each crate's `src/` keeps the modules that tie it together at the top and groups the rest in folders, one folder per
part. Every module starts with a doc comment saying what it is for; a folder's `mod.rs` says what the folder holds.

### `crates/moebius` (the binary)

| Module | What it is for |
|---|---|
| `main.rs` | The `moebius` binary: `init` writes the example configuration (with the `pi-bolt` found on `PATH`) and the harness configuration repository next to it (`harness::repo::create`), `serve` starts the server, `check-script` runs a column script once against a made-up card (`board::check_script`), `export` writes the training set (`improve::export`). |
| `cli.rs` | The commands that operate a running server through the desktop API (`api` for any route, and `doctor`, `board`, `issue`, `agent`, `repo` and the others for what is done often). |

### `crates/moebius-plugin-api`

| Module | What it is for |
|---|---|
| `lib.rs` | The `Plugin` trait, the call and decision types, the `Host` that calls plugins with a time limit and fails closed. |

### `crates/moebius-core` (the server)

At the top:

| Module | What it is for |
|---|---|
| `app.rs` | The core put together; the operations on cards, changes and issues, each published on the bus, including publishing a change and running its checks. |
| `config.rs` | `moebius.toml`: parsing, defaults, validation. |
| `blobs.rs` | The content-addressed blob store and secret redaction. |
| `bus.rs` | The event bus behind the event stream. |
| `router.rs` | The router: model endpoints, run tokens, usage, budgets. |

`store/`, the records:

| Module | What it is for |
|---|---|
| `store/mod.rs` | The store: opening the SQLite database, its migrations, the connection and the server's local clock, the helpers that read rows, and every record type re-exported as `store::<Type>`. |
| `store/cards.rs` | Repositories, their cards, the issues that block a card and the moves of a card between columns. |
| `store/changes.rs` | Changes: a branch offered for merging, with its pull request, checks and review. |
| `store/runs.rs` | Runs: an agent's work in a harness, from its start to its end. |
| `store/events.rs` | A run's events, in order, redacted before they are stored. |
| `store/chats.rs` | Chats: where they were started, what they work in and how far a person has read them. |
| `store/judgements.rs` | Judgements at decision points, their outcomes, and the refusals counted by rule. |
| `store/replays.rs` | Replays of refused tool calls and the refusals still to replay. |
| `store/inbox.rs` | The inbox: what waits for a person, until it is resolved. |
| `store/grillings.rs` | What a grilling chat ended in: the issues it proposes or its report. |
| `store/shadows.rs` | The decision model's shadows, how well they agree, and the takeovers proposed from them. |
| `store/usage.rs` | The usage of every model call and what it adds up to by card, day, model, agent, activity and tool. |
| `store/found.rs` | What went wrong in a window of time, as the Retro collects it. |
| `store/retros.rs` | The Retro of each day, its proposals and their trials. |
| `store/research.rs` | Research: its brief, its runs and the experiments they try. |

`server/`, serving it all:

| Module | What it is for |
|---|---|
| `server/mod.rs` | Wires everything from the configuration and runs the loops. |
| `server/api/mod.rs` | The desktop API: the state it needs, the token check, the router that merges the routes of its parts, how a failure becomes a status, and `GET /v1/health`. |
| `server/api/cards.rs` | The snapshot of every board, issues, cards and their changes (edit, approve, retry, merge, reject), a card's runs and history. |
| `server/api/repositories.rs` | Adding and editing repositories, and their column scripts and files. |
| `server/api/runs.rs` | A run and its events in pages, steering and stopping it. |
| `server/api/chats.rs` | Chats with an agent (start, write, offer, end, seen, settings) and what grilling chats ended in. |
| `server/api/harness.rs` | The agents, skills and MCP servers of the harness configuration repository, and `POST /v1/harness/setup`. |
| `server/api/rules.rs` | The rules page and the rules in prose. |
| `server/api/settings.rs` | `moebius.toml`, the API keys and the restart, and what the Models, Plugins and Get started pages show. |
| `server/api/improve.rs` | Usage, judgements and replays, the Retro and its proposals, the decision model and its takeovers, research and exports. |
| `server/api/uploads.rs` | `POST /v1/uploads`: zip files for the Moebius agent to import from. |
| `server/api/terminals.rs` | The terminal WebSockets: a shell in a run's worktree or on the server, and Pi's terminal interface of a chat. |
| `server/api/events.rs` | The event WebSocket `GET /v1/events`. |
| `server/settings.rs` | Editing `moebius.toml` while the server runs: changes applied to the file's text, the keys that take effect at once, the API keys kept in `secrets.toml`, and the restart. |
| `server/setup.rs` | What the Models, Plugins and Get started pages show: the router's upstreams, the agents, the teacher, the decision model and the plugins, without a secret, and the steps to a first card (`GET /v1/start`). |

`repos/`, the repositories and their forges:

| Module | What it is for |
|---|---|
| `repos/mod.rs` | Clones, worktrees, branches, pushes, merges and conflict checks through the `git` command line. |
| `repos/forge.rs` | The `Forge` trait with the forges `none` and `github`. |
| `repos/github.rs` | The GitHub App client: JWT, installation tokens, issues with ETags, pull requests, checks, reviews, merge. |
| `repos/poll.rs` | The poll of GitHub: issues, pull requests, checks, reviews. |
| `repos/files.rs` | A repository's files on its default branch, read and committed (the Files and Editor tabs of a project). |
| `repos/check.rs` | The tool-call check endpoints `/moebius/check` and `/moebius/result`, and the local checks of a change. |

`board/`, the workflow:

| Module | What it is for |
|---|---|
| `board/mod.rs` | The board engine: the Luau sandbox, the `moebius` table and the default workflow. |
| `board/scripts.rs` | Editing a column's script: the script in force, saved as a commit in the repository. |

`decide/`, every decision on the way:

| Module | What it is for |
|---|---|
| `decide/mod.rs` | Decision points: routing a question to a decider, failing closed, recording the judgement. |
| `decide/guard.rs` | The `guard` decider: built-in pattern rules and the teacher model. |
| `decide/rules.rs` | The `rules.md` format and where rules come from. |
| `decide/model.rs` | The decision model: asking an OpenAI-compatible endpoint, shadowing held-out judgements in the background, takeover proposals and their merge, dismissal and release. |

`harness/`, the agents and what they are given:

| Module | What it is for |
|---|---|
| `harness/pibolt.rs` | The Pi-Bolt harness: starting, reading and stopping runs and chats. |
| `harness/terminal.rs` | A person's shell in a run's worktree on a pseudo-terminal (`portable-pty`), served over the WebSocket `GET /v1/runs/{id}/terminal` (and `GET /v1/terminal`, in the directory of `moebius.toml`); at most 4 per run or on the server, not checked by the guard. The same serving carries Pi-Bolt's interactive mode for a waiting chat (`GET /v1/chats/{id}/tui`, one per chat), whose tool calls the bridge still checks; `harness/pibolt.rs` ends the chat's own Pi for it, continues the chat's Pi session in it, records what it adds to the session file as the chat's events, and starts the chat's Pi again on the same session when it closes. |
| `harness/repo.rs` | The harness configuration repository: its clone, reading agents, skills, rules and MCP servers from it, and edits as commits pushed to its origin; `create` makes a new one from the agents and rules of `moebius.toml` (`POST /v1/harness/setup`, `moebius init`). |
| `harness/mcp.rs` | The MCP servers of `mcp.toml`, checked when read, and the `mcp.json` a run of an agent that names them is given. |
| `harness/skill.rs` | The built-in skills, bundled with `include_str!` and written to `<data_dir>/skills/<name>/` at start: the `moebius` skill (the guide and the references) and the task skills of the Moebius agent (`moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions` and `moebius-plugins`, in `crates/moebius-core/skill/<name>/SKILL.md`). |
| `harness/grilling.rs` | The Grilling partner's end: the issues or report it left in its worktree, confirming the issues as cards (on GitHub opened there first) and dismissing. |

`improve/`, learning from what was recorded:

| Module | What it is for |
|---|---|
| `improve/retro.rs` | The Retro: the day's errors collected into a report without a model, the Retro analyst's run, and its proposal branches. |
| `improve/trial.rs` | Trials: each proposal tried on the cards of the errors it addresses, measured against the cards' first runs, and offered when positive. |
| `improve/replay.rs` | Replays: refused, reversible tool calls run again in a throwaway copy of the worktree, and the judgement's outcome from what happened. |
| `improve/research.rs` | Autoresearch: a confirmed brief, its baseline, the Autoresearcher's levers as branches, an experiment for each, and the kept ones offered as changes. |
| `improve/export.rs` | The training-set export: judgements with their contexts and outcomes as JSON Lines per kind, the held-out tenth, the exports a person started in the app. |

Outside `src/`: `workflow/*.luau` holds the six built-in column scripts, `bridge/moebius-bridge.ts` the Pi extension
loaded into every run, `skill/` the built-in skills.

### `crates/moebius-desktop` (the app)

At the top:

| Module | What it is for |
|---|---|
| `main.rs` | The desktop app's start: fonts, theme, window. |
| `app.rs` | The app's state, `Message`, `update` and view: the connection, the page shown, reading the snapshot again on events. |
| `client.rs` | The desktop API client: every route typed, and the event stream as an iced subscription that reconnects with a backoff. |
| `settings.rs` | The address and token, saved in the config directory with mode 0600. |
| `theme.rs` | Every colour, size, radius and spacing of `docs/design.md`, and the bundled fonts (Manrope, JetBrains Mono). |
| `sidebar.rs`, `search.rs` | The sidebar on every screen, and its search field. |

`screens/`, one module per page:

| Module | What it is for |
|---|---|
| `connect.rs`, `start.rs` | The connect screen, and the Get started page: the steps of `GET /v1/start` with a button to the page that fixes each. |
| `inbox.rs`, `chat.rs`, `fleet.rs` | The inbox and its actions, the chat screen, every working run at a glance. |
| `project.rs`, `board.rs`, `add_repository.rs` | A project's page with the tabs Board, Files and Editor; the board's columns and cards; adding a repository. |
| `live.rs`, `history.rs` | What a click on a card opens: the live view of its latest run (conversation, steer, stop, the run's panel), or the card's history when it has no run. |
| `agents.rs`, `skills.rs`, `mcp_servers.rs`, `rules.rs`, `harness_setup.rs` | The Agents, Skills, MCP servers and Rules pages, and setting up a harness configuration repository; an edit is a commit in it. |
| `usage.rs`, `retro.rs`, `experiments.rs`, `decision_model.rs` | The Improve pages: usage with its heatmap, the Retro's days and proposals, autoresearch, the training set and shadowing. |
| `setup.rs`, `server_terminal.rs` | The Setup pages (models and settings, which edit `moebius.toml` through the API, and the plugins), and the Terminal page: a shell on the server (`GET /v1/terminal`). |

`widgets/`, the pieces screens share:

| Module | What it is for |
|---|---|
| `page.rs`, `icons.rs` | The page header and box every page is made of, and the line icons. |
| `conversation.rs` | Folds a run's events into a conversation (text, tool calls with their checks, steers, errors); pure, takes events in any order and drops repeats. |
| `transcript.rs`, `steps.rs` | How a conversation is drawn (Markdown, tool calls, highlighted code), and the steps between two answers. |
| `terminal.rs`, `chat_terminal.rs` | The shell pane: bytes from the server parsed by `alacritty_terminal` into a grid and drawn in JetBrains Mono, keys turned into bytes, the pane's size in cells sent as a resize (`client::terminal`); and a chat's terminal, the same pane on `GET /v1/chats/{id}/tui`. |
| `float_chat.rs` | The floating chat with the Moebius agent on every screen, given the context of the page. |
| `inline_edit.rs`, `edits.rs`, `script_editor.rs`, `new_issue.rs`, `import.rs`, `model_picker.rs` | Editing in place (the pencil and its editor, a card's title and body, a column's script), the new-issue form, importing skills and MCP servers, and the model dropdown. |
