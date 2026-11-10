# Expressions

A parameter value may be filled from an earlier node with an expression. An expression is written between `{{` and
`}}` in a string and is Luau, evaluated in the same sandbox column scripts use. Every node kind's parameters are filled
this way, so a node kind does not have to implement expressions itself.

```toml
[[trigger]]
kind = "test"

[[nodes]]
id = "build"
kind = "test"
params = { command = "cargo build --repository={{ trigger.repository }}" }

[[nodes]]
id = "report"
kind = "test"
params = { command = "echo '{{ node(\"build\").output }}'", attempts = 1 }
```

## Scope

Only three names are visible to an expression:

- `trigger`: the value the workflow's trigger produced.
- `input`: the value that reached this node through its input edge.
- `node("<id>")`: the value an earlier node produced, by its id. Calling it with an id that has not run is an error.

The trigger, input and node values are Lua values made from their JSON: objects and arrays become tables, and a missing
member is `nil`. Nothing else is visible. There are no standard libraries, no files, no network, no processes and no
other globals, including `_G`, `require`, `io`, `os`, `load`, `loadfile`, `dofile` and `debug`.

## What a string becomes

A string with no `{{` is a literal parameter and is not evaluated. A string that is exactly one expression, with no
text around it, becomes that expression's value with its type kept:

```toml
params = { attempts = "{{ trigger.attempts + 1 }}", command = "{{ input.command }}" }
```

A number, boolean, table or `nil` returned by such an expression becomes the corresponding JSON value. A Lua table with
the keys `1` through `n` and nothing else becomes a JSON array; any other table becomes a JSON object.

When an expression is part of a longer string, its value is inserted into the text as a string: a string value as it is,
and any other value as its JSON form. A string with several expressions inserts each one in turn.

```toml
params = { command = "cargo test -p {{ input.package }} -- --nocapture" }
```

A `{{` without a closing `}}` is an error, not literal text. Object keys and values that are not strings are not
evaluated; only string parameter values are.

## Limits and errors

The expression runs under the sandbox's time and memory limits and a limit on the work it may do (instructions). A limit
stops the expression and the node fails closed: no partially evaluated value is used. The error names the limit it hit,
either `the expression ran out of time`, `the expression ran out of instructions` or `the expression ran out of memory`.

A value nested deeper than 100 levels — a table that holds itself, for instance — is refused the same way, as
`the expression returned a value nested deeper than 100 levels`, rather than followed until the stack is gone.

An error in an expression is reported with the node's id and the parameter's name. A parameter nested in a table or
array uses `.key` and `[n]` after its name; for example a failure in the second item of `args` in the `build` node is
reported as node `"build"`, parameter `"args[1]"`.

The module that does this is `crates/moebius-core/src/workflows/decorate.rs`; `fill` evaluates all the parameters of a
node against the values earlier in the workflow.
