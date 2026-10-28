# Desktop API

The desktop API is how the desktop app, or any client such as `curl`, reads the boards and acts on them. This page is for
anyone who writes a client or a script against a running server. The code is in `crates/moebius-core/src/api.rs`; the
events are published from `app.rs`, `pibolt.rs`, `check.rs`, `poll.rs` and `server.rs`.

The API is HTTP with JSON bodies plus one WebSocket, served on `listen` (default `127.0.0.1:7171`). Keep it on a private
network.

## Authentication

Every request, the WebSocket upgrade included, carries `Authorization: Bearer <token>`. The token is the content of
`token_file` (by default `<data_dir>/token`, created when the server first starts). A missing or wrong token is answered
with `401` and the plain text `a valid token is needed`.

```sh
TOKEN=$(cat .moebius/token)
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7171/v1/health
```

## Errors

A failed request has a status code and a plain-text message as its body.

| Status | Meaning |
|---|---|
| `401` | No valid token. |
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
back to coding, or null) and `retried_after` (the id of the card's latest run when a person last retried it, or null).
`Run`: `id`, `card_id` (null for a chat), `agent`, `harness`, `purpose` (`code`, `review`, other purposes of column scripts,
or `chat`), `status` (`starting`, `running`, `waiting`, `succeeded`, `failed` or `stopped`; `waiting` is a chat waiting for
the person's next message), `worktree`, `started_at`, `ended_at`, `summary` and `repository_id` (the repository a chat
works in, or null).

`Chat`: `id` (the id of its run), `agent`, `status` (that of the run), `repository_id` (or null), `started_at`, `ended_at`
and `title` (the first line of the first message, at most 80 characters).

`Change`: `id`, `repository_id`, `card_id` (or null), `branch`, `state` (`open`, `merged` or `closed`), `pull_request`
(the number on GitHub, or null), `checks` (`pending`, `success`, `failure` or null), `review` (the latest review's state in
lower case, such as `approved`, or null), `head` (the published commit), `published_at`, `checks_head` (the commit
`checks` are for), `failed_checks` (the failing checks of a pull request, one per line, or null) and `check_output` (the
end of the output of the repository's own checks, or null). A change's `branch` is `moebius/card-<number>`, or `moebius/chat-<run id>` for the work of a chat, which has no card.

`InboxItem`: `id`, `kind`, `card_id` (or null), `title`, `data` (an object that depends on the kind), `resolved_at` (null
in the snapshot, which holds only open items), `at`. Kinds the core adds: `approve_issue` (`data.reason`), `stuck`
(`data.reason`), `ready_to_merge` (`data.change`, `data.branch`) and `change_closed` (`data.change`, `data.pull_request`).
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
  "inbox": []
}
```

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

Creates an issue in a repository without a forge. Its card starts in `Backlog`, with the author `person`, which is
trusted.

Request: `{"title": "...", "body": "..."}`. `title` is required and must not be empty or blank; `body` is optional.

Response: the `Card`. Errors: `404` no such repository; `422` the title is empty, or the repository has a forge (its
issues are created there).

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
those about that card, `kind` only those of that kind (such as `retry`, `merge` or `tool_call`); `limit` (default 100, at
most 1000, at least 1) is the most it returns. A card that does not exist simply has none.

Response: an array of `{"id": number, "kind": string, "choice": string, "reason": string, "decider": string,
"outcome": string or null, "card": number or null, "run": number or null, "at": number}`. The blobs of the question's
context are not included.

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
| `agent` | string | An agent of the configuration, or `Moebius`. Without it the chat is with the Moebius agent (see [configuration](configuration.md)). |
| `message` | string | Required, not blank. Sent to the agent as it is, with no issue prompt around it. |
| `repository` | number | The id of a repository to work in. |

Response: the `Run` (`purpose` `chat`, `card_id` null). Errors: `404` no such repository; `422` the message is blank, the
agent does not exist, or it is the Moebius agent and neither `[agents.Moebius]` nor `[agents.Coder]` is configured (the
message says so); `500` the worktree cannot be made.

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

The chat ends as `succeeded`, which publishes `run_ended`; the change is offered before that.

Request: `{}`.

Response: the `Run`, as it was when the request was made. Errors: `404` no such run; `422` the run is not a chat or has
ended.

## Events

`GET /v1/events` sends one JSON object per message: `{"kind": "<kind>", "data": ...}`. These are all the kinds.

| Kind | When | `data` |
|---|---|---|
| `repository_added` | A repository was added. | The `Repository`. |
| `card_added` | An issue became a card: created through the API or found on GitHub. | The `Card`. |
| `card_updated` | The title or body of a GitHub issue changed. | The `Card`. |
| `card_moved` | A card moved to another column. | `{"card": id, "from": column, "to": column}` |
| `card_waiting` | The reason a card waits changed: set, replaced, or cleared because the card moved or its script did not wait. Not sent when it stays the same. | `{"card": id, "reason": string or null}` |
| `change_offered` | A card's change, or the work of a chat, was offered to a person to merge. | The `Change` (`card_id` is null for a chat's). |
| `change_updated` | A change was published or published again, its checks started or ended, or the checks or review of its pull request changed. | The `Change`. |
| `change_merged` | A change was merged, here or on GitHub. | `{"change": id, "commit": sha or null}` |
| `change_closed` | A change's pull request was closed on GitHub without a merge. | `{"change": id}` |
| `run_started` | A harness process for a run started. | `{"run": id, "card": id or null}` (null for a chat) |
| `run_event` | Something happened in a run. | `{"run": id, "seq": number, "kind": string, "data": object}` (below) |
| `run_ended` | A run ended. | `{"run": id, "status": "succeeded" \| "failed" \| "stopped"}` |
| `board_error` | A column script failed; the card stays where it is. | `{"card": id, "message": string}` |
| `forge_error` | Polling a GitHub repository, or refreshing the clone of a repository without a forge, failed; it is tried again at the next poll or refresh (every 30 seconds without a forge). | `{"repository": "owner/repo" (GitHub) or the repository's name, "message": string}` |
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
