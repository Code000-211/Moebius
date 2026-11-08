# `agent`

Runs one agent of the [harness configuration](../harness-configuration.md#agentsnametoml) in a worktree the workflow
run owns, on a branch named for the run, and takes the node's value from the fenced JSON block the agent ends with.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `agent` | string | Required. The name of an agent of the harness configuration, as `agents/<name>.toml` names it. |
| `prompt` | string | Required. What the agent is told. It may hold [expressions](expressions.md), filled from the run's trigger value as `trigger` and from the value that reached the node as `input`, before the agent starts. |
| `output` | string | Required. The name of the node's one output port, and the name of what the agent's block holds. |

## Ports

- Input `in`: the value from the edge that reaches the node, in scope as `input`.
- Output `<output>`: the `output` parameter names the port, so an edge leaves by `port = "summary"` for
  `output = "summary"`. Its value is what the agent's block held.

```toml
[[nodes]]
id = "summarise"
kind = "agent"
params = { agent = "Coder", output = "summary", prompt = """
Read the changelog of {{ trigger.release }} and say what it means.

End with a fenced JSON block holding {"summary": "<one paragraph>"}.
""" }

[[edges]]
from = "trigger"
to = "summarise"
port = "out"

[[edges]]
from = "summarise"
to = "report"
port = "summary"
```

## Behaviour

- The first agent node of a run checks the run's repository out into a worktree of its own, on the branch
  `moebius/workflow-run-<workflow run id>` of the repository's clone, and the run keeps that worktree's path with
  itself. The later agent nodes of the same run work in that worktree and on that branch, so what one agent left is
  there for the next; one work of a run ends as one branch.
- The node asks the agent to end its final message with a fenced JSON block; the node's value is that block, the last
  one of the answer, parsed as JSON. The `output` parameter names the port the value leaves by, and is not a key
  inside the value: the block holds the value whole.
- An expression of `prompt` sees the value the trigger produced as `trigger` and the value that reached the
  node as `input`; it cannot name another node's value, which the executor that runs the node does not pass
  to it yet.
- The agent is started through the harness picker and launcher, under the agent's own configuration, in the worktree.
  Its transcript is kept in the log of its run, as for any other run, and its run is a run of the workflow.
- When the agent ends, what it left in the worktree is committed to the run's branch; the worktree stays on disk, so
  the next node, and a person, can look at it.

## Failure

The node fails closed, and the run ends `failed` with its error, when:

- `agent`, `prompt` or `output` is missing, empty or not a string;
- the run, its repository, or the worktree the run already made, is gone;
- no agent of that name is configured, or the agent cannot be started;
- the agent's run ends any other way than `succeeded`, or ends without a final message;
- the answer holds no fenced block, or its last block is not JSON.

A node that fails leaves the worktree and the branch as the agent's run left them.

## Running

`Agent::run_node(app, runs, node_id, run_id, input)` does the node's work: it needs the workflow run, for its
repository and its worktree, and the harness picker to start the agent with. `Node::run` on its own fails, because the
plain executor gives a node no run. The executor that calls `run_node` is not part of this page's change.
