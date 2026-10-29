# Drive Moebius from a script

## When you want this

You want to do from a shell script or another program what the desktop app does: add a repository, create issues,
watch the board, follow a run, approve, reject, retry, merge, chat. The desktop app and the `moebius` command line
use the same API, so whatever they do, a script can do too. Every route and every type is in
[`docs/reference/desktop-api.md`](../reference/desktop-api.md). This page is a walk through the routes in the order you
need them.

## The steps

1. Start the server (`moebius serve`). It prints where its API listens (`listen`, by default `127.0.0.1:7171`) and
   creates the token file, by default `<data_dir>/token`.
2. Send the token with every request as `Authorization: Bearer <token>`. A POST needs the header
   `content-type: application/json` and a JSON object as its body, `{}` at least.
3. Read answers as JSON. A failed request has a status and a plain-text message.

The examples use `curl` and `jq` (to pick a field out of JSON; any JSON tool will do):

```sh
TOKEN=$(cat .moebius/token)
AUTH="Authorization: Bearer $TOKEN"
JSON='content-type: application/json'
API=http://127.0.0.1:7171
curl -s -H "$AUTH" $API/v1/health
```

The answer is `{"version": "<the server's version>"}`.

## Add a repository and create an issue

`POST /v1/repositories` clones `url`, which is anything `git clone` takes (a path on the server will do), and makes the
clone's checked-out branch the default branch. `POST /v1/repositories/{id}/issues` creates an issue in a repository
without a forge, as the author `person`, which is trusted, so it starts at once. A repository on GitHub, GitLab or Forgejo takes
its issues from there instead; see the reference for the `forge`, `github`, `project` and `trusted` fields.

```sh
REPO=$(curl -s -X POST -H "$AUTH" -H "$JSON" \
  -d '{"name": "demo", "url": "/srv/git/demo"}' $API/v1/repositories | jq .id)
CARD=$(curl -s -X POST -H "$AUTH" -H "$JSON" \
  -d '{"title": "Add a greeting", "body": "Add hello.txt that says hello."}' $API/v1/repositories/$REPO/issues | jq .id)
```

## Read and change a repository's files

`GET /v1/repositories/{id}/tree?path=<dir>` lists a directory of the default branch (directories first) and
`GET /v1/repositories/{id}/files/<path>` reads a file: its `text`, or `binary` or `too_large` with its `size`.
`PUT` on the same route writes the file as a commit on the default branch (pushed for a repository on GitHub), which is
what the Editor of a project's page in the desktop app does. A refused push is undone and answered with `409`.

```sh
curl -s -H "$AUTH" "$API/v1/repositories/$REPO/tree?path=" | jq -r '.entries[] | "\(.kind) \(.path)"'
curl -s -H "$AUTH" $API/v1/repositories/$REPO/files/hello.txt | jq -r .text
curl -s -X PUT -H "$AUTH" -H "$JSON" -d '{"text": "hello, world\n", "message": "Greet the world"}' \
  $API/v1/repositories/$REPO/files/hello.txt
```

## Follow the board

`GET /v1/snapshot` is everything: the boards with their cards and changes, the reasons waiting cards wait, and the open
inbox. Each card also carries what a board shows of it: its rounds and tokens, its latest run, what blocks it, its
refused tool calls and its open change's checks (see the reference). A card is in the column named by `column`.

```sh
curl -s -H "$AUTH" $API/v1/snapshot \
  | jq -r --argjson c $CARD '.boards[].cards[] | select(.id == $c) | .column'
curl -s -H "$AUTH" $API/v1/snapshot | jq '.boards[].waiting'
curl -s -H "$AUTH" $API/v1/snapshot | jq '.inbox'
```

`GET /v1/cards/{id}/history` is the card's timeline: moves with their notes, runs starting and ending, decisions and inbox
items.

```sh
curl -s -H "$AUTH" $API/v1/cards/$CARD/history | jq -c '.[] | {at, kind}'
```

`GET /v1/judgements` lists the recorded decisions, newest first, with the reason each was made for; narrow it with
`card`, `kind` and `limit`.

```sh
curl -s -H "$AUTH" "$API/v1/judgements?card=$CARD&limit=20" | jq -c '.[] | {kind, choice, decider, reason}'
```

To be told instead of asking, connect a WebSocket client to `GET /v1/events` with the same header. Every event arrives
as one JSON message `{"kind": ..., "data": ...}`; the kinds are listed in the reference. Events are not replayed, so
read the snapshot after you connect. For example, with `websocat`:

```sh
websocat -H "$AUTH" ws://127.0.0.1:7171/v1/events
```

## Follow a run

`GET /v1/cards/{id}/runs` lists the card's runs. `GET /v1/runs/{id}/events` has everything a run did, numbered by `seq`.
Ask for the events after the last one you have, and read the `kind` of each:

```sh
RUN=$(curl -s -H "$AUTH" $API/v1/cards/$CARD/runs | jq '.[-1].id')
curl -s -H "$AUTH" $API/v1/runs/$RUN | jq '{status, summary}'
curl -s -H "$AUTH" "$API/v1/runs/$RUN/events?after=0&limit=500" | jq -c '.[] | {seq, kind}'
```

On the event stream the same events arrive as `run_event` with the same `seq`, so a client that loads the stored events
and then follows the stream drops the ones it has and sees each one once. A `tool_check` event is the answer to a tool
call check (see [Rules](rules.md)).

## Act on a card

Each action below is recorded as a decision by `person`. [Workflows](workflows.md) says when each is possible.

```sh
# Approve an issue that waits in the backlog (a card with an approve_issue item).
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"reason": "fine"}' $API/v1/cards/$CARD/approve

# Retry a card that is stuck.
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{}' $API/v1/cards/$CARD/retry

# Steer a run that is going, or stop it.
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"message": "use the existing helper"}' $API/v1/runs/$RUN/steer
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"reason": "wrong direction"}' $API/v1/runs/$RUN/stop

# Reject or merge a change that is offered. Its id is in the snapshot's changes.
CHANGE=$(curl -s -H "$AUTH" $API/v1/snapshot \
  | jq -r --argjson c $CARD '.boards[].changes[] | select(.card_id == $c and .state == "open") | .id')
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"reason": "add a test first"}' $API/v1/changes/$CHANGE/reject
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"reason": "looks good"}' $API/v1/changes/$CHANGE/merge
```

A merge answers `{"commit": "<sha>"}` and the card moves to `Done`. A merge that conflicts answers 409 and leaves the
default branch as it was.

## Chat

A chat is an interactive run without a card, with any agent of `moebius.toml` or with the Moebius agent, which a chat
without `agent` is with (see [Agents](agents.md)).

```sh
CHAT=$(curl -s -X POST -H "$AUTH" -H "$JSON" \
  -d '{"agent": "Coder", "message": "What does the board engine do?", "repository": '$REPO'}' $API/v1/chats | jq .id)
curl -s -H "$AUTH" "$API/v1/runs/$CHAT/events?after=0" | jq -c '.[] | {seq, kind}'
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"message": "And the poll?"}' $API/v1/chats/$CHAT/messages
curl -s -H "$AUTH" $API/v1/chats | jq -c '.[] | {id, agent, status, title}'
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{}' $API/v1/chats/$CHAT/end
```

The chat is `waiting` when the agent has answered and `running` while it works. A message sent while it works is
delivered when it is done with the turn; to change what it is doing, steer the run. `steer` and `stop` work on a chat
as on any run.

When a chat in a repository ends and its branch has commits that the default branch does not, they are offered as a
change without a card: the inbox gets a `ready_to_merge` item, an event `change_offered` is sent, and a person merges it
like any change, with `POST /v1/changes/{id}/merge`. This is how what the Moebius agent builds reaches the repository.

## A whole flow in one script

This creates an issue, waits until its change is offered, and merges it. It stops when the card gets stuck, so that a
person can decide.

```sh
#!/bin/sh
set -eu
TOKEN=$(cat .moebius/token)
AUTH="Authorization: Bearer $TOKEN"
JSON='content-type: application/json'
API=http://127.0.0.1:7171

REPO=$(curl -s -X POST -H "$AUTH" -H "$JSON" \
  -d '{"name": "demo", "url": "/srv/git/demo"}' $API/v1/repositories | jq .id)
CARD=$(curl -s -X POST -H "$AUTH" -H "$JSON" \
  -d '{"title": "Add a greeting", "body": "Add hello.txt that says hello."}' $API/v1/repositories/$REPO/issues | jq .id)

while true; do
  SNAPSHOT=$(curl -s -H "$AUTH" $API/v1/snapshot)
  COLUMN=$(echo "$SNAPSHOT" | jq -r --argjson c $CARD '.boards[].cards[] | select(.id == $c) | .column')
  STUCK=$(echo "$SNAPSHOT" | jq -r --argjson c $CARD '.inbox[] | select(.card_id == $c and .kind == "stuck") | .data.reason')
  [ "$COLUMN" = "Ready to merge" ] && break
  [ -z "$STUCK" ] || { echo "card $CARD is stuck: $STUCK"; exit 1; }
  sleep 5
done

CHANGE=$(echo "$SNAPSHOT" | jq -r --argjson c $CARD '.boards[].changes[] | select(.card_id == $c and .state == "open") | .id')
curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"reason": "merged by the script"}' $API/v1/changes/$CHANGE/merge
```

## What can go wrong and how you see it

| What happens | How you see it |
|---|---|
| 401 `a valid token is needed` | The token is missing or wrong. Send the content of `token_file` as `Authorization: Bearer <token>`. |
| 415 or 400 | The body is not JSON or the content type is not `application/json`. `curl -d` alone sends a form. Add `-H 'content-type: application/json'`. |
| 422 | The request is understood but not allowed: a blank title, a card in the wrong column for the action, a run that is not running, an agent that does not exist. The body says which. It is also what a JSON body with a missing or mistyped field gets. |
| 404 | The repository, card, change or run in the path does not exist. |
| 409 | A merge conflicts. Reject the change, or let the board send the card back: a card in `Ready to merge` whose change no longer merges goes back to `In progress` by itself. |
| 500 | Git, the store or a forge failed. The body says which. |
| The script waits for ever. | The card may wait for approval, for a free run slot, or for a person to merge. Read `waiting` and the inbox in the snapshot. |
| Cannot connect. | `listen` is an `ip:port`, not a host name, and is `127.0.0.1:7171` by default. Keep it on a private network: the token is the only protection. |
