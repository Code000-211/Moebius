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

- **Sign-in.** The browser signs in with `POST /v1/session` (a person's access token, the server's token, or the
  one-time code of the `?login=<code>` link the server prints) and then carries an HttpOnly cookie, whose session the
  store keeps as a hash for 30 days. Every request is same origin and sends
  `X-Moebius: 1`. `GET /v1/session` tells the app whether it is signed in and as whom (a 401 shows the sign-in page).
- **Live data.** One WebSocket to `/v1/events` feeds the query cache: an event that changes what the snapshot shows
  invalidates it (the reference says the snapshot is read on every event), `card_waiting` is patched in place, and a
  socket that connects again, after a wait that grows from 1 to 30 seconds, makes every read stale, because events are
  not replayed (the first connect leaves what was read in the last 5 seconds, and while the socket is up the snapshot is
  not read again when the tab gets the focus). A card named in the address (`/card/KEY`, `?card=`) has its reads started
  beside the snapshot, and the chat sheet is loaded when it is first shown. A socket that cannot connect asks `GET /v1/session` again: 401 sends the person to the sign-in page, no
  answer shows a banner over what is on screen.
- **Serving.** The server embeds `web/dist` and serves it at `/`, with `index.html` for the paths of the pages. In
  development `pnpm dev` proxies `/v1` to a running server. The desktop app embeds the same build and serves it from
  its own address; see [The desktop app](#the-desktop-app).
- **Components.** `web/src/ui/` holds components copied from Vibe Kanban's `@vibe/ui` (Apache-2.0), rebuilt on the web
  app's own tokens (`web/src/styles/theme.css`), which are its own: the web app is built to `docs/design-web.md`.

## The desktop app

`crates/moebius-desktop` (Tauri 2, ADR 0021) shows the web app of its own release in a window without a system frame,
on Linux (an AppImage and a `.deb`) and Windows (an installer):

1. The window opens on the connect page (`connect/`, bundled by Tauri): a server's address and an access token.
2. `server::check` asks `GET /v1/health` with the token. A 401 is a wrong token; a server whose major and minor version
   differ from the app's is refused with the address of the release whose app matches it, since the bundled web app was
   built for the API of its own version.
3. `proxy::start` listens on a free port of the loopback. It serves the web app embedded from `web/dist` (with the
   policies of `web/csp.json`, as the server does) and forwards `/v1/` to the server, HTTP and WebSockets, adding
   `Authorization: Bearer <token>`. The web app talks to its own address as it does when the server serves it, needs no
   cookie of the server and never holds the token. Dropping the proxy stops it and the WebSockets it relays. The proxy
   forwards only requests that carry a cookie with a random key (named after its port), set by the first address the
   window opens, that name the proxy in `Host`, and that have no `Origin` or the
   proxy's own.
4. The rail (`rail::Rail`) remembers the servers: `servers.json` in the app's configuration directory holds each one's
   id, name and address, and the access token is in the system's keychain (Secret Service on Linux, Credential Manager
   on Windows; `rail::secrets`), never in a file. Each server that has been opened has a proxy of its own (its own port,
   cookie and origin) that stays up while another is shown, and the page the window was on when another was opened
   (`Rail::note_page`), which `Proxy::entry_url_at` leads back to. A server over SSH keeps its tunnel
   (`rail::ssh`, the same `ssh -L` as `moebius connect`, in batch mode) beside its proxy; a host with no server is set up
   by `moebius connect --yes` after a yes. `rail::poll` asks each server for its Inbox (`GET /v1/status`, one small route, never the snapshot; the open server is
   not asked while the tray follows it, whose last figures the rail shows), and one that does not answer is drawn with a grey dot. The web app draws the servers in the switch of the sidebar's head (`desktop/ServerSwitch.tsx`)
   and calls the commands of `servers.rs` through the bridge; removing a server there is also how a person disconnects,
   since sign-out in the web app does nothing in the app. At its start the app opens the server that was open last.
   The window (`pages.rs`) holds a webview for the connect page and one for every server that was opened, all of its
   size, and shows one: switching to a server opened before shows its webview with the page as it was left, and a
   server opened for the first time loads out of sight and is shown once it has drawn its rail, so the window is never
   blank. `views::Views` decides which webviews exist and lets go of those left for half an hour and of all but three
   hidden ones (their page is noted by the rail).
5. The window opens that address, and keeps only its own pages: a link to anywhere else, such as a pull request on a
   forge, opens in the system's browser (`links::target`). Every page of the window gets `window.moebiusDesktop` first, which the web app's
   `desktop/WindowControls.tsx` calls to minimize, maximize and close the window from a strip on the textured ground.
5. The global shortcut (`quick`, `quick_window`): `Ctrl+Shift+Space`, or the one a person chose (kept in `quick.json` in
   the app's configuration directory), opens a small window without a frame that stays over the others, even while the
   app is in the back. It shows the web app's `/quick` page from the same proxy (`Proxy::entry_url_at` of the open server sets the cookie
   and leads there), where Enter starts a chat with the Moebius agent or makes an issue in the chosen project through
   the ordinary API. The page then asks the program to hide the window and to send the main window's page an
   `moebius:open` event with the path of the chat or the board, which the web app's router opens
   (`desktop/OpenRequests.tsx`). The small window's page may call only `quick_shortcut`, `set_quick_shortcut` and
   `quick_finish` (`capabilities/quick.json`); a shortcut another program holds is reported there and a person picks
   another.

The connect page also offers a server on this machine (Linux, and Windows inside WSL 2). `local_state` only looks; the
button calls `local_connect`, which has `local::Local::start` bring the release of the app's version into `~/.moebius/bin`
(checked against `SHA256SUMS`), run `init` in `~/.moebius/server`, start `serve` in a process of its own that outlives
the app, and read the token the server writes there; the token goes to the proxy and never to the page. The next start
finds the server again by that directory: it answers, or it is started again without a new `init`.

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
   For each card that `Engine::is_quiet` does not say can be left alone (its last script did nothing, no row was
   written to the store since, the limits are the same and a minute has not passed; `board/cache.rs`) it calls
   `Engine::settle_card`, which calls `Engine::settle`, which calls `Engine::tick` until a script stops moving the card.
   `Engine::tick` finds the script for the card's column: the `<script>` file of the card's project, read by
   `App::project_file` from `projects/<project name>/` of the harness configuration repository's clone, else the built-in one from `kanban/` (`Engine::script`). The column that decides which script runs is the card's own, or, for a column its project's `kanban.toml` does not have or whose script is missing, the one of its class (`Engine::column_of`; `app/columns/`). `Engine::run_script`
   runs it in a fresh sandboxed Luau state with `card_table` and the `moebius` table, and applies the effects.
3. **Backlog to Not started.** `backlog.luau` moves a trusted card on (`App::is_trusted`), and one made from a confirmed grilling (`card.confirmed`). Others get an `approve_issue` inbox
   item (`moebius.ask`) until a person calls `POST /v1/cards/{id}/approve` (`App::approve_issue`), which records a
   judgement by `person` and moves the card to `Not started`.
4. **Not started to In progress.** `ready.luau` waits for blockers and for a free run slot (and never starts an issue labelled `wayfinder:...`, a question), then moves the card.
5. **A run starts.** `code.luau` calls `moebius.start_run("Coder", "code", note)`, which calls `PiBolt::start` (the
   `Runs` trait). It stores the run (`Store::add_run`), makes a worktree on branch `moebius/run-<id>` from the branch of the
   card's latest successful `code` run (`Store::code_branch`), or from the default branch when it has none
   (`Repos::add_worktree`); a card over several repositories (`App::card_repositories`) gets one in each, from its branch
   there (`Store::code_branch_in`), all recorded with `Store::add_run_worktree` so the tool-call check lets the run write
   in each (the context key `worktrees`); then it issues the run's token (`Router::issue_token`, carrying the agent's budget), writes the run's
   agent directory and the bridge to `<data_dir>/runs/<id>/`, spawns Pi-Bolt (`PiBolt::launch`), sets the run `running`,
   publishes `run_started` and sends the prompt (`prompt_for`, `harness/run/prompts/mod.rs`: the issue with its comments, the instructions for the run's purpose, and
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
   (the branch stays) and publishes `run_ended`. A `code` run whose summary asks for another repository
   (`REPOSITORY: <name>`) succeeds with it added to the card and a note that sends the card back to coding
   (`App::end_code_run`). The text of the agent's final answer, reported in Pi's `agent_end`
   event, was stored as the run's summary when that event came (`Store::set_run_summary`).
9. **The change is published.** On a later tick `code.luau` sees the succeeded `code` run (and no `card.note`) and moves
   the card to `In review`. `review.luau` calls `moebius.publish_change`, which calls `App::publish_card_changes`, once
   for each repository the card works in that has work (one change per repository; another than the card's own on
   `moebius/card-<KEY>-<number>`): the branch
   `moebius/card-<number>` is pointed at the latest successful `code` run's commit (`Repos::point_branch`; each round's
   branch descends from the last, so the branch only moves forward) and the forge pushes it (`Forge::push`; `NoForge` does nothing). No pull request is opened yet: it opens when the
   review approved the change and `moebius.offer_change` offers it (`Forge::publish`: `GitHubForge` pushes and, the first
   time, opens the pull request; `GitLabForge` the same with a merge request, `ForgejoForge` with a pull request on
   Forgejo). The change is stored
   (`Store::add_change`, later `Store::set_change_head`) with the published commit and time, and `change_updated` is
   published. A card has one change, one branch and one pull request for all its rounds.
    The forge's own checks are read from the pull request, so a forge's checks come after the offer; `merge.luau` sends
    the card back when they fail.
10. **The change is checked.** On GitHub the poll records the check runs and commit statuses of the pull request's head commit, on GitLab
    the latest pipeline of the merge request's head commit and its failed jobs, on Forgejo the commit statuses of the
    pull request's head commit (`poll::poll_requests`, which reads them
    as a `poll::Request` through the client's `Polled::request`, then `App::record_change_status`), with the head they
    are for and the failing ones. Without
    a forge, `moebius.run_checks` calls `App::run_checks`: if the project has a `checks` file, a thread
    runs it with `sh -c` in a throwaway worktree of the change's commit (`App::check_change`, `execute`) with the time
    limit `checks.timeout_seconds`, and records `success` or `failure` with the end of the output, redacted
    (`Store::set_local_checks`). `App::checks_of` gives scripts the `checks` of each of `card.changes`: checks for another commit than the
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
    `card.stopped` makes the scripts mark the card stuck instead of starting another run. `POST /v1/runs/pause` and `/resume`
    (`App::pause_runs`, `App::resume_runs`) keep a pause in `runs_paused`: `Engine::tick` returns at once while it holds,
    `board::may_start` refuses every starter, and `CardRuns::hold` signals the process groups of the runs going.
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
12ab. **Issues are a tracker.** (Duplicates and related issues are Moebius's own links, `store/links.rs`, made by
    `App::mark_duplicate` and `add_related` in `app/issues/links.rs`; `create_tracked_issue` offers the open issues
    alike in words, `app/issues/similar.rs`, in a comment of the Moebius agent and closes none.) An issue has a parent (`cards.parent`, an issue number of the same repository), blockers
    (`dependencies`), labels (`card_labels`), a claim (`cards.claimed_by`), comments (`card_comments`) and a state
    (`cards.closed`, which with a card in `Done` makes `Card::closed`), all in `store/issues.rs`. `App::set_parent`,
    `set_labels`, `add_blocker` (refusing a cycle), `remove_blocker`, `claim_issue`, `release_issue`, `comment_issue`,
    `close_issue` and `reopen_issue` (`app/issues/`) tell the forge first, through the `Forge` trait (`set_issue_labels`,
    `set_issue_blocker`, `set_issue_parent` where `has_sub_issues`, `comment_issue`, `set_issue_open`; GitLab and Forgejo
    keep a parent as a line `Part of #n` of the body), then store, then publish an event of their own
    (`card_parent_changed`, `card_labels_changed`, `card_blocking_changed`, `card_links_changed`, `card_claimed`, `card_released`,
    `card_commented`, `card_closed`, `card_reopened`). Taking an issue (`claim_issue`, or a column script's `moebius.start_run` or `moebius.start_runs`) also assigns the person's login on the forge (`App::take_issue`, `Forge::assign_issue`), and the column scripts' starters ask `may_take` first, which makes a card taken by someone else on another server, or not labelled where a repository's agents take only labelled issues, wait (`App::left_alone`); the poll records an issue's assignees (`cards.forge_assignees`), keeps a repository's ready label (`App::mark_ready_issues`) and follows the open pull requests of people on other servers (`App::follow_open_requests`; [ADR 0027](adr/0027-working-with-people-on-other-servers-through-the-forge.md)) and, where a repository turned them on, reads `/take`, `/release` and `/done` in the comments it has read (`App::read_commands`, from `sync_comments`; [ADR 0028](adr/0028-helpers-take-issues-with-commands-in-comments.md)). The poll reads the forge's side back: the clients put labels, a
    parent and a comment count in the forge-neutral `Issue`, `poll_issues` reads the comments and (`Polled::blockers`,
    kept per issue while it is unchanged) the blockers of the issues it syncs, and `App::sync_issue_details` copies it
    all and announces what changed. It acts only on what the forge changed since it was last seen to say it
    (`forge_seen`, recorded once it is applied, and set to what Moebius wrote after a write), so a snapshot read before
    a write of Moebius's own ended changes nothing (`App::stale_snapshot`); `poll_issues` also reads the blockers it
    remembers again once old, when the forge listed nothing. Only issues a person can have vouched for count as
    sub-issues (`App::counts_as_sub_issue`, which the snapshot's cards say too), and edges that would make issues wait
    for each other are refused (`waits_for`) or, from a forge, ignored with a `forge_error`. The routes are in `server/api/issues.rs`; `GET /v1/repositories/{id}/issues` filters. A column
    script sees `card.parent`, `card.children`, `card.labels` and `card.claimed_by`, and `moebius.close_issue` closes the
    issue when the script returns (`Effects::close`); the default `ready.luau` uses both for a parent. A card or an inbox item can also be given to people (`App::set_assignees`, `App::set_inbox_assignees`, routes in
    `server/api/assignees.rs`, events `card_assignees_changed` and `inbox_assignees_changed`); a script sees the handles as
    `card.assignees`, and the default `ready.luau` leaves a card that has any in `Not started`. A card's priority and due date are its `priority:` and `due:` labels (`App::set_priority`, `App::set_due`, routes in `server/api/planning.rs`; a poll that brings the labels brings them), scripts see `card.priority` and `card.due`, and the tick settles urgent cards first (`server::loops::settle_order`). A card's estimate in points is its `estimate:` label the same way (`App::set_estimate`, `card.estimate`); `App::propose_estimate` proposes one from the like cards that have one and names them, and the Retro's report lists the cards whose estimate was furthest off from the tokens and time they took (`improve/retro/estimates.rs`). What concerns a person (a
    card given to them, a comment that mentions them with `@handle`, a move or a comment on a card they follow) is a
    notice of theirs, made in `app/notices.rs`, listed under the Inbox's Mentions tab and sent on the stream to them alone. An owner defines kinds of issues with fields of their own, a project picks the ones it offers and a card has one (`App::set_card_kind`, `App::set_card_fields`, routes in `server/api/kinds.rs`, event `card_kind_changed`); a script sees them as `card.kind` and `card.fields`, and `GET /v1/issues` filters and sorts the cards of every project by them.
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
    (`harness/run/prompts/mod.rs`) puts the best matches for the issue's text under "Lessons from earlier cards" in a code or
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
    `mixed`, `negative` or `untried` (`proposal_status`). Before that, unless the own trials made it negative or `[regression]`
    turns the gate off, `trial::regression` tries it the same way (`run_trial`) on the regression set of each repository of
    its cards: the latest merged cards (`Store::merged_card_changes`), those whose code runs name a reproduction test (`REPRODUCE:` in the summary)
    first, each from the commit its first code run started at (`Store::run_base`, recorded by `PiBolt::start_card_run`),
    stopping at the first that fails; these trials are stored with `regression` set. Every proposal but a negative one gets a `proposal` inbox item, a mixed one so that a person weighs its trade-off; `trial::merge`
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
    (`CardRuns::has_agent`) and the place, and writes the automation as the `schedule` workflow with one `chat` node
    (`App::automation_definition`) in the harness configuration repository, keeping its row (`automations` table) for the
    id its firings use; a row saved before that is migrated to a file (`App::migrate_automations`). Once a minute
    `automations::on_schedule` ends each automation's chat that has answered and waits (`Store::waiting_automation_chats`,
    `App::end_chat`, so its work is offered as for any chat). The workflow schedule fires an automation (`fire`, also
    `POST /v1/automations/{id}/run`): skipped while the chat of its last started firing has neither ended nor answered;
    otherwise a repository automation starts a workflow run whose `chat` node starts the chat (`app/workflows/chat.rs`),
    and one that works in the harness configuration repository calls `App::start_chat` directly. A row whose file was
    edited into an ordinary workflow fires from neither its row nor the automation check: that workflow owns its
    schedule. Each firing is stored (`firings` table: `started` with its chat and workflow run, `skipped` or `failed`
    with the reason, also when the run's `chat` node cannot start the chat) and published as `automation_fired`, naming
    the workflow run.
12i. **The workflow schedules.** The same minute, `server/schedule.rs` asks `schedule::on_workflow_schedules`, which reads
    every workflow file in the `projects/<project>/workflows/` folder of each project (ADR 0025), one project and one file
    at a time (`workflows::definition::read`), and for each one whose trigger is a `schedule`
    (`workflows/trigger/schedule.rs`) asks whether its cron came due after the minute that workflow last fired at, kept
    per (project, workflow) by the project's id (`Store::schedule_firing`, at most five minutes back) and up to now. A file
    that cannot be read wakes no schedule of its own and is published as `retro_error`, while the schedules of the
    other workflows go on. The minute it fired at is recorded (`Store::record_project_schedule_firing`) before the run starts,
    so one occurrence starts one run across a restart; the run goes through `App::start_project_workflow_run`, which
    applies the repository rule (the trigger's repository must be one of the project's) and records the project on the
    run, with the value the firing gives (`fired` and the inputs' defaults). A firing that
    starts no run is written to the log and spends the occurrence, so a workflow is not started every minute it is
    awake. An automation is such a workflow, fired here instead of starting a run of its own definition's repository
    when it works in the harness configuration repository.
13. **The forge reports.** `poll::poll_requests` reads each open change's pull or merge request: checks and review
    (on GitLab, whether anyone approved; on Forgejo, the latest review given) are recorded (`change_updated`), a merged one calls `App::change_merged`, a
    closed one calls `App::change_closed`.
14. **A person merges.** `POST /v1/changes/{id}/merge` calls `App::merge_change`: it records a judgement by `person`,
    the forge merges (`Repos::merge`, a `--no-ff` merge, `GitHub::merge_pull_request`, `GitLab::merge` or
    `Forgejo::merge`), and `App::change_merged`
    marks the change merged, resolves the inbox, publishes `change_merged` and moves the card to `Done` once it has no
    other open change. A card's changes merge in the order of its repositories; `moebius.merge_change` calls
    `App::merge_card`, which stops at the first merge that fails and marks the card with `merge_stopped`; a change closed on the forge without being merged marks it the same way and keeps it out of Done.
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
| `cli/login.rs` | `login`: checks a person's access token with the server and saves it, with the address, in `cli.toml`. |
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
| `cli/wiki.rs` | `wiki list`, `wiki search`, `wiki read` and `wiki propose`: the pages of the wiki, and an edit that waits in the inbox. |
| `cli/workflow.rs` | `workflow list`, `show`, `save` and `delete`. |
| `cli/packages.rs` | `pkg search`, `info`, `install`, `update`, `remove` and `list`, and how their answers are written. |
| `cli/plugins.rs` | `plugin enable` and `plugin disable [--unwrap-runs] [--allow-unchecked]`: turning a plugin off or on, and how the answer is written. |
| `cli/ext.rs` | `ext search`, `info`, `add`, `update`, `remove` and `list`: Pi packages, and how a proposed change is written. |
| `cli/repos.rs` | `repos` and `repo add`. |
| `cli/factory.rs` | `backlog` (where a project's backlog is, and the sync), `upkeep` and `digest`: the software factory's commands. |
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

### `crates/moebius-desktop` (the desktop app)

| Module | What it is for |
|---|---|
| `main.rs` | The `moebius-desktop` program, without a console window of its own on Windows. |
| `lib.rs` | The parts tested without a window, and how a window comes to show the web app against one server. |
| `links.rs` | Where an address goes: the window keeps the connect page and the web app on the proxy's origin, and sends forges, chat links and the rest to the system's browser. |
| `local/mod.rs` | A server of this machine that the app sets up and starts (D3): what the machine has (`State`: not set up, stopped, running, unavailable), and `start`, which brings `moebius`, runs `init` where there is no configuration, starts `serve` in the background and waits for its token and its answer. It shares `~/.moebius/bin` and `~/.moebius/server` with `moebius connect`. |
| `local/host.rs` | The system the local server runs on, reached through a POSIX shell: `sh` on Linux, `wsl.exe -e sh` on Windows (WSL 2). |
| `local/release.rs` | The program the app brings: `moebius` of the app's own version downloaded from its release, checked against that release's `SHA256SUMS` and put in `~/.moebius/bin`. |
| `rail/mod.rs` | The rail of servers: the servers the app remembers, a proxy and the page of each that has been opened, adding (by address or over SSH), opening, removing, and `poll`, which says for each whether it answers and the count of its Inbox. |
| `rail/names.rs` | What the rail calls a server: the host it reports or its short address, and the address added when two servers share a name. |
| `rail/records.rs` | `servers.json`: the remembered servers (id, name, address) and the one open last; no token. |
| `rail/secrets.rs` | Where the tokens are kept: the system's keychain (`Keychain`), or `Memory` in tests. |
| `rail/ssh.rs` | A server over SSH: the `ssh` tunnel that forwards a port and prints the host's token, and setting a host up with `moebius connect --yes`. |
| `rail/status.rs` | What the rail shows of a server: whether it answers and its Inbox's count (`Entry`). |
| `frame.rs` | The script every page of the desktop window gets so that a page with round corners squares them while the window is maximized or full screen. |
| `edges.rs` | The script every page of the desktop window gets so that the window, which has no frame of the system, can be resized from its edges and corners. |
| `views.rs` | Which servers have a webview of their own in the window, and which to let go of (the ones left for long, or beyond three): the bookkeeping, tested without a window. |
| `pages.rs` | The Tauri side of the window's webviews, in the program: one for the connect page and one per opened server, one shown at a time, a new one shown once it has drawn its rail. |
| `servers.rs` | The rail's commands on the Tauri side (`connect`, `connect_ssh`, `servers`, `switch_server`, `remove_server`, and the local server's `local_state` and `local_connect`): showing a server's web app in the window, the connect page when none is left, and opening the server that was open last at the start. |
| `server.rs` | A server the app connects to (`Server`: its address and the access token), and `check`: the token and the server's version with `GET /v1/health`, refusing a server of another minor version with the release that matches. |
| `quick.rs` | The global shortcut without a window: which key combinations are accepted and how they are written, the file that keeps the chosen one across restarts, and the script that asks the main window to open a page of the web app. |
| `quick_window.rs` | The Tauri side of the global shortcut, in the program: registering it, the small window over everything that shows the web app's `/quick` page, and the commands that page calls (`quick_shortcut`, `set_quick_shortcut`, `quick_finish`). |
| `window.rs` | The Tauri side, in the program rather than the library so that the library's tests do not link Tauri: the window without a system frame, the bridge `window.moebiusDesktop` every page gets, and the handlers that send a link leaving the app to the browser. |
| `notices.rs` | A person's notices on a server for the app: `watch` follows `/v1/events` (and reads `GET /v1/notices` after each connection) with the token and says, as `Effect`s, to notify a notice once and what the count of unread ones is; the first read is the baseline and notifies nothing; `coalesce` passes the first notice of a burst on at once and the ones after it within five seconds as one `Digest`. |
| `badge.rs` | What the icon says of that count: the title `(3) Moebius`, and the red disc with digits drawn over the taskbar icon on Windows. |
| `notifications.rs` | The Tauri side of notices, in the program: the system's notification (waited on by a task of the runtime for two minutes at most, not by a thread each; a click shows the window and opens the notice in the web app), the badge count of the icon, the overlay and the title. |
| `render.rs` | WebKitGTK's rendering mode on Linux: compositing off where there is no GPU render node, `MOEBIUS_RENDERING` to override. |
| `instance.rs` | Which start of the app is the first: a lock in the configuration directory and a loopback port; a later start tells the first to show its window and ends. |
| `tray.rs` | What the tray shows of a server and asks of it: the figures read from `GET /v1/status` (agents working, the Inbox, today's tokens against the budget, whether all agents are paused), their lines, `follow`, which reads them again after a burst of events (the steps of runs only every ten seconds), and the pause and resume routes. |
| `tray_icon.rs` | The Tauri side of the tray, in the program: the icon and its menu (the figures as lines, New issue, Ask Moebius, Open Moebius, Pause all agents), following the server the window is connected to. |
| `proxy/mod.rs` | The app's own address on the loopback: the web app at every path, `/v1/` forwarded to the server, and the key, `Host` and `Origin` checks that let only the window use it. |
| `proxy/assets.rs` | The web app embedded from `web/dist`, served with the policies of `web/csp.json`. |
| `proxy/forward.rs` | Forwarding to the server with the token: HTTP streamed both ways, and a WebSocket relayed message by message. |

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
| `app/columns/mod.rs` | The columns of a board ([ADR 0030](adr/0030-custom-board-columns-per-project.md)): the six built-in ones, their names, and the class (the built-in column a column counts as) that built-in logic asks for instead of a column's name. |
| `app/columns/layout.rs` | Reading and checking a project's `kanban.toml`: its custom columns, where each goes, the class each counts as, and every mistake of a file said at once. |
| `app/columns/project.rs` | What the app does with a project's `kanban.toml`: the board kept for as long as the file reads the same, the last valid one when it is not, columns the file lost while cards are in them, the class of a column and the check of a save. |
| `app/cards.rs` | Cards and their issues: kept in line with the forge's issues, created, edited, approved, retried and moved; why a card waits, what blocks it, its history. |
| `app/issues/mod.rs` | The issue tracker: parents and sub-issues, blocking that is written as well as read (cycles refused), labels, claims, comments, closing and reopening, creating an issue with all of that, and the queries over them. |
| `app/issues/links.rs` | Duplicates and related issues: marking a duplicate (closing it with a comment that points to the original), relating issues in both directions, and the offer of likely duplicates in a comment when an issue is made. |
| `app/issues/similar.rs` | The words of an issue and how alike two issues are, for finding the open issues that probably say the same. |
| `app/issues/sync.rs` | What the poll copies besides an issue's text, acting only on what the forge changed; which sub-issues count; the graph of what waits for what. |
| `app/issues/text.rs` | The lines of a description that hold a parent ("Part of #n") or a blocker ("Blocked by #n") where the forge has none. |
| `app/snapshot.rs` | What the snapshot needs to know about each card (repositories, people, approvals, dependencies, open changes), read once for all cards. |
| `app/moving.rs` | A person moving a card to another column, as a script moves it: a fresh start that keeps, replaces or clears its note. |
| `app/stopping.rs` | Holding (blocking), releasing and deleting a card, which stops its runs and, for a delete, removes what it left in the clone. |
| `app/backlog.rs` | Backlog sync ([ADR 0032](adr/0032-the-factory-keeps-the-owners-work-small.md)): a project's backlog file read as items, and the issues made, updated, labelled and closed as not planned from them, a few at a time. |
| `app/digest.rs` | The owner's daily digest ([ADR 0032](adr/0032-the-factory-keeps-the-owners-work-small.md)): one inbox item a day with what merged, was refused, reverted, reviewed, taken and released, what the stale handling did, the tokens spent, what is held and what needs a person. |
| `app/upkeep.rs` | Stale handling ([ADR 0032](adr/0032-the-factory-keeps-the-owners-work-small.md)): an issue taken and not worked on is released, a followed pull request nobody pushes to after a review asked for changes is reminded and then closed; never on a held card. |
| `app/flow/mod.rs` | Reviews and measurements that flow without the owner's clicks ([ADR 0033](adr/0033-measurements-and-reviews-flow-without-the-owners-clicks.md)): the flow settings of a repository, `review_on_green` (a round once the checks of a head have been green for a few minutes) and what a new head does to a measurement. |
| `app/flow/measure.rs` | A registered measurer's `## Measurement` comment read once at the poll: who wrote it, which commits and fields it names, the reply that says what is wrong or the round it starts, the accepted measurement and the section the Reviewer is given. |
| `app/flow/committed.rs` | The measurement an author committed with the change, accepted for the head when the repository trusts it and a round of the Reviewer (not a person's) approves a head whose merge writes a file under `author_measurement_paths`. |
| `app/flow/bind.rs` | Binding a pull request with no closing keyword to the one issue its title or branch names by a backlog item's ID, or `#N` of an issue its author is assigned to. |
| `app/flow/unbound.rs` | A pull request that names no issue: asked once to, labelled `unbound`, closed politely after `unbound_close_days`; never in a frozen repository or for the owner's own. |
| `app/flow/status.rs` | Status labels on the forge: one label with the repository's prefix on every open issue that has a card, the column's slug or a finer state (frozen, awaiting measurement, changes requested, taken), swapped in one write after each poll. |
| `app/forms/mod.rs` | Templates for what is written on a forge ([ADR 0034](adr/0034-templates-and-agents-that-work-on-the-forge.md)): the pull request template and the comment templates, which a project replaces in the harness configuration repository. |
| `app/forms/pull.rs` | The pull request template as a Markdown file with fixed headings, a body read into fields by those headings (`Fixes #N`, the sections, the tests checklist, the measurement state, what is missing) and a body written from the template. |
| `app/forms/comments.rs` | The comment templates: which kind a comment is by its first line (`## Measurement`, `/done`, `## Review answer`, `/take`, `/release`), its fields and what it lacks, and the server's own replies in the same form (`## Missing`, `## Posted`). |
| `app/forms/keep.rs` | The templates in effect for a repository (the project's or the shipped ones) and the pull request that keeps the repository's copies in `.github/` current, which waits for the owner's click. |
| `app/forms/own.rs` | Moebius's own agents on the forge: the draft pull request from the first round with a body written from the template, the Reviewer's verdict as a review with comments on lines, the answer to each finding, and taking the pull request out of draft on approval. |
| `app/forms/read.rs` | The pull requests and comments people write, read by their templates at the poll: the fields of a followed pull request, the reply once that names a section it lacks, and the comments that are refused naming the field they lack. |
| `app/commands.rs` | Commands in issue comments ([ADR 0028](adr/0028-helpers-take-issues-with-commands-in-comments.md)): `/take`, `/release` and `/done` read once from the comments the poll read, answered on the issue, limited per login, and the owner's answers to a `/done` and release from the card. |
| `app/reviews.rs` | The Reviewer reads a followed pull request when its author says `/done` ([ADR 0029](adr/0029-the-reviewer-reads-a-pull-request-when-its-author-says-done.md)): rounds that wait for green checks, a review run read as a structured verdict, a review with comments on lines written on the forge, and the merge pinned to the head that was read, or Ready to merge. |
| `app/factory/mod.rs` | The software factory ([ADR 0031](adr/0031-one-merge-gate-and-a-brake-for-the-factory.md)): the settings a repository keeps for it (measurement, locked paths, review brief, testing label, measurers, measurement-free paths) and how an edit is checked, a person accepting a measurement, and the names its parts share. |
| `app/factory/gate.rs` | The merge gate every merge the server makes goes through (`merge_one`): frozen, hold and pause, checks, measurement, locked paths and the rules as the decision `merge_check`; a refusal puts the card in Ready to merge with an Inbox item that names the reason. |
| `app/factory/freeze.rs` | Freezing a repository: by a person, by the label `factory:freeze`, or by a breaker (two reverts in seven days, 80% of the day's tokens, a failed nightly check); what a frozen repository does not do, and who lifts it. |
| `app/factory/paths.rs` | Path globs for the paths that never merge without a person, and the built-in list. |
| `app/lane/mod.rs` | The test lane ([ADR 0035](adr/0035-a-test-lane-measures-on-the-owners-own-gpus.md)): the lanes of the configuration, kept where the app can ask for them, and the modules of the part. |
| `app/lane/definition.rs` | A project's job, `projects/<project>/lane.toml`: steps with commands and time limits, which of them measure, and the checks that refuse a definition as a whole. |
| `app/lane/request.rs` | Queueing a job for a change at its head (a card entering a `lane = true` column, `moebius.lane_job()`, a person, the lane's pass) and everything that stops one: freeze, hold, pause, locked path, no review verdict yet; and the pass that starts the next job of each free lane. |
| `app/lane/exec.rs` | Running a command on a lane's machine, the server or a runner over SSH, with nothing of the server's environment; the arguments of the container every command of an isolated job runs in; sending the pinned commits to the machine. |
| `app/lane/job.rs` | Running one job: the preflight (a missing container runtime refuses the job), the pinned checkouts, the steps, a measuring step's runs of the merge base and the head by turns, and the end of the job. |
| `app/lane/rows.rs` | The rows of a measurement read from a step's output lines or a JSON file it wrote, treated as data. |
| `app/lane/report.rs` | The report of a job (the machine, the steps, every run), the measurement text the Reviewer reads, the section of its prompt, and the one reply written on the pull request when a step fails. |
| `app/lane/results.rs` | What the end of a job does: the failure answered and the card sent back to the author, the measurement handed to the Reviewer by the path of a measurer's comment, a lane that paused itself, and what a new push cancels. |
| `app/lane/view.rs` | The lane as the API shows it: a repository's "Test lane" settings and a card's report. |
| `app/review_sweep.rs` | Ending the rounds of the Reviewer that cannot end by themselves: the sweep of rounds left starting or writing a review, a closed pull request's rounds, and a person's end of a round. |
| `app/review_context.rs` | What the Reviewer of a pull request someone else opened is given to read: the diff and the changed files as of the reviewed commit, built from git objects alone (no checkout, no filters; a changed file too large to give whole is given as the lines of its own part of the diff, and over the total the largest whole files are given so first, sizes read from object headers first), as text for its prompt, and the check that the default branch did not change what the merge does since the review. |
| `app/taking.rs` | Working with people on other servers through the forge ([ADR 0027](adr/0027-working-with-people-on-other-servers-through-the-forge.md)): taking an issue assigns the person on the forge, an issue someone else took is left alone, the ready label kept on the issues that are ready, and the pull requests of people on other servers followed to their merge or close. |
| `app/pausing.rs` | Pausing all agents: the pause kept in the store, which the board engine and every starter of a run read, and the harness holding the runs going. |
| `app/runs.rs` | A person's hand in a run: steering, stopping, a shell in its worktree, a chat's terminal interface, and a shell on the server. |
| `app/automations/mod.rs` | Automations: the automation origin, and the parts that make an automation a workflow, migrate a row to one, fire it and show it. |
| `app/automations/definition.rs` | What makes a workflow an automation, and how an automation's row is read from its file and written to one. |
| `app/automations/firing.rs` | Firing an automation: a workflow run whose `chat` node starts the chat, or a chat with origin `automation` for the harness configuration repository, or a skipped or failed firing; and the check once a minute that ends their answered chats and fires a row no file holds yet. |
| `app/automations/migration.rs` | The upgrade that writes an older automation's row to a workflow file in the harness configuration repository. |
| `app/automations/view.rs` | The automations as the page and the API read them, with their next firing and latest one; and the old routes' save and delete of the workflow file behind a row. |
| `app/automations/cron.rs` | A schedule: a cron expression of five fields in the server's local time, parsed, and the next minute it comes due. |
| `app/planning.rs` | Setting a card's priority, due date and estimate: all are labels (`priority:high`, `due:2026-12-01`, `estimate:5`), so they are written to the forge and announced as labels are; and proposing an estimate from the like cards that have one. |
| `app/notices.rs` | What concerns a person: the `@handle`s a text mentions, the notice each assignment, mention and change to a followed card makes, reading one, and following a card. |
| `app/templates.rs` | Templates for issues and pages: listing and editing them in the harness configuration repository, starting an issue from its kind's template, making a page from a page template. |
| `app/kinds.rs` | Kinds of issues and their fields: what is written is checked, a project picks the kinds it offers, a card gets a kind and values for its fields, and the cards of every project are filtered and sorted by a field. |
| `app/card_repositories.rs` | The repositories a card works in: its own issue's first, the others a person or the Moebius agent names from the project's, and a `code` run's ask for another (`REPOSITORY:`), which adds it and sends the card back to coding. |
| `app/assignees.rs` | Giving a card or an inbox item to people: the handles are checked against the people of the organisation, the set is replaced, and a change is announced. |
| `app/chat_access.rs` | Whose a chat is: its owner reads and writes it, others read it once it is shared, and the lists `mine` and `shared`; the Retro reads the store and so every chat. |
| `app/chats.rs` | Chats: starting one, continuing an ended one, writing to it, its model and reasoning, ending it, and the list of chats. |
| `app/deletions.rs` | Deleting a chat: a chat that is going is stopped, then it is taken out of the store with its changes, inbox items and grillings, and its worktree and branch are removed; also the stop-and-wait that research and grillings share. |
| `app/diffs.rs` | What a change or a run changed, as a unified diff (secrets removed, cut at 2 MB) for a client to draw. |
| `app/changes/mod.rs` | Changes: a card's work and a chat's work offered in the inbox, rejected or merged, and what the forge says became of them. |
| `app/changes/merge.rs` | A card's changes merged together, in the order the card names its repositories; a merge that fails stops there and marks the card with `merge_stopped`. |
| `app/changes/publish.rs` | A card's work published through the forges of its repositories, one change per repository it works in, and offered in the inbox. |
| `app/checks.rs` | How a change's checks stand, and a repository's own checks (the project's `checks`) run in a throwaway worktree with a time limit and a cleared environment. |
| `app/board.rs` | What a board shows of a card, worked out once for the snapshot and the scripts: rounds, latest run, and the batched reads of a whole board. |
| `app/attempts.rs` | Attempts: judging each attempt of a hard card by the repository's checks and its reproduction test on a thread of its own, settling a round (one picked, or none passed), and reading a selection run's pick. |
| `app/projects/mod.rs` | Projects (ADR 0019): making one with a name and a key no other has (the name is also a folder), renaming it with its key kept and its settings moved, and the `ProjectView` the Projects page shows (repositories, members, open cards, whether it is private and the person a member). |
| `app/projects/files.rs` | A project's own store of files (ADR 0019): the git repository at `<data_dir>/projects/<key>` made for a project, kept as a repository without a forge whose `files_of` is the project, where the cards of a project without a repository run, are reviewed and are merged. |
| `app/analytics.rs` | Analytics (#389): throughput, the time from start to done and in each column, tokens per point and the burn-up of cycles and milestones, read from the cards the asker may see and narrowed by window, repository and agent. |
| `app/cycles.rs` | Cycles (ADR 0019): a project's time boxes, a card in at most one open cycle of its own project, the figures and burn-up of a cycle's page, and closing, which carries unfinished cards to the next cycle or out. |
| `app/jira/mod.rs` | The import from Jira (issue #390): what a Jira record becomes in Moebius, the options and the report, and the order the parts run in; `App::import_jira` never talks to Jira. |
| `app/jira/read.rs` | Reads what the client fetched into issues, sprints, links, comments and custom fields, with Jira's timestamps. |
| `app/jira/kinds.rs` | Issue types as kinds and custom fields as their fields; an existing kind is completed. |
| `app/jira/people.rs` | Jira's users as disabled people, mapped people or names. |
| `app/jira/planning.rs` | Epics as milestones and sprints as cycles, and closing a closed sprint's cycle. |
| `app/jira/cards.rs` | Issues as cards, then their parent, milestone, cycle, comments and links; written only when what Jira shows differs from the note kept. |
| `app/jira/tests.rs` | The import against a fake Jira: every record, a second import, names and mapped people. |
| `app/milestones.rs` | Milestones (ADR 0019): making and editing them, putting a card in them, and their progress counted and listed only within the projects the asker may see (`App::may_see_project`). |
| `app/view_filters.rs` | The parts of a saved view's filter that look at a run or at today: the agents that ran on a card, its status and its due range. |
| `app/views.rs` | Saved views: making and changing them for a person or for everyone, and the cards each shows the person who opens it, within what they may see (`App::sight`). |
| `app/intake.rs` | Intake (ADR 0019): a project's requests, written by anyone who may see it, grilled into a draft issue by a chat of the grilling agent and accepted into a card the requester follows, edited, or declined with a reason the requester is told. |
| `app/projects/sight.rs` | Who sees what of a private project (ADR 0018): a person's `Sight` (the private projects they are no member of, and the repositories all of whose projects those are), whether it sees a card, run, lesson or judgement, which lessons a run of a project may find, and a Retro's proposal unquoted for someone who does not see a project it names. |
| `app/projects/settings.rs` | A project's and a repository's settings read from the harness configuration repository (`App::project_file`, `App::repository_file`), the migration of the repositories' column scripts to their home projects (`App::move_scripts_to_projects`), and the notice, once for each repository, that its default branch still has a `.moebius/` folder, which is not read. |
| `app/localize.rs` | The hint of files a code run is given, whether a repository wants it, and the outcome recorded from the files the card's change touched at the end of a code run and on merge. |
| `app/plugin_settings.rs` | The settings plugins ask of each repository and of the server in their manifest: what the repository's page and the Settings page show, a person's save checked against the manifest and recorded; a repository's values are files of its project settings, the server's are kept by plugin name. |
| `app/packages/mod.rs` | Pi packages as the API offers them: the catalog search, a package's info, the installed packages, and the branches that install, update and remove one offered as changes. |
| `app/packages/updates.rs` | The nightly check of the installed packages for newer versions, and the `extension_update` inbox items. |
| `app/whereabouts.rs` | Where Moebius runs: what the server found about itself at start, what the web app reported about itself, and the "Where Moebius runs" section of the Moebius agent's instructions built from both (with the official repository). In memory only. |
| `app/reproduce.rs` | Test first: a repository's opt-in (the project's `test-first`), what a run names in the `REPRODUCE:` lines of its summary, and the runs of that reproduction test on the change and on the commit it starts from with the test's files added. |
| `app/workflows/mod.rs` | The wiring that turns a stored workflow into a run, recording the run and its nodes in the store and on the bus. |
| `app/workflows/run.rs` | Starting a workflow run from its definition or its name, driving it off the request path, and recording and publishing each node and the run's end as they happen. |
| `app/workflows/migrate.rs` | The one-time migration of the server-wide workflow files to their projects (ADR 0025): the repository and home-project rules, one commit through `HarnessRepo::move_workflows_to_projects` (the project each file goes to is recorded first in `workflow_moves`), and the idempotent store step that gives the runs, schedule rows and automations of a moved file to its project, repeated at every start so a crash between the two is repaired. What stays is listed for Get started. |
| `app/workflows/project.rs` | The workflows a project owns (ADR 0025): which repository a run of one belongs to (one of the project's repositories, its own files when it has none, the only one by default, a refusal that names the choices otherwise) and the door that starts it. |
| `app/workflows/schedule.rs` | When a workflow's `schedule` trigger last fired and fires next, as the project's workflow list and a workflow's page show it: the trigger's cron in the server's local time, as the scheduler reads it, and the minute the store says it last fired at. |
| `app/workflows/approval.rs` | The part of a run that waits for a person: the wait of the `approval` node, the decision coming back through the resume route, and driving again the runs a stopped server left waiting. |
| `app/workflows/chat.rs` | The part of a run that starts the chat of a `chat` node, in the run's repository, and records that chat on the firing the run carries. |
| `app/workflows/stopping.rs` | Cancelling a workflow run: the run's cancellation flag, what each status gives up, and ending the run `cancelled` once, whoever gets there first. |

`config/`, `moebius.toml`:

| Module | What it is for |
|---|---|
| `config/mod.rs` | `Config`: the file parsed, every default applied, every path made absolute, and the errors that refuse it. |
| `config/example.rs` | The commented example `moebius init` writes, every key at its default. |
| `config/raw.rs` | The file as it is written: every key optional, unknown keys refused. |
| `config/registries.rs` | The package registries from `[[registries]]`, with the default one first. |
| `config/sections.rs` | The types of the sections (the router's upstreams, the GitHub App or access token, the teacher, the decision model, the agents) and their checks. |
| `config/lanes.rs` | The test lanes (`[[lanes]]`): where each is, how it isolates (`always`, `external_only`, `off`), what a report says of its machine, and their checks. |

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
| `store/column_classes.rs` | The class of each custom column of each project (`column_classes`), kept in step with the project's `kanban.toml`, and `class_sql`, the SQL expression for the class of a card's column, so that a query can ask for it. |
| `store/access.rs` | Access tokens and the sessions of browsers, each kept as the SHA-256 of its secret: making, finding a person by one, the last use, revoking, and ending sessions. |
| `store/planning.rs` | A card's priority and due date, read from its labels and written back into them; it keeps nothing of its own. |
| `store/notices.rs` | The notices of a person with their read mark, and the cards they follow: the tables `notices` and `card_follows`. |
| `store/kinds.rs` | Kinds of issues, their fields, the kinds each project picked and the values cards hold: the tables `kinds`, `kind_fields`, `project_kinds` and `card_fields`, and `cards.kind_id`. |
| `store/card_repositories.rs` | The repositories a card names besides its own (`card_repositories`) and the worktrees a run of it has in each (`run_worktrees`), with the code branch of the card in each repository. |
| `store/assignees.rs` | Who a card or an inbox item is given to: the tables `card_assignees` and `inbox_assignees`, set as a whole and read as handles. |
| `store/people.rs` | The people of the organisation (id, handle, display name, role, disabled), never deleted, and the migration that makes the first owner and gives them the records from before that say `person`. |
| `store/links.rs` | The links between issues besides parent and blockers: a duplicate to its original, and issues related in both directions. |
| `store/issues.rs` | What an issue has besides its text: its parent, labels, claim, whether it was closed, and its comments. |
| `store/changes.rs` | Changes: a branch offered for merging, with its pull request, checks and review. |
| `store/tending.rs` | What the store keeps for the software factory: the log the digest reads, the days a digest was made, when each issue was taken, a repository's stale settings and a project's backlog. |
| `store/commands.rs` | What the store keeps for commands in issue comments: the repository's settings for them, the newest comment read per issue, the login a card was taken by command, and how many issues a login holds. |
| `store/flow.rs` | What keeps measurements and reviews flowing: each repository's flow settings, the measurements a measurer posted as comments, when the checks of a head turned green, and the pull requests that name no issue. |
| `store/lane.rs` | What the test lane keeps: which lane a repository uses, the jobs of every lane with the lock that lets one run at a time, and the lanes that paused themselves. |
| `store/forms.rs` | What Moebius wrote on a forge by its templates (the draft state and parsed fields of a change, and the log of posts that makes each write happen once). |
| `store/factory.rs` | The software factory's state: a repository's factory settings and freeze, the commit a change merged as, accepted measurements, reverts kept once and the breakers that tripped. |
| `store/reviews.rs` | The rounds of the Reviewer on followed pull requests (state, head, verdict, summary, comments on lines, the review's address) and the repository's `merge_reviewed`. |
| `store/taking.rs` | What a server knows of who took an issue: a person's logins on the forges, the logins an issue is assigned to, why an assignment failed, and a repository's settings for working with others. |
| `store/attempts.rs` | Attempts: the runs a card was tried with at once, how each fared, which was picked and why, and the rounds of which none passed. |
| `store/runs.rs` | Runs: an agent's work in a harness, from its start to its end. |
| `store/workflows.rs` | Workflow runs and the runs of each node of them, with their value, input and output redacted before they are stored. |
| `store/workflow_schedules.rs` | The minute each workflow on a `schedule` trigger last fired at, per project, so one occurrence of its cron starts one run even when the server restarts in between. |
| `store/outgoing.rs` | The outgoing webhooks (URL, event kinds, projects named) and the flag that makes a repository private. |
| `store/wiki_search.rs` | The wiki's search index (SQLite FTS5, a title counting far more than the text): derived data, replaced whole when the wiki's head moves. |
| `store/analytics.rs` | What the analytics need of every card (moves, start, done, estimate, tokens) read in a few passes, and the members of every cycle and milestone. |
| `store/cycles.rs` | The cycles (project, name, start, end, closed) and which cards each holds, with the sums a cycle's page needs. |
| `store/wiki_links.rs` | The links between the wiki's pages and to cards, derived from the files and replaced as a whole when the wiki changes, and the cards whose text may link to a page. |
| `store/imports.rs` | What an import made (by source, kind and the source's id) with a note of how the source showed it, so that an import can be run again. |
| `store/milestones.rs` | The milestones (name, target date, description) and which cards each holds, of any projects. |
| `store/views.rs` | The saved views (name, owner, shared, filter, layout, sort). |
| `store/intake.rs` | A project's switch for intake and the requests made to it, with their draft issue, status, card and reason. |
| `store/projects.rs` | The projects (name, key, repositories in order), a repository's home project and a card's project, how a key is made from a name, the migration that makes one project for each repository and puts every card on it, whether a project is private with its members, and the project a judgement belongs to. |
| `store/paused.rs` | Whether all agents are paused, who paused them and when: the table `runs_paused`, one row at most. |
| `store/favourites.rs` | The places a person pinned, in their order, and the places they opened lately (the last 20), kept per person so they follow them to every browser. |
| `store/events.rs` | A run's events, in order, redacted before they are stored: queued and written in batches (streamed deltas wait up to 100 ms), the deltas of an ended message joined, and told to the bus after they are committed. |
| `store/automations.rs` | Automations, their firings with the status of each chat, and the server's local minute; the workflow file is the definition and the row keeps the id the firings use. |
| `store/chats.rs` | Chats: where they were started, what they work in and how far a person has read them. |
| `store/deletions.rs` | Deleting a chat, a research or a replay from the store in one transaction each; usage and judgements stay, without a run. |
| `store/localize.rs` | The search index of `localize`: a repository's files in an FTS5 table of their own, split into words for ranking by BM25, and the commit it was made from. |
| `store/preferences.rs` | What a person chose for themselves in the web app (whether thinking is shown open), one row per person. |
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
| `server/loops.rs` | The board's tick, the polls of GitHub, GitLab and Forgejo (also at once when a webhook wakes one), the test lane's pass every few seconds, the refresh of the clones every 30 seconds, and the hour's ask of the upstreams for the reasoning level names their models take. |
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
| `server/schedule.rs` | Once a minute: the Retro, trials, research, replays, the ending of answered automation chats, the workflows on a `schedule` trigger when they are due (automations included), and the pause of chats that wait idle. |
| `server/fake_github.rs` | For the tests only: a fake GitHub with one repository, and a server that polls it. |
| `server/fake_gitlab.rs` | For the tests only: a fake GitLab with one project, a server that polls it, and the tests of the GitLab forge through it. |
| `server/fake_forgejo.rs` | For the tests only: a fake Forgejo with one repository, a server that polls it, and the tests of the Forgejo forge through it. |
| `server/tending_tests.rs` | For the tests only: the daily digest and its clock, the stale handling and the backlog sync, against the fake GitHub. |
| `server/commands_tests.rs` | For the tests only: commands in issue comments, against the fake GitHub. |
| `server/taking_tests.rs` | For the tests only: working with people on other servers, against the fake GitHub. |
| `server/flow_tests.rs` | For the tests only: measurements read from comments, review on green checks and pull requests that name no issue, against the fake GitHub. |
| `server/lane_tests.rs` | For the tests only: the test lane against the fake GitHub, a fake `ssh` and a fake container runtime that records its arguments. |
| `server/forms_tests.rs` | Tests of [ADR 0034](adr/0034-templates-and-agents-that-work-on-the-forge.md) against the fake GitHub: pull requests read by their template and the reply that names a missing section, the comment templates, the sync of the templates, and the draft pull request, review, answer and end of draft of a card's own work. |
| `server/status_tests.rs` | For the tests only: status labels on issues following moves and the finer states, against the fake GitHub. |
| `server/factory_tests.rs` | For the tests only: the merge gate, the freeze and its breakers, the review brief and `approve_code`, against the fake GitHub. |
| `server/reviews_tests.rs` | For the tests only: the review of a followed pull request after `/done`, against the fake GitHub. |
| `server/api/mod.rs` | The desktop API: the state it needs, the token check, the router that merges the routes of its parts, how a failure becomes a status, and `GET /v1/health`. |
| `server/api/cards.rs` | The snapshot of every board, issues, cards and their changes (edit, approve, block, unblock, delete, retry, merge, reject), a card's runs and history. |
| `server/api/planning.rs` | The routes that set a card's priority and due date. |
| `server/api/notices.rs` | The routes of a person's notices and of following a card. |
| `server/api/status.rs` | `GET /v1/status`: the figures of the desktop app's tray and rail, without the snapshot. |
| `server/api/kinds.rs` | The routes of kinds of issues, the kinds a project offers, a card's kind and fields, and the cards of every project filtered and sorted by them. |
| `server/api/card_repositories.rs` | The route that names the repositories a card works in. |
| `server/api/assignees.rs` | The routes that give a card or an inbox item to people. |
| `server/api/issues.rs` | The issue tracker's routes: the issue list and one issue, parent, labels, blockers, claim, close and reopen, comments. |
| `server/api/factory.rs` | The software factory's routes: a repository's stale settings, a project's backlog and its sync, and making the digest now. |
| `server/api/lane.rs` | The test lane's routes: a repository's lane, the lanes and resuming one, and a card's report. |
| `server/api/repositories.rs` | Adding and editing repositories, and their column scripts and files. |
| `server/api/analytics.rs` | The analytics of a window of days, narrowed by repository and agent. |
| `server/api/cycles.rs` | The cycles: a project's list, making, reading, editing, deleting and closing one, and a card's cycle. |
| `server/api/jira.rs` | `POST /v1/projects/{id}/import/jira`: reads a Jira project with a token kept in `secrets.toml` and imports it. |
| `server/api/milestones.rs` | The milestones: listing them with their progress, making, reading, editing and deleting one, and a card's milestones. |
| `server/api/views.rs` | The saved views: listing, making, reading, editing and deleting one, and the cards it shows. |
| `server/api/projects.rs` | The projects: listing them, making one with a free key, reading and renaming one, the column scripts of a project's board, and making one private with its members. |
| `server/api/intake.rs` | Intake: turning it on for a project, making and reading its requests, editing a draft, accepting and declining. |
| `server/api/templates.rs` | The routes of templates for issues and wiki pages, and making a wiki page from one. |
| `server/api/privacy/mod.rs` | The check, after the role, that answers `404` for a route naming by id a card, run, change, repository, project, lesson or other thing of a private project the person is no member of; and whether an inbox item is seen. |
| `server/api/privacy/tests/mod.rs` | For the tests only: a private project with one thing of every guarded kind and the people who see it or not, every guarded route and list answered for them, and the event stream. |
| `server/api/privacy/tests/body_ids.rs` | For the tests only: a repository of a private project named in a request's body (an automation, a research, a workflow run's input), a Retro started or retried, and a replay deleted, by someone who does not see it. |
| `server/api/privacy/tests/links.rs` | For the tests only: an issue's relations and an estimate's comparison, which never name a card of a private project the asker does not see. |
| `server/api/privacy/tests/counts.rs` | For the tests only: what a private project would show through a list or a count (a kind's projects, the count of a refused kind delete, the Retro list's errors, a card's blockers). |
| `server/api/privacy/tests/stream.rs` | For the tests only: the events that name a private project's things by another field than its card or run, and the notices of a private project's cards. |
| `server/api/wiki.rs` | The wiki's routes: its pages listed, read (an older version too), written, moved and deleted, a page's history, and a refresh now. |
| `server/api/runs.rs` | A run and its events in pages, steering and stopping it. |
| `server/api/automations/mod.rs` | Automations: list, add, read, replace, delete, run now, and their firings. |
| `server/api/automations/tests.rs` | For the tests only: the automations routes, and an older row upgraded to a workflow beside a workflow of the same name. |
| `server/api/chats.rs` | Chats with an agent (start, write, offer, end, seen, settings) and what grilling chats ended in. |
| `server/api/harness.rs` | The agents, skills and MCP servers of the harness configuration repository, and `POST /v1/harness/setup`. |
| `server/api/rules.rs` | The rules page and the rules in prose. |
| `server/api/workflows.rs` | The workflows a project owns, addressed as `/v1/projects/{id}/workflows/...` (the routes that name a workflow without its project answer `410`): list, read, save and delete, saving and deleting as commits, a person's decision on a waiting run, and cancelling a run of one. |
| `server/api/workflow_hooks.rs` | `POST /api/hooks/projects/{project}/{path}` and `POST /api/hooks/projects/{project}/{repository}/{path}` (and the deprecated `POST /api/hooks/{path}` and `POST /api/hooks/{repository}/{path}`), the door a workflow's `webhook` trigger answers on: the credential it asks for, the run its body starts, and its ceiling of runs going at once. |
| `server/api/workflow_runs.rs` | `GET /v1/projects/{id}/workflows/{name}/run-inputs` and `POST /v1/projects/{id}/workflows/{name}/runs` (their forms without a project answer `410`): the inputs a manual trigger names, and starting a run by hand with them. |
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
| `server/api/session/mod.rs` | Signing in from a browser: with an access token, the server's token or a one-time login code, sessions kept in the store, the codes in memory, and `/v1/session` and `/v1/session/codes`. |
| `server/api/session/tests.rs` | For the tests only: signing in with each kind of token and code, a session that survives a restart, and what ends one. |
| `server/api/preferences.rs` | A person's preferences: reading them and replacing them. |
| `server/api/favourites.rs` | A person's pins and recent places: listing both, pinning, reordering and removing a pin, noting a place opened, and forgetting the places they can no longer see. |
| `server/api/people.rs` | The people and their access tokens: listing people, an owner adding one with their first token, making, listing and revoking tokens. |
| `server/api/logins.rs` | `GET` and `PUT /v1/logins`: a person's logins on the forges. |
| `server/api/roles/mod.rs` | The role each route of the desktop API needs, and whether the Moebius agent's run token may use it: the table the reference repeats, and the check `authenticate` makes with it. |
| `server/api/roles/tests.rs` | For the tests only: the role table against the reference and the routes of the code, and every route called as a viewer, a member, an owner and the Moebius agent. |
| `server/api/web.rs` | The web app embedded from `web/dist` and served at `/` without a token, with `index.html` for any path that is no file, or a page that says it was not built. |
| `server/api/events.rs` | The event WebSocket `GET /v1/events`: what a person may see, a run's pieces only for the runs a client subscribed to. |
| `server/api/testing.rs` | For the tests only: a server on port 0 with a fake Pi, and the requests the tests of the API share. |
| `server/settings/mod.rs` | Editing `moebius.toml` while the server runs: what `PUT /v1/settings` takes and answers, and reading and writing the file. |
| `server/settings/edit.rs` | A change applied to the file's text with `toml_edit`, its comments and order kept. |
| `server/settings/keys.rs` | Every key with its default and whether it takes effect at once, and their values without passwords. |
| `server/settings/live.rs` | The configuration the running server reads: a change saved and its live keys applied, the agents and rules moved into a new harness configuration repository, and the restart. |
| `server/settings/wiki_tests.rs` | For the tests only: the wiki of a whole server, started with and without `[wiki] url` and with one that cannot be cloned, and a git repository connected while it runs. |
| `server/settings/secrets.rs` | The API keys kept in `secrets.toml`, and the keys the upstreams and the decision model are given. |
| `server/outgoing.rs` | Outgoing webhooks: listens to the bus, picks the hooks that want an event's kind and may see its project (a private project's events go only to a hook that names it), posts it signed with HMAC-SHA256, retries with backoff, and gives up with a `webhook_failed` item in the inbox. |
| `server/webhooks.rs` | Webhooks from the forges (ADR 0012): the check of a delivery against the secret of its forge (HMAC-SHA256 or token, in constant time), and the wake-up that makes the poll of a forge run at once. |
| `server/api/outgoing.rs` | `GET` and `POST /v1/outgoing-webhooks` and `DELETE /v1/outgoing-webhooks/{id}`: the owner's outgoing webhooks, and the secret each is signed with. |
| `server/api/webhooks.rs` | `POST /v1/webhooks/{forge}` (no token, a signature instead), `GET /v1/webhooks` and `POST /v1/webhooks/{forge}/secret`. |
| `server/setup.rs` | What the Models, Plugins and Get started pages show: the router's upstreams, the agents, the teacher, the decision model and the plugins, without a secret, and the steps to a first card (`GET /v1/start`). |

`repos/`, the repositories and their forges:

| Module | What it is for |
|---|---|
| `repos/mod.rs` | Clones, worktrees, branches, pushes, merges and conflict checks through the `git` command line. |
| `repos/conflicts.rs` | What `git merge-tree` finds for a change, kept per (clone, base, branch) until the head of either branch moves; the heads are read from the clone's ref files, so asking again starts no git process. |
| `repos/kept.rs` | A clone the server keeps in step with its `origin`, as the harness configuration repository and the wiki are: cloned once, refreshed by a fast-forward, and an edit a commit pushed to `origin` that is undone when the push fails. |
| `repos/forge.rs` | The `Forge` trait with the forges `none`, `github`, `gitlab` and `forgejo`, and the forge-neutral `Issue` (with its labels, parent, blockers and comments). |
| `repos/tracker_tests.rs` | Tests (only) of what the forge clients write and read for an issue tracker, against a fake forge that records its requests. |
| `repos/github.rs` | The GitHub App client: JWT, installation tokens, issues newest-updated first with ETags (paging stops at the last poll's newest), pull requests, checks, reviews, merge. |
| `repos/gitlab.rs` | The GitLab client: the access token, issues newest-updated first with ETags (paging stops at the last poll's newest), merge requests, pipelines and their failed jobs, approvals, merge. |
| `repos/jira.rs` | The Jira client: the field catalogue, a project's issues over pages (by token, or by position where the site has no such search) and the comments they left out; the token never in an error. |
| `repos/jira_fake.rs` | A fake Jira on a port of its own for the tests: a project with an epic, a story, a sub-task, duplicate bugs and sprints. |
| `repos/forgejo.rs` | The Forgejo client (Codeberg, Forgejo, Gitea): the access token, issues with ETags where given, pull requests, commit statuses, reviews, merge. |
| `repos/poll/mod.rs` | The poll of the forges: the loop over their repositories, issues into cards, and what became of each change (`Polled`, `Request`). |
| `repos/poll/github.rs` | What GitHub answers, read for the poll: issues, a pull request with its check runs, commit statuses and latest review. |
| `repos/poll/gitlab.rs` | What GitLab answers, read for the poll: issues, a merge request with its latest pipeline, failed jobs and approvals. |
| `repos/poll/forgejo.rs` | What Forgejo answers, read for the poll: issues, a pull request with the statuses of its head commit and its latest review. |
| `repos/settings.rs` | A repository's settings (what is about its code; the column scripts are a project's), for `GET /v1/repositories/{id}/settings` and its `PUT` and `DELETE` per file: listed, checked and committed to the harness configuration repository, with the `.moebius/` files still on the default branch. |
| `repos/files.rs` | A repository's files on its default branch, read and committed (the Files sidebar and editor tabs of a project). |
| `repos/search.rs` | A repository's default branch searched for the project page: the list of its paths for "Go to file", and a full-text search with `git grep`. |

`board/`, the Kanban:

| Module | What it is for |
|---|---|
| `board/mod.rs` | What the board folder holds, and the paths the rest of the server uses. |
| `board/engine.rs` | The board engine: runs the script of a card's column (`tick`, which skips a held card, and `settle`) and applies what it did. |
| `board/api.rs` | The `moebius` table, the functions a script acts through. |
| `board/review.rs` | The review along two axes: the findings with evidence of the Standards and Spec runs read into one verdict. |
| `board/structured.rs` | The structured review of a pull request someone else opened: the JSON object at the end of the Reviewer's answer, with the verdict, the summary and the comments on lines. |
| `board/attempts.rs` | `moebius.start_runs`, `moebius.check_attempt` and `moebius.settle_attempts`, and the `card.attempts` and `card.selection` fields. |
| `board/card.rs` | The `card` table a script reads, with the card's change and the verdict of its review. |
| `board/cache.rs` | What the engine remembers between script calls: the bytecode of each script text, the cards a tick can leave alone (settled with nothing to do, while the store's write counter and the limits stay the same, for at most a minute) and the project scripts read during one tick. |
| `board/sandbox.rs` | The Luau sandbox every script runs in, with its memory and time limits. |
| `board/limits.rs` | The run, round and token limits scripts see, and a script call's memory and time. |
| `board/kanban.rs` | The six built-in columns, the default Kanban's scripts built into the server, and a project's overrides; a project's custom columns are `app/columns/`. |
| `board/runs.rs` | What the engine needs of a harness (`Runs`), and the agents it lists. |
| `board/testing.rs` | For the tests only: a fake harness and a store with a repository and a card. |
| `board/dry_run.rs` | `check_script`: a script run once against a made-up card, for `moebius check-script`. |
| `board/custom_columns.rs` | Tests of custom columns from the engine's side: the script of a card in one, `moebius.move`, a column that goes away. |
| `board/scripts.rs` | Editing a column's script of a project: the script in force, saved as a commit in the harness configuration repository. |

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
| `decide/guard/restricted.rs` | The restricted mode of a review run of a pull request someone else opened: the refusal of every call of a restricted review run, which has no tools, and the narrower mode for reading a worktree. |
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
| `harness/run/coding_agents.rs` | The built-in instructions of the Coder and the Reviewer, which complement the per-run prompts of `prompts/`. |
| `harness/run/prompts/mod.rs` | What a run of a card is told: the issue and its comments, the instructions of its purpose, the files likely to matter, the lessons of earlier cards and the notes of the last round. |
| `harness/run/prompts/repositories.rs` | What a run of a card over several repositories is told of them: a code run where its worktrees are and which it may ask for, a review where the change in each is. |
| `harness/run/lifecycle.rs` | Making a run, its worktree (one in each repository of a card over several) and its token (`card_run`, `detached_run`, `chat_run`), undoing them when its process cannot be started (`launch_or_undo`), and `RunningRun`, which commits what the agent left and removes the worktree. |
| `harness/run/start.rs` | Starting a run: the harness is asked only to start its process (`Launching`, `Launch`) in the run `lifecycle` made. |
| `harness/run/end.rs` | Ending a run: its records kept as its events and summary (`keep`), and its end (`finish`). |
| `harness/run/scratch.rs` | The run's own scratch directory, `<data_dir>/runs/<id>/scratch`: made before its process starts on this machine, named to it as its `TMPDIR`, `TEMP`, `TMP` and `$MOEBIUS_RUN_SCRATCH`, and removed when the run ends. A run on a runner has none, and keeps the temporary directory of the runner. |
| `harness/run/testing.rs` | For the tests only: a server with one repository and both harnesses on the fake of each process, and ways to wait for a run. |
| `harness/run/several_tests.rs` | For the tests only: a card over several repositories, on the fake `pi-bolt`: a worktree and a change in each, and a run that asks for another. |
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
| `harness/repo/projects.rs` | A project's settings, the files of `projects/<project name>/` (its column scripts): read from the clone, written or removed as commits like any edit, moved when the project is renamed, and the migration that moves a repository's scripts to its home project. |
| `harness/repo/repositories.rs` | A repository's settings, the files of `repos/<repository name>/` (rules, review notes, checks, test first, the hint of files, plugins' settings): read from the clone, and written or removed as commits like any edit. |
| `harness/repo/edits.rs` | A person's edits of agents, skills, MCP servers and rules, as commits pushed to its origin. |
| `harness/repo/templates.rs` | Templates in the clone: `templates/issues/<kind>.toml` and `templates/pages/<name>.md`, read and edited as commits. |
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
| `improve/retro/estimates.rs` | The cards of the Retro's day whose estimate was furthest off from the tokens and time they took, for the report. |
| `improve/retro/traces.rs` | The raw traces the Retro analyst reads besides the report: the events and diffs of the report's runs, redacted and bounded, as files under `.retro/` of its worktree. |
| `improve/trial/mod.rs` | Trials: the trial window, and what the parts share. |
| `improve/trial/verdict.rs` | What a trial run and the card's first code run measured, whether a trial is positive, and the status a proposal gets from its trials. |
| `improve/trial/card.rs` | One card's trial: its task run again by its agent with the definition from the proposal's branch, then measured and compared, its worktree and branch removed. |
| `improve/trial/regression.rs` | The regression set of a repository (its latest merged cards, those with a reproduction test first) and the trials of a proposal on it. |
| `improve/trial/schedule.rs` | Trying a proposal on a thread of its own, card after card, each trial stored and published, its status and inbox item; and the schedule's start inside the trial window. |
| `improve/trial/decision.rs` | A person's merge or dismissal of a positive, mixed or untried proposal. |
| `improve/own/mod.rs` | The Retro's proposals for Moebius's own configuration (`moebius.toml`, a repository's rules and its home project's scripts): the keys and files it may change, and what the analyst is told about them. |
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
| `workflows/definition.rs` | The workflow file (`projects/<project>/workflows/<name>.toml`): its trigger, nodes and edges, parsing it whole, validating its graph, and reading and listing it from the harness configuration repository; saving and deleting it are commits in `harness/repo/edits.rs`. |
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
| `workflows/nodes/chat.rs` | The `chat` node: the agent, prompt and place of the agent chat an automation is stored as; the run that reaches it starts the chat from `app/workflows/chat.rs`. |
| `workflows/nodes/publish.rs` | The `publish` node: the run's branch offered as its change with a title and body filled from the value, the failure closed when the run has no agent node or nothing was committed, and the change's id and kind as its value. |
| `workflows/trigger/mod.rs` | The trigger kinds a workflow file may name, the module that owns each, and how a trigger's `kind` is resolved by its module when one owns it and by the node registry otherwise. |
| `workflows/trigger/manual.rs` | The `manual` trigger: the inputs it names with their types, requiredness, defaults and choices, checked when the file is read, and the value a started run begins with. |
| `workflows/trigger/schedule.rs` | The `schedule` trigger: a cron of five fields in the server's local time, its on/off switch, the minute it next comes due, the minute it last fired at, and the value a firing starts a run with; a cron that cannot be read is refused when the file loads, naming the wrong field. |
| `workflows/trigger/webhook.rs` | The `webhook` trigger: the path its workflow answers on, the secret a delivery must carry and the repository its runs belong to, and which workflow a path leads to. |
| `workflows/run.rs` | The executor: following the edges from the trigger with one value, giving each node the trigger's value and the values of the nodes that ran before, taking the edge that leaves by the port a node names, running each node under its timeout, failing the run on a node's error or its node ceiling, stopping where a person cancels the run, and reporting each step to an observer. |

`wiki/`, the organisation's pages:

| Module | What it is for |
|---|---|
| `wiki/mod.rs` | The wiki's repository (a kept clone, `repos/kept.rs`, with no `origin` until one is connected): refreshed with `wiki_changed` when that brought commits, the names of pages and the folder of a project's, and the person an edit is by. |
| `wiki/repository.rs` | Where the wiki's repository is: opened at start (a clone of `[wiki] url`, or a repository of its own with none), connected to a git repository while the server runs (history pushed to an empty one, a repository with pages adopted when the wiki has none, both having pages refused), and the status the Wiki page shows. |
| `wiki/pages.rs` | Reading the wiki: the list of pages, a page as it is or as a commit of its history left it, and its history through renames and moves. |
| `wiki/proposals.rs` | An agent's edit of a page: a branch of the clone with one commit that waits in the inbox as a `wiki_proposal` and reaches the pages, and `origin`, only when a person merges it. |
| `wiki/prompt.rs` | The pages a card's issue links to with `[[Page name]]`, as the section of a run's prompt that holds them. |
| `wiki/search.rs` | Searching the wiki: the index made again from the pages when the head moved, matches ranked with the title first, and the pages of a project the asker may not see left out. |
| `wiki/sight.rs` | Which pages a person or a run sees: a private project's pages are its members', and a run's prompt holds them only for a card of that project. |
| `wiki/labels.rs` | A page's labels: read from the YAML front matter of its file, set by rewriting only the `labels` key (other keys stay), and the text without its front matter. |
| `wiki/links.rs` | The links of the wiki: `[[Page name]]`, card names and `#12` found in a text, resolved to pages, the links between pages made again from the files on every change, backlinks, and the links of a moved page rewritten. |
| `wiki/edits.rs` | A person's edits: a page written, moved or deleted, each a commit by the person after following `origin`, pushed or undone. |

Outside `src/`: `kanban/*.luau` holds the six built-in column scripts, `bridge/moebius-bridge.ts` the Pi extension
loaded into every run, `bridge/moebius-ask.ts` the question tool of the agents that operate Moebius, `skill/` the built-in skills.
