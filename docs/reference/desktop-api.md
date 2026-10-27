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
| `404` | The repository, card or change in the path does not exist. |
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
back to coding, or null).

`Change`: `id`, `repository_id`, `card_id` (or null), `branch`, `state` (`open`, `merged` or `closed`), `pull_request`
(the number on GitHub, or null), `checks` (`pending`, `success`, `failure` or null), `review` (the latest review's state in
lower case, such as `approved`, or null), `head` (the published commit), `published_at`, `checks_head` (the commit
`checks` are for), `failed_checks` (the failing checks of a pull request, one per line, or null) and `check_output` (the
end of the output of the repository's own checks, or null). A change's `branch` is `moebius/card-<number>`.

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
                "column": "Backlog", "created_at": 1790000000}],
     "waiting": {},
     "changes": []}
  ],
  "inbox": []
}
```

`boards` has one entry per repository, with all its cards and changes. `waiting` maps the id of each card whose column
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
request), the change becomes `merged`, its inbox items are resolved and its card moves to `Done`.

Request: `{"reason": "..."}`; `reason` is optional, but the body is not: send `{}`.

Response: `{"commit": "<sha of the merge commit>"}`. Errors: `404` no such change; `422` the change is not `open`; `409` the
branch does not merge cleanly; `500` the forge failed.

## Events

`GET /v1/events` sends one JSON object per message: `{"kind": "<kind>", "data": ...}`. These are all the kinds.

| Kind | When | `data` |
|---|---|---|
| `repository_added` | A repository was added. | The `Repository`. |
| `card_added` | An issue became a card: created through the API or found on GitHub. | The `Card`. |
| `card_updated` | The title or body of a GitHub issue changed. | The `Card`. |
| `card_moved` | A card moved to another column. | `{"card": id, "from": column, "to": column}` |
| `card_waiting` | The reason a card waits changed: set, replaced, or cleared because the card moved or its script did not wait. Not sent when it stays the same. | `{"card": id, "reason": string or null}` |
| `change_offered` | A card's change was offered to a person to merge. | The `Change`. |
| `change_updated` | A change was published or published again, its checks started or ended, or the checks or review of its pull request changed. | The `Change`. |
| `change_merged` | A change was merged, here or on GitHub. | `{"change": id, "commit": sha or null}` |
| `change_closed` | A change's pull request was closed on GitHub without a merge. | `{"change": id}` |
| `run_started` | A harness process for a run started. | `{"run": id, "card": id}` |
| `run_event` | Something happened in a run. | `{"run": id, "kind": string, "data": object}` (below) |
| `run_ended` | A run ended. | `{"run": id, "status": "succeeded" \| "failed" \| "stopped"}` |
| `board_error` | A column script failed; the card stays where it is. | `{"card": id, "message": string}` |
| `forge_error` | Polling a GitHub repository failed; it is tried again at the next poll. | `{"repository": "owner/repo", "message": string}` |
| `lagged` | This client fell too far behind (more than 1024 events) and lost the oldest. | `{"missed": number}` |

A `run_event` has two sources, told apart by its inner `kind`:

- `tool_check`: the answer to a tool-call check, `{"tool", "tool_call_id", "decision": "allow" | "refuse", "reason"}`. See
  the [bridge protocol](bridge-protocol.md).
- Every other kind is a record Pi-Bolt wrote on its RPC channel, with its `type` as the kind (for example
  `agent_settled`, `tool_execution_end`, `response`) and the whole record as `data`. A line that is not JSON is passed on
  as kind `unparsed` with `{"type": "unparsed", "error", "text"}`.

All of a run's events are also kept in the store, with a sequence number, together with `moebius_error` events
(`{"message"}`) that explain why a run failed. Those are stored, not published.
