# Cancelling a run

A run that has not ended can be cancelled, by a person or by the Moebius agent. Cancelling ends the run `cancelled`
and stops what the run was doing; a run that has ended refuses the cancel.

```sh
"$MOEBIUS" workflow cancel 4
```

The same thing over the desktop API is `POST /v1/workflows/runs/4/cancel`, answered with the run as it now stands (see
[the desktop API](../desktop-api.md)).

## What each status gives up

| The run was | What cancelling does |
| --- | --- |
| `queued` | It never starts: no node is recorded, its `started_at` stays empty, and nothing is said about a start. |
| `running` | It stops at its current node. The agent run of the node is stopped through the same stopping machinery a card's run uses, and the node is given up as one is when it times out and is recorded `cancelled`; no later node starts. |
| `waiting` | It ends, and the inbox item it left for a person is withdrawn. |
| `succeeded`, `failed`, `cancelled` | The cancel is refused: `409` with the status the run ended with. |

An `approval` wait looks at the cancellation flag while it waits, so a cancel that lands just as the wait starts
withdraws its inbox item and marks its node `cancelled` instead of writing the run back to `waiting` after the run has
ended. Once a run has ended, nothing can move it or its nodes back to `waiting`.

The agent run of the node going, if there is one, is stopped through the runs' own stopping machinery, as a person
stopping a card stops its run: the harness ends its process and the node sees the cancellation when the run ends. It
is the agent run the node recorded as belonging to this workflow run; another workflow run's agent run in the same
repository is left going. A node that runs no agent is given up rather than waited for, exactly as a node that runs
past its timeout is: the run ends and the executor stops looking at the thread.

## What is left behind

The run's worktree and branch, which a node may have opened, are left exactly as they are, with whatever is in them:
cancelling is not a cleanup. The run keeps its `error`, `cancelled by a person`, and each node keeps what it had
already recorded, so the run's page says where it stopped and the node that was going says it was cancelled.

Cancelling publishes the run's usual events, `workflow_run_ended` and `workflow_run_status` with status `cancelled`,
once, by whoever ended the run; a cancel that comes after the run ended on its own publishes nothing.
