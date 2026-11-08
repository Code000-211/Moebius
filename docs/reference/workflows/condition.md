# The `condition` node

`kind = "condition"` sends the value that reached it down exactly one of its named output ports. Each port has a Luau
expression; they are evaluated in the order listed and the first that is true is taken. The edges that leave by that
port carry the value on; the edges of the other ports do not run.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `ports` | array of tables | At least one. Each has `name`, the output port's name, and `when`, its Luau expression. Names are not empty and unique. |
| `fallback` | string | Optional. The name of one more output port, taken when no `when` is true. It cannot be the name of a port in `ports`. |

```toml
[[nodes]]
id = "check"
kind = "condition"
params = { ports = [
  { name = "pass", when = "input.ok" },
  { name = "retry", when = "input.attempts < 3" },
], fallback = "give_up" }
```

A parameter that is not listed, a missing key, an empty name or expression, a repeated name, or a fallback that is also a
port is refused when the file is read: `invalid parameters for node kind "condition": ...`.

## Ports

- Input: `in`.
- Outputs: the names in `ports`, in order, then `fallback` if there is one.

An edge whose `port` is not one of these is refused by validation, like any edge by a port its source does not have. A
port with no edge leaving by it is refused too, `fallback` included: `node "<id>": port "<port>" has no edge leaving by
it`.

## Expressions

A `when` is a Luau expression in the sandbox of [workflow expressions](expressions.md), without the `{{ }}` around it.
It sees `trigger`, `input` and `node("<id>")` as that page describes, and nothing else. It is true unless it returns
`nil` or `false` (so `0` and `""` are true). It may not contain `}}`.

## Result

The value leaving by the taken port is the input, unchanged.

The node fails closed, so nothing runs after it, when:

- no `when` is true and there is no `fallback`: `no port's expression was true and no fallback is declared`;
- an expression fails or hits a sandbox limit: `port "<name>": ...`. A failing expression is not treated as false and
  does not take the fallback. Expressions after the first true one are not evaluated.

The module is `crates/moebius-core/src/workflows/nodes/condition.rs`. The executor calls `Node::run_routed` with the
trigger's value and the values of the nodes that ran before, and follows the first edge out of the node that leaves by
the port it names; a node that names no port is followed by its first edge, and a run that names a port with no edge
fails with `node "<id>" named port "<port>", which has no edge leaving by it`. `Condition::route` returns the name of the
port taken. `Node::run` alone, which has no trigger and no earlier nodes, checks that a port is taken and returns the
input.
