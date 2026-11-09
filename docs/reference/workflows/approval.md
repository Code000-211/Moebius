# `approval`

Pauses the run for a person: the node shows a prompt built from its parameters and the value that reached it, and the run
waits until a decision comes. Approving continues with the value, editing continues with the person's value, rejecting ends
the run `failed` with their reason, and a wait that reaches its deadline ends the run `failed` naming the deadline. This is
the one place where Moebius waits for a person: a card run never does (see [the decision](../../adr/0008-runs-never-wait-for-a-person.md)),
and this node is only reachable from a workflow run, whose wait has a deadline.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `prompt` | string | Required. The text a person reads when deciding. |
| `timeout_seconds` | integer | How long the decision is waited for. Optional; defaults to `3600`, one hour. This replaces the run's `node_timeout_seconds` for this node, so the workflow's own node timeout does not shorten or lengthen the wait. |

## Ports

- Input `in`: the value the person decides on.
- Output `out`: the value the decision leaves on the run — the value itself for an approval, the person's for an edit.
- Output `failed`: nothing leaves by it. A rejection fails the node, and a failed node ends the run.

```toml
[[nodes]]
id = "release"
kind = "approval"
params = { prompt = "Ship change {{ trigger.card.number }}?", timeout_seconds = 86400 }

[[edges]]
from = "build"
to = "release"
port = "out"

[[edges]]
from = "release"
to = "publish"
port = "out"
```

## Behaviour

- The node takes the run and its own row `waiting`; nothing else of the run runs while it waits.
- An open `workflow_approval` inbox item carries the decision: its `data` is `run`, `node`, `prompt`, `value` (what
  reached the node) and `deadline` (whole seconds since the epoch). It has no card, as the run has none. Every secret in
  the item's title and data is redacted before it is stored, as everywhere else in the store.
- A decision comes back through [`POST /v1/workflows/runs/{run_id}/resume`](../desktop-api.md#post-v1workflowsrunsrun_idresume).
  It records the decision in the node's row, resolves the inbox item and wakes the wait: `approve` continues with the
  value, `edit` with `value` from the request, and `reject` ends the run `failed` with `reason` (`rejected` when the
  request gives none). A second decision on a finished wait is refused. The value an `approve` continues with is the
  value as the run held it when it reached the node; a wait taken up after a server stop continues with the value the
  store kept for it, whose secrets are redacted, so a secret does not survive the restart and is never stored
  unredacted.
- A wait that reaches its deadline without a decision fails the node with
  `no decision for the approval before its deadline <RFC 3339>`, so the run ends `failed` and the inbox item is resolved:
  nobody is left asked by a run that is over.
- The decision stands in the node's row, and the deadline stands in the inbox item, so a server that stops while a run
  waits loses neither: at start it drives the runs left waiting again, which takes up a decision that was made or waits
  again to the deadline that was fixed. When a waiting run can no longer be replayed, because its workflow or its rows
  cannot be read or its thread cannot be started, it ends `failed` saying why and its inbox item is resolved: nobody is
  left asked by a run that is over.

## Validation

A workflow file is refused when, for an `approval` node:

- `prompt` is missing, empty or not a string:
  `node "<id>": invalid parameters for node kind "approval": "prompt" must be the text a person reads when deciding`;
- `timeout_seconds` is not a positive whole number of seconds:
  `node "<id>": invalid parameters for node kind "approval": "timeout_seconds" must be a positive whole number of seconds`.

## Running

`Approval::continues_with(value, decision)` says what one of the three decisions leaves on the run; `Approval::deadline_error`
is the failure the wait ends with. `Node::run` on its own fails, because the waiting belongs to the run: the executor of
`app/workflows/approval.rs` waits on the node's behalf, holding the node's thread and no other part of the run.
