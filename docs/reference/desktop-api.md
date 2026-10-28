# Desktop API

The desktop API is how the desktop app, or any client such as `curl`, reads the boards and acts on them. This page is for
anyone who writes a client or a script against a running server. The code is in `crates/moebius-core/src/api.rs`; the
events are published from `app.rs`, `pibolt.rs`, `check.rs`, `poll.rs`, `retro.rs`, `trial.rs` and `server.rs`.

The API is HTTP with JSON bodies plus one WebSocket, served on `listen` (default `127.0.0.1:7171`). Keep it on a private
network.

## Authentication

Every request, the WebSocket upgrade included, carries `Authorization: Bearer <token>`. The token is the content of
`token_file` (by default `<data_dir>/token`, created when the server first starts). A missing or wrong token is answered
with `401` and the plain text `a valid token is needed`.

The run token of a run of the Moebius agent (`MOEBIUS_TOKEN` in its environment, see the [bridge
protocol](bridge-protocol.md#the-environment-of-the-harness)) is accepted as well, while the run lasts: it is how the
agent operates Moebius with the [command line](cli.md). A request made with it acts as the agent: every decision it
records has the decider `agent:Moebius` instead of `person`, and the run is the judgement's `run` (for a decision about
another run, such as a steer, the judgement keeps that run, and the agent's run is in the context as `acting_run`).
The run token of any other agent, and a revoked one, is answered with `401`. Two routes stay with a person and answer
`403` with `only a person can do this` to a run token: `PUT /v1/secrets/{name}` (an agent must not set keys) and
`GET /v1/runs/{id}/terminal` (the shell). Where a route below says "a person" and "decider `person`", an agent that
uses its token is the decider instead.

```sh
TOKEN=$(cat .moebius/token)
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7171/v1/health
```

## Errors

A failed request has a status code and a plain-text message as its body.

| Status | Meaning |
|---|---|
| `401` | No valid token. |
| `403` | A run token of the Moebius agent on a route that only a person may use. |
| `404` | The repository, card, change or run in the path does not exist. |
| `409` | A merge conflicts: the default branch is left as it was. |
| `422` | The request is understood but not allowed: invalid values, or a card or change in the wrong state. Also what axum answers for a JSON body with a missing or mistyped field. |
| `400`, `415` | The body is not valid JSON, or the content type is not `application/json`. |
| `500` | Anything else: git, the store or a forge failed. |

A request with a body must have `Content-Type: application/json` and a JSON object, `{}` at least.

## Types

These objects appear in the responses. All ids are numbers; times are Unix seconds.

`Repository`: `id`, `name`, `path` (the clone on the server), `forge` (`none` or `github`), `default_branch`, `github`
(`owner/repo` or null), `trusted` (array of GitHub logins).

`Card`: `id`, `repository_id`, `number`, `title`, `body`, `author`, `column`, `created_at`, `note` (why the card was sent
back to coding, or null), `retried_after` (the id of the card's latest run when a person last retried it, or null) and
`confirmed` (true for a card made from a confirmed grilling, which needs no approval).
`Run`: `id`, `card_id` (null for a chat), `agent`, `harness`, `purpose` (`code`, `review`, other purposes of column scripts,
`chat`, `retro`, the Retro analyst's run, `trial`, the trial of a proposal on a card, or `research`, a round of the Autoresearcher), `status` (`starting`, `running`, `waiting`, `succeeded`, `failed` or `stopped`; `waiting` is a chat waiting for
the person's next message), `worktree`, `started_at`, `ended_at`, `summary`, `repository_id` (the repository a chat
works in, or null) and `harness_repository` (true for a chat that works in the [harness configuration
repository](harness-configuration.md#changing-it-in-a-chat); `harness` is the name of the harness that runs the agent).

`Grilling`: `id`, `run_id` (the chat), `repository_id`, `issues` (array of `{"title", "body", "blocked_by"}`, where
`blocked_by` is an array of indexes into `issues`; empty for a report and for an empty grilling), `report` (text or null),
`status` (`proposed`, `reported`, `empty`, `confirmed` or `dismissed`), `reason` (why a grilling is `empty`; else empty) and
`at`.

`Chat`: `id` (the id of its run), `agent`, `status` (that of the run), `repository_id` (or null), `harness_repository`
(true for a chat in the harness configuration repository), `started_at`, `ended_at`, `title` (the first line of the
first message, at most 80 characters), `answered` (the `seq` of the chat's last `agent_settled` or `moebius_error`
event, 0 without one) and `seen` (how far a person has read it, see `POST /v1/chats/{id}/seen`). A chat is unread while
`answered` is greater than `seen`.

`Change`: `id`, `repository_id` (null for a change of the harness configuration repository), `harness_repository` (true
for such a change), `card_id` (or null), `branch`, `state` (`open`, `merged` or `closed`), `pull_request`
(the number on GitHub, or null), `checks` (`pending`, `success`, `failure` or null), `review` (the latest review's state in
lower case, such as `approved`, or null), `head` (the published commit), `published_at`, `checks_head` (the commit
`checks` are for), `failed_checks` (the failing checks of a pull request, one per line, or null) and `check_output` (the
end of the output of the repository's own checks, or null). A change's `branch` is `moebius/card-<number>`, `moebius/chat-<run id>` for the work of a chat, which has no card, or `research/<research id>-<name>` for a lever of a research, which has none either.

`InboxItem`: `id`, `kind`, `card_id` (or null), `title`, `data` (an object that depends on the kind), `resolved_at` (null
in the snapshot, which holds only open items), `at`. Kinds the core adds: `approve_issue` (`data.reason`), `stuck`
(`data.reason`), `ready_to_merge` (`data.change`, `data.branch`), `change_closed` (`data.change`, `data.pull_request`) and
`proposal` (`data.proposal`, `data.branch`, `data.trials`: an array of `{"card", "outcome", "reason"}`; its `title` is the
proposal's and it has no card), `grilling_issues` (`data.grilling`, `data.repository`, `data.issues`: an array of `{"title",
"body", "blocked_by"}`; its `title` is "N issues from a grilling" and it has no card) and `grilling_report` (`data.grilling`,
`data.repository`, `data.report`: the text; no card) and `takeover` (`data.takeover`, `data.kind`, `data.agreement`,
`data.count`; its `title` is "The decision model can take over <kind>" and it has no card).
A column script can add other kinds with `moebius.ask`.

## Endpoints

### `GET /v1/health`

Response: `{"version": "0.1.0"}`, the server's version.

### `GET /v1/snapshot`

Everything the app shows. Response:

```json
{
  "boards": [
    {"repository": {"id": 1, "name": "demo", "path": "/srv/.moebius/repos/demo", "forge": "none",
                    "default_branch": "main", "github": null, "trusted": []},
     "cards": [{"id": 1, "repository_id": 1, "number": 1, "title": "Add a feature", "body": "", "author": "person",
                "column": "Backlog", "created_at": 1790000000, "note": null, "retried_after": null,
                "rounds": 0, "tokens": 0, "run": null, "blocked_by": [], "refusals": 0, "change": null}],
     "round_limit": 5,
     "waiting": {},
     "changes": []}
  ],
  "harness_changes": [],
  "inbox": []
}
```

`harness_changes` are the `Change`s of the harness configuration repository, which belongs to no board.
`boards` has one entry per repository, with all its cards and changes. `round_limit` is the most `code` rounds a card
gets. Each entry of `cards` is a `Card` with what a board shows of it, so that a client needs no other read:

| Field | Meaning |
|---|---|
| `rounds`, `tokens` | The card's `code` runs and the tokens of all its runs since it was last retried, as `card.rounds` and `card.tokens` in [column scripts](column-scripts.md). |
| `run` | The card's latest run, `{"id", "agent", "purpose", "status"}`, or null when it has none. A board counts the runs working on it from the cards whose `run.status` is `starting` or `running`. |
| `blocked_by` | The numbers of the issues that block the card and are not done yet, as `card.blocked_by`. |
| `refusals` | How many tool calls of the latest run were refused (its `tool_check` run events with the decision `refuse`). |
| `change` | The card's open change, `{"id", "state", "checks", "review_verdict"}`, or null. `checks` is `pending`, `success`, `failure` or `none`, as `card.change.checks`; `review_verdict` is `approve` or `changes` once a review ended since the latest `code` run, else null. Whether the change conflicts is not in the snapshot. |

A card in `Done` has `rounds` and `tokens` 0, no `run`, no `change`, no blockers and no refusals: the snapshot is read on
every event, and a finished card shows none of it. `waiting` maps the id of each card whose column
script is waiting (`moebius.wait`, `ask`, `stuck`) to the reason, as text; a card that does not wait has no entry. The
reasons are kept in memory, so after a restart they are missing until the board engine's next tick; `card_waiting`
(below) keeps them current. `inbox` holds the items that are not
resolved yet, oldest first.

### `GET /v1/events`

A WebSocket. After the upgrade, every event the core publishes arrives as one JSON text message, see Events below. Events
are not replayed: a client sees only what happens after it connected, so read the snapshot after connecting. Messages the
client sends are ignored; closing the socket ends the stream.

### `POST /v1/repositories`

Adds a repository: the server clones `url` into its data directory and reads the clone's checked-out branch as the
default branch.

Request:

| Field | Type | Meaning |
|---|---|---|
| `name` | string | Required. Letters, digits, `-`, `_` and `.`, and not starting with a dot. Must be unique. |
| `url` | string | Required. Anything `git clone` takes: a URL or a path on the server. |
| `forge` | string | `none` (the default) or `github`. |
| `github` | string | `owner/repo` on GitHub. Required with the forge `github`, not allowed otherwise. Both parts are made of letters, digits, `-`, `_` and `.`. |
| `trusted` | array of strings | GitHub logins whose issues need no approval. Only with the forge `github`. |

Response: the `Repository`. Errors: `422` for an invalid name, a `github` that is not `owner/repo`, or a combination other
than "no forge, no `github`, no `trusted`" and "forge `github` with `github`"; `422` also when the name is taken (nothing is
cloned then); `500` when the clone fails.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "demo", "url": "/srv/git/demo"}' http://127.0.0.1:7171/v1/repositories
```

For GitHub, with the App set up, the clone authenticates as the App's installation, so a private repository works:

```json
{"name": "widgets", "url": "https://github.com/acme/widgets.git", "forge": "github", "github": "acme/widgets", "trusted": ["leon"]}
```

### `POST /v1/repositories/{id}/issues`

Creates an issue from a person in the app. Its card starts in `Backlog`, with the author `person`.

Without a forge the card is trusted and moves on. On a repository with a forge the issue is opened there first, the same
way a confirmed grilling opens its issues, and the card gets the issue's number right away, so that the poll finds it
existing. The card is `confirmed`, since a person wrote it, and so moves on from `Backlog` without approval, whatever the
repository's `trusted` list says.

Request: `{"title": "...", "body": "..."}`. `title` is required and must not be empty or blank; `body` is optional.

Response: the `Card`. Errors: `404` no such repository; `422` the title is empty; `500` the forge refuses to open the
issue (no card is made).

### `POST /v1/cards/{id}/approve`

A person approves an issue that waits in the backlog: the decision is recorded (kind `approve_issue`, decider `person`),
the card's inbox items are resolved, and the card moves to `Ready`.

Request: `{"reason": "..."}`; `reason` is optional.

Response: the `Card`. Errors: `404` no such card; `422` the card is not in `Backlog`.

### `POST /v1/changes/{id}/merge`

A person merges an open change: the decision is recorded (kind `merge`, decider `person`), the forge merges it (a
no-fast-forward merge into the default branch of the clone for a repository without a forge; the GitHub API for a pull
request), the change becomes `merged`, its inbox items are resolved and its card, if it has one, moves to `Done`. A
change from a chat has no card: its `ready_to_merge` item is resolved and the commit message is `Merge <branch>`.

A change of the harness configuration repository (`harness_repository` true) is merged into the default branch of its
clone with a merge commit, which is pushed to `origin`; the agents are read again, so the next run uses what the change
brought. A push that `origin` refuses undoes the merge and is a `500`. The decision is recorded as for any change, with
the repository `harness`.

Request: `{"reason": "..."}`; `reason` is optional, but the body is not: send `{}`.

Response: `{"commit": "<sha of the merge commit>"}`. Errors: `404` no such change; `422` the change is not `open`; `409` the
branch does not merge cleanly; `500` the forge failed.

### `POST /v1/changes/{id}/reject`

A person rejects an open change whose card waits in `Ready to merge`: the decision is recorded (kind `reject`, decider
`person`), the card's `ready_to_merge` inbox item is resolved, and the card moves back to `In progress` with the reason as
its `note`, so that the next Coder round is told (without a reason the note says a person rejected the change). The change
stays open: the next round updates it, and on GitHub its pull request stays.

Request: `{"reason": "..."}`; `reason` is optional, but send `{}` without one.

Response: the `Card`. Errors: `404` no such change; `422` the change is not `open` or its card is not in `Ready to merge`.

### `POST /v1/cards/{id}/retry`

A person retries a card that waits in the inbox as stuck or whose pull request was closed (it reached its round limit or its token budget, or a person
stopped its run): the decision is recorded (kind `retry`, decider `person`), the card's inbox items are resolved, its
waiting reason is cleared and its `retried_after` is set to its latest run, so that its `rounds`, `reviews` and `tokens`
(see [column scripts](column-scripts.md)) count from the runs after it. The board then starts work on the card again.

A card whose pull request was closed on the forge without a merge waits in `Ready to merge` with an open `change_closed`
inbox item. Retrying it does the same, and moves the card to `In progress` with the note `The pull request was closed
without a merge.`; the closed change stays closed and the next publish opens a new change and pull request.

Request: `{"reason": "..."}`; `reason` is optional, but send `{}` without one.

Response: the `Card`. Errors: `404` no such card; `422` the card has neither an open `stuck` nor an open `change_closed` inbox item.

### `GET /v1/runs/{id}`

One run, for the live view. Response: the `Run`. Errors: `404` no such run.

### `GET /v1/runs/{id}/events`

The stored events of a run, in order. A client loads a long run in pieces and then follows `GET /v1/events` from the last
`seq` it has: every `run_event` on the stream carries the `seq` the event was stored under, so the client drops those
it already has and sees every event exactly once.

Query: `after` (default 0) returns only events with a `seq` above it; `limit` (default 500, at most 5000, at least 1)
is the most it returns.

Response: an array of `{"seq": number, "kind": string, "data": object, "at": number}`, oldest first, where `kind` and
`data` are those of the `run_event` (below). Errors: `404` no such run.

### `GET /v1/cards/{id}/runs`

The card's runs, oldest first. Response: an array of `Run`. Errors: `404` no such card.

### `GET /v1/cards/{id}/history`

The card's timeline, oldest first (entries of the same second are in the order of the kinds below). Response: an array
of `{"at": number, "kind": string, "data": object}` with these kinds:

| Kind | When `at` is | `data` |
|---|---|---|
| `moved` | The card moved to another column. Moves from before moves were recorded are not in the history. | `{"from": column, "to": column, "note": string or null}`, the card's `note` when it moved |
| `run_started` | The run was stored. | `{"run": id, "agent": string, "purpose": string}` |
| `run_ended` | The run ended. | `{"run": id, "status": "succeeded" \| "failed" \| "stopped"}` |
| `decided` | A decision was recorded for the card. | `{"judgement": id, "kind": string, "choice": string, "decider": string, "reason": string}` |
| `inbox_added` | An inbox item about the card was added. | `{"item": id, "kind": string, "title": string}` |
| `inbox_resolved` | That item was resolved. | `{"item": id, "kind": string, "title": string}` |

Errors: `404` no such card.

### `GET /v1/judgements`

The recorded decisions (what a person or a decider chose at a decision point), newest first. Query: `card` returns only
those about that card, `run` only those about that run (a chat has no card, so this is how its tool calls' deciders are
found), `kind` only those of that kind (such as `retry`, `merge` or `tool_call`); `limit` (default 100, at most 1000, at
least 1) is the most it returns. A card or run that does not exist simply has none.

Response: an array of `{"id": number, "kind": string, "choice": string, "reason": string, "decider": string,
"outcome": string or null, "card": number or null, "run": number or null, "at": number}`. The blobs of the question's
context are not included. For a refused tool call that was replayed, `outcome` is the replay's sentence, `replayed: <how
it ended>; <what happened to the files>` (see [the replays](harness-configuration.md#the-replays)).

### `POST /v1/judgements/{id}/replay`

Runs the call of a refused `tool_call` judgement again in a throwaway copy of its worktree, now, at any hour, as the
schedule does inside `[trial_window]`. The call runs once; the copy is removed; the judgement's `outcome` becomes the
replay's sentence; the event `replayed` is sent. Request: no body. Response: the `Replay`:

```json
{"id": 1, "judgement": 12, "run": 9, "exit": 0, "timed_out": false, "output": "", "created": [],
 "changed": ["src/lib.rs"], "deleted": ["src/a.rs", "src/b.rs"], "at": 1790000000}
```

`exit` is the exit code, null when the call was killed (`timed_out` says whether for the time) or did not run (then
`output` says why). `output` is what the call printed, secrets removed, the last 4 KB. `created`, `changed` and
`deleted` are paths relative to the worktree. Errors: `404` no such judgement; `409` it was replayed already, or has an
outcome already; `422` it is not a refused `tool_call`, or its call is not reversible, or its run or repository is gone,
with the reason.

### `GET /v1/replays`

The stored replays, newest first, at most 1000. Query: `judgement` returns the replay of that judgement, `run` those of
that run's calls. Response: an array of `Replay`.

### `GET /v1/judgements/kinds`

What the store holds now, per kind of judgement, kinds in alphabetical order. Response: an array of `KindCounts`:
`{"kind": string, "count": number, "with_outcome": number, "by_person": number, "held_out": number}`. `count` is the
judgements of the kind, `with_outcome` those with an outcome, `by_person` those whose decider is `person`, `held_out`
those in the held-out tenth (see [the training export](training-export.md#the-split)).

### `POST /v1/exports`

Writes the training-set export now (see [the training export](training-export.md)) to `<data_dir>/exports/<id>/` and
records the person's decision to do so as a judgement of kind `export` (decider `person`, context: `export` the id,
`kinds` the kinds written, joined by commas, and `counts` the `KindCounts` as JSON text). Request: `{"kinds": [string]}`,
the kinds to export; without `kinds`, or with `null`, every kind. The body is required, `{}` is the smallest. Response:
the `Export`:

```json
{"id": 1, "at": 1790000000, "kinds": [{"kind": "merge", "count": 12, "with_outcome": 12, "by_person": 12, "held_out": 1}]}
```

`id` counts up from 1. The export holds the judgements there were when it read them, not the judgement of its own
decision. Errors: `422` a kind has no judgements (no file is written for any kind).

### `GET /v1/exports`

The exports made through the API, newest first. Response: an array of `Export`.

### `GET /v1/exports/{id}/{file}`

Downloads a file of export `id`, `{file}` being `<kind>.jsonl` (such as `tool_call.jsonl`), streamed, as `application/x-ndjson`. The body is the file as the
export wrote it. Errors: `404` no such export or no such file in it.

### `GET /v1/usage`

What the model calls used: tokens, calls, how long they took, and where they went. Query: `days` (a whole number from 1
to 365, default 1) is the window, counted in local days of the server, today included: `1` is since local midnight (the
same "today" the day budget counts), `7` the last seven days. Anything else is `422`. Tokens are input plus output tokens.

Response:

```json
{
  "today": "2026-10-28",
  "days_in_window": 7,
  "totals": {"tokens": 1840000, "calls": 1284, "cached_share": 0.61, "model_ms": 11520000, "day_budget": 8000000},
  "days": [{"date": "2026-10-27", "tokens": 2520000, "model": "claude-sonnet-5-5"}],
  "models": [{"model": "claude-sonnet-5-5", "tokens": 960000, "calls": 690, "input_tokens": 700000, "cached_tokens": 520000}],
  "agents": [{"agent": "Coder", "calls": 702, "tokens": 1120000, "input_tokens": 1000000, "cached_tokens": 610000, "model_ms": 7080000}],
  "activities": [{"activity": "bash", "model_ms": 2140000, "calls": 214, "output_tokens": 61000}],
  "tool_waits": [{"tool": "bash", "calls": 31, "seconds": 2460}]
}
```

- `today`: the server's date, `YYYY-MM-DD`; the last day of the heatmap.
- `totals` covers the window. `cached_share` is the cached input tokens divided by the input tokens (0 to 1, and 0
  when there was no input). `model_ms` is the time the model calls took, in milliseconds, each measured from the request
  to the end of the response or stream. `day_budget` is `day_tokens` of `moebius.toml`, or `null` without one.
- `days` does not depend on `days_in_window`: it holds each of the last 365 local days that had usage, oldest first, with
  the tokens of the day and its leading model (the one with the most tokens that day; the first by name on a tie).
  Days without usage are not listed.
- `models` holds each model used in the window, the one with the most tokens first: tokens, calls (one per model call),
  input tokens (part of the tokens) and cached tokens (part of the input tokens).
- `agents` holds each agent whose runs used tokens in the window, the one with the most tokens first: calls, tokens,
  input tokens, cached tokens and model time. Usage that belongs to no run is counted under the agent `No run`.
- `activities` holds what the model did with its time in the window, the activity with the most model time first: the
  `activity` is a tool's name, `answering` or `unknown`. A model call that asked for `n` tools gives each of them `1/n`
  of its duration and of its output tokens (the remainder of a division, whole milliseconds or tokens, goes to the first
  tools; a tool named twice by one call gets both shares). A call that asked for no tool is `answering`. `model_ms` and
  `output_tokens` are the sums of those shares; `calls` counts a call once for each distinct tool it named, so it can
  add up to more than the calls of the window. A call recorded before the router read tools is `unknown`; `unknown` is
  listed only when the window holds no other call. What the router reads from answers is in `router.md`.
- `tool_waits` holds the tools whose calls waited longest in the window, at most 10: for each tool name, the number of
  calls and the seconds between their `tool_execution_start` and `tool_execution_end` run events, summed. Events
  are stamped in whole seconds. A call that has not ended is not counted.

Errors: `422` `days` is not from 1 to 365.

### `POST /v1/runs/{id}/steer`

A person sends a message to a running run: the harness passes it to the agent (Pi-Bolt's `steer` command), the run
stores a `steered` event with `{"message": ...}` (secrets removed), and the decision is recorded (kind `steer`, decider
`person`, the message in its context).

Request: `{"message": "..."}`; `message` is required and must not be blank.

Response: the `Run`. Errors: `404` no such run; `422` the message is blank or the run is not running.

### `POST /v1/runs/{id}/stop`

A person stops a running run: the decision is recorded (kind `stop`, decider `person`) and the harness asks the agent to
abort (Pi-Bolt's `abort` command; a Pi-Bolt that has not exited after 10 seconds is killed). The run ends as `stopped`,
which publishes `run_ended`. The card is not retried by itself: its script puts it in the inbox as stuck ("stopped by a
person"), and a person decides with retry.

Request: `{"reason": "..."}`; `reason` is optional, but send `{}` without one.

Response: the `Run`, as it was when the request was made (still `running`). Errors: `404` no such run; `422` the run is not
running.

A chat that is stopped, also while it waits, ends as `stopped`; what it left in its repository's worktree is committed
on its branch first.

### `GET /v1/runs/{id}/terminal`

A shell in the run's worktree, for the person to look around and run commands. It upgrades to a WebSocket (with the
token, like every route) and starts the person's shell (`$SHELL`, else `/bin/sh`) on a pseudo-terminal in the worktree
of the run or chat, with `TERM=xterm-256color`.

- Binary messages carry bytes both ways: from the client to the shell's input, from the shell's output to the client.
- A text message `{"type": "resize", "cols": N, "rows": M}` sets the size of the terminal in cells (the size is 80 by
  24 until then).
- When the shell exits the server closes the socket; when the socket closes the server kills the shell and its process
  group.
- A run has at most 4 terminals open at a time.
- It is a person's shell, so the guard (which checks the tool calls of agents) does not look at it, and nothing the
  person types or sees is stored. Opening one stores a `shell_opened` run event, `{"who": "person"}`.

The worktree exists while the run or chat is `starting`, `running` or `waiting`. Errors, answered before the upgrade:
`404` no such run; `422` the run has no worktree (it is over) or already has 4 terminals open.

### `GET /v1/agents`

The agents the server can run, for choosing one to chat with: the Moebius agent first and the Grilling partner second (each
when its own `[agents.<name>]` or `[agents.Coder]` gives it a model), then the others by name. Response: an array of `{"name": string, "model":
string, "protocol": "openai" or "anthropic", "thinking": string, "token_budget": integer, "built_in": boolean, "error": string}`;
`thinking` and `token_budget` are the agent's, left out when its configuration does not set them; `built_in` is true for the Moebius
agent and the `Grilling-partner`, which Moebius brings itself. `error` is left out unless the agent's files in the [harness
configuration](harness-configuration.md) are broken: such an agent has an empty `model`, cannot be run, and `error` says
why (the built-in agents still run, on the Coder's model). Instructions, skills, extensions and anything secret are not
included.

### `GET /v1/agents/{name}`

An agent of the harness configuration repository, as its files say: `{"name": string, "settings": {"model", "protocol",
"thinking", "token_budget", "skills", "extensions", "mcp"}, "instructions": string}` (`thinking` and `token_budget` are
left out when not set). The files hold no secrets and are returned in full. Errors: `404` no such agent; `422` its files
are broken (the message says how), or the server has no `[harness]` and its agents are in `moebius.toml`.

### `PUT /v1/agents/{name}`

A person adds or changes an agent of the harness configuration repository. Request: `{"settings": {...}, "instructions":
string}`, the shape `GET` answers without `name`; `settings.model` is required and unknown keys are refused;
`instructions` may be left out. The edit is validated as a file read is and, when valid, written, committed on the
default branch and pushed to `origin`; the decision is recorded (kind `edit_agent`, decider `person`), and the agent is
used from then on. Response: the agent as `GET` answers it, with `commit`, the short id of the commit the edit made (left
out when the edit changed nothing and so made no commit). Errors: `422` an invalid name or settings (the message says
what), or no `[harness]`; `500` git failed, such as a push that `origin` refused, and then nothing changed (see [harness
configuration](harness-configuration.md)).

### `DELETE /v1/agents/{name}`

A person deletes an agent of the harness configuration repository, committed and pushed the same way (kind `edit_agent`).
Response: `null`. Errors: `404` no such agent; `422` no `[harness]`; `500` as for `PUT`.

### `GET /v1/rules`

The rules page: what the guard refuses and why. Response:

```json
{
  "patterns": [{"name": "no-force-push", "description": "git push --force rewrites history on the remote"}],
  "prose": {
    "source": "harness",
    "editable": true,
    "text": "## no-vendoring\nDo not vendor.\n",
    "rules": [{"name": "no-vendoring", "text": "Do not vendor."}],
    "error": null
  },
  "repositories": [{"repository": "demo", "text": "...", "rules": [{"name": "...", "text": "..."}], "error": null}],
  "refusals": {"no-force-push": 2}
}
```

- `patterns`: the built-in rules in the order they are tested, each with its name and what it refuses (the reason a
  refusal gives).
- `prose`: the server's rules file as it is now. `source` is `harness` for the `rules.md` of the harness configuration
  repository, `file` for the `rules_file` of the configuration, and `null` when the server has neither (then `text` is
  empty and there are no rules). `editable` is true exactly when `source` is `harness`. `text` is the file's text (empty
  when it does not exist), `rules` the rules as the parser reads them. `error` is `null` unless the file cannot be used:
  it cannot be read, a `## ` line has no name, or a name is repeated (the repeat is dropped by the parser).
- `repositories`: the repositories that have a `.moebius/rules.md` on their default branch, in the order of their ids,
  with its text (without the final new line), its rules and `error` as for `prose`. A repository without the file is
  left out.
- `refusals`: for each rule name, the number of `tool_call` judgements in the last 7 days that refused a call because of
  it, from the reason `<rule>: <why>` of a built-in pattern or of a rule in prose. A refusal that names no rule (the
  teacher failing, no teacher, a rules file that cannot be read) is not counted. A rule with no refusals is left out.

### `PUT /v1/rules`

A person replaces the rules in prose of the harness configuration repository. Request: `{"text": string}`, the whole
`rules.md`. The text must be fit to be saved: a `## ` line must have a name and a name must not be given twice (text
before the first `## ` line is free). It is written to `rules.md` (a final new line is added if missing), committed on the
default branch with the message `Update the rules` and pushed to `origin`; then the decision is recorded (kind
`edit_rules`, decider `person`, the text in its context). Every tool call reads the file afresh, so the next one is
checked against the new rules. Response: the same as `GET /v1/rules` after the change, with `commit`, the short id of the
commit (left out when the text changed nothing and so made no commit). Errors: `409` the server has no `[harness]`, so the
rules are a file of the server and read only here; `422` the text is not fit to be saved (the message says which line);
`500` git failed, such as a push that `origin` refused, and then nothing changed and no decision was recorded (see
[harness configuration](harness-configuration.md)).

### `GET /v1/skills`

The skills an agent's `skills` can name, for an editor to offer: an array of `{"name": string, "description": string,
"built_in": boolean}`, the built-in `moebius` skill first, then the directories of `skills/` in the harness configuration
repository by name. `description` is the `description` of the frontmatter of the skill's `SKILL.md`, empty when there is
none. Without `[harness]` the array is empty.

### `GET /v1/skills/{name}`

A skill of the harness configuration repository, or the built-in `moebius` skill: `{"name": string, "description":
string, "text": string, "files": [string]}`. `text` is the `SKILL.md` after its frontmatter; `files` are the paths of the
skill's other files, relative to its directory, sorted (they are listed, not returned). Errors: `404` no such skill;
`409` the server has no `[harness]`: skills and MCP servers are edited in a harness configuration repository.

### `PUT /v1/skills/{name}`

A person adds or changes a skill. Request: `{"description": string, "text": string}` (`text` may be left out);
unknown keys are refused. The name is made of letters, digits, `-` and `_`, and the description is one line and not
blank. It writes `skills/<name>/SKILL.md` with the frontmatter `name` and `description`, then `text`; the skill's other
files are left as they are. The edit is committed (`Add the skill <name>` or `Update the skill <name>`) on the default
branch and pushed to `origin`, the decision is recorded (kind `edit_skill`, decider `person`), and the agents are read
again. Response: the skill as `GET` answers it, with `commit`, the short id of the commit (left out when nothing
changed). Errors: `409` no `[harness]`, or the skill is `moebius`, which is built in and read only; `422` an invalid
name or description; `500` as for `PUT /v1/agents/{name}`.

### `DELETE /v1/skills/{name}`

A person deletes a skill and its directory, committed and pushed the same way (`Delete the skill <name>`, kind
`edit_skill`). Response: `null`. Errors: `404` no such skill; `409` no `[harness]`, the skill is `moebius`, or an agent
names it (the message names the agents); `500` as for `PUT`.

### `GET /v1/mcp-servers`

The servers of `mcp.toml` in the harness configuration repository, by name, for an editor to offer: an array of
`{"name": string, "kind": "stdio" or "remote", "error": string}`. `error` is left out unless the server cannot be used
(then `kind` is `unknown` when the table is too broken to tell). Commands, arguments, environments, URLs and headers are
never included. The array is empty without `[harness]` or when `mcp.toml` cannot be read.

### `GET /v1/mcp-servers/{name}`

A server of `mcp.toml` as its table says: `{"name": string, "command": string, "args": [string], "env": {name:
string}}` for a stdio server, or `{"name": string, "url": string, "headers": {name: string}}` for a remote one; empty
`args`, `env` and `headers` are left out. A value holds its `${NAME}` references as written, never what they stand for.
Errors: `404` no such server; `409` no `[harness]`; `422` the server or `mcp.toml` cannot be used (the message says why).

### `PUT /v1/mcp-servers/{name}`

A person adds or changes a server. Request: the shape `GET` answers, without `name`. It is checked as reading `mcp.toml`
checks it (see [harness configuration](harness-configuration.md#mcptoml)): one of `command` and `url`, only the keys
that belong to it, and `${NAME}` only for a `NAME` in `mcp_env` of `[harness]`. The server's table in `mcp.toml` is
rewritten, keeping the file's other tables and comments, committed (`Add the MCP server <name>` or `Update the MCP
server <name>`) and pushed, the decision is recorded (kind `edit_mcp_server`, decider `person`), and the agents are read
again. Response: the server as `GET` answers it, with `commit`. Errors: `409` no `[harness]`; `422` the server would not
read, with the reason; `500` as for `PUT /v1/agents/{name}`.

### `DELETE /v1/mcp-servers/{name}`

A person deletes a server from `mcp.toml`, committed and pushed the same way (`Delete the MCP server <name>`, kind
`edit_mcp_server`). Response: `null`. Errors: `404` no such server; `409` no `[harness]`, or an agent names the server
(the message names the agents); `500` as for `PUT`.

### `GET /v1/models`

The models the server uses and where it reaches them. Never a key: an upstream or the decision model says only whether it
has one. Response:

```json
{
  "upstreams": [{"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_set": true}],
  "agents": [{"name": "Coder", "model": "qwen3.8-flash-next", "protocol": "openai", "thinking": "medium", "token_budget": 2000000, "built_in": false}],
  "teacher": {"model": "qwen3.8-flash-next", "protocol": "openai", "timeout_seconds": 20},
  "decision_model": {"url": "http://127.0.0.1:8081/v1", "model": "moebius-decider", "timeout_seconds": 20, "api_key_set": false}
}
```

- `upstreams` are the `[[router.upstreams]]` the server runs with, in the order of the configuration. `api_key_set` is
  true when the upstream has an `api_key_env` or an `api_key_secret`; the server does not start when that variable is not
  set or no key is stored under that name. Neither the key nor the name of the variable or the secret is shown. The configuration names no models an upstream serves, so none are listed.
- `agents` are the agents of [`GET /v1/agents`](#get-v1agents), with the same fields.
- `teacher` and `decision_model` are `null` without a `[teacher]` or `[decision_model]` table.
- A user name and password in a URL are removed from `base_url` and `url`.

### `GET /v1/plugins`

The plugins the plugin host has loaded, which answer the decisions (see [the plugin API](plugin-api.md)). Response:

```json
{
  "time_limit_seconds": 30,
  "plugins": [{"name": "guard", "version": "0.1.0", "kinds": ["decider"], "decides": ["tool_call"]}]
}
```

`time_limit_seconds` is how long the host lets a call into any plugin take before it counts as refused. For each plugin,
by name, `kinds` are the kinds of its manifest (`forge`, `harness`, `runner`, `decider`, `router`, `view`) and `decides`
the kinds of decision it answers now, in alphabetical order: those routed to it, and those the decision model took over
with a merged takeover.

### `GET /v1/settings`

Every key of [`moebius.toml`](configuration.md) with its value in the file, or its default, for the Settings and
Models pages to build their forms from. The file is read afresh, so a change made by hand shows. Response:

```json
{
  "file": "/srv/moebius/moebius.toml",
  "values": {
    "listen": {"value": "127.0.0.1:7171", "default": true},
    "board.round_limit": {"value": 7, "default": false},
    "budgets.day_tokens": {"value": null, "default": true},
    "router.upstreams": {"value": [{"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_secret": "anthropic"}], "default": false},
    "agents": {"value": {"Coder": {"model": "qwen3.8-flash-next", "token_budget": 2000000}}, "default": false}
  },
  "secrets": ["anthropic"],
  "restart_needed": false,
  "restart_keys": []
}
```

- `file` is the absolute path of the configuration file.
- `values` has an entry for every key of the reference, by its dotted name (`board.round_limit`; `harness.mcp_env`;
  `decision_model.shadow`), with the whole `router.upstreams` (an array of objects) and the whole `agents` (an object of
  agents by name). `value` is what the file says, as JSON, or the default when `default` is true; `null` when the key
  has no default and the file does not set it. Paths are as written in the file.
- `secrets` are the names of the stored API keys, never the keys. An upstream or the decision model names one with
  `api_key_secret`, and an environment variable with `api_key_env`; neither the variable's value nor a key is ever
  in an answer. A user name and password in `harness.url`, `decision_model.url` and the upstreams' `base_url` are
  removed.
- `restart_keys` are the keys whose value in the file is not the one the running server started with and that the
  server uses only after it restarts (see [Editing the file from the app](configuration.md#editing-the-file-from-the-app));
  `restart_needed` is true when there are any.

### `PUT /v1/settings`

A person changes `moebius.toml`. Request: `{"set": {...}, "remove": [...]}`, both optional.

- `set` maps a dotted key to its new value: a JSON string, number, boolean or array of them for a key, an object for a
  table (`"teacher": {"model": "m"}` replaces the table, `"agents.Coder": {...}` replaces that agent), an array of
  objects for the router's upstreams (`"router.upstreams": [...]` replaces them all). One upstream is addressed by its
  position: `router.upstreams.0` replaces the first, the position after the last adds one. A key or table that is not
  in the file is added. A value is never `null`: remove the key instead.
- `remove` lists dotted keys or tables to take out (`budgets.day_tokens`, `teacher`, `agents.Coder`,
  `router.upstreams.1`); one that is not in the file is ignored. Removals are made before the sets.

The change is made to the text of the file, so its comments and the order of its keys stay. The result is read as
`moebius serve` would read it; if that fails (an unknown key, a value of the wrong type or out of range, an environment
variable of an upstream that is not set, a secret that is not stored, `[agents.*]` together with `[harness]`) the
answer is `422` with the message of the parse and nothing is written. Otherwise the file is written in one step (a
temporary file in its directory that replaces it), the decision is recorded (kind `edit_settings`, decider `person`,
the `set` and `remove` in its context; a URL has no user name and password there, and a key is never part of a
change), and the keys that are live take effect at once. Response: `{"restart_needed": boolean, "restart_keys":
[string]}`, as in `GET /v1/settings`: true with the keys that the running server uses only after it restarts. Errors:
`422` as above or a key that is not a key of the configuration; `500` the file cannot be read or written.

### `PUT /v1/secrets/{name}`

Stores an API key under `name` (letters, digits, `-` and `_`) in `secrets.toml` of the data directory, which only its
owner can read, replacing a key stored under the name before. Request: `{"value": string}`, not blank. The key is not
returned, stored anywhere else or logged. Response: the names of the stored keys. Errors: `422` the name or the key is
not allowed.

### `DELETE /v1/secrets/{name}`

Removes the key stored under `name`. Response: the names of the keys left. Errors: `404` there is none. A key that
`api_key_secret` still names is not checked: the next start fails until the name is stored again.

### `POST /v1/server/restart`

Restarts the server. It answers `202` with an empty body, and a moment later stops the board, the schedule and the Pi
processes of the runs going (the next start fails those runs, as it does after any stop, so the board starts them
again) and starts the same program again with the same arguments, on the same configuration file. Clients lose their
connection and make it again. A server that is not the `moebius` binary only stops serving.

### `POST /v1/chats`

Starts a chat: an interactive run without a card. The agent answers the first message as a stream of run events, and
the chat then waits (status `waiting`) for the next message. It works in a new worktree on the branch `moebius/chat-<run id>`
from the repository's default branch when `repository` is given, so that what the agent builds can be offered as a
change (see `POST /v1/chats/{id}/end`), or otherwise in an empty directory `chats/<run id>` under the data directory, which is removed when the
chat ends. The chat's run uses the agent's configuration like any run; its `token_budget` counts the chat's own tokens, the
daily budget applies, and a chat does not count against the board's run limit.

Request:

| Field | Type | Meaning |
|---|---|---|
| `agent` | string | An agent of the configuration, `Moebius` or `Grilling-partner`. Without it the chat is with the Moebius agent (see [configuration](configuration.md)). With `Grilling-partner` the chat is a grilling (see `POST /v1/grillings/{id}/confirm`) and `repository` is required. |
| `message` | string | Required, not blank. Sent to the agent as it is, with no issue prompt around it. |
| `repository` | number | The id of a repository to work in. |
| `harness` | boolean | Work in the harness configuration repository instead (default false): in a worktree on the branch `moebius/chat-<run id>` of its default branch, so that the agents, skills, extensions, MCP servers and rules can be built as files and merged (see [changing it in a chat](harness-configuration.md#changing-it-in-a-chat)). The run has `harness_repository` true. Not together with `repository`. |
| `continues` | number | The id of an ended chat this one goes on from. Its agent and place (repository or harness configuration repository) are used unless `agent` or `repository` name others, and the agent is given what was said in it (the person's messages and the agent's answers, at most the last 20 000 characters) before `message`. The new chat's `said` event holds only `message`. |

Response: the `Run` (`purpose` `chat`, `card_id` null). Errors: `404` no such repository or no run `continues`; `422`
the message is blank, the agent does not exist, it is a built-in agent and neither its own `[agents.<name>]` nor
`[agents.Coder]` is configured (the message says so), it is the Grilling partner and there is no `repository`, or
`continues` is not a chat or has not ended (write to it with `POST /v1/chats/{id}/messages`), `harness` is set together
with `repository`, or `harness` is set and the server has no `[harness]`; `500` the worktree cannot be made.

### `GET /v1/chats`

The chats, newest first. Response: an array of `Chat`. A chat's history is its run's events, read with
`GET /v1/runs/{id}/events` and followed on `GET /v1/events`.

### `POST /v1/chats/{id}/messages`

A person writes to a chat that has not ended. The message is stored as a `said` run event (secrets removed) and sent to
the agent as a prompt: when the agent is waiting it answers at once; when it is working the message is delivered when it
is done with the current turn (a follow-up). To change what a working agent is doing, steer it with
`POST /v1/runs/{id}/steer`.

Request: `{"message": "..."}`; `message` is required and must not be blank.

Response: the `Run`. Errors: `404` no such run; `422` the run is not a chat, the chat has ended, or the message is blank.

### `POST /v1/chats/{id}/seen`

A person has read the chat up to the event `seq`: `{"seq": number}`. The chat's `seen` becomes that `seq`, unless it is
already further, since reading never goes back. The desktop app sends it while the chat is open on its Chat page, and
counts on the sidebar's Chat entry the chats that are unread. Response: `{}`. Errors: `404` no such run; `422` the run
is not a chat.

### `POST /v1/chats/{id}/end`

A person ends a chat that has not ended: Pi is asked to shut down (its standard input is closed; a Pi that has not exited
after 10 seconds is killed). With a repository, what the agent left in the worktree is committed on the chat's branch
(a chat that changed nothing has still succeeded), and then the worktree, or the empty directory, is removed.

If the chat's branch then has commits that the repository's default branch does not have, they are offered as a change
that has no card: a `Change` with `card_id` null on the branch `moebius/chat-<run id>`, published through the
repository's forge (on GitHub the branch is pushed and a pull request is opened whose title is the chat's title and
whose body says it came from the chat; without a forge there is nothing to publish), an open `ready_to_merge` inbox item
with the chat's title (`card_id` null, `data.change`, `data.branch`) and the event `change_offered`. A person merges it
with `POST /v1/changes/{id}/merge`, which resolves the item; the merge commit's message is `Merge <branch>`. A chat that
changed nothing, or has no repository, offers nothing, and a chat that ends as `failed` offers nothing. If the offer
fails, the chat's run gets a `moebius_error` event saying why and the work stays on the branch. A chat that is stopped
offers what it made in the same way.

A chat in the harness configuration repository (`harness` true) is committed the same way and offered as a change of
that repository: a `Change` with `repository_id` null and `harness_repository` true, no card and no pull request, on the
branch `moebius/chat-<run id>`, with the same `ready_to_merge` inbox item and `change_offered` event. Before it is
offered, the branch is checked as a branch of a Retro proposal is: it may change only harness configuration files
(`agents/`, `skills/`, `extensions/`, `rules.md`, `mcp.toml`), and every agent that could be read on the default branch
must still be readable with it. A branch that passes is pushed to `origin`. A branch that fails is not offered: the chat's
run gets a `moebius_error` event saying why, and the branch stays in the clone.

A grilling (a chat with `Grilling-partner`) commits nothing and offers no change: before the worktree is removed Moebius
reads what the agent left in it (see `POST /v1/grillings/{id}/confirm`) and stores a `Grilling`, whose status and id are
the event `grilling_ended`.

The chat ends as `succeeded`, which publishes `run_ended`; the change is offered before that.

Request: `{}`.

Response: the `Run`, as it was when the request was made. Errors: `404` no such run; `422` the run is not a chat or has
ended.

### `POST /v1/retros`

Runs the Retro of a day now: the day's errors are collected into a report and, when the day had errors and `[harness]`
is configured, the Retro analyst starts (see [harness configuration](harness-configuration.md#the-retro)). The analysis
goes on after the response, which has the status `analysing`; its end is the event `retro_done`. Without `[harness]`, or
without errors, no run is made and the Retro is `reported` at once. The server does this by itself once a day, at
`retro_at` (see [configuration](configuration.md)).

Request: `{}` for today, or `{"date": "YYYY-MM-DD"}` for a local day. Response: the `RetroView`. Errors: `409` the day has
a Retro already; `422` the date is not a date.

`RetroView`: `date`, `status` (`reported`: the report is all there is; `analysing`: the analyst is running; `proposed`: it
ran and its proposals are stored, there may be none; `failed`: its run failed or was stopped, or the server stopped
meanwhile), `run_id` (the analyst's run, or null), `warnings` (array of strings: the branches left out and why, or why it
failed), `at`, `report` and `proposals` (array of `Proposal`).

`report`: `{"date", "errors": [...]}`, in the order in time. Each error is `{"kind", "run", "card", "agent", "text"}`;
`run`, `card` and `agent` are null where the error has none, and `text` is at most 500 characters, with secrets removed as
stored. The kinds are `tool_failed` (a tool call that ended in error: the tool and the start of what it said, not a call
Moebius refused), `tool_refused` (the tool and the rule's reason), `run_failed` (the run's last `moebius_error`),
`run_stopped`, `steered` (a person's message to a running run), and `checks_failed`, `conflict` and `review_findings`
(a card sent back from In review to In progress, with its note). The runs of the Retro analyst itself are left out.

`Proposal`: `id`, `retro_date`, `branch` (`proposal/<date>-<name>`, also on the harness repository's `origin`), `title` (the
subject of its commit), `body` (the rest of the message, without its `Cards:` line), `agents` (the names of the agents
whose files it changes), `cards` (the ids of the cards whose errors it addresses, from the `Cards: 48, 50` line that ends
its commit message; empty without one), `status`, `trials` (array of `Trial`, oldest first) and `at`.

`status` of a proposal: `open` (not tried yet), `trying` (its trials are going on), `positive` (every trial was positive),
`negative` (some trial was), `untried` (no trial was negative, but some was skipped or failed, or it names no cards, so
nobody knows), `merged` or `dismissed`. A `positive` or `untried` proposal has an open inbox item of the kind `proposal`.

`Trial`: `id`, `proposal_id`, `card_id` (the card it was tried on; null for the one trial of a proposal that names no
cards), `run_id` (the trial's run, or null when none was started), `outcome` (`positive`, `negative`, `skipped`: there was
nothing to try it on, or `failed`: it could not be tried), `reason` (text), `measurements` (an object, `{}` when
nothing was run) and `at`. `measurements` is `{"trial": {...}, "original": {...}}`: for the trial run `succeeded`,
`errors` (the kinds of the addressed errors that happened in the run), `checks` (true, false, or null when the repository
has no checks or the run made nothing to check), `tokens` and `rounds` (always 1); for the card's first code run `errors`
(the kinds of the card's errors in the report), `checks_failed` (the card was once sent back because the repository's
checks failed), `rounds` (the card's code runs) and `tokens`. See [the trials](harness-configuration.md#the-trials).

### `GET /v1/retros`

The Retros, the latest day first. Response: an array of `{"date", "status", "run_id", "errors" (how many the report
holds), "proposals" (how many), "at"}`.

### `GET /v1/retros/{date}`

One Retro as the `RetroView`. Errors: `404` the day has none.

### `GET /v1/proposals`

All proposals, the newest first. Response: an array of `Proposal`, each with its trials.

### `POST /v1/proposals/{id}/trial`

Tries a proposal now, at any hour, as the schedule does inside `[trial_window]`: its status becomes `trying`, the trials
go on after the response, and each trial ends with the event `trial_done` and the proposal with `proposal_status`.
Request: no body. Response: the `Proposal`, with the status `trying`. Errors: `404` no such
proposal; `409` the proposal is not `open`, or another proposal is being tried (only one is at a time); `422` there is no
harness configuration repository.

### `POST /v1/proposals/{id}/merge`

A person merges a `positive` or `untried` proposal: its branch is merged into the default branch of the harness
configuration repository's clone with a merge commit, pushed to `origin`, and the agents are read again. Then the decision
is recorded (kind `merge_proposal`, choice `merge_proposal`, decider `person`), the proposal's inbox item is resolved and
its status becomes `merged`. A merge that conflicts, and a push that fails, change nothing. Request: `{"reason": string}`
(optional). Response: the `Proposal`. Errors: `404` no such proposal; `409` the proposal is neither `positive` nor `untried`,
or its branch does not merge cleanly; `422` there is no harness configuration repository.

### `POST /v1/proposals/{id}/dismiss`

A person dismisses a `positive` or `untried` proposal: the decision is recorded (kind `merge_proposal`, choice `dismiss`,
decider `person`), the inbox item is resolved and the status becomes `dismissed`. The repository is not changed. Request
and errors as for `merge`, without the conflict. Response: the `Proposal`.

### `GET /v1/decision-model`

What the decision model has measured (see [the decision model](decision-model.md)). Response, a `DecisionModelView`:

```json
{"configured": true, "model": "moebius-decider", "agreement": 0.95, "min_judgements": 100,
 "kinds": [{"kind": "tool_call", "shadows": 120, "agreement": 0.97, "failed": 2, "dropped": 0, "routed": false}],
 "takeovers": [{"id": 1, "kind": "tool_call", "agreement": 0.97, "count": 120, "status": "open", "at": 1790000000}]}
```

`configured` is false without a `[decision_model]` table, and then `model`, `agreement` and `min_judgements` are null and
`kinds` is empty. Otherwise `model`, `agreement` (the share a takeover needs) and `min_judgements` are the configuration's.
`kinds` has an entry, kinds in alphabetical order, for each kind with a shadow, a dropped request or a merged takeover:
`shadows` (held-out judgements the model was asked about), `agreement` (the share of them where it chose the recorded
choice, null before there is a shadow), `failed` (the shadows where it failed, counted as disagreement), `dropped`
(requests dropped because the queue was full, since the server started) and `routed` (the model decides the kind now).
`takeovers` are all of them, oldest first.

### `GET /v1/takeovers`

All takeovers, oldest first. Response: an array of `Takeover`: `id`, `kind`, `agreement` (between 0 and 1, when it was
proposed), `count` (the shadows it was measured on), `status` (`open`, `merged`, `dismissed` or `released`) and `at`.
An `open` takeover has an open inbox item of the kind `takeover`.

### `POST /v1/takeovers/{id}/merge`

A person lets the decision model take the kind over. The decision is recorded (kind `takeover`, choice `merge`, decider
`person`), the inbox item is resolved, the status becomes `merged` and the kind is routed to the model from now on and
after a restart. Request: `{"reason": string}` (optional). Response: the `Takeover`. Errors: `404` no such takeover; `409`
the takeover is not `open`.

### `POST /v1/takeovers/{id}/dismiss`

A person refuses an `open` takeover: recorded as choice `dismiss`, the inbox item is resolved and the status becomes
`dismissed`. The kind is proposed again only after `min_judgements` more shadows. Request, response and errors as for
`merge`.

### `POST /v1/takeovers/{id}/release`

A person hands a `merged` kind back to the decider it had: recorded as choice `release`, the status becomes `released`.
Request and response as for `merge`. Errors: `404` no such takeover; `409` the takeover is not `merged`.

### `GET /v1/grillings`

All grillings, the newest first. Response: an array of `Grilling`.

### `GET /v1/grillings/{id}`

One `Grilling`. Errors: `404` no such grilling.

### `POST /v1/grillings/{id}/confirm`

A grilling is a chat with the built-in agent `Grilling-partner` in a repository (`POST /v1/chats`). The agent works in the
chat's worktree and ends by writing exactly one of `.moebius/grilling/issues.json` (a JSON array of `{"title": string,
"body": string, "blocked_by": [number]}`, where `blocked_by` are indexes into the array) or `.moebius/grilling/report.md`
(a research report). It never commits or pushes them. When the chat ends or is stopped, Moebius reads the files before
the worktree is removed and stores a `Grilling`:

| Result | `status` | Inbox item |
|---|---|---|
| `issues.json` is a list of issues, each with a title, whose `blocked_by` are in range, not the issue itself and not in a circle | `proposed` | `grilling_issues`, titled "N issues from a grilling" |
| no `issues.json`, and a `report.md` that is not blank | `reported`, with `report` | `grilling_report` |
| `issues.json` exists but is broken or does not hold together (it decides even when a report is there), or neither file exists | `empty`, with `reason` | none |

The person reads the proposal, may edit, drop and reorder the issues, and confirms with the final list. Request:
`{"issues": [{"title": string, "body": string, "blocked_by": [number]}]}`; `body` and `blocked_by` may be left out, and
`blocked_by` indexes this list, not the proposed one. The server

1. checks the list as it checked the agent's (at least one issue, every title not blank, no `blocked_by` out of range, the
   issue itself or in a circle) and that the status is `proposed`, and sets the status to `confirmed`;
2. makes one card per issue in the repository's `Backlog`, each blocked by the cards of its `blocked_by` (their numbers
   are the card's dependencies, as `card.blocked_by` reads them), blockers first. The cards are `confirmed`
   (`card.confirmed`), so `backlog.luau` moves them on to `Ready` without approval, where `ready.luau` holds each until
   its blockers are done. Without a forge the cards are Moebius cards, authored by `person`. On a GitHub repository each
   issue is opened first through the GitHub API, its body ending in a line `Blocked by #3, #4` for the numbers of the
   issues that block it (the line the poll reads), and its card gets the issue's number, so that the poll finds the card
   existing; its `author` is `person`;
3. records the decision (kind `confirm_grilling`, choice `confirm_grilling`, decider `person`, the context holding the
   grilling's id and the final list as JSON) and resolves the inbox item.

If opening an issue fails the request fails with the error of the forge, the status is `proposed` again and the issues
already opened stay on GitHub; the cards made so far stay. Response: the `Grilling`, `confirmed`. Errors: `404` no such
grilling; `422` the grilling is not `proposed`, or the list is not valid (the message says why).

### `POST /v1/grillings/{id}/dismiss`

A person dismisses the proposal of a `proposed` grilling, or reads the report of a `reported` one and dismisses it: the
inbox item is resolved and the status becomes `dismissed`. Nothing is made. Request: `{}`. Response: the `Grilling`. Errors:
`404` no such grilling; `422` it is not `proposed` or `reported`.

### `POST /v1/research`

Stores a brief for a research as a draft (see [the Autoresearcher](harness-configuration.md#autoresearch)). Nothing runs
until a person confirms it. Request: `{"repository": id, "goal": string, "measure": string, "better": "lower" | "higher",
"token_budget": number}`. `measure` is a shell command; it runs in a worktree of the repository with the environment
cleared (`PATH` and `HOME` only), and its result is the last number it prints on standard output; an exit status other
than 0, running longer than 10 minutes, or printing no number means the measurement failed. `better` says which way the
number should move. `token_budget` is the most tokens (input plus output) the research's runs may use, more than 0.
Response: the `ResearchView`. Errors: `404` no such repository; `422` a blank `goal` or `measure`, another `better`, or a
budget that is not more than 0.

`ResearchView`: `id`, `repository_id`, `goal`, `measure`, `better`, `token_budget`, `status`, `baseline` (the measurement
of the repository as it is, or null until it is measured), `reason` (why the research ended; empty before), `at`, `tokens`
(used by its runs so far) and `experiments` (array of `Experiment`, the first first: one for each lever).

`status` of a research: `draft` (stored, not confirmed), `confirmed` (waits for the trial window or for `start`),
`running`, `done` (a round proposed no more levers), `spent` (its runs used the budget), `stopped` (a person stopped it) or
`failed` (the baseline could not be measured, or the Autoresearcher's run failed; `reason` says which).

`Experiment`: `id`, `research_id`, `branch` (`research/<research id>-<name>`, the lever), `title` (the subject of the
lever's commit), `value` (what the measure printed on the branch; null when the measurement failed), `checks` (true or
false: the repository's own checks on the branch, or null when it has none or the measurement failed), `kept`, `reason`
(text), `change_id` (the change offered for a kept lever, or null) and `at`. A lever is kept when `value` is better than
`baseline` in the direction of `better` by more than 1 % of the baseline, and `checks` is not false.

### `GET /v1/research`

All research, the newest first. Response: an array of `ResearchView`.

### `GET /v1/research/{id}`

One research as the `ResearchView`. Errors: `404` no such research.

### `POST /v1/research/{id}/confirm`

A person confirms the brief of a draft: the decision is recorded (kind `confirm_brief`, choice `confirm`, decider
`person`) and the status becomes `confirmed`. The research then starts by itself inside `[trial_window]` (see
[configuration](configuration.md)), when no proposal is being tried and no research is running; the oldest confirmed one
goes first. Request: `{}`. Response: the `ResearchView`. Errors: `404` no such research; `409` it is not a draft.

### `POST /v1/research/{id}/start`

Starts a confirmed research now, at any hour. Its status becomes `running` and the research goes on after the response:
the baseline is measured in a fresh worktree of the repository at its default branch, then rounds follow. In a round the
Autoresearcher (a run with the purpose `research` and no card) proposes levers, each a branch
`research/<research id>-<name>` with one commit; an experiment measures each new branch and runs the repository's own
checks; each kept lever is offered as a change without a card, like the work of a chat: published through the
repository's forge, with a `ready_to_merge` inbox item (`data.change`, `data.branch`) and the event `change_offered`. A
person merges it with `POST /v1/changes/{id}/merge`. Before each round the tokens of the research's runs are compared with
the budget; the research is `spent` when they have reached it (a round may run over). It is `done` when a round leaves no
new lever branch. Every status change is the event `research_status` and every experiment the event `experiment_done`.
When the server starts, research that was `running` goes back to `confirmed`; started again it goes on with the baseline
and the experiments it has. Request: `{}`. Response: the `ResearchView`, `running`. Errors: `404` no such research; `409`
the research is not `confirmed`, or another research is running (only one is at a time).

### `POST /v1/research/{id}/stop`

A person stops a research that has not ended: the status becomes `stopped` and a research run that is going is stopped
like `POST /v1/runs/{id}/stop` does. Experiments done stay, and so do the lever branches in the repository's clone. Request:
`{}`. Response: the `ResearchView`. Errors: `404` no such research; `409` it has ended already.

## Events

`GET /v1/events` sends one JSON object per message: `{"kind": "<kind>", "data": ...}`. These are all the kinds.

| Kind | When | `data` |
|---|---|---|
| `repository_added` | A repository was added. | The `Repository`. |
| `card_added` | An issue became a card: created through the API or found on GitHub. | The `Card`. |
| `card_updated` | The title or body of a GitHub issue changed. | The `Card`. |
| `card_moved` | A card moved to another column. | `{"card": id, "from": column, "to": column}` |
| `card_waiting` | The reason a card waits changed: set, replaced, or cleared because the card moved or its script did not wait. Not sent when it stays the same. | `{"card": id, "reason": string or null}` |
| `change_offered` | A card's change, the work of a chat or a kept lever of a research was offered to a person to merge. | The `Change` (`card_id` is null for a chat's or a lever's). |
| `change_updated` | A change was published or published again, its checks started or ended, or the checks or review of its pull request changed. | The `Change`. |
| `change_merged` | A change was merged, here or on GitHub. | `{"change": id, "commit": sha or null}` |
| `change_closed` | A change's pull request was closed on GitHub without a merge. | `{"change": id}` |
| `run_started` | A harness process for a run started. | `{"run": id, "card": id or null}` (null for a chat) |
| `run_event` | Something happened in a run. | `{"run": id, "seq": number, "kind": string, "data": object}` (below) |
| `run_ended` | A run ended. | `{"run": id, "status": "succeeded" \| "failed" \| "stopped"}` |
| `board_error` | A column script failed; the card stays where it is. | `{"card": id, "message": string}` |
| `forge_error` | Polling a GitHub repository, or refreshing the clone of a repository without a forge, failed; it is tried again at the next poll or refresh (every 30 seconds without a forge). | `{"repository": "owner/repo" (GitHub) or the repository's name, "message": string}` |
| `harness_error` | An agent's files in the harness configuration repository are broken (it is left out), or refreshing the repository failed (every 30 seconds). | `{"agent": name (left out when the refresh failed), "message": string}` |
| `retro_done` | A Retro ended: it was `reported`, or the analyst's run ended and its proposals were stored (`proposed`), or it `failed`. | `{"date": "YYYY-MM-DD", "status": "reported" \| "proposed" \| "failed", "proposals": number}` |
| `retro_error` | The schedule could not start the day's Retro, the trials of a proposal, a research or the replays, or the trials of a proposal broke off; it is tried again a minute later. | `{"message": string}` |
| `replayed` | A refused tool call was replayed and its judgement has the outcome. | `{"judgement": id, "run": id, "outcome": string}` |
| `trial_done` | One trial of a proposal ended and was stored. | `{"proposal": id, "card": id or null, "outcome": "positive" \| "negative" \| "skipped" \| "failed"}` |
| `proposal_status` | A proposal's status changed: it is being tried, was tried, was merged or dismissed. | `{"proposal": id, "status": string}` (the statuses above) |
| `takeover_proposed` | The decision model earned a takeover proposal for a kind, which is in the inbox. | `{"takeover": id, "kind": string, "agreement": number}` |
| `takeover_status` | A takeover was merged, dismissed or released. | `{"takeover": id, "status": string}` (the statuses above) |
| `research_status` | A research was stored, confirmed, started, stopped or ended. | `{"research": id, "status": string}` (the statuses above) |
| `experiment_done` | An experiment of a research ended and was stored. | `{"research": id, "lever": the lever's branch, "value": number or null, "kept": boolean}` |
| `grilling_ended` | A grilling chat ended and its result was stored. | `{"grilling": id, "status": "proposed" \| "reported" \| "empty"}` |
| `lagged` | This client fell too far behind (more than 1024 events) and lost the oldest. | `{"missed": number}` |

A `run_event` has two sources, told apart by its inner `kind`:

- `tool_check`: the answer to a tool-call check, `{"tool", "tool_call_id", "decision": "allow" | "refuse", "reason"}`. See
  the [bridge protocol](bridge-protocol.md).
- Every other kind is a record Pi-Bolt wrote on its RPC channel, with its `type` as the kind (for example
  `agent_settled`, `tool_execution_end`, `response`) and the whole record as `data`. A line that is not JSON is passed on
  as kind `unparsed` with `{"type": "unparsed", "error", "text"}`.

`seq` is the number the event is stored under, the same as in `GET /v1/runs/{id}/events`. Events of a run are published in
the order of their `seq`. Every event of a run is published and stored: Pi's records, `tool_check` events, `steered` events
(`{"message"}`, a person's steer, with secrets removed), `said` events (`{"message"}`, a person's message to a chat, the first
one included, with secrets removed; stored before the agent's records of its answer) `shell_opened` events (`{"who"}`, a person opened a shell in the worktree; see `GET /v1/runs/{id}/terminal`) and `moebius_error` events (`{"message"}`) that explain why a
run failed. The one exception is a run that was still going when the server stopped: it is failed at the next start with
a `moebius_error` that is stored but not published.
