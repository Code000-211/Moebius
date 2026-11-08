# `merge`

Joins the two branches that reach it and emits what arrived, naming the side each value arrived by: `{ "a": value }` or
`{ "b": value }` when one side may arrive, or, when the definition says both may, the ordered list of those objects.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `a` | string | Required. The id of the node whose value arrives by port `a`. |
| `b` | string | Required. The id of the node whose value arrives by port `b`. Another node than `a`, and not the trigger. |
| `both` | boolean | Optional. Both sides may arrive, so the node emits the list of the arrivals. Defaults to `false`, where exactly one side arrives. |

```toml
[[nodes]]
id = "join"
kind = "merge"
params = { a = "ship", b = "fix", both = true }

[[edges]]
from = "ship"
to = "join"
port = "out"

[[edges]]
from = "fix"
to = "join"
port = "out"
```

A parameter that is not listed here, a side that is missing, empty, not a string, named `trigger`, or the same node as
the other side, and a `both` that is not a boolean, are refused when the file is read:
`invalid parameters for node kind "merge": ...`.

## Ports

- Inputs `a` and `b`: the two branches. An edge names the port its _source_ leaves by, so it is the definition's `a`
  and `b` that say which side a node's value arrives by; the two input ports are where the canvas draws the edges in.
- Output `out`: what arrived.

## Which side arrived

A side has arrived by the time the node runs when the node it names ran earlier in the run, and its value is what that
node produced. A [`condition`](condition.md) sends the value down one port, so the branch of the other port never runs
and its side never arrives: the merge does not wait for it and the run goes on with what it got. Reaching a merge
whose sides have all not run fails the node rather than emitting nothing.

## Result

- One side arrived and `both` is not set: `{ "a": value }`, or `{ "b": value }`.
- `both` is set: the ordered list of the arrivals, `[{"a": value}, {"b": value}]`. The list follows the order the
  definition names the sides, and a side that did not arrive is left out, so a join after a `condition` is a list of
  one.
- No side arrived: `no branch reached the merge: neither "<a>" nor "<b>" ran`.
- Two sides arrived and `both` is not set: `both sides reached the merge, whose parameters wait for one: "<a>" and
  "<b>"`.

## Running

`Merge::join(arrivals)` makes the value out of the sides that arrived, given as `(port, value)` pairs in the order they
arrived; an executor that runs several branches at once calls it with the arrivals it gathered. The executor that
follows one edge at a time calls `Node::run_routed`, which takes the arrivals from the nodes that ran before the merge.
`Node::run` on its own fails, because which side arrived belongs to a run.
