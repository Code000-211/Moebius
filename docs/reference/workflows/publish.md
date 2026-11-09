# The `publish` node

Offers the workflow run's branch as a change: what an [`agent`](agent.md) node committed becomes a pull request on the
repository's forge, or a branch waiting to be merged in the clone when the repository has no forge. The change goes
through the same offer as a card's change (`App::offer_branch_change`), so its review, checks and merge work exactly
as they do for a card's work; it has no card, and it belongs to the run and the repository.

```toml
[[nodes]]
id = "publish"
kind = "publish"
title = "Release {{ trigger.version }}"
body = "Offered by workflow run {{ trigger.run }}."

[[nodes]]
id = "agent"
kind = "agent"
agent = "Coder"
prompt = "Prepare release {{ trigger.version }}."
output = "summary"
```

## Parameters

| name | type | |
| --- | --- | --- |
| `title` | string | Required. The change's title, after its expressions. |
| `body` | string | Required. The change's body, after its expressions. |

Both are [expressions](expressions.md): values from the trigger and from the value that arrived at the node are
filled in when it runs. An empty result is refused.

## Ports and value

One input, `in`, and one output, `out`. The value is the change's id and kind:

```json
{"id": 12, "kind": "change"}
```

## When it fails

The node fails closed, and the run with it, when:

- the run has no agent node, so its worktree and branch do not exist: `the run has no agent node: there is nothing to
  publish`;
- the branch has no commit beyond the repository's default branch: `the run's branch moebius/workflow-run-<id> has
  nothing committed`.

## The change

The branch is `moebius/workflow-run-<run id>`. The change is recorded with `run_id` set to the workflow run and with
the repository it was offered in. Its `ready_to_merge` inbox item carries `data.run`, so the inbox shows the run it
belongs to, alongside `data.change` and `data.branch`. Merging it is a normal merge and records `merged` as the
state of the run's change; rejecting it records the rejection and closes the change, so the run's change is `closed`.
Neither needs a card.
