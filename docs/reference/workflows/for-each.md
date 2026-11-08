# `for_each`

Takes a list from its input, runs a sub-path once per element with that element as the value, and emits the gathered
outputs, in order, as a list.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `rejoin` | string | Required. The id of the node where the sub-path rejoins; its output is what the sub-run gives back for the element. |
| `max_elements` | integer | The most elements one run of the node may fan out over. Optional; defaults to `100`. |

## Ports

- Input `in`: the value must be a JSON list.
- Output `each`: the value of one element, once per element, in the list's order. The edges leaving by `each` begin the
  sub-path.
- Output `done`: the list of what the `rejoin` node produced for each element, in the same order.

```toml
[[nodes]]
id = "per-file"
kind = "for_each"
params = { rejoin = "summarise" }

[[edges]]
from = "per-file"
to = "read"
port = "each"

[[edges]]
from = "read"
to = "summarise"
port = "out"

[[edges]]
from = "per-file"
to = "report"
port = "done"
```

## Behaviour

- Three elements run the sub-path three times, one after the other in the list's order, and `done` carries one list of
  three outputs.
- An empty list does not run the sub-path; `done` carries an empty list.
- A list longer than `max_elements` fails the node before any element runs. A list of exactly `max_elements` is fine.
- An input that is not a list fails the node.
- A sub-path that fails for an element fails the node with that element's index and the sub-path's error; later
  elements do not run and nothing is gathered.

## Validation

A workflow file is refused when, for a `for_each` node:

- `rejoin` is missing, empty or not a string, or `max_elements` is not a non-negative integer;
- `rejoin` names the node itself or a node that does not exist: `node "<id>": rejoin names "<rejoin>", which is not another node of the workflow`;
- no edge leaves by `each`: `node "<id>": no edge leaves by port "each", so there is no sub-path`;
- the rejoin node cannot be reached from where the `each` edges lead: `node "<id>": the sub-path leaving "each" does not rejoin at "<rejoin>"`.

## Running

`ForEach::gather(input, sub_path)` does the node's work: the executor passes a function that runs the sub-path once for
one element and returns the rejoin node's output. `Node::run` on its own fails, because the sub-path belongs to the
executor. The executor that calls `gather` is not part of this page's change.
