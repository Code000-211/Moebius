# Architecture

This is how Moebius is put together as built today: the processes, how a card travels from an issue to a merged change,
what the store holds, which code runs on which thread, and what each module is for. Read [`GLOSSARY.md`](../GLOSSARY.md)
for the words and [`spec.md`](spec.md) for what the first version includes. The exact contracts are in
[`reference/`](reference/).

## Processes

```
                  web app / curl
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
- The web app (`web/`) is a client in a browser that talks to the server only through the desktop API (HTTP and the
  event WebSocket). Any HTTP client can stand in for it; see [The web app](#the-web-app).

## The web app

`web/` is a React app (Vite, TypeScript, Tailwind 4, TanStack Query, React Router) built with pnpm into `web/dist`. It is
not a crate, so it has no rows in the module map below; `web/README.md` has its layout. It is a client of the desktop API
and does not know anything the API does not say.

- **Sign-in.** The browser signs in with `POST /v1/session` (the server's token, or the one-time code of the
  `?login=<code>` link the server prints) and then carries an HttpOnly cookie. Every request is same origin and sends
  `X-Moebius: 1`. `GET /v1/session` tells the app whether it is signed in (a 401 shows the sign-in page).
- **Live data.** One WebSocket to `/v1/events` feeds the query cache: an event that changes what the snapshot shows
  invalidates it (the reference says the snapshot is read on every event), `card_waiting` is patched in place, and a
  socket that connects again, after a wait that grows from 1 to 30 seconds, makes every read stale, because events are
  not replayed. A socket that cannot connect asks `GET /v1/session` again: 401 sends the person to the sign-in page, no
  answer shows a banner over what is on screen.
- **Serving.** The server embeds `web/dist` and serves it at `/`, with `index.html` for the paths of the pages. In
  development `pnpm dev` proxies `/v1` to a running server.
- **Components.** `web/src/ui/` holds components copied from Vibe Kanban's `@vibe/ui` (Apache-2.0), rebuilt on the web
  app's own tokens (`web/src/styles/theme.css`), which are its own: the web app is built to `docs/design-web.md`.

## The life of a card

1. **An issue arrives.** Without a forge: `POST /v1/repositories/{id}/issues` (`api::create_issue`) calls
   `App::create_tracked_issue`, which opens the issue (on a forge) and stores its card held in `Backlog`
   (`Store::add_held_card`): the board runs no script for it until its parent, labels and blockers are set, and only
   then is it released and `card_added` published, so no script ever sees it unblocked. A plain
   `App::create_issue` stores a card in `Backlog` (`Store::add_card`) and publishes `card_added`. On GitHub: the
   loop in `Server::serve` calls `poll::poll` with the GitHub client every `github.poll_seconds`, on GitLab with the
   GitLab client every `gitlab.poll_seconds`, and on Forgejo with the Forgejo client every `forgejo.poll_seconds`. That
   fetches the clone (`App::refresh`), lists issues with `GitHub::issues`, `GitLab::issues` or `Forgejo::issues`
   (conditional on the ETag where the forge gives one), turns them into the forge-neutral `forge::Issue` (the client's
   `poll::Polled`), and `App::sync_issues` adds or updates the cards and their "blocked by #n" dependencies
   (`Store::set_dependencies`): a card it adds is held until those dependencies are read, then released and announced. A repository without a forge has no
   poll; the loop in `Server::serve` calls `Server::refresh_plain_repositories` every 30 seconds, which refreshes it
   (`NoForge::refresh` fetches `origin` and fast-forwards the default branch when it can) and publishes a failure as
   `forge_error`.
2. **The board ticks.** The loop in `Server::serve` calls `Server::tick` on a blocking thread every `board.tick_ms`.
   For each card it calls `Engine::settle`, which calls `Engine::tick` until a script stops moving the card.
   `Engine::tick` finds the script for the card's column: the project's `<script>` file, read by
   `App::project_file` from `repos/<repository name>/` of the harness configuration repository's clone, else the built-in one from `kanban/` (`Engine::script`). `Engine::run_script`
   runs it in a fresh sandboxed Luau state with `card_table` and the `moebius` table, and applies the effects.
3. **Backlog to Not started.** `backlog.luau` moves a trusted card on (`App::is_trusted`), and one made from a confirmed grilling (`card.confirmed`). Others get an `approve_issue` inbox
   item (`moebius.ask`) until a person calls `POST /v1/cards/{id}/approve` (`App::approve_issue`), which records a
   judgement by `person` and moves the card to `Not started`.
4. **Not started to In progress.** `ready.luau` waits for blockers and for a free run slot (and never starts an issue labelled `wayfinder:...`, a question), then moves the card.
5. **A run starts.** `code.luau` calls `moebius.start_run("Coder", "code", note)`, which calls `PiBolt::start` (the
   `Runs` trait). It stores the run (`Store::add_run`), makes a worktree on branch `moebius/run-<id>` from the branch of the
   card's latest successful `code` run (`Store::code_branch`), or from the default branch when it has none
   (`Repos::add_worktree`), issues the run's token (`Router::issue_token`, carrying the agent's budget), writes the run's
   agent directory and the bridge to `<data_dir>/runs/<id>/`, spawns Pi-Bolt (`PiBolt::launch`), sets the run `running`,
   publishes `run_started` and sends the prompt (`prompt_for`, `harness/run/prompts.rs`: the issue with its comments, the instructions for the run's purpose, and
   the note if there is one). The server's `Runs` is `Harnesses` (`harness/pick.rs`), which hands the run to the harness
   the agent's `harness` names: an agent on `claude-code` is started by `ClaudeCode::start` the same way (`card_run`
   makes the run, worktree and token for both), with a checked token (`Router::issue_checked_token`), so that the router
   checks the tool calls of its answers (`router/calls/`), since Claude Code has no bridge.
   When the agent names a runner (`agents.<name>.runner`, or the fourth argument of `moebius.start_run`, which reaches the
   harness as the run's `Variant`), `card_run` takes one of the runner's runs first (`Runners::claim`; a full runner makes
   no run) and the run carries a `RemoteRun` (`harness/remote/`). Its launch opens the reverse tunnel to the router
   (`RemoteRun::connect`), pushes the worktree's commit to the runner's clone (`prepare`), writes the agent directory with
   the tunnel's URL in place of the router's, and starts the harness through `RemoteRun::command`, which turns the local
   command into one `ssh` that copies the files it names and runs it in the run's checkout there. Everything after that is
   as for a local run, the reader included. When the run ends `finish` first has the runner commit what the agent left and
   fetches it into the worktree (`RunningRun::bring_back`), so that the commit, the checks and the change are done here.
   When a launcher is configured (`launches = true` on a `[[plugins]]` entry),
   `PiBolt::launch` asks it (`Launcher::start`, `harness/launch.rs`) just before it spawns Pi: "unchanged" starts Pi as
   above, a wrapped launch starts the launcher's program instead, with the launcher's environment and the addresses it
   names written into the agent directory, and `Live` keeps its stop and cleanup commands (`kill_process` runs the stop
   command, `shut_down` the cleanup). A launcher that errors or is too slow fails the run before Pi starts, and a start that fails after it answered
   (`Unstarted`) runs its cleanup; the server
   asks it for leftovers at its start (`Server::new`).
6. **Pi works.** A thread per run (`Reader::read`) reads Pi's records: each is stored and published as `run_event` with
   its `seq` (`App::record_run_event`, which `check::call::ask` and `App::steer_run` use too). Pi asks the model through the router (`Router::forward`), which counts the tokens and may
   refuse with 429.
7. **Every tool call is checked.** The bridge posts to `/moebius/check` (`check::check`). On a blocking thread the
   entry `check::call::ask` builds the `tool_call` question (`call::tool_call_question`, with the applicable rules from
   `rules::applicable`, or the `rules_error` when the server's rules file cannot be read), and `Decisions::decide` asks the `guard` plugin through the `Host` on a thread of its own with
   a time limit. `Guard::call` tests the built-in patterns, then the prose rules with the teacher model (`Teacher::judge`,
   which calls the router with a token for this run; once a takeover of `tool_call` is merged, `DecisionModel::ask` answers the prose rules instead). The answer is stored as one judgement, as a `tool_check` run
   event, and returned. A refusal goes back to the model as the tool's result and the run goes on. A run whose agent has MCP servers reaches
   them only through the MCP gateway (`POST /mcp`, `harness/gateway/`): its `mcp.json` names the gateway with the run's
   token, which `PiBolt::write_agent_dir` gave the agent's servers (`Router::allow_mcp`). The gateway starts or connects
   to them at the run's first request, lists their tools as `<server>__<tool>`, checks every call with
   `check::decide_call` before the server sees it, cleans the result of secrets and records an `mcp_call` run event;
   it stops the servers when the run ends (`Gateway::stop_ended_runs`).
   The question tool (`bridge/moebius-ask.ts`), loaded for the agents that operate Moebius, registers `ask_user_question`, which
   answers at once; the web app draws its arguments as a card and the person's choices come back as their next message.
8. **The run ends.** At `agent_settled`, `Reader::end` closes Pi's standard input, revokes the token, commits what the
   agent left (`Reader::commit`: `git add --all` and a commit by Moebius; for `code` runs, and for chats, see 12b), sets the status (`succeeded`,
   `failed` when a `code` run changed nothing or anything went wrong, `stopped` when aborted), removes the worktree
   (the branch stays) and publishes `run_ended`. The text of the agent's final answer, reported in Pi's `agent_end`
   event, was stored as the run's summary when that event came (`Store::set_run_summary`).
9. **The change is published.** On a later tick `code.luau` sees the succeeded `code` run (and no `card.note`) and moves
   the card to `In review`. `review.luau` calls `moebius.publish_change`, which calls `App::publish_change`: the branch
   `moebius/card-<number>` is pointed at the latest successful `code` run's commit (`Repos::point_branch`; each round's
   branch descends from the last, so the branch only moves forward) and the forge pushes it (`Forge::push`; `NoForge` does nothing). No pull request is opened yet: it opens when the
   review approved the change and `moebius.offer_change` offers it (`Forge::publish`: `GitHubForge` pushes and, the first
   time, opens the pull request; `GitLabForge` the same with a merge request, `ForgejoForge` with a pull request on
   Forgejo). The change is stored
   (`Store::add_change`, later `Store::set_change_head`) with the published commit and time, and `change_updated` is
   published. A card has one change, one branch and one pull request for all its rounds.
    The forge's own checks are read from the pull request, so a forge's checks come after the offer; `merge.luau` sends
    the card back when they fail.
10. **The change is checked.** On GitHub the poll records the check runs of the pull request's head commit, on GitLab
    the latest pipeline of the merge request's head commit and its failed jobs, on Forgejo the commit statuses of the
    pull request's head commit (`poll::poll_requests`, which reads them
    as a `poll::Request` through the client's `Polled::request`, then `App::record_change_status`), with the head they
    are for and the failing ones. Without
    a forge, `moebius.run_checks` calls `App::run_checks`: if the project has a `checks` file, a thread
    runs it with `sh -c` in a throwaway worktree of the change's commit (`App::check_change`, `execute`) with the time
    limit `checks.timeout_seconds`, and records `success` or `failure` with the end of the output, redacted
    (`Store::set_local_checks`). `App::checks_of` gives scripts `card.change.checks`: checks for another commit than the
    published one do not count, and on a forge the change is, once it has a pull request, `pending` for a grace period after the publish and then
    `none`. `App::conflict_of` asks `git merge-tree` whether the change still merges into the default branch
    (`Repos::conflicts`).
11. **The change is sent back or reviewed.** `review.luau` waits while the checks are `pending`. Failing checks or a
    conflict move the card back to `In progress` with a note (`moebius.move(column, note)`, stored on the card by
    `Engine::tick` as `cards.note`); `code.luau` starts the next `Coder` round on top of the previous branch with the
    note in its prompt (`prompt_for`), and `Store::set_run_status` clears the note when a `code` run succeeds. Then the
    built-in `Reviewer` (on the Coder's model unless it has its own) starts two runs one after the other (purposes `review-standards` and `review-spec`, each by
    the agent `Standards` or `Spec` if there is one) in worktrees on the code run's branch; they commit nothing. Each
    summary holds findings with evidence and ends with `VERDICT: approve` or `VERDICT: changes`; `board::review` drops
    findings without evidence and reads the two into `card.review`, whose `findings` are the blocking ones and
    `judgements` the rest. Blocking findings send the card back as the note; `offer_change` stores the judgements in
    the inbox item.
11a. **A hard card is tried several times.** A repository's own `code.luau` (the Kanban guide has one) may call
    `moebius.start_runs`, which starts n runs with purpose `attempt` through `CardRuns::start` (each in a worktree and
    on a branch of its own, from the same commit, with a model or class from a list in turn) and stores them with
    `Store::add_attempt`. Each succeeded attempt is judged by `moebius.check_attempt` (`App::check_attempt`: the
    repository's checks and, test first, the reproduction test, on a thread of its own, the result kept in `attempts`).
    `moebius.settle_attempts` ends the round: with one attempt that passed it is picked at once, with several a run with
    purpose `review-select` of the Reviewer reads their diffs and ends with `PICK: <run id>` (`card.selection`), and with
    none the round counts as failed (`card.rounds`) and its note goes to the next. The picked attempt's run becomes a
    `code` run (`Store::settle_attempts`), so `Store::code_branch` and the review build on it; the others keep their
    branches. The router's budget of the card counts every attempt's tokens.
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
    `POST /v1/runs/{id}/stop` (`App::steer_run`, `App::stop_run`) go through the harness (`CardRuns::steer`, `CardRuns::stop`,
    which `ApiState` holds), a steer also leaving a `steered` run event. A stopped run ends `stopped` and
    `card.stopped` makes the scripts mark the card stuck instead of starting another run.
12aa. **A person, or the agent, holds a card, and a person deletes one.** `POST /v1/cards/{id}/block` (`App::block_card`)
    sets `cards.held_by`, `held_reason` and `held_at` (`Card::held`) and stops the card's runs that have not ended with
    `App::stop_run` (a run still starting is tried again for a few seconds; `board::may_start` makes every starter,
    and the harness once the run is recorded, refuse a card held meanwhile); `Engine::tick` returns at once for a held card, before its script is found or called, so nothing
    moves it or starts a run until `POST /v1/cards/{id}/unblock` (`App::unblock_card`) clears the columns (and, when the block stopped the card's latest run, starts it afresh as a retry does). Both are
    decisions (`block_card`, `unblock_card`) and publish `card_updated`. `DELETE /v1/cards/{id}` (`App::delete_card`,
    for a person only) holds the card, stops its runs and waits for them to end, removes its worktrees and `moebius/`
    branches from the clone (`Repos::remove_card_work`), then `Store::delete_card` removes every record of the card in
    one transaction (the statements it shares with `Store::delete_repository`). On a forge the card's number goes
    into `deleted_cards`, which `App::sync_issues` consults so that the poll never makes the card again. It records
    `delete_card` and publishes `card_deleted`.
12ab. **Issues are a tracker.** An issue has a parent (`cards.parent`, an issue number of the same repository), blockers
    (`dependencies`), labels (`card_labels`), a claim (`cards.claimed_by`), comments (`card_comments`) and a state
    (`cards.closed`, which with a card in `Done` makes `Card::closed`), all in `store/issues.rs`. `App::set_parent`,
    `set_labels`, `add_blocker` (refusing a cycle), `remove_blocker`, `claim_issue`, `release_issue`, `comment_issue`,
    `close_issue` and `reopen_issue` (`app/issues/`) tell the forge first, through the `Forge` trait (`set_issue_labels`,
    `set_issue_blocker`, `set_issue_parent` where `has_sub_issues`, `comment_issue`, `set_issue_open`; GitLab and Forgejo
    keep a parent as a line `Part of #n` of the body), then store, then publish an event of their own
    (`card_parent_changed`, `card_labels_changed`, `card_blocking_changed`, `card_claimed`, `card_released`,
    `card_commented`, `card_closed`, `card_reopened`). The poll reads the forge's side back: the clients put labels, a
    parent and a comment count in the forge-neutral `Issue`, `poll_issues` reads the comments and (`Polled::blockers`,
    kept per issue while it is unchanged) the blockers of the issues it syncs, and `App::sync_issue_details` copies it
    all and announces what changed. It acts only on what the forge changed since it was last seen to say it
    (`forge_seen`, recorded once it is applied, and set to what Moebius wrote after a write), so a snapshot read before
    a write of Moebius's own ended changes nothing (`App::stale_snapshot`); `poll_issues` also reads the blockers it
    remembers again once old, when the forge listed nothing. Only issues a person can have vouched for count as
    sub-issues (`App::counts_as_sub_issue`, which the snapshot's cards say too), and edges that would make issues wait
    for each other are refused (`waits_for`) or, from a forge, ignored with a `forge_error`. The routes are in `server/api/issues.rs`; `GET /v1/repositories/{id}/issues` filters. A column
    script sees `card.parent`, `card.children`, `card.labels` and `card.claimed_by`, and `moebius.close_issue` closes the
    issue when the script returns (`Effects::close`); the default `ready.luau` uses both for a parent.
12b. **A person chats.** A chat is a run without a card and with purpose `chat`. `POST /v1/chats` (`App::start_chat`)
    calls `PiBolt::start_chat`: the run (`Store::add_run` with no card, and `Store::set_run_repository` when it works
    in a repository) gets a worktree on `moebius/chat-<id>` from the repository's base ref (`Repos::add_chat_worktree`),
    or an empty directory `<data_dir>/chats/<id>`, and a token whose budget is the agent's `token_budget` for the run
    alone. The first message is stored as a `said` run event and sent to Pi as the prompt, without an issue prompt around
    it. The same `Reader` reads a chat, with one difference: at `agent_settled` the chat does not end but becomes
    `waiting` (`running` again at the next `agent_start`). `POST /v1/chats/{id}/messages` (`App::say_in_chat`) stores a
    `said` event and sends a `prompt` with `streamingBehavior: "followUp"` (`ChatRuns::say`), which Pi starts or queues
    depending on whether the agent is working. A message to a chat that is `stopped` or `failed`, whose agent went away, takes that
    chat up again instead of losing it: `App::wake_chat` ends it no longer (`Store::reopen_chat`) and
    `ChatRuns::wake` makes its worktree again on the branch it already has (`Repos::reuse_chat_worktree`) and starts Pi
    with what was said in the chat before (`lifecycle::woken_chat`), capped the way a continuing chat's conversation is. A blank message (the web app's Retry) stores nothing and asks the agent to answer again, warned that uncommitted work of a failed turn is gone. Steer and stop are the run's own routes; a stop of a chat also closes Pi's
    standard input, since a waiting chat has no turn to abort. `POST /v1/chats/{id}/end` (`App::end_chat`,
    `ChatRuns::end`) closes Pi's standard input. When Pi has exited the chat is committed on its branch (nothing changed is
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
    `ChatRuns::say` starts its Pi again with `--continue` before the message, and an end or stop finishes it at once.
    A chat can work in the harness configuration repository (`harness: true`; the run's `harness_repository`): its worktree
    is a chat worktree of the harness clone (`HarnessRepo::add_chat_worktree`), and what it leaves is checked as a Retro
    proposal is (`HarnessRepo::check_chat_branch`, which shares `check_branch` with `proposed`) before `App::offer_chat_change`
    stores a change with a null repository (`Store::add_harness_change`; `harness_repository` set). A branch that fails the
    check is not offered and the chat gets a `moebius_error`. `App::merge_change` merges such a change with
    `HarnessRepo::merge_branch`, which `merge_proposal` uses too.
12c0. **Lessons.** `retro::conclude` stores the `lessons` block of the analyst's answer (`lessons::store_written`). `prompt_with`
    (`harness/run/prompts.rs`) puts the best matches for the issue's text under "Lessons from earlier cards" in a code or
    review prompt (`lessons::for_prompt`, FTS5 BM25 in `store/lessons.rs`); the bridge's tool `lessons` asks
    `/moebius/lessons` (`lessons::tool`). `server/schedule.rs` retires the lessons not retrieved for 60 days, and
    `trial::card::run_trial` those given to a negative trial run.
12c. **The Retro.** Once a minute `server/schedule.rs` asks `retro::on_schedule`, which starts today's Retro when `retro_at` has passed
    and the `retros` table has no row for the day (`retro::due`); `POST /v1/retros` calls `retro::start` too, and `POST /v1/retros/{date}/retry` calls `retro::retry`, which claims a `failed` day with `Store::claim_failed_retro` and analyses its stored report again. `retro::report`
    reads the day's errors from the store, one `Store` function per source (failed and refused tool calls, failed and stopped
    runs, steering, cards sent back from review), into the stored report. With errors and a harness repository,
    `ImproveRuns::start_retro` (`PiBolt::start_retro`) starts a `retro` run of the built-in `Retro-analyst` in a detached worktree of
    the harness clone, with the raw traces of the report's runs (`retro::traces`: each run's events and the diff of what it
    made, redacted, at most 20 runs) written under `.retro/` of the worktree and left out of git through the clone's
    `info/exclude`; a thread waits for the run, then `HarnessRepo::proposed` checks, pushes and returns the
    `proposal/<date>-*` branches, which are stored in `proposals` with the cards of their `Cards:` line, and `retro_done`
    is published. Before that, `HarnessRepo::take_own_branches` reads and deletes the branches that change only files
    under `moebius/`, the analyst's proposals for Moebius's own configuration (what it is told about them is
    `own::brief`); `own::store` checks each (a `moebius.toml` change through `Live::preview`, a project-settings change with
    the rules check and `check_script`) and stores it in `proposals` as `settings` or `repository` with its diff
    (`own::unified`), `untried`, with a `proposal` inbox item at once. They are never tried: `trial::merge` hands them
    to `own::merge`, which saves a `settings` one with `Live::save` (the merge's route then restarts the server when a
    key needs it) and commits a `repository` one on `moebius/proposal-<id>` of the repository and offers it with
    `App::offer_branch_change`; `own::dismiss` changes nothing.
12d. **The trials.** The same minute, `server/schedule.rs` asks `trial::on_schedule`, which inside `[trial_window]` (`trial::in_window`)
    starts the oldest `open` proposal when no other is `trying` and the board has a run to spare; `POST
    /v1/proposals/{id}/trial` calls `trial::start` at any hour. A thread tries the proposal on each card: the agent is read
    from the proposal's branch in a throwaway worktree (`HarnessRepo::agent_on_branch`), `ImproveRuns::start_trial`
    (`PiBolt::start_trial`) starts a `trial` run of it on the card's task on a branch of its own, `App::local_checks` runs the
    repository's checks on the result, and `trial::compare` sets the measurements against the card's first code run. The
    trial is stored in `trials`, its branch and worktrees are removed (`trial_done`), and the proposal becomes `positive`,
    `negative` or `untried` (`proposal_status`). Before that, unless the own trials made it negative or `[regression]`
    turns the gate off, `trial::regression` tries it the same way (`run_trial`) on the regression set of each repository of
    its cards: the latest merged cards (`Store::merged_card_changes`), those whose code runs name a reproduction test (`REPRODUCE:` in the summary)
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
    otherwise `ImproveRuns::start_research` (`PiBolt::start_research`) starts a `research` run of the built-in `Autoresearcher`
    in a detached worktree of the repository, which leaves `research/<id>-<name>` branches. Each new branch with a commit is
    measured, checked by `App::local_checks`, judged by `research::judge` (better than the baseline by more than 1 %, checks
    passing or none), stored in `experiments` (`experiment_done`) and, when kept, offered by `App::offer_branch_change`, the
    function that also offers a chat's work. A round without new branches ends the research `done`; a failing baseline ends
    it `failed`; `research::stop` ends it `stopped` and stops its run. Every status change is `research_status`. A server
    start sends `running` research back to `confirmed`. `trial::on_schedule` waits while a research runs.
12g. **Grilling.** A grilling is a chat (12b) with the built-in agent `Grilling-partner` (`pibolt::GRILLING_AGENT`);
    `App::start_chat` refuses it without a repository. `Reader::end` treats it differently from other chats: nothing is
    committed (the folder `.moebius/` is ignored by git in its worktree, `grilling::ignore_result_folder`), and before the worktree is removed `grilling::conclude` reads `.moebius/grilling/issues.json` or `report.md`
    (`grilling::read_result`), stores a row of `grillings` (`proposed`, `reported` or `empty`), adds the inbox item
    `grilling_issues` or `grilling_report` and publishes `grilling_ended`; `offer_chat_change` is not called.
    `POST /v1/grillings/{id}/confirm` calls `grilling::confirm`: `grilling::creation_order` puts blockers first, and
    `App::create_confirmed_issue` makes each card (on GitHub after `Forge::create_issue`, which opens the issue with a
    `Blocked by` line, through `GitHub::create_issue`) with `Store::add_confirmed_card`, which stores the card as
    `confirmed` together with its dependencies. `backlog.luau` moves such a card to Not started (`card.confirmed`); a
    `confirm_grilling` judgement by `person` is recorded and the inbox item resolved.
12h. **Automations.** `POST /v1/automations` (`App::save_automation`) checks the schedule (`cron::parse`), the agent
    (`CardRuns::has_agent`) and the place, and stores the automation (`automations` table) with `checked_at`, the local
    minute it was made. Once a minute `server/schedule.rs` reads the server's local minute (`Store::local_minute`, SQLite's
    `localtime`) and asks `automations::on_schedule`, which first ends each automation's chat that has answered and waits
    (`Store::waiting_automation_chats`, `App::end_chat`, so its work is offered as for any chat), then fires each
    automation that is on and whose schedule came due after its `checked_at` (at most five minutes back) and up to now,
    and sets every `checked_at` to now. A firing (`fire`, also `POST /v1/automations/{id}/run`) is skipped while the chat
    of the automation's last started firing has neither ended nor answered; otherwise it calls `App::start_chat` with the
    automation's agent, place and prompt and the origin `automation`. Each firing is stored (`firings` table: `started`
    with its chat, `skipped` or `failed` with the reason) and published as `automation_fired`.
12i. **The workflow schedules.** The same minute, `server/schedule.rs` asks `schedule::on_workflow_schedules`, which reads
    every workflow file the harness configuration repository holds, one at a time (`workflows::definition::read`), and
    for each one whose trigger is a `schedule` (`workflows/trigger/schedule.rs`) asks whether its cron came due after
    the minute that workflow last fired at (`Store::schedule_firing`, at most five minutes back) and up to now. A file
    that cannot be read wakes no schedule of its own and is published as `retro_error`, while the schedules of the
    other workflows go on. The minute it fired at is recorded (`Store::record_schedule_firing`) before the run starts,
    so one occurrence starts one run across a restart; the run goes through `App::start_workflow_run` with the
    repository the trigger names and the value the firing gives (`fired` and the inputs' defaults). A firing that
    starts no run is written to the log and spends the occurrence, so a workflow is not started every minute it is
    awake.
13. **The forge reports.** `poll::poll_requests` reads each open change's pull or merge request: checks and review
    (on GitLab, whether anyone approved; on Forgejo, the latest review given) are recorded (`change_updated`), a merged one calls `App::change_merged`, a
    closed one calls `App::change_closed`.
14. **A person merges.** `POST /v1/changes/{id}/merge` calls `App::merge_change`: it records a judgement by `person`,
    the forge merges (`Repos::merge`, a `--no-ff` merge, `GitHub::merge_pull_request`, `GitLab::merge` or
    `Forgejo::merge`), and `App::change_merged`
    marks the change merged, resolves the inbox, publishes `change_merged` and moves the card to `Done`.
15. **Done.** `done.luau` calls `moebius.record_outcome`, with `merged` when a change of the card was merged (`card.merged`)
    and `closed` otherwise, so every judgement made for the card gets an outcome.

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
| `card_labels`, `card_comments` | The labels of a card's issue; its comments, each with the id it has on the forge when it has one. `cards` also has `parent`, `closed`, `claimed_by` and `claimed_at`. |
| `changes` | A card's, chat's or workflow run's branch (`moebius/card-<number>`, `moebius/chat-<run id>` or `moebius/workflow-run-<run id>`): repository (none, with `harness_repository` set, for the harness configuration repository), card or workflow run, state (`open`, `merged`, `closed`), pull request number, the published commit and time, checks with the commit they are for, failing checks, output of the repository's own checks, review. |
| `runs` | One per harness run: card (none for a chat), agent, harness, purpose, status, worktree, repository (a chat's) or whether it works in the harness configuration repository, start and end, summary (the agent's final answer). |
| `events` | Everything a run emitted, numbered per run, as JSON. |
| `workflow_runs` | One per run of a workflow: workflow name, repository, trigger kind, status (`queued`, `running`, `waiting`, `succeeded`, `failed` or `cancelled`), the run's value as JSON text, when it was stored, started and finished, and its error. |
| `workflow_node_runs` | One per attempt of a node in a workflow run: run, node id, attempt, status, input and output as JSON text, when it started and finished, and its error. |
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
- Each Pi run has one reader thread (`harness/pibolt/reader.rs`), and `CardRuns::stop` (`PiBolt`) starts a short thread that kills Pi when it does not
  settle in time.
- The store is one SQLite connection behind a mutex. Calls are short and take it one at a time.
- The event bus is a tokio broadcast channel of 1024 events; a slow listener loses the oldest and is told with `lagged`.

## Known limits

- The server is documented and tested on Linux and WSL 2. It builds and starts on Windows (`platform.rs` keeps what
  differs), with these limits there: a repository's own checks, replays and research measures run `sh` and `bash`, and
  say that they need Linux or WSL when those are missing; a stopped program's processes are found by `taskkill /T`,
  which misses one whose parent has already ended; files with secrets keep the permissions of their directory instead
  of mode `0600`; skills are copied into a Claude Code run instead of linked; the terminal pages open PowerShell. The
  web app and the `moebius` commands that operate a server have no such limits.
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
| `cli/client.rs` | Where the server is (`--server` and `--token`, the saved `cli.toml`, `moebius.toml`), the one request a command makes, and sending it with the token. |
| `cli/remote.rs` | Setting a server up on a host over SSH from the terminal, for `connect <host>`: the system, `moebius` found or installed (or replaced when older) from the release of the same version and checked against its `SHA256SUMS`, `moebius init`, the server started (a systemd user service of its own, else detached), its token awaited, each step one `ssh` call over one shared connection and nothing changed before a yes; a server that runs already is never set up over. |
| `cli/remote_tests.rs` | The tests of that setup, against a fake `ssh` that runs the script on this machine with a home directory of its own. |
| `cli/ssh.rs` | A server reached over SSH: one `ssh -L` that also reads the token, for `--ssh <host>` and `connect <host>`, setting the server up first when none answers, waiting until the forwarded port answers, and the `connected.toml` that `connect` leaves for the commands that follow. |
| `cli/window.rs` | The web app as a window of its own: finds a Chromium-based browser, builds its app-mode arguments with a profile folder of its own and starts it, or the default browser; for `open` and `connect`. |
| `cli/api.rs` | `api`: any route, with a method and a JSON body. |
| `cli/board.rs` | `board`, `card`, `inbox` (and `inbox act`, as the app's buttons) and the `issue` commands (create, list, show, edit, close, reopen, claim, release, labels, parent, blocker, comment). |
| `cli/chats.rs` | `chats`, `chat` (start, say, offer, end), `grillings` and `grilling`. |
| `cli/automations.rs` | `automation` (list, save, run, delete). |
| `cli/agents.rs` | `agents` and `agent` (show, save, delete). |
| `cli/rules.rs` | `rules` and `rules save`. |
| `cli/workflow.rs` | `workflow list`, `show`, `save` and `delete`. |
| `cli/packages.rs` | `pkg search`, `info`, `install`, `update`, `remove` and `list`, and how their answers are written. |
| `cli/plugins.rs` | `plugin enable` and `plugin disable [--unwrap-runs] [--allow-unchecked]`: turning a plugin off or on, and how the answer is written. |
| `cli/ext.rs` | `ext search`, `info`, `add`, `update`, `remove` and `list`: Pi packages, and how a proposed change is written. |
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
| `launcher.rs` | The launcher kind: the request a harness process is started with, the answer (unchanged or a wrapped launch with its stop and cleanup commands), the leftovers, and `Host::launch` and `Host::leftovers`. |

### `crates/moebius-core` (the server)

At the top:

| Module | What it is for |
|---|---|
| `blobs.rs` | The content-addressed blob store and secret redaction. |
| `bus.rs` | The event bus behind the event stream. |
| `lib.rs` | The crate's parts, and `VERSION`. |
| `platform.rs` | What differs between Linux and Windows: process groups and killing them, files only their owner reads, linking a skill, the shell of a terminal, the computer's name, and the error of a feature that needs a Unix shell. |

`app/`, the core put together:

| Module | What it is for |
|---|---|
| `app/mod.rs` | `App`: the store, bus, repositories, forges and decisions put together, its errors and the lookups every operation shares; every change to a card is published on the bus. |
| `app/repositories.rs` | Adding a repository (a path, a URL or GitHub), whom a GitHub repository trusts, and whether a card's issue is trusted. |
| `app/cards.rs` | Cards and their issues: kept in line with the forge's issues, created, edited, approved, retried and moved; why a card waits, what blocks it, its history. |
| `app/issues/mod.rs` | The issue tracker: parents and sub-issues, blocking that is written as well as read (cycles refused), labels, claims, comments, closing and reopening, creating an issue with all of that, and the queries over them. |
| `app/issues/sync.rs` | What the poll copies besides an issue's text, acting only on what the forge changed; which sub-issues count; the graph of what waits for what. |
| `app/issues/text.rs` | The lines of a description that hold a parent ("Part of #n") or a blocker ("Blocked by #n") where the forge has none. |
| `app/stopping.rs` | Holding (blocking), releasing and deleting a card, which stops its runs and, for a delete, removes what it left in the clone. |
| `app/runs.rs` | A person's hand in a run: steering, stopping, a shell in its worktree, a chat's terminal interface, and a shell on the server. |
| `app/automations/mod.rs` | Automations: saving and listing them with their next firing, firing one (a chat with origin `automation`, or a skipped or failed firing), and the check once a minute that ends their answered chats and fires those that came due. |
| `app/automations/cron.rs` | A schedule: a cron expression of five fields in the server's local time, parsed, and the next minute it comes due. |
| `app/chats.rs` | Chats: starting one, continuing an ended one, writing to it, its model and reasoning, ending it, and the list of chats. |
| `app/deletions.rs` | Deleting a chat: a chat that is going is stopped, then it is taken out of the store with its changes, inbox items and grillings, and its worktree and branch are removed; also the stop-and-wait that research and grillings share. |
| `app/diffs.rs` | What a change or a run changed, as a unified diff (secrets removed, cut at 2 MB) for a client to draw. |
| `app/changes.rs` | Changes: a card's work published through the forge, offered with a chat's work in the inbox, rejected or merged, and what the forge says became of them. |
| `app/checks.rs` | How a change's checks stand, and a repository's own checks (the project's `checks`) run in a throwaway worktree with a time limit and a cleared environment. |
| `app/board.rs` | What a board shows of a card, worked out once for the snapshot and the scripts: rounds, latest run, and the batched reads of a whole board. |
| `app/attempts.rs` | Attempts: judging each attempt of a hard card by the repository's checks and its reproduction test on a thread of its own, settling a round (one picked, or none passed), and reading a selection run's pick. |
| `app/projects.rs` | A project's settings read from the harness configuration repository (`App::project_file`), and the notice, once for each repository, that its default branch still has a `.moebius/` folder, which is not read. |
| `app/localize.rs` | The hint of files a code run is given, whether a repository wants it, and the outcome recorded from the files the card's change touched at the end of a code run and on merge. |
| `app/plugin_settings.rs` | The settings plugins ask of each repository and of the server in their manifest: what the repository's page and the Settings page show, a person's save checked against the manifest and recorded; a repository's values are files of its project settings, the server's are kept by plugin name. |
| `app/packages/mod.rs` | Pi packages as the API offers them: the catalog search, a package's info, the installed packages, and the branches that install, update and remove one offered as changes. |
| `app/packages/updates.rs` | The nightly check of the installed packages for newer versions, and the `extension_update` inbox items. |
| `app/whereabouts.rs` | Where Moebius runs: what the server found about itself at start, what the web app reported about itself, and the "Where Moebius runs" section of the Moebius agent's instructions built from both (with the official repository). In memory only. |
| `app/reproduce.rs` | Test first: a repository's opt-in (the project's `test-first`), what a run names in the `REPRODUCE:` lines of its summary, and the runs of that reproduction test on the change and on the commit it starts from with the test's files added. |
| `app/workflows/mod.rs` | The wiring that turns a stored workflow into a run, recording the run and its nodes in the store and on the bus. |
| `app/workflows/run.rs` | Starting a workflow run from its definition or its name, driving it off the request path, and recording and publishing each node and the run's end as they happen. |
| `app/workflows/approval.rs` | The part of a run that waits for a person: the wait of the `approval` node, the decision coming back through the resume route, and driving again the runs a stopped server left waiting. |
| `app/workflows/stopping.rs` | Cancelling a workflow run: the run's cancellation flag, what each status gives up, and ending the run `cancelled` once, whoever gets there first. |

`config/`, `moebius.toml`:

| Module | What it is for |
|---|---|
| `config/mod.rs` | `Config`: the file parsed, every default applied, every path made absolute, and the errors that refuse it. |
| `config/example.rs` | The commented example `moebius init` writes, every key at its default. |
| `config/raw.rs` | The file as it is written: every key optional, unknown keys refused. |
| `config/registries.rs` | The package registries from `[[registries]]`, with the default one first. |
| `config/sections.rs` | The types of the sections (the router's upstreams, the GitHub App or access token, the teacher, the decision model, the agents) and their checks. |

`router/`, the model provider:

| Module | What it is for |
|---|---|
| `router/mod.rs` | The router, its upstreams and its routes `POST /v1/chat/completions` and `POST /v1/messages`. |
| `router/proxy.rs` | A run's request forwarded to the upstream with the provider's key or login, and the answer back, streamed or not. |
| `router/classes.rs` | Model classes (`[router.classes]`): a class named as a model is served by the first of its models within its day's tokens, on the first upstream of the protocol that answers, falling back while an upstream fails before it answers. |
| `router/responses/mod.rs` | The Codex Responses protocol of a ChatGPT subscription: the chat completions request turned into a Responses request with the headers the ChatGPT backend needs (the account read from the token), and the subscription's models. |
| `router/responses/answer.rs` | The Responses events of the answer turned back into chat completions, streamed or whole: text, tool calls, finish reason, usage, errors. |
| `router/responses/tests.rs` | Tests of the translation against a fake Codex endpoint. |
| `router/usage.rs` | The tokens and tools of an answer, read from a body or from a stream as it passes, recorded against the run. |
| `router/grants.rs` | The runs' tokens and what each allows: the card's, the run's and the day's token budgets, the MCP servers the gateway gives the run, and which tokens' answers are checked. |
| `router/calls/mod.rs` | The check of the tool calls in the answers to a harness without a bridge (Claude Code): each call through `decide("tool_call")`, a refused one replaced by a text naming the rule; whole answers, and the secrets of tool results in requests. |
| `router/calls/stream.rs` | A streamed answer checked as it passes: a tool call's events held back until the call is complete and decided. |
| `router/models.rs` | The models the upstreams serve and the model classes, for the pickers of the web app, and the reasoning level names of each model: what its endpoint lists, or what `[router.model_thinking]` gives it, which a run's `models.json` is given. |
| `router/oauth/mod.rs` | The OAuth login of an upstream: its `oauth` table, the warning confirmed before a login, the access token refreshed before it expires and the refresh token kept in `secrets.toml`. |
| `router/oauth/device.rs` | The device authorization grant: the code the person enters at the provider, and the token URL polled until it is confirmed. |
| `router/oauth/code.rs` | An authorization code with PKCE, the browser sent back to a loopback listener. |
| `router/oauth/config.rs` | The `oauth` table as configured, filled in from its `preset`, and the warning confirmed before a login. |
| `router/oauth/preset.rs` | The built-in providers (Anthropic, OpenAI): client ids, URLs, and what their requests need (headers, system prefix, or the Codex Responses protocol). |
| `router/oauth/anthropic.rs` | What an Anthropic subscription token needs on `POST /v1/messages`: the `x-anthropic-billing-header` block, section-aware sanitization of Pi and Pi-Bolt system prompts, and raising the Claude Code version floor on `claude_code_version_too_old`. |
| `router/oauth/flows.rs` | Tests of the presets against a fake provider. |
| `router/oauth/testing.rs` | The fake OAuth provider and upstream the tests of the logins share. |

`store/`, the records:

| Module | What it is for |
|---|---|
| `store/mod.rs` | The store: opening the SQLite database, its migrations, the connection and the server's local clock, the helpers that read rows, and every record type re-exported as `store::<Type>`. |
| `store/cards.rs` | Repositories, their cards, whether a card is held, the issues that block a card, the moves of a card between columns, deleting a card or a repository with all it has, and the numbers of deleted cards. |
| `store/issues.rs` | What an issue has besides its text: its parent, labels, claim, whether it was closed, and its comments. |
| `store/changes.rs` | Changes: a branch offered for merging, with its pull request, checks and review. |
| `store/attempts.rs` | Attempts: the runs a card was tried with at once, how each fared, which was picked and why, and the rounds of which none passed. |
| `store/runs.rs` | Runs: an agent's work in a harness, from its start to its end. |
| `store/workflows.rs` | Workflow runs and the runs of each node of them, with their value, input and output redacted before they are stored. |
| `store/workflow_schedules.rs` | The minute each workflow on a `schedule` trigger last fired at, so one occurrence of its cron starts one run even when the server restarts in between. |
| `store/events.rs` | A run's events, in order, redacted before they are stored. |
| `store/automations.rs` | Automations, their firings with the status of each chat, and the server's local minute. |
| `store/chats.rs` | Chats: where they were started, what they work in and how far a person has read them. |
| `store/deletions.rs` | Deleting a chat, a research or a replay from the store in one transaction each; usage and judgements stay, without a run. |
| `store/localize.rs` | The search index of `localize`: a repository's files in an FTS5 table of their own, split into words for ranking by BM25, and the commit it was made from. |
| `store/plugin_settings.rs` | The values of plugins' settings for the server, by plugin and key; a save replaces a plugin's values whole. A repository's are files of its project settings. |
| `store/judgements.rs` | Judgements at decision points, their outcomes, and the refusals counted by rule. |
| `store/replays.rs` | Replays of refused tool calls and the refusals still to replay. |
| `store/inbox.rs` | The inbox: what waits for a person, until it is resolved. |
| `store/grillings.rs` | What a grilling chat ended in: the issues it proposes or its report. |
| `store/shadows.rs` | The decision model's shadows, how well they agree, and the takeovers proposed from them. |
| `store/usage.rs` | The usage of every model call and what it adds up to by card, day, model, agent, activity and tool. |
| `store/lessons.rs` | Lessons of the Retro per repository with their source cards, an FTS5 index kept by triggers, the record of which card was given which lesson, and retirement. |
| `store/found.rs` | What went wrong in a window of time, as the Retro collects it. |
| `store/retros.rs` | The Retro of each day, its proposals and their trials. |
| `store/research.rs` | Research: its brief, its runs and the experiments they try. |

`server/`, serving it all:

| Module | What it is for |
|---|---|
| `server/mod.rs` | Wires everything from the configuration and serves the desktop API, the router and the loops. |
| `server/leftovers.rs` | What a server that stopped left going, ended at the next start: its runs failed and their worktrees removed, its Retros, research and trials set back. |
| `server/loops.rs` | The board's tick, the polls of GitHub, GitLab and Forgejo (also at once when a webhook wakes one), the refresh of the clones every 30 seconds, and the hour's ask of the upstreams for the reasoning level names their models take. |
| `server/plugins.rs` | Starting the plugins in their own process that `[[plugins]]` names, with their standard error in `<data_dir>/plugins/<name>.log`, redacted; holding one that is off with the manifest it last sent (`<data_dir>/plugins/<name>.manifest.json`); routing the deciders and building the launchers from the plugins that are on; and turning a plugin off or on while the server runs (`set_enabled`). |
| `server/packages/mod.rs` | The package manager: the errors, the registries' catalog, the choice of a version, and search, info, list, install, update and remove (see `docs/reference/packages.md`). |
| `server/packages/version.rs` | Versions and the ranges of Moebius versions a package works with. |
| `server/packages/manifest.rs` | A package as `moebius-package.toml` and an entry of `index.toml` say it, and the checks of an entry. |
| `server/packages/registry.rs` | Reading a registry: cloning or fetching its git repository and parsing its `index.toml` as a whole. |
| `server/packages/lock.rs` | `<data_dir>/packages.lock`, what is installed. |
| `server/packages/download.rs` | Downloading what a package installs and checking its sha256 before anything is written. |
| `server/packages/plugin.rs` | Installing, updating and removing a plugin: its program, its `[[plugins]]` entry through the live settings, and starting it in the plugin host. |
| `server/packages/propose.rs` | Kanban (`workflow`), rules and skills packages: their files proposed as a change to a repository or to the harness configuration repository. |
| `server/packages/check.rs` | The nightly look for newer versions of the installed packages, and the inbox items it adds. |
| `server/schedule.rs` | Once a minute: the Retro, trials, research, replays, automations and the workflows on a `schedule` trigger when they are due, and the pause of chats that wait idle. |
| `server/fake_github.rs` | For the tests only: a fake GitHub with one repository, and a server that polls it. |
| `server/fake_gitlab.rs` | For the tests only: a fake GitLab with one project, a server that polls it, and the tests of the GitLab forge through it. |
| `server/fake_forgejo.rs` | For the tests only: a fake Forgejo with one repository, a server that polls it, and the tests of the Forgejo forge through it. |
| `server/api/mod.rs` | The desktop API: the state it needs, the token check, the router that merges the routes of its parts, how a failure becomes a status, and `GET /v1/health`. |
| `server/api/cards.rs` | The snapshot of every board, issues, cards and their changes (edit, approve, block, unblock, delete, retry, merge, reject), a card's runs and history. |
| `server/api/issues.rs` | The issue tracker's routes: the issue list and one issue, parent, labels, blockers, claim, close and reopen, comments. |
| `server/api/repositories.rs` | Adding and editing repositories, and their column scripts and files. |
| `server/api/runs.rs` | A run and its events in pages, steering and stopping it. |
| `server/api/automations.rs` | Automations: list, add, read, replace, delete, run now, and their firings. |
| `server/api/chats.rs` | Chats with an agent (start, write, offer, end, seen, settings) and what grilling chats ended in. |
| `server/api/harness.rs` | The agents, skills and MCP servers of the harness configuration repository, and `POST /v1/harness/setup`. |
| `server/api/rules.rs` | The rules page and the rules in prose. |
| `server/api/workflows.rs` | The workflows of the harness configuration repository: list, read, save and delete, saving and deleting as commits, a person's decision on a waiting run, and cancelling a run of one. |
| `server/api/workflow_hooks.rs` | `POST /api/hooks/{path}` and `POST /api/hooks/{repository}/{path}`, the door a workflow's `webhook` trigger answers on: the credential it asks for, the run its body starts, and its ceiling of runs going at once. |
| `server/api/workflow_runs.rs` | `GET /v1/workflows/{name}/run-inputs` and `POST /v1/workflows/{name}/runs`: the inputs a manual trigger names, and starting a run by hand with them. |
| `server/api/settings.rs` | `moebius.toml`, the API keys and the restart, and what the Models, Plugins and Get started pages show. |
| `server/api/lessons.rs` | The lessons: list, read, edit and delete. |
| `server/api/extensions.rs` | `/v1/extensions/...`: search, info, the installed packages, and proposals to install, update and remove; and the refusal of an agent's merge of a change that touches a package. |
| `server/api/improve.rs` | Usage, judgements and replays, the Retro and its proposals, the decision model and its takeovers, research and exports. |
| `server/api/whereabouts.rs` | `GET /v1/server`, and `GET` and `PUT /v1/client`: where the server runs and what the app reports; a person's token only for the report. |
| `server/api/uploads.rs` | `POST /v1/uploads`: zip files for the Moebius agent to import from. |
| `server/ports/mod.rs` | The ports that processes of runs, chats and terminals listen on: whose each is, and the watch that publishes `port_opened` and `port_closed` every 3 seconds. |
| `server/ports/proc.rs` | Reading the listening sockets, their holders and the process tree from `/proc` (Linux; nothing elsewhere). |
| `server/api/packages.rs` | The package manager's routes: search, list, info, install, update and remove. |
| `server/api/ports.rs` | `GET /v1/ports` and the WebSocket tunnel to a port of the server's loopback address. |
| `server/api/terminals.rs` | The terminal WebSockets: a shell in a run's worktree or on the server, and Pi's terminal interface of a chat. |
| `server/api/scope.rs` | What a Grilling partner's run token reaches in the issue tracker: its chat's repository and the cards with a `wayfinder:` label. |
| `server/api/session.rs` | Signing in from a browser: the sessions in memory, the one-time login codes, and `/v1/session` and `/v1/session/codes`. |
| `server/api/web.rs` | The web app embedded from `web/dist` and served at `/` without a token, with `index.html` for any path that is no file, or a page that says it was not built. |
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
| `repos/forge.rs` | The `Forge` trait with the forges `none`, `github`, `gitlab` and `forgejo`, and the forge-neutral `Issue` (with its labels, parent, blockers and comments). |
| `repos/tracker_tests.rs` | Tests (only) of what the forge clients write and read for an issue tracker, against a fake forge that records its requests. |
| `repos/github.rs` | The GitHub App client: JWT, installation tokens, issues newest-updated first with ETags (paging stops at the last poll's newest), pull requests, checks, reviews, merge. |
| `repos/gitlab.rs` | The GitLab client: the access token, issues newest-updated first with ETags (paging stops at the last poll's newest), merge requests, pipelines and their failed jobs, approvals, merge. |
| `repos/forgejo.rs` | The Forgejo client (Codeberg, Forgejo, Gitea): the access token, issues with ETags where given, pull requests, commit statuses, reviews, merge. |
| `repos/poll/mod.rs` | The poll of the forges: the loop over their repositories, issues into cards, and what became of each change (`Polled`, `Request`). |
| `repos/poll/github.rs` | What GitHub answers, read for the poll: issues, a pull request with its check runs and latest review. |
| `repos/poll/gitlab.rs` | What GitLab answers, read for the poll: issues, a merge request with its latest pipeline, failed jobs and approvals. |
| `repos/poll/forgejo.rs` | What Forgejo answers, read for the poll: issues, a pull request with the statuses of its head commit and its latest review. |
| `repos/settings.rs` | A project's settings other than the column scripts, for `GET /v1/repositories/{id}/settings` and its `PUT` and `DELETE` per file: listed, checked and committed to the harness configuration repository, with the `.moebius/` files still on the default branch. |
| `repos/files.rs` | A repository's files on its default branch, read and committed (the Files sidebar and editor tabs of a project). |
| `repos/search.rs` | A repository's default branch searched for the project page: the list of its paths for "Go to file", and a full-text search with `git grep`. |

`board/`, the Kanban:

| Module | What it is for |
|---|---|
| `board/mod.rs` | What the board folder holds, and the paths the rest of the server uses. |
| `board/engine.rs` | The board engine: runs the script of a card's column (`tick`, which skips a held card, and `settle`) and applies what it did. |
| `board/api.rs` | The `moebius` table, the functions a script acts through. |
| `board/review.rs` | The review along two axes: the findings with evidence of the Standards and Spec runs read into one verdict. |
| `board/attempts.rs` | `moebius.start_runs`, `moebius.check_attempt` and `moebius.settle_attempts`, and the `card.attempts` and `card.selection` fields. |
| `board/card.rs` | The `card` table a script reads, with the card's change and the verdict of its review. |
| `board/sandbox.rs` | The Luau sandbox every script runs in, with its memory and time limits. |
| `board/limits.rs` | The run, round and token limits scripts see, and a script call's memory and time. |
| `board/kanban.rs` | The columns, the default Kanban's scripts built into the server, and a repository's overrides. |
| `board/runs.rs` | What the engine needs of a harness (`Runs`), and the agents it lists. |
| `board/testing.rs` | For the tests only: a fake harness and a store with a repository and a card. |
| `board/dry_run.rs` | `check_script`: a script run once against a made-up card, for `moebius check-script`. |
| `board/scripts.rs` | Editing a column's script: the script in force, saved as a commit in the harness configuration repository. |

`decide/`, every decision on the way:

| Module | What it is for |
|---|---|
| `decide/mod.rs` | Decision points: routing a question to a decider, failing closed, recording the judgement. |
| `decide/check/mod.rs` | The tool-call check's endpoint `/moebius/check` on the router's listener: the token, the parse; the MCP gateway decides its calls with the same question. |
| `decide/check/call.rs` | The one entry that decides a tool call (`decide_call`): the `tool_call` question with the applicable rules, `Decisions::decide` for the order (fixed rules, guard, decision model, shadowing and takeover), the recorded decider, and the `tool_check` run event. |
| `decide/guard/mod.rs` | The `guard` decider: built-in rules first, then the rules in prose for the teacher or the decision model. |
| `decide/guard/shell.rs` | How the guard reads a tool call: simple commands, redirects, nested shells, `cd`, and paths. |
| `decide/guard/patterns.rs` | The built-in rules, fixed patterns over a tool call. |
| `decide/guard/reversible.rs` | Whether a tool call can be replayed: its effects stay in the worktree, and a `bash` command names no absolute path. |
| `decide/guard/teacher.rs` | The teacher model: what it is told, the request through the router, and reading its answer. |
| `decide/localize/mod.rs` | The decision point `localize`: the ranked list of files as a choice, the outcome of a hint with its recall and precision, and the baseline decider that answers it. |
| `decide/localize/index.rs` | The search index of a repository, read from the files of its default branch and made again when the branch moves. |
| `decide/localize/rank.rs` | The baseline's ranking: BM25 for the issue's words, boosts for the paths and identifiers it names and for files earlier cards with similar issues changed. |
| `decide/localize/text.rs` | Reading text for the search: identifiers split on camelCase and snake_case, the issue's terms, and the identifiers and file names it names. |
| `decide/model/localize.rs` | How the decision model's answers to `localize` are compared: its recall at ten against the baseline's over the cards whose change is known, and the takeover that follows. |
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
| `harness/pibolt/mod.rs` | The Pi-Bolt harness: what it holds of every run's Pi, and `CardRuns`, `ChatRuns` and `ImproveRuns` for it, steering and stopping runs. |
| `harness/pibolt/process.rs` | Starting Pi: its agent directory (`models.json`, `mcp.json`), its command line and its process. |
| `harness/pibolt/reader.rs` | Reading Pi's records as run events, and ending the run. |
| `harness/pibolt/chat.rs` | Chats: their Pi between messages, their settings, and pausing one that waited long. |
| `harness/pibolt/tui.rs` | A waiting chat in Pi's interactive mode, and following its session file. |
| `harness/pibolt/testing.rs` | For the tests only: the fake Pi, a server set up for it, and reading what Pi was started with. |
| `harness/run/mod.rs` | The life of a run, the same for every harness: what the folder holds and re-exports. |
| `harness/run/agents.rs` | The built-in agents (the Moebius agent, the Grilling partner, the Coder, the Reviewer, the Retro analyst, the Autoresearcher) with their instructions and skills, and the agent a run uses, on either harness. |
| `harness/run/coding_agents.rs` | The built-in instructions of the Coder and the Reviewer, which complement the per-run prompts of `prompts.rs`. |
| `harness/run/prompts.rs` | What a run of a card is told: the issue and its comments, the instructions of its purpose, the files likely to matter, the lessons of earlier cards and the notes of the last round. |
| `harness/run/lifecycle.rs` | Making a run, its worktree and its token (`card_run`, `detached_run`, `chat_run`), undoing them when its process cannot be started (`launch_or_undo`), and `RunningRun`, which commits what the agent left and removes the worktree. |
| `harness/run/start.rs` | Starting a run: the harness is asked only to start its process (`Launching`, `Launch`) in the run `lifecycle` made. |
| `harness/run/end.rs` | Ending a run: its records kept as its events and summary (`keep`), and its end (`finish`). |
| `harness/run/testing.rs` | For the tests only: a server with one repository and both harnesses on the fake of each process, and ways to wait for a run. |
| `harness/run/tests.rs` | For the tests only: the run lifecycle tests, the same against Pi-Bolt and Claude Code. |
| `harness/claude/mod.rs` | The Claude Code harness: what it holds of every run's process, and `CardRuns`, `ChatRuns` and `ImproveRuns` for it: card runs, trials, the Retro analyst, the Autoresearcher and chats, steered with a user message and stopped by killing it. |
| `harness/claude/process.rs` | Starting Claude Code: its own configuration directory with the agent's skills, `mcp.json`, its command line and environment (the router and the run's token) and its process. |
| `harness/claude/reader.rs` | Reading Claude Code's `stream-json` records as the run events a Pi-Bolt run has, and ending the run. |
| `harness/claude/tests.rs` | For the tests only: Claude Code runs and chats against the fake `claude`, and the router's check of its calls. |
| `harness/pibolt/launcher_tests.rs` | For the tests only: Pi through a fake launcher, compiled in and in its own process (`testdata/fake-launcher`). |
| `harness/pibolt/remote_tests.rs` | For the tests only: runs on a runner over the fake `ssh` of `testdata/`. |
| `harness/launch.rs` | The launcher of harness processes (`Launcher`, from the `[[plugins]]` entry with `launches = true`): asks it how to start a process, builds the wrapped command, runs its stop and cleanup commands, and removes what earlier runs left; any failure is an error that fails the run. `LauncherSlot` holds the launchers the harnesses share, built again when a launcher plugin is turned off or on; a run reads them once, as it starts. |
| `harness/remote/mod.rs` | The runners (`[[runners]]`): which machines there are and how many runs go on each (`Runners`, a `Slot` per run, the list `GET /v1/runners` answers). |
| `harness/remote/session.rs` | One run on a runner over SSH (`RemoteRun`): the reverse tunnel to the router, the run's branch pushed to the machine's clone and checked out, the harness's command line turned into an `ssh` command with the files it names copied over, and the commits fetched back. |
| `harness/pick.rs` | `Harnesses`, the server's `Runs`: Pi-Bolt and Claude Code side by side, a lookup from an agent to the harness it runs on. |
| `harness/terminal.rs` | A person's shell in a run's worktree on a pseudo-terminal (`portable-pty`), served over the WebSocket `GET /v1/runs/{id}/terminal` (and `GET /v1/terminal`, in the directory of `moebius.toml`, and `GET /v1/repositories/{id}/terminal`, in a repository's clone); at most 4 per run or on the server, not checked by the guard. The same serving carries Pi-Bolt's interactive mode for a waiting chat (`GET /v1/chats/{id}/tui`, one per chat), whose tool calls the bridge still checks; `harness/pibolt/tui.rs` ends the chat's own Pi for it, continues the chat's Pi session in it, records what it adds to the session file as the chat's events, and starts the chat's Pi again on the same session when it closes. |
| `harness/repo/mod.rs` | The harness configuration repository's clone: opened, refreshed, and its agents read again when it changes. |
| `harness/repo/read.rs` | Reading the agents, skills and MCP servers from it, each agent checked as a run uses it. |
| `harness/repo/offer.rs` | Files written on a branch of the default branch and offered as a change in the inbox; the package manager proposes skills with it. |
| `harness/repo/projects.rs` | A project's settings, the files of `repos/<repository name>/`: read from the clone, and written or removed as commits like any edit. |
| `harness/repo/edits.rs` | A person's edits of agents, skills, MCP servers and rules, as commits pushed to its origin. |
| `harness/repo/skill_files.rs` | A skill's other files, beside its `SKILL.md`: read, and written or deleted as commits like any edit. |
| `harness/repo/packages.rs` | The Pi packages in the clone: the lock of the default branch, the agents that name a package, the branch that installs, updates or removes one (made in a throwaway worktree), and its file diff. |
| `harness/packages/mod.rs` | Pi packages vendored into the harness configuration repository (`packages/<name>/`, `packages.lock`): the folder's parts and `PackagesError`. |
| `harness/packages/source.rs` | The `npm:` and `git:` sources as `pi install` takes them, and the versions an npm source may ask for. |
| `harness/packages/lock.rs` | `packages.lock`: read, written by name. |
| `harness/packages/manifest.rs` | A package's resources, from its `pi` manifest or the convention directories, with the glob patterns and exclusions of the manifest. |
| `harness/packages/registry.rs` | The npm registry (`[packages] npm_registry`): the catalog search, a version's metadata and the tarball. |
| `harness/packages/fetch.rs` | A package's files from a source into a directory, the integrity check, `npm install --ignore-scripts` for the dependencies, and whether a newer version exists. |
| `harness/packages/changelog.rs` | The sections of a package's changelog between two versions. |
| `harness/packages/resolve.rs` | The `pkg:` names of an agent's `extensions` and `skills`, resolved from `packages.lock` to files. |
| `harness/repo/branches.rs` | Its branches: the Retro's proposals and chats' branches checked, pushed, merged or dismissed, the Retro's branches for Moebius's own configuration read and deleted, and a trial's worktree of a branch. |
| `harness/repo/create.rs` | `create`: a new repository from the agents and rules of `moebius.toml` (`POST /v1/harness/setup`, `moebius init`). |
| `harness/mcp.rs` | The MCP servers of `mcp.toml`, checked when read, their `${NAME}` values filled in, and the `mcp.json` that names the MCP gateway for a run of an agent that names them. |
| `harness/gateway/mod.rs` | The MCP gateway, `POST /mcp` on the router's listener: a run's servers bundled as one MCP server, every tool call checked, its result cleaned of secrets and recorded as an `mcp_call` run event. |
| `harness/gateway/upstream.rs` | One server of a run as the gateway reaches it: a stdio child process or a remote server over streamable HTTP, initialized and its tools listed. |
| `harness/skill.rs` | The built-in skills, bundled with `include_str!` and written to `<data_dir>/skills/<name>/` at start: the `moebius` skill (the guide and the references) and the task skills of the Moebius agent (`moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions`, `moebius-plugins` and `moebius-report`, in `crates/moebius-core/skill/<name>/SKILL.md`), and the engineering skills copied unchanged from Matt Pocock's skills (`crates/moebius-core/skill/vendor/mattpocock/`, MIT, pinned to a commit) that the Coder, the Reviewer and the Grilling partner are given with `moebius-autonomous`, which says what to do where the skills ask a person. |
| `harness/grilling.rs` | The Grilling partner's end: the issues or report it left in its worktree, confirming the issues as cards (on GitHub opened there first) and dismissing. |

`improve/`, learning from what was recorded:

| Module | What it is for |
|---|---|
| `improve/mod.rs` | What the improve folder holds. |
| `improve/retro/mod.rs` | The Retro: the day's errors collected into a report without a model, the Retro analyst's run, and its proposal branches. |
| `improve/lessons/mod.rs` | Lessons: read from the analyst's answer and stored, the prompt section of a code or review run (`for_prompt`), retirement of unused and harmful ones. |
| `improve/lessons/tool.rs` | `POST /moebius/lessons` on the router's listener, the route of the Pi tool `lessons(query)`, authenticated by the run's token. |
| `improve/retro/traces.rs` | The raw traces the Retro analyst reads besides the report: the events and diffs of the report's runs, redacted and bounded, as files under `.retro/` of its worktree. |
| `improve/trial/mod.rs` | Trials: the trial window, and what the parts share. |
| `improve/trial/verdict.rs` | What a trial run and the card's first code run measured, whether a trial is positive, and the status a proposal gets from its trials. |
| `improve/trial/card.rs` | One card's trial: its task run again by its agent with the definition from the proposal's branch, then measured and compared, its worktree and branch removed. |
| `improve/trial/regression.rs` | The regression set of a repository (its latest merged cards, those with a reproduction test first) and the trials of a proposal on it. |
| `improve/trial/schedule.rs` | Trying a proposal on a thread of its own, card after card, each trial stored and published, its status and inbox item; and the schedule's start inside the trial window. |
| `improve/trial/decision.rs` | A person's merge or dismissal of a positive or untried proposal. |
| `improve/own/mod.rs` | The Retro's proposals for Moebius's own configuration (`moebius.toml` and a project's settings): the keys and files it may change, and what the analyst is told about them. |
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
| `improve/research/remove.rs` | Deleting a research: stopped first if it runs, then its experiments, runs, changes, lever branches and worktrees. |
| `improve/research/measure.rs` | A brief's measure run in a fresh worktree with a cleared environment and a time limit, the last number it printed, and whether a lever beats the baseline by enough to be kept. |
| `improve/research/rounds.rs` | The thread that drives a research: its baseline, the Autoresearcher's rounds, each new lever measured and kept or offered, and the schedule's start of the oldest confirmed research. |
| `improve/export.rs` | The training-set export: judgements with their contexts and outcomes as JSON Lines per kind, the held-out tenth, the exports a person started in the app. |

`workflows/`, the graphs a person keeps in the harness configuration:

| Module | What it is for |
|---|---|
| `workflows/mod.rs` | What the workflows folder holds, and what the rest of the crate uses from it. |
| `workflows/decorate.rs` | The expressions in a node's parameters: evaluating `{{ ... }}` in the sandbox column scripts use, with the trigger, the node's input and earlier nodes in scope. |
| `workflows/definition.rs` | The workflow file (`workflows/<name>.toml`): its trigger, nodes and edges, parsing it whole, validating its graph, and reading and listing it from the harness configuration repository; saving and deleting it are commits in `harness/repo/edits.rs`. |
| `workflows/nodes/mod.rs` | The node kinds: the `Node` trait, the ports and metadata a kind reports, the value and port a node returns, and the registry that maps a kind name and its parameters to a node. |
| `workflows/nodes/agent/mod.rs` | The `agent` node: its kind, its parameters, its ports and metadata, and the value it takes from the fenced JSON block the agent ends with. |
| `workflows/nodes/agent/run.rs` | Running an agent node: the workflow run's repository checked out into the worktree the run owns, the prompt filled from the run, the agent started through the picker, and its transcript kept in its run's log. |
| `workflows/nodes/agent/value.rs` | The value of an agent node: the last fenced JSON block of the agent's final answer, parsed, or a failure closed when the block is missing or malformed. |
| `workflows/nodes/code.rs` | The `code` node: Luau that reshapes the value in the expression sandbox, with the run's scope, and outputs what it returns. |
| `workflows/nodes/condition.rs` | The `condition` node: one Luau expression per named output port, tried in order, the first true one taken, else the fallback port or a failure. |
| `workflows/nodes/for_each.rs` | The `for_each` node: fans a list out over a sub-path once per element, gathers the outputs in order, and checks that the sub-path rejoins where the definition says. |
| `workflows/nodes/approval.rs` | The `approval` node: the prompt, the deadline and what each of the three decisions leaves on the run; the waiting itself belongs to `app/workflows/approval.rs`. |
| `workflows/nodes/http/mod.rs` | The `http` node: the node kind, its configuration from the server, and the run that fills its parameters and fails closed on a refused or failed call. |
| `workflows/nodes/http/params.rs` | Reading the `http` node's parameters into a request, when the file is read and again when the run has filled its expressions. |
| `workflows/nodes/http/policy.rs` | The `http` node's URL policy: `https` or `http`, no user name or password, and only a host the server configuration names. |
| `workflows/nodes/http/call.rs` | The `http` node's request: a named secret resolved and sent, no redirects, a time limit, the answer read as status, headers and body, and failures whose message carries no URL query or secret. |
| `workflows/nodes/merge.rs` | The `merge` node: joins the two branches that reached it and emits what arrived by each side, one side's object or the list of the arrivals. |
| `workflows/nodes/publish.rs` | The `publish` node: the run's branch offered as its change with a title and body filled from the value, the failure closed when the run has no agent node or nothing was committed, and the change's id and kind as its value. |
| `workflows/trigger/mod.rs` | The trigger kinds a workflow file may name, the module that owns each, and how a trigger's `kind` is resolved by its module when one owns it and by the node registry otherwise. |
| `workflows/trigger/manual.rs` | The `manual` trigger: the inputs it names with their types, requiredness, defaults and choices, checked when the file is read, and the value a started run begins with. |
| `workflows/trigger/schedule.rs` | The `schedule` trigger: a cron of five fields in the server's local time, the minute it next comes due, the minute it last fired at, and the value a firing starts a run with; a cron that cannot be read is refused when the file loads, naming the wrong field. |
| `workflows/trigger/webhook.rs` | The `webhook` trigger: the path its workflow answers on, the secret a delivery must carry and the repository its runs belong to, and which workflow a path leads to. |
| `workflows/run.rs` | The executor: following the edges from the trigger with one value, giving each node the trigger's value and the values of the nodes that ran before, taking the edge that leaves by the port a node names, running each node under its timeout, failing the run on a node's error or its node ceiling, stopping where a person cancels the run, and reporting each step to an observer. |

Outside `src/`: `kanban/*.luau` holds the six built-in column scripts, `bridge/moebius-bridge.ts` the Pi extension
loaded into every run, `bridge/moebius-ask.ts` the question tool of the agents that operate Moebius, `skill/` the built-in skills.
