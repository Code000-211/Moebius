# `webhook`

A trigger that gives the workflow a path on the server. A delivery to that path starts a run of the workflow, with the
delivery's JSON body as the value the run starts with. The trigger is the workflow's own door: it needs no token, only
the path and, when the trigger names a secret, the key that secret holds.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `path` | string | Required. The one URL segment the hook answers on: letters, digits, `-` and `_`, at most 64 of them. |
| `secret` | string | The name of a key in the server's secret store (`secrets.toml` of the data directory, which `PUT /v1/secrets/{name}` writes) whose value a delivery must carry. Optional; without it anyone who knows the path can start a run. |
| `repository` | string | The name of the repository a run belongs to, for a delivery whose address names none. Optional. |

Any other key is refused. A workflow file is validated when it is read, so a trigger with no `path`, a `path` that is
not one safe segment, or a `secret` that is not a name is refused then, with the reason in the message.

The key itself never goes in the workflow file: that file is committed to a git repository and pushed to a forge, and
a secret committed there has left the server. The file only names where the key is kept.

## Ports

The trigger has no input and one output, `out`, the port an edge leaves the trigger by.

## Routes

- `POST /api/hooks/{path}` starts a run of the workflow whose trigger has this `path`, in the repository its
  `repository` names.
- `POST /api/hooks/{repository}/{path}` the same, for a delivery that names which repository the run belongs to. The
  repository in the address wins; a workflow whose trigger names no repository can be run this way.

Neither route is under `/v1/` and neither needs the server's token (see the [desktop API's
authentication](../desktop-api.md#authentication)). A delivery carries its credential as `Authorization: Bearer <key>`.

## Behaviour

- The body is taken as one JSON value and is the run's value as it is, with no fields added. It is what the trigger
  gives the first node, and a script or a node reads it with an [expression](expressions.md).
- The answer is `202` with `{"run": number, "workflow": string}` as soon as the run is stored, while its nodes run on a
  thread of their own. `run` is the id of the stored run, the same id the `workflow_run_*` events of
  `GET /v1/events` carry.
- A hook that names no secret starts a run for any delivery to its path.
- A hook that names a secret starts a run only for a delivery carrying that key. A delivery with no credential, a
  wrong one, and a hook whose named key the server does not have are all answered `404` with the plain text
  `no hook on this path` — the same answer an unknown path gets, so nothing here says which paths have hooks or what
  they ask for. The key is compared in constant time.
- A body that is not one JSON value is refused `400` saying `the body is not a JSON value: ...`. Nothing starts.
- A workflow whose trigger names no repository, addressed without one, is refused `422` saying what to name; so is a
  repository the server does not have.
- At most eight runs of one workflow may be queued or running at once through its hook. A delivery past that is refused
  `429` with a message that says to try again later; it is never dropped in silence. The ceiling is
  `workflows::trigger::webhook::MAX_ACTIVE_RUNS`.
- Two workflows whose triggers name one path are a configuration error: a delivery that opens every trigger on the
  path is refused `422` naming both, and starts nothing. A delivery that cannot open all of them is answered exactly as
  an unknown path is, so the credential check never says which workflows claim a path.

## Example

The trigger in a workflow file:

```toml
[[trigger]]
kind = "webhook"
[trigger.params]
path = "ci-ready"
secret = "ci-hook"
repository = "radiance"
```

The key is stored once, out of git:

```sh
curl -X PUT -H "Authorization: Bearer $(cat ~/.moebius/token)" \
  -d '{"value":"7f7c…"}' http://127.0.0.1:7171/v1/secrets/ci-hook
```

and CI opens the door with it:

```sh
curl -X POST -H "Authorization: Bearer 7f7c…" \
  -d '{"tag":"v0.9.5","commit":"4a1c9f2"}' http://localhost:7171/api/hooks/ci-ready
```

## Where the code is

`workflows/trigger/webhook.rs` reads the trigger and says which workflow a path leads to;
`server/api/workflow_hooks.rs` is the route that answers. A trigger kind is resolved by the trigger module that owns
it, and `webhook` is not a node kind: a workflow that names `webhook` as a node's `kind` is refused as an unknown node
kind.
