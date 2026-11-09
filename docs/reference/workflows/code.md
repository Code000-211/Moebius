# The `code` node

`kind = "code"` runs a piece of Luau and outputs what the Luau returns. It reshapes the value that reached it: pick
fields out of it, count things, build the object the next node wants. The Luau runs in the sandbox of
[workflow expressions](expressions.md) and can reach no filesystem, network or processes.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `source` | string | The Luau to run. What it returns is the node's output; a chunk that returns nothing outputs `null`. |

```toml
[[nodes]]
id = "summarize"
kind = "code"
params = { source = "local names = {} for _, user in input.users do names[#names + 1] = user.name end return { names = names, count = #names }" }
```

A parameter that is not `source`, a `source` that is not a string, and a `source` that is empty or only spaces are
refused when the file is read: `invalid parameters for node kind "code": ...`.

## The source is filled from expressions like any parameter

`source` is a parameter like every other, so its [expressions](expressions.md) are filled from the run before the Luau
compiles. Text around an expression has the value substituted into it, and a `source` that is exactly one expression
becomes the string that expression returned, which then runs. An expression that fails, or a `source` that is not a
string or is empty once filled, fails the node closed.

Luau writes a table of tables as `{{1}, {2}}`, where `{{` starts an expression. Write it as `{ {1}, {2} }`, with a
space, so the code is the code the workflow wrote.

The code also sees the whole run through its scope, so it can read a value by name instead of having it substituted
into the text:

| Name | Value |
| --- | --- |
| `trigger` | The value the trigger handed the run, or `nil` where there is none. |
| `input` | The value that reached the node. |
| `node("<id>")` | The value an earlier node produced, by its id. Calling it with an id that has not run is an error. |

So `return { branch = trigger.branch, total = node("count").total }` reads the same three values an expression is
filled from.

## Ports

- Input: `in`.
- Output: `out`.

The code chooses no port: the value leaves by `out` and the run follows the first edge that leaves by it, as for any
node that is not a router.

## Result

The node fails closed, so nothing runs after it, when:

- the code fails, or does not compile: Luau's own message, with the line it happened on, such as `attempt to index nil
  with 'name'`;
- a limit of the sandbox stops it, and the error names the limit: `the code ran out of time`, `the code ran out of
  instructions` or `the code ran out of memory`;
- the returned value is not one a node may carry: `the code returned a function JSON cannot hold` for a value JSON has
  no word for, or `the code returned a value nested deeper than 100 levels` for a value nested past the depth
  [expressions](expressions.md) states, which is also what a table that holds itself hits instead of the process.

The limits are the ones an expression has, from `board::Limits`, except for the work the code may do: 1,000,000
interrupt steps where an expression gets 100,000, since the code is a program rather than one expression. Which limit
stopped the code is known from how the sandbox stopped it and not from the words of an error message, so code that
names a limit in an error of its own — a field called `memory`, say — reports that error and no limit that never
fired.

The module is `crates/moebius-core/src/workflows/nodes/code.rs`. The executor calls `Node::run_routed` with the
trigger's value and the values of the nodes that ran before, and follows the first edge out of the node. `Node::run`
alone, which has no trigger and no earlier nodes, runs the code with its input and a `nil` trigger.
