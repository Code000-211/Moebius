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
 │  core: App ── store (SQLite + blobs), bus, repos (git), forges, decisions   (app/, ...)     │
 │  poll ── GitHub, GitLab, Forgejo issues and pull/merge requests, every poll_seconds (poll/) │
 │  router + check + MCP gateway ── model endpoints, tool-call check, MCP (127.0.0.1:7172)      │
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
   loop in `Server::serve` calls `poll::poll` with the GitHub client every `github.poll_seconds`, on GitLab with the
   GitLab client every `gitlab.poll_seconds`, and on Forgejo with the Forgejo client every `forgejo.poll_seconds`. That
   fetches the clone (`App::refresh`), lists issues with `GitHub::issues`, `GitLab::issues` or `Forgejo::issues`
   (conditional on the ETag where the forge gives one), turns them into the forge-neutral `forge::Issue` (the client's
   `poll::Polled`), and `App::sync_issues` adds or updates the cards and their "blocked by #n" dependencies
   (`Store::set_dependencies`). A repository without a forge has no
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
   the note if there is one). The server's `Runs` is `Harnesses` (`harness/pick.rs`), which hands the run to the harness
   the agent's `harness` names: an agent on `claude-code` is started by `ClaudeCode::start` the same way (`card_run`
   makes the run, worktree and token for both), with a checked token (`Router::issue_checked_token`), so that the router
   checks the tool calls of its answers (`router/calls/`), since Claude Code has no bridge.
   When the agent names a runner (`agents.<name>.runner`, or the fourth argument of `moebius.start_run`, which reaches the
   harness as `Runs::start_on`), `card_run` takes one of the runner's runs first (`Runners::claim`; a full runner makes
   no run) and the run carries a `RemoteRun` (`harness/remote/`). Its launch opens the reverse tunnel to the router
   (`RemoteRun::connect`), pushes the worktree's commit to the runner's clone (`prepare`), writes the agent directory with
   the tunnel's URL in place of the router's, and starts the harness through `RemoteRun::command`, which turns the local
   command into one `ssh` that copies the files it names and runs it in the run's checkout there. Everything after that is
   as for a local run, the reader included. When the run ends `finish` first has the runner commit what the agent left and
   fetches it into the worktree (`RunningRun::bring_back`), so that the commit, the checks and the change are done here.
6. **Pi works.** A thread per run (`Reader::read`) reads Pi's records: each is stored and published as `run_event` with
   its `seq` (`App::record_run_event`, which `check::ask` and `App::steer_run` use too). Pi asks the model through the router (`Router::forward`), which counts the tokens and may
   refuse with 429.
7. **Every tool call is checked.** The bridge posts to `/moebius/check` (`check::check`). On a blocking thread,
   `check::ask` builds the `tool_call` question (`tool_call_question`, with the applicable rules from
   `rules::applicable`, or the `rules_error` when the server's rules file cannot be read), and `Decisions::decide` asks the `guard` plugin through the `Host` on a thread of its own with
   a time limit. `Guard::call` tests the built-in patterns, then the prose rules with the teacher model (`Teacher::judge`,
   which calls the router with a token for this run; once a takeover of `tool_call` is merged, `DecisionModel::ask` answers the prose rules instead). The answer is stored as one judgement, as a `tool_check` run
   event, and returned. A refusal goes back to the model as the tool's result and the run goes on. The result of each
   tool is cleaned of secrets through `/moebius/result` (`check::result`). A run whose agent has MCP servers reaches
   them only through the MCP gateway (`POST /mcp`, `harness/gateway/`): its `mcp.json` names the gateway with the run's
   token, which `PiBolt::write_agent_dir` gave the agent's servers (`Router::allow_mcp`). The gateway starts or connects
   to them at the run's first request, lists their tools as `<server>__<tool>`, checks every call with
   `check::decide_call` before the server sees it, cleans the result of secrets and records an `mcp_call` run event;
   it stops the servers when the run ends (`Gateway::stop_ended_runs`).
8. **The run ends.** At `agent_settled`, `Reader::end` closes Pi's standard input, revokes the token, commits what the
   agent left (`Reader::commit`: `git add --all` and a commit by Moebius; for `code` runs, and for chats, see 12b), sets the status (`succeeded`,
   `failed` when a `code` run changed nothing or anything went wrong, `stopped` when aborted), removes the worktree
   (the branch stays) and publishes `run_ended`. The text of the agent's final answer, reported in Pi's `agent_end`
   event, was stored as the run's summary when that event came (`Store::set_run_summary`).
9. **The change is published.** On a later tick `code.luau` sees the succeeded `code` run (and no `card.note`) and moves
   the card to `In review`. `review.luau` calls `moebius.publish_change`, which calls `App::publish_change`: the branch
   `moebius/card-<number>` is pointed at the latest successful `code` run's commit (`Repos::point_branch`; each round's
   branch descends from the last, so the branch only moves forward) and the forge publishes it (`Forge::publish`:
   `NoForge` does nothing; `GitHubForge` pushes and, the first time, opens the pull request; `GitLabForge` the same with a
   merge request, `ForgejoForge` with a pull request on Forgejo). The change is stored
   (`Store::add_change`, later `Store::set_change_head`) with the published commit and time, and `change_updated` is
   published. A card has one change, one branch and one pull request for all its rounds.
10. **The change is checked.** On GitHub the poll records the check runs of the pull request's head commit, on GitLab
    the latest pipeline of the merge request's head commit and its failed jobs, on Forgejo the commit statuses of the
    pull request's head commit (`poll::poll_requests`, which reads them
    as a `poll::Request` through the client's `Polled::request`, then `App::record_change_status`), with the head they
    are for and the failing ones. Without
    a forge, `moebius.run_checks` calls `App::run_checks`: if `.moebius/checks` exists on the default branch, a thread
    runs it with `sh -c` in a throwaway worktree of the change's commit (`App::check_change`, `execute`) with the time
    limit `checks.timeout_seconds`, and records `success` or `failure` with the end of the output, redacted
    (`Store::set_local_checks`). `App::checks_of` gives scripts `card.change.checks`: checks for another commit than the
    published one do not count, and on a forge the change is `pending` for a grace period after the publish and then
    `none`. `App::conflict_of` asks `git merge-tree` whether the change still merges into the default branch
    (`Repos::conflicts`).
11. **The change is sent back or reviewed.** `review.luau` waits while the checks are `pending`. Failing checks or a
    conflict move the card back to `In progress` with a note (`moebius.move(column, note)`, stored on the card by
    `Engine::tick` as `cards.note`); `code.luau` starts the next `Coder` round on top of the previous branch with the
    note in its prompt (`prompt_for`), and `Store::set_run_status` clears the note when a `code` run succeeds. Then the
    `Reviewer`, if configured, starts two runs one after the other (purposes `review-standards` and `review-spec`, each by
    the agent `Standards` or `Spec` if there is one) in worktrees on the code run's branch; they commit nothing. Each
    summary holds findings with evidence and ends with `VERDICT: approve` or `VERDICT: changes`; `board::review` drops
    findings without evidence and reads the two into `card.review`, whose `findings` are the blocking ones and
    `judgements` the rest. Blocking findings send the card back as the note; `offer_change` stores the judgements in
    the inbox item.
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
    (`Store::latest_change_on_branch`, `set_change_head`) instead of making another. Once a minute `server/schedule.rs` calls
    `PiBolt::pause_idle_chats`, which ends the Pi of each chat that has waited `pibolt::IDLE` (30 minutes, from its
    last `agent_settled`) the way the terminal interface does, keeping the chat `waiting` among the paused ones;
    `Runs::say` starts its Pi again with `--continue` before the message, and an end or stop finishes it at once.
    A chat can work in the harness configuration repository (`harness: true`; the run's `harness_repository`): its worktree
    is a chat worktree of the harness clone (`HarnessRepo::add_chat_worktree`), and what it leaves is checked as a Retro
    proposal is (`HarnessRepo::check_chat_branch`, which shares `check_branch` with `proposed`) before `App::offer_chat_change`
    stores a change with a null repository (`Store::add_harness_change`; `harness_repository` set). A branch that fails the
    check is not offered and the chat gets a `moebius_error`. `App::merge_change` merges such a change with
    `HarnessRepo::merge_branch`, which `merge_proposal` uses too.
12c. **The Retro.** Once a minute `server/schedule.rs` asks `retro::on_schedule`, which starts today's Retro when `retro_at` has passed
    and the `retros` table has no row for the day (`retro::due`); `POST /v1/retros` calls `retro::start` too. `retro::report`
    reads the day's errors from the store, one `Store` function per source (failed and refused tool calls, failed and stopped
    runs, steering, cards sent back from review), into the stored report. With errors and a harness repository,
    `Runs::start_retro` (`PiBolt::start_retro`) starts a `retro` run of the built-in `Retro-analyst` in a detached worktree of
    the harness clone, with the raw traces of the report's runs (`retro::traces`: each run's events and the diff of what it
    made, redacted, at most 20 runs) written under `.retro/` of the worktree and left out of git through the clone's
    `info/exclude`; a thread waits for the run, then `HarnessRepo::proposed` checks, pushes and returns the
    `proposal/<date>-*` branches, which are stored in `proposals` with the cards of their `Cards:` line, and `retro_done`
    is published. Before that, `HarnessRepo::take_own_branches` reads and deletes the branches that change only files
    under `moebius/`, the analyst's proposals for Moebius's own configuration (what it is told about them is
    `own::brief`); `own::store` checks each (a `moebius.toml` change through `Live::preview`, a `.moebius/` change with
    the rules check and `check_script`) and stores it in `proposals` as `settings` or `repository` with its diff
    (`own::unified`), `untried`, with a `proposal` inbox item at once. They are never tried: `trial::merge` hands them
    to `own::merge`, which saves a `settings` one with `Live::save` (the merge's route then restarts the server when a
    key needs it) and commits a `repository` one on `moebius/proposal-<id>` of the repository and offers it with
    `App::offer_branch_change`; `own::dismiss` changes nothing.
12d. **The trials.** The same minute, `server/schedule.rs` asks `trial::on_schedule`, which inside `[trial_window]` (`trial::in_window`)
    starts the oldest `open` proposal when no other is `trying` and the board has a run to spare; `POST
    /v1/proposals/{id}/trial` calls `trial::start` at any hour. A thread tries the proposal on each card: the agent is read
    from the proposal's branch in a throwaway worktree (`HarnessRepo::agent_on_branch`), `Runs::start_trial`
    (`PiBolt::start_trial`) starts a `trial` run of it on the card's task on a branch of its own, `App::local_checks` runs the
    repository's checks on the result, and `trial::compare` sets the measurements against the card's first code run. The
    trial is stored in `trials`, its branch and worktrees are removed (`trial_done`), and the proposal becomes `positive`,
    `negative` or `untried` (`proposal_status`). Before that, unless the own trials made it negative or `[regression]`
    turns the gate off, `trial::regression` tries it the same way (`run_trial`) on the regression set of each repository of
    its cards: the latest merged cards (`Store::merged_card_changes`), those whose change touches `.moebius/reproduce`
    first, each from the commit its first code run started at (`Store::run_base`, recorded by `PiBolt::start_card_run`),
    stopping at the first that fails; these trials are stored with `regression` set. A positive or untried one gets a `proposal` inbox item; `trial::merge`
    (`HarnessRepo::merge_proposal`) or `trial::dismiss` ends it. `Store::runs_for_card` and the Retro's sources leave trial
    runs out. A server start opens the `trying` proposals again.
12e. **The replays.** After the trials, `server/schedule.rs` asks `replay::on_schedule`, which inside `[trial_window]` calls
    `replay::run_due` while the night's 20 are not used up: `replay::due` takes the oldest refused `tool_call` judgements without
    an outcome or replay (`Store::replay_candidates`) whose call `guard::reversible` accepts, and each is run by
    `replay::replay_judgement` in a copy of the worktree (or a `git archive`) under `<data_dir>/replays`, never while a proposal
    is `trying`. The result is stored in `replays`, becomes the judgement's outcome (`Store::set_outcome`) and is published
    (`replayed`). `POST /v1/judgements/{id}/replay` calls `replay::replay` at any hour.
12f. **Autoresearch.** `POST /v1/research` stores a brief (`research`, `experiments` and `research_runs` tables); `confirm`
    records a `confirm_brief` judgement by a person. The same minute, `server/schedule.rs` asks `research::on_schedule`, which inside
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
12h. **Automations.** `POST /v1/automations` (`App::save_automation`) checks the schedule (`cron::parse`), the agent
    (`Runs::has_agent`) and the place, and stores the automation (`automations` table) with `checked_at`, the local
    minute it was made. Once a minute `server/schedule.rs` reads the server's local minute (`Store::local_minute`, SQLite's
    `localtime`) and asks `automations::on_schedule`, which first ends each automation's chat that has answered and waits
    (`Store::waiting_automation_chats`, `App::end_chat`, so its work is offered as for any chat), then fires each
    automation that is on and whose schedule came due after its `checked_at` (at most five minutes back) and up to now,
    and sets every `checked_at` to now. A firing (`fire`, also `POST /v1/automations/{id}/run`) is skipped while the chat
    of the automation's last started firing has neither ended nor answered; otherwise it calls `App::start_chat` with the
    automation's agent, place and prompt and the origin `automation`. Each firing is stored (`firings` table: `started`
    with its chat, `skipped` or `failed` with the reason) and published as `automation_fired`.
13. **The forge reports.** `poll::poll_requests` reads each open change's pull or merge request: checks and review
    (on GitLab, whether anyone approved; on Forgejo, the latest review given) are recorded (`change_updated`), a merged one calls `App::change_merged`, a
    closed one calls `App::change_closed`.
14. **A person merges.** `POST /v1/changes/{id}/merge` calls `App::merge_change`: it records a judgement by `person`,
    the forge merges (`Repos::merge`, a `--no-ff` merge, `GitHub::merge_pull_request`, `GitLab::merge` or
    `Forgejo::merge`), and `App::change_merged`
    marks the change merged, resolves the inbox, publishes `change_merged` and moves the card to `Done`.
15. **Done.** `done.luau` calls `moebius.record_outcome("merged")`, so every judgement made for the card gets an outcome.

At start, `Server::new` ends the runs a previous server left going (`end_runs_of_a_previous_server`): they are marked
`failed` and their worktrees (a chat's empty directory too) removed, so the board starts them again.

## The store

SQLite in `<data_dir>/moebius.sqlite` (WAL mode) with large content in `<data_dir>/blobs`. The schema is migrated at
start; its version is `PRAGMA user_version`.

| Record | Holds |
|---|---|
| `repositories` | Name, clone path, forge (`none`, `github`, `gitlab` or `forgejo`), default branch, GitHub name, trusted logins, and the project's path on another forge (GitLab, Forgejo). |
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
- Forge methods block. `GitHubForge`, `GitLabForge` and `ForgejoForge` wait for their async clients with `Handle::block_on`, so they must
  be called from a blocking thread, never from an async task.
- Column scripts run on the blocking thread of the tick. A Luau state is made and dropped per script run, and is never
  shared between threads.
- Every plugin call runs on a thread of its own, started by the `Host` for that call, with a time limit. The teacher uses a
  blocking HTTP client there.
- Each plugin in its own process has a thread that looks after it (`process/supervise.rs` of the plugin API), one that
  reads its standard output and one its standard error. A call writes its line and waits for the reply that thread hands
  it by id.
- A repository's own checks run on a thread of their own per change (`App::run_checks`), so the board is not blocked.
- Each Pi run has one reader thread (`harness/pibolt/reader.rs`), and `Runs::stop` (`PiBolt`) starts a short thread that kills Pi when it does not
  settle in time.
- The store is one SQLite connection behind a mutex. Calls are short and take it one at a time.
- The event bus is a tokio broadcast channel of 1024 events; a slow listener loses the oldest and is told with `lagged`.

## Known limits

- The server is documented and tested on Linux and WSL 2. It builds and starts on Windows (`platform.rs` keeps what
  differs), with these limits there: a repository's own checks, replays and research measures run `sh` and `bash`, and
  say that they need Linux or WSL when those are missing; a stopped program's processes are found by `taskkill /T`,
  which misses one whose parent has already ended; files with secrets keep the permissions of their directory instead
  of mode `0600`; skills are copied into a Claude Code run instead of linked; the terminal pages open PowerShell. The
  desktop app and the `moebius` commands that operate a server have no such limits.
- Times in the store are whole seconds. A card's history keeps entries from the same second in the order of their kinds
  (moves, runs, decisions, inbox items), not necessarily the order they happened in.

## Modules

Each crate's `src/` keeps the modules that tie it together at the top and groups the rest in folders, one folder per
part. Every module starts with a doc comment saying what it is for; a folder's `mod.rs` says what the folder holds.

### `crates/moebius` (the binary)

| Module | What it is for |
|---|---|
| `main.rs` | The `moebius` binary: `init` writes the example configuration (with the `pi-bolt` found on `PATH`) and the harness configuration repository next to it (`harness::repo::create`), `serve` starts the server, `check-script` runs a column script once against a made-up card (`board::check_script`), `export` writes the training set (`improve::export`). |
| `cli/mod.rs` | The commands that operate a running server through the desktop API (`api` for any route, and `doctor`, `board`, `issue`, `agent`, `repo` and the others for what is done often): the commands as clap sees them, the dispatch to their groups, and writing answers as plain text. |
| `cli/client.rs` | Where the server is (`--server` and `--token`, the desktop app's saved settings, `moebius.toml`), the one request a command makes, and sending it with the token. |
| `cli/api.rs` | `api`: any route, with a method and a JSON body. |
| `cli/board.rs` | `board`, `card`, `inbox` (and `inbox act`, as the app's buttons) and `issue create`. |
| `cli/chats.rs` | `chats`, `chat` (start, say, offer, end), `grillings` and `grilling`. |
| `cli/automations.rs` | `automation` (list, save, run, delete). |
| `cli/agents.rs` | `agents` and `agent` (show, save, delete). |
| `cli/rules.rs` | `rules` and `rules save`. |
| `cli/repos.rs` | `repos` and `repo add`. |
| `cli/setup.rs` | `doctor`, `settings`, `secrets set`, `usage` and `server restart`. |
| `cli/improve.rs` | `retro`, `proposals` and `proposal`, `research`, `decision-model`, `takeovers` and `takeover`. |

### `crates/moebius-plugin-api`

| Module | What it is for |
|---|---|
| `lib.rs` | The `Plugin` trait, the call and decision types, the `Host` that calls plugins with a time limit and fails closed. |
| `process/mod.rs` | A plugin in its own process (`Process`): its status, and a call sent as a JSON line and its reply awaited. |
| `process/supervise.rs` | The thread that starts a plugin process, reads its manifest and replies, logs its standard error, and starts it again after a pause when it stops. |
| `process/serve.rs` | `serve`: the plugin's side of the JSON Lines, which runs a Rust `Plugin` as its own program. |

### `crates/moebius-core` (the server)

At the top:

| Module | What it is for |
|---|---|
| `blobs.rs` | The content-addressed blob store and secret redaction. |
| `bus.rs` | The event bus behind the event stream. |
| `lib.rs` | The crate's parts, and `VERSION`. |
| `platform.rs` | What differs between Linux and Windows: process groups and killing them, files only their owner reads, linking a skill, the shell of a terminal, and the error of a feature that needs a Unix shell. |

`app/`, the core put together:

| Module | What it is for |
|---|---|
| `app/mod.rs` | `App`: the store, bus, repositories, forges and decisions put together, its errors and the lookups every operation shares; every change to a card is published on the bus. |
| `app/repositories.rs` | Adding a repository (a path, a URL or GitHub), whom a GitHub repository trusts, and whether a card's issue is trusted. |
| `app/cards.rs` | Cards and their issues: kept in line with the forge's issues, created, edited, approved, retried and moved; why a card waits, what blocks it, its history. |
| `app/runs.rs` | A person's hand in a run: steering, stopping, a shell in its worktree, a chat's terminal interface, and a shell on the server. |
| `app/automations/mod.rs` | Automations: saving and listing them with their next firing, firing one (a chat with origin `automation`, or a skipped or failed firing), and the check once a minute that ends their answered chats and fires those that came due. |
| `app/automations/cron.rs` | A schedule: a cron expression of five fields in the server's local time, parsed, and the next minute it comes due. |
| `app/chats.rs` | Chats: starting one, continuing an ended one, writing to it, its model and reasoning, ending it, and the list of chats. |
| `app/changes.rs` | Changes: a card's work published through the forge, offered with a chat's work in the inbox, rejected or merged, and what the forge says became of them. |
| `app/checks.rs` | How a change's checks stand, and a repository's own checks (`.moebius/checks`) run in a throwaway worktree with a time limit and a cleared environment. |
| `app/reproduce.rs` | Test first: a repository's opt-in (`.moebius/test-first`), what a change names in `.moebius/reproduce`, and the runs of that reproduction test on the change and on the commit it starts from with the test's files added. |

`config/`, `moebius.toml`:

| Module | What it is for |
|---|---|
| `config/mod.rs` | `Config`: the file parsed, every default applied, every path made absolute, and the errors that refuse it. |
| `config/example.rs` | The commented example `moebius init` writes, every key at its default. |
| `config/raw.rs` | The file as it is written: every key optional, unknown keys refused. |
| `config/sections.rs` | The types of the sections (the router's upstreams, the GitHub App, the teacher, the decision model, the agents) and their checks. |

`router/`, the model provider:

| Module | What it is for |
|---|---|
| `router/mod.rs` | The router, its upstreams and its routes `POST /v1/chat/completions` and `POST /v1/messages`. |
| `router/proxy.rs` | A run's request forwarded to the upstream with the provider's key or login, and the answer back, streamed or not. |
| `router/classes.rs` | Model classes (`[router.classes]`): a class named as a model is served by the first of its models within its day's tokens, on the first upstream of the protocol that answers, falling back while an upstream fails before it answers. |
| `router/usage.rs` | The tokens and tools of an answer, read from a body or from a stream as it passes, recorded against the run. |
| `router/grants.rs` | The runs' tokens and what each allows: the card's, the run's and the day's token budgets, the MCP servers the gateway gives the run, and which tokens' answers are checked. |
| `router/calls/mod.rs` | The check of the tool calls in the answers to a harness without a bridge (Claude Code): each call through `decide("tool_call")`, a refused one replaced by a text naming the rule; whole answers, and the secrets of tool results in requests. |
| `router/calls/stream.rs` | A streamed answer checked as it passes: a tool call's events held back until the call is complete and decided. |
| `router/models.rs` | The models the upstreams serve and the model classes, for the pickers of the desktop app. |
| `router/oauth/mod.rs` | The OAuth login of an upstream: its `oauth` table, the warning confirmed before a login, the access token refreshed before it expires and the refresh token kept in `secrets.toml`. |
| `router/oauth/device.rs` | The device authorization grant: the code the person enters at the provider, and the token URL polled until it is confirmed. |
| `router/oauth/code.rs` | An authorization code with PKCE, the browser sent back to a loopback listener. |
| `router/oauth/testing.rs` | The fake OAuth provider and upstream the tests of the logins share. |

`store/`, the records:

| Module | What it is for |
|---|---|
| `store/mod.rs` | The store: opening the SQLite database, its migrations, the connection and the server's local clock, the helpers that read rows, and every record type re-exported as `store::<Type>`. |
| `store/cards.rs` | Repositories, their cards, the issues that block a card and the moves of a card between columns. |
| `store/changes.rs` | Changes: a branch offered for merging, with its pull request, checks and review. |
| `store/runs.rs` | Runs: an agent's work in a harness, from its start to its end. |
| `store/events.rs` | A run's events, in order, redacted before they are stored. |
| `store/automations.rs` | Automations, their firings with the status of each chat, and the server's local minute. |
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
| `server/mod.rs` | Wires everything from the configuration and serves the desktop API, the router and the loops. |
| `server/leftovers.rs` | What a server that stopped left going, ended at the next start: its runs failed and their worktrees removed, its Retros, research and trials set back. |
| `server/loops.rs` | The board's tick, the polls of GitHub, GitLab and Forgejo (also at once when a webhook wakes one), and the refresh of the clones every 30 seconds. |
| `server/plugins.rs` | Starting the plugins in their own process that `[[plugins]]` names, with their standard error in `<data_dir>/plugins/<name>.log`, redacted. |
| `server/schedule.rs` | Once a minute: the Retro, trials, research, replays and automations when they are due, and the pause of chats that wait idle. |
| `server/fake_github.rs` | For the tests only: a fake GitHub with one repository, and a server that polls it. |
| `server/fake_gitlab.rs` | For the tests only: a fake GitLab with one project, a server that polls it, and the tests of the GitLab forge through it. |
| `server/fake_forgejo.rs` | For the tests only: a fake Forgejo with one repository, a server that polls it, and the tests of the Forgejo forge through it. |
| `server/api/mod.rs` | The desktop API: the state it needs, the token check, the router that merges the routes of its parts, how a failure becomes a status, and `GET /v1/health`. |
| `server/api/cards.rs` | The snapshot of every board, issues, cards and their changes (edit, approve, retry, merge, reject), a card's runs and history. |
| `server/api/repositories.rs` | Adding and editing repositories, and their column scripts and files. |
| `server/api/runs.rs` | A run and its events in pages, steering and stopping it. |
| `server/api/automations.rs` | Automations: list, add, read, replace, delete, run now, and their firings. |
| `server/api/chats.rs` | Chats with an agent (start, write, offer, end, seen, settings) and what grilling chats ended in. |
| `server/api/harness.rs` | The agents, skills and MCP servers of the harness configuration repository, and `POST /v1/harness/setup`. |
| `server/api/rules.rs` | The rules page and the rules in prose. |
| `server/api/settings.rs` | `moebius.toml`, the API keys and the restart, and what the Models, Plugins and Get started pages show. |
| `server/api/improve.rs` | Usage, judgements and replays, the Retro and its proposals, the decision model and its takeovers, research and exports. |
| `server/api/uploads.rs` | `POST /v1/uploads`: zip files for the Moebius agent to import from. |
| `server/api/terminals.rs` | The terminal WebSockets: a shell in a run's worktree or on the server, and Pi's terminal interface of a chat. |
| `server/api/events.rs` | The event WebSocket `GET /v1/events`. |
| `server/api/testing.rs` | For the tests only: a server on port 0 with a fake Pi, and the requests the tests of the API share. |
| `server/settings/mod.rs` | Editing `moebius.toml` while the server runs: what `PUT /v1/settings` takes and answers, and reading and writing the file. |
| `server/settings/edit.rs` | A change applied to the file's text with `toml_edit`, its comments and order kept. |
| `server/settings/keys.rs` | Every key with its default and whether it takes effect at once, and their values without passwords. |
| `server/settings/live.rs` | The configuration the running server reads: a change saved and its live keys applied, the agents and rules moved into a new harness configuration repository, and the restart. |
| `server/settings/secrets.rs` | The API keys kept in `secrets.toml`, and the keys the upstreams and the decision model are given. |
| `server/webhooks.rs` | Webhooks from the forges (ADR 0012): the check of a delivery against the secret of its forge (HMAC-SHA256 or token, in constant time), and the wake-up that makes the poll of a forge run at once. |
| `server/api/webhooks.rs` | `POST /v1/webhooks/{forge}` (no token, a signature instead), `GET /v1/webhooks` and `POST /v1/webhooks/{forge}/secret`. |
| `server/setup.rs` | What the Models, Plugins and Get started pages show: the router's upstreams, the agents, the teacher, the decision model and the plugins, without a secret, and the steps to a first card (`GET /v1/start`). |

`repos/`, the repositories and their forges:

| Module | What it is for |
|---|---|
| `repos/mod.rs` | Clones, worktrees, branches, pushes, merges and conflict checks through the `git` command line. |
| `repos/forge.rs` | The `Forge` trait with the forges `none`, `github`, `gitlab` and `forgejo`, and the forge-neutral `Issue`. |
| `repos/github.rs` | The GitHub App client: JWT, installation tokens, issues with ETags, pull requests, checks, reviews, merge. |
| `repos/gitlab.rs` | The GitLab client: the access token, issues with ETags, merge requests, pipelines and their failed jobs, approvals, merge. |
| `repos/forgejo.rs` | The Forgejo client (Codeberg, Forgejo, Gitea): the access token, issues with ETags where given, pull requests, commit statuses, reviews, merge. |
| `repos/poll/mod.rs` | The poll of the forges: the loop over their repositories, issues into cards, and what became of each change (`Polled`, `Request`). |
| `repos/poll/github.rs` | What GitHub answers, read for the poll: issues, a pull request with its check runs and latest review. |
| `repos/poll/gitlab.rs` | What GitLab answers, read for the poll: issues, a merge request with its latest pipeline, failed jobs and approvals. |
| `repos/poll/forgejo.rs` | What Forgejo answers, read for the poll: issues, a pull request with the statuses of its head commit and its latest review. |
| `repos/files.rs` | A repository's files on its default branch, read and committed (the Files and Editor tabs of a project). |

`board/`, the workflow:

| Module | What it is for |
|---|---|
| `board/mod.rs` | What the board folder holds, and the paths the rest of the server uses. |
| `board/engine.rs` | The board engine: runs the script of a card's column (`tick`, `settle`) and applies what it did. |
| `board/api.rs` | The `moebius` table, the functions a script acts through. |
| `board/review.rs` | The review along two axes: the findings with evidence of the Standards and Spec runs read into one verdict. |
| `board/card.rs` | The `card` table a script reads, with the card's change and the verdict of its review. |
| `board/sandbox.rs` | The Luau sandbox every script runs in, with its memory and time limits. |
| `board/limits.rs` | The run, round and token limits scripts see, and a script call's memory and time. |
| `board/workflow.rs` | The columns, the default workflow's scripts built into the server, and a repository's overrides. |
| `board/runs.rs` | What the engine needs of a harness (`Runs`), and the agents it lists. |
| `board/testing.rs` | For the tests only: a fake harness and a store with a repository and a card. |
| `board/dry_run.rs` | `check_script`: a script run once against a made-up card, for `moebius check-script`. |
| `board/scripts.rs` | Editing a column's script: the script in force, saved as a commit in the repository. |

`decide/`, every decision on the way:

| Module | What it is for |
|---|---|
| `decide/mod.rs` | Decision points: routing a question to a decider, failing closed, recording the judgement. |
| `decide/check/mod.rs` | The tool-call check: the endpoints `/moebius/check` and `/moebius/result` the bridge calls on the router's listener, every call asked before it runs and every result reported; the MCP gateway decides its calls with the same question. |
| `decide/check/result.rs` | `/moebius/result`: each text of a tool's result cleaned of secrets and, with the agent's `tool_output`, made smaller by `harness/output/`, recorded as a `tool_output` run event. |
| `decide/guard/mod.rs` | The `guard` decider: built-in rules first, then the rules in prose for the teacher or the decision model. |
| `decide/guard/shell.rs` | How the guard reads a tool call: simple commands, redirects, nested shells, `cd`, and paths. |
| `decide/guard/patterns.rs` | The built-in rules, fixed patterns over a tool call. |
| `decide/guard/reversible.rs` | Whether a tool call can be replayed: its effects stay in the worktree. |
| `decide/guard/teacher.rs` | The teacher model: what it is told, the request through the router, and reading its answer. |
| `decide/rules.rs` | The `rules.md` format and where rules come from. |
| `decide/model/mod.rs` | The decision model (ADR 0010): its name as a decider plugin, the kind its decider keeps, and the kinds routed to it. |
| `decide/model/ask.rs` | Asking the model through its OpenAI-compatible endpoint, with every field of the context cut; reading its answer, which must be exactly the JSON asked for; and the model as the decider plugin `decision-model`. |
| `decide/model/shadow.rs` | Shadowing: held-out judgements of the shadowed kinds asked in the background behind a bounded queue, each answer stored as a shadow, and a takeover proposed for a kind that has earned one. |
| `decide/model/view.rs` | What `GET /v1/decision-model` shows: the model, its thresholds, and per kind its shadows, agreement, failures, dropped requests and whether it is routed, with the takeovers. |
| `decide/model/takeovers.rs` | A person's decision about a takeover, recorded as a judgement: an open one merged or dismissed, a merged one released. |

`harness/`, the agents and what they are given:

| Module | What it is for |
|---|---|
| `harness/mod.rs` | What the harness folder holds. |
| `harness/pibolt/mod.rs` | The Pi-Bolt harness: what it holds of every run's Pi, and `Runs` for it, steering and stopping runs. |
| `harness/pibolt/agents.rs` | The built-in agents with their instructions and skills, and the agent a run uses, on either harness. |
| `harness/pibolt/process.rs` | Starting Pi: its agent directory (`models.json`, `mcp.json`), its command line and its process. |
| `harness/pibolt/runs.rs` | The runs of a card, of the Retro analyst and of the Autoresearcher, and committing what a run left. |
| `harness/pibolt/reader.rs` | Reading Pi's records as run events, and ending the run. |
| `harness/pibolt/chat.rs` | Chats: their Pi between messages, their settings, and pausing one that waited long. |
| `harness/pibolt/tui.rs` | A waiting chat in Pi's interactive mode, and following its session file. |
| `harness/pibolt/testing.rs` | For the tests only: the fake Pi, a server set up for it, and reading what Pi was started with. |
| `harness/claude/mod.rs` | The Claude Code harness: what it holds of every run's process, and `Runs` for it: card runs, trials, the Retro analyst, the Autoresearcher and chats, steered with a user message and stopped by killing it. |
| `harness/claude/process.rs` | Starting Claude Code: its own configuration directory with the agent's skills, `mcp.json`, its command line and environment (the router and the run's token) and its process. |
| `harness/claude/reader.rs` | Reading Claude Code's `stream-json` records as the run events a Pi-Bolt run has, and ending the run. |
| `harness/claude/tests.rs` | For the tests only: Claude Code runs and chats against the fake `claude`, and the router's check of its calls. |
| `harness/pibolt/remote_tests.rs` | For the tests only: runs on a runner over the fake `ssh` of `testdata/`. |
| `harness/remote/mod.rs` | The runners (`[[runners]]`): which machines there are and how many runs go on each (`Runners`, a `Slot` per run, the list `GET /v1/runners` answers). |
| `harness/remote/session.rs` | One run on a runner over SSH (`RemoteRun`): the reverse tunnel to the router, the run's branch pushed to the machine's clone and checked out, the harness's command line turned into an `ssh` command with the files it names copied over, and the commits fetched back. |
| `harness/pick.rs` | `Harnesses`, the server's `Runs`: Pi-Bolt and Claude Code side by side, each run on the harness its agent names. |
| `harness/terminal.rs` | A person's shell in a run's worktree on a pseudo-terminal (`portable-pty`), served over the WebSocket `GET /v1/runs/{id}/terminal` (and `GET /v1/terminal`, in the directory of `moebius.toml`); at most 4 per run or on the server, not checked by the guard. The same serving carries Pi-Bolt's interactive mode for a waiting chat (`GET /v1/chats/{id}/tui`, one per chat), whose tool calls the bridge still checks; `harness/pibolt/tui.rs` ends the chat's own Pi for it, continues the chat's Pi session in it, records what it adds to the session file as the chat's events, and starts the chat's Pi again on the same session when it closes. |
| `harness/repo/mod.rs` | The harness configuration repository's clone: opened, refreshed, and its agents read again when it changes. |
| `harness/repo/read.rs` | Reading the agents, skills and MCP servers from it, each agent checked as a run uses it. |
| `harness/repo/edits.rs` | A person's edits of agents, skills, MCP servers and rules, as commits pushed to its origin. |
| `harness/repo/skill_files.rs` | A skill's other files, beside its `SKILL.md`: read, and written or deleted as commits like any edit. |
| `harness/repo/branches.rs` | Its branches: the Retro's proposals and chats' branches checked, pushed, merged or dismissed, the Retro's branches for Moebius's own configuration read and deleted, and a trial's worktree of a branch. |
| `harness/repo/create.rs` | `create`: a new repository from the agents and rules of `moebius.toml` (`POST /v1/harness/setup`, `moebius init`). |
| `harness/output/mod.rs` | Keeping a tool's output small where it is made: compressed, cut to the agent's budget (head and tail), and the whole saved to `.moebius/out/<n>.txt` of the worktree, which git ignores. |
| `harness/output/compress.rs` | The compressors: escape codes, progress bars and repeated lines, and the noise of `cargo test`, `pytest`, `go test`, `jest`/`vitest` and cargo builds; only known noise is removed. |
| `harness/mcp.rs` | The MCP servers of `mcp.toml`, checked when read, their `${NAME}` values filled in, and the `mcp.json` that names the MCP gateway for a run of an agent that names them. |
| `harness/gateway/mod.rs` | The MCP gateway, `POST /mcp` on the router's listener: a run's servers bundled as one MCP server, every tool call checked, its result cleaned of secrets and recorded as an `mcp_call` run event. |
| `harness/gateway/upstream.rs` | One server of a run as the gateway reaches it: a stdio child process or a remote server over streamable HTTP, initialized and its tools listed. |
| `harness/skill.rs` | The built-in skills, bundled with `include_str!` and written to `<data_dir>/skills/<name>/` at start: the `moebius` skill (the guide and the references) and the task skills of the Moebius agent (`moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions` and `moebius-plugins`, in `crates/moebius-core/skill/<name>/SKILL.md`), and the engineering skills copied unchanged from Matt Pocock's skills (`crates/moebius-core/skill/vendor/mattpocock/`, MIT, pinned to a commit) that the Coder, the Reviewer and the Grilling partner are given with `moebius-autonomous`, which says what to do where the skills ask a person. |
| `harness/grilling.rs` | The Grilling partner's end: the issues or report it left in its worktree, confirming the issues as cards (on GitHub opened there first) and dismissing. |

`improve/`, learning from what was recorded:

| Module | What it is for |
|---|---|
| `improve/mod.rs` | What the improve folder holds. |
| `improve/retro/mod.rs` | The Retro: the day's errors collected into a report without a model, the Retro analyst's run, and its proposal branches. |
| `improve/retro/traces.rs` | The raw traces the Retro analyst reads besides the report: the events and diffs of the report's runs, redacted and bounded, as files under `.retro/` of its worktree. |
| `improve/trial/mod.rs` | Trials: the trial window, and what the parts share. |
| `improve/trial/verdict.rs` | What a trial run and the card's first code run measured, whether a trial is positive, and the status a proposal gets from its trials. |
| `improve/trial/card.rs` | One card's trial: its task run again by its agent with the definition from the proposal's branch, then measured and compared, its worktree and branch removed. |
| `improve/trial/regression.rs` | The regression set of a repository (its latest merged cards, those with a reproduction test first) and the trials of a proposal on it. |
| `improve/trial/schedule.rs` | Trying a proposal on a thread of its own, card after card, each trial stored and published, its status and inbox item; and the schedule's start inside the trial window. |
| `improve/trial/decision.rs` | A person's merge or dismissal of a positive or untried proposal. |
| `improve/own/mod.rs` | The Retro's proposals for Moebius's own configuration (`moebius.toml` and a repository's `.moebius/`): the keys and files it may change, and what the analyst is told about them. |
| `improve/own/read.rs` | The analyst's branches for Moebius's own configuration checked, stored with their diff and put in the inbox. |
| `improve/own/apply.rs` | A person's merge of such a proposal, saved through the settings code or offered as a change on the repository, or its dismissal. |
| `improve/own/diff.rs` | A unified diff of a file's text before and after, as a person reads such a proposal. |
| `improve/replay/mod.rs` | Replays: one replay a person asks for, the due refusals, and the schedule's replays inside the trial window, at most `PER_NIGHT` a night. |
| `improve/replay/call.rs` | The refused call read back from its judgement: tool, input, worktree, run and repository, and only when it is reversible. |
| `improve/replay/copy.rs` | The throwaway copy, filled from the run's worktree or a checkout; its files listed with sizes and hashes; and what a call created, changed and deleted. |
| `improve/replay/process.rs` | Running the call in the copy: `bash`, `grep` and `find` with an empty environment and a time limit that kills their process group, and the file tools applied by Moebius. |
| `improve/replay/outcome.rs` | A replay made and recorded: the call run in the copy, the sentence that becomes the judgement's outcome, and the replay stored and published, or why it could not be run. |
| `improve/research/mod.rs` | Autoresearch: what the API shows of a research, and the lookups its parts share. |
| `improve/research/brief.rs` | A research's brief: stored as a draft, written anew while it is one, confirmed as a recorded decision, and a research stopped. |
| `improve/research/measure.rs` | A brief's measure run in a fresh worktree with a cleared environment and a time limit, the last number it printed, and whether a lever beats the baseline by enough to be kept. |
| `improve/research/rounds.rs` | The thread that drives a research: its baseline, the Autoresearcher's rounds, each new lever measured and kept or offered, and the schedule's start of the oldest confirmed research. |
| `improve/export.rs` | The training-set export: judgements with their contexts and outcomes as JSON Lines per kind, the held-out tenth, the exports a person started in the app. |

Outside `src/`: `workflow/*.luau` holds the six built-in column scripts, `bridge/moebius-bridge.ts` the Pi extension
loaded into every run, `skill/` the built-in skills.

### `crates/moebius-desktop` (the app)

At the top:

| Module | What it is for |
|---|---|
| `main.rs` | The desktop app's start: fonts, theme, window. |
| `lib.rs` | The crate's parts. |
| `settings.rs` | The address and token, saved in the config directory with mode 0600. |
| `theme.rs` | Every colour, size, radius and spacing of `docs/design.md`, and the bundled fonts (Manrope, JetBrains Mono). |
| `sidebar.rs`, `search.rs` | The sidebar on every screen, and its search field. |

`app/`, the app's state and how it changes:

| Module | What it is for |
|---|---|
| `app/mod.rs` | The state: the connection, the page chosen, what a click on a card opened, each screen's own state. |
| `app/message.rs` | The pages the sidebar opens, and every `Message`. |
| `app/update.rs` | How a message changes the state: handed to its screen, a refused token back to the connect screen, connecting, reading the snapshot again, opening a card or a page. |
| `app/stream.rs` | What the event stream changes: what to read again on an event, and a run's steps to the screens that show them. |
| `app/navigation.rs` | Where the person is and the way back: "Back", Alt+Left, the mouse's back button, Escape in an editor. |
| `app/context.rs` | What the floating chat tells the Moebius agent of the page. |
| `app/view.rs` | How the window is drawn (sidebar, page, link back, floating chat, search list), and the subscriptions the page needs. |

`client/`, the desktop API client:

| Module | What it is for |
|---|---|
| `client/mod.rs` | `Client` and `ClientError`; every type is re-exported here. |
| `client/board.rs`, `client/runs.rs`, `client/harness.rs`, `client/server.rs`, `client/improve.rs` | The answers by part of the API: boards, cards, the inbox and the snapshot; runs and chats; agents, rules, skills and MCP servers; models, plugins, the first start and the settings; the Improve pages. |
| `client/calls/mod.rs` | The calls of the client, one module per part of the API, and the requests with the token they share. |
| `client/calls/board.rs` | The calls about boards: the snapshot, repositories, cards and issues, the inbox's decisions, a repository's files and scripts. |
| `client/automations.rs` | Automations and their firings, and what a save sends. |
| `client/calls/automations.rs` | The calls about automations: list, save, delete, run now, firings. |
| `client/calls/runs.rs` | The calls about runs and chats: a run's events and decisions, steering and stopping, every step of a chat. |
| `client/calls/harness.rs` | The calls about the harness configuration: agents, rules, skills, MCP servers and uploads. |
| `client/calls/server.rs` | The calls about what the server runs with: health, models, plugins, the first start, settings, secrets and the restart. |
| `client/calls/improve.rs` | The calls of the Improve pages: usage, the Retro and its proposals, the decision model, exports and research. |
| `client/events.rs` | The event stream as an iced subscription that reconnects with a backoff. |
| `client/terminal.rs` | A shell's WebSocket: what it prints, and the keys and sizes sent to it. |

`screens/`, one module per page (`screens/mod.rs` lists them):

| Module | What it is for |
|---|---|
| `screens/mod.rs` | The pages of the app, one module each. |
| `screens/connect.rs` | The connect screen, for when there are no settings yet or the connection fails. |
| `screens/start.rs` | The Get started page: the steps of `GET /v1/start` with a button to the page that fixes each. |
| `screens/inbox/mod.rs`, `screens/inbox/view.rs` | The inbox: everything that needs the person, newest first, filtered by kind and repository, and its actions; and how it is drawn. |
| `screens/inbox/item.rs` | What an inbox item says of itself: what it is about, its kind, why it is there, its facts. |
| `screens/automations/mod.rs`, `screens/automations/view.rs` | The Automations page (the list and the editor of the one chosen, with its schedule, chat, "Run now" and firings), how a message changes it, and how it is drawn. |
| `screens/chat/mod.rs`, `screens/chat/update.rs`, `screens/chat/view.rs` | The chat screen (the chats with agents and the one chosen), how a message changes it, and how it is drawn. |
| `screens/chat/thread.rs` | A chat's conversation wherever it is shown, here and in the floating chat. |
| `screens/chat/draft.rs` | A chat not yet started, and the pickers of agent, place, model and reasoning level under the field. |
| `screens/fleet.rs` | Every working run at a glance, one tile per card. |
| `screens/project/mod.rs`, `screens/project/update.rs` | A project: the page of one repository with the tabs Board, Files and Editor, and how a message changes them. |
| `screens/project/view.rs`, `screens/project/editor.rs` | The tabs and the Files tab's tree, and the Editor tab with the open files. |
| `screens/project/context.rs` | What the floating chat is told of a project's tab. |
| `screens/board.rs` | The board of a repository: its columns in order with their cards. |
| `screens/add_repository.rs` | The "Add a repository" page the "+" of the sidebar opens. |
| `screens/live/mod.rs`, `screens/live/update.rs` | The live view of a run: the run being watched, its loading and the shell's subscription, and how a message changes it. |
| `screens/live/view.rs` | The header with status, shell and stop, the conversation and the steer field. |
| `screens/live/panel.rs` | "This run": the card's rounds and tokens, the agent, the judgements and the branch. |
| `screens/history.rs` | The page of a card that has no run to watch: its issue, runs and decisions. |
| `screens/agents/mod.rs`, `screens/agents/update.rs`, `screens/agents/view.rs` | The Agents page (the agents and an editor for the one chosen; an edit is a commit), how a message changes it, and the list with the editor's head. |
| `screens/agents/editor.rs` | The editor of one agent, what a save sends, and an agent of `moebius.toml`. |
| `screens/agents/boxes.rs` | The boxes under the editor's head: model and settings, instructions, skills, MCP servers, extensions. |
| `screens/skills/mod.rs`, `screens/mcp_servers.rs` | The Skills and MCP servers pages, each a list with an editor; an edit is a commit. |
| `screens/skills/files.rs` | A skill's files on the Skills page: the list, `SKILL.md` first, and the editor of another file. |
| `screens/rules/mod.rs`, `screens/rules/update.rs` | The Rules page (the built-in patterns and what each refused, and the rules in prose, edited in place): its state and the file a save sends, and how a message changes it. |
| `screens/rules/file.rs` | A rules file as text: a rule's section put back, appended or dropped, and what a name may be. |
| `screens/rules/view.rs`, `screens/rules/cards.rs` | The boxes of the page, and the rules in them as cards with the editor a card grows into. |
| `screens/harness_setup.rs` | Setting up a harness configuration repository from the app. |
| `screens/usage/mod.rs` | The Usage page's window, state and layout, and the tokens of today for the sidebar's meter. |
| `screens/usage/figures.rs` | How the page tells its figures: tokens in three figures, counts, shares, times, and the dates of the heatmap. |
| `screens/usage/heatmap.rs` | The heatmap of the last year, and the colour each model keeps on every chart. |
| `screens/usage/boxes.rs` | The tiles of the totals and the boxes of tokens by model, model time, tool waits and agents. |
| `screens/usage/table.rs` | The panel and the table the boxes are made of. |
| `screens/retro.rs` | The Retro page: the Retros of the days, their errors by kind, and their proposals. |
| `screens/experiments/mod.rs`, `screens/experiments/update.rs` | The Experiments page (autoresearch): its state and layout, and how a message changes it. |
| `screens/experiments/brief.rs` | The form of a new brief and the editor of a draft's, with the inputs they share. |
| `screens/experiments/research.rs` | A research's card, brief, baseline and experiments table, with its numbers and changes. |
| `screens/decision_model.rs` | The Decision model page: what was recorded of each kind of decision, the exports, shadowing and takeovers. |
| `screens/setup/mod.rs`, `screens/setup/update.rs`, `screens/setup/view.rs` | The Setup pages (Models, Plugins and Settings, which edit `moebius.toml` through the API), how a message changes them, and how they are drawn. |
| `screens/setup/login.rs` | The OAuth login of an upstream on the Models page: the warning to confirm, the code or the browser, and whether the login holds. |
| `screens/setup/fields.rs`, `screens/setup/forms.rs`, `screens/setup/inputs.rs` | The keys of `moebius.toml` as inputs, the forms made of them with what a save sends, and how each input is drawn. |
| `screens/server_terminal.rs` | The Terminal page: a shell on the server (`GET /v1/terminal`). |

`widgets/`, the pieces screens share:

| Module | What it is for |
|---|---|
| `widgets/mod.rs` | The pieces that more than one screen uses or that stand above every screen. |
| `widgets/page.rs`, `widgets/icons.rs` | The page header and box every page is made of, and the line icons. |
| `widgets/form.rs` | The pieces of the editing pages' forms: labelled inputs, the line after a save, a box's heading, the code editor, the dropdowns' style. |
| `widgets/list_editor.rs` | A list with an editor beside it: its layout, its cards, and the head with Save and Delete. |
| `widgets/conversation.rs` | Folds a run's events into a conversation (text, tool calls with their checks, steers, errors); pure, takes events in any order and drops repeats. |
| `widgets/transcript.rs`, `widgets/steps.rs` | How a conversation is drawn (Markdown, tool calls, highlighted code), and the steps between two answers. |
| `widgets/terminal.rs` | The shell pane: bytes from the server parsed by `alacritty_terminal` into a grid and drawn in JetBrains Mono, keys turned into bytes, the pane's size in cells sent as a resize (`client::terminal`). |
| `widgets/chat_terminal.rs` | A chat's terminal: the same pane on `GET /v1/chats/{id}/tui`, opened by the chat's "Chat \| Terminal" switch. |
| `widgets/float_chat.rs` | The floating chat with the Moebius agent on every screen, given the context of the page. |
| `widgets/inline_edit.rs`, `widgets/edits.rs` | Editing in place: the pencil and its editor, and the editors that belong to no screen (a card's title and body). |
| `widgets/script_editor.rs` | The editor of a column's script, opened by the pencil in a column's header. |
| `widgets/new_issue.rs` | The new-issue form of a board: written by hand or by the Moebius agent. |
| `widgets/import.rs` | Importing skills or MCP servers from a path, a URL or an uploaded zip. |
| `widgets/model_picker.rs` | The model dropdown of the Models and Agents pages, with "Another model…". |
