# Workflows

A workflow is a graph kept as a file in the harness configuration repository: one trigger starts it, nodes do the work,
and edges carry a value from one node's output port to another node. This page specifies that file exactly. Each node
kind has a section here and a page of its own in [`workflows/`](workflows/README.md); the registry in
`crates/moebius-core/src/workflows/nodes/mod.rs` has one match arm per kind. A parameter may be filled from an earlier
node with an expression: see [Expressions](workflows/expressions.md).

The file lives at `workflows/<name>.toml` under the harness configuration repository's root, beside `agents/`, `skills/`
and `mcp.toml`. The `<name>` is the file's stem and may contain only letters, digits, `-` and `_`.

The example below shows the file's shape. Its kind names are placeholders: no node kind is registered yet, so a file
naming any kind is refused.

## The file

```toml
name = "release"
description = "Build the change and run its tests."

[[trigger]]
kind = "test"
params = { repository = "moebius" }

[[nodes]]
id = "build"
kind = "test"
params = { command = "cargo build" }
position = { x = 0.0, y = 1.0 }

[[nodes]]
id = "test"
kind = "test"
position = { x = 1.0, y = 2.0 }

[[edges]]
from = "trigger"
to = "build"
port = "out"

[[edges]]
from = "build"
to = "test"
port = "out"
```

Unknown keys are refused. Every table below is required unless it says otherwise.

### `name`

The workflow's name for people, as a string.

### `description`

One sentence on what the workflow does, as a string.

### `trigger`

An array of tables with exactly one entry. A file with no trigger, or with more than one, is refused. A trigger is
written `[[trigger]]` and has:

| Key | Type | Meaning |
| --- | --- | --- |
| `kind` | string | The kind name; the same registry resolves it as a node's `kind`. |
| `params` | table | The kind's settings, read by that kind. Optional; defaults to an empty table. |

An edge starts at the trigger with `from = "trigger"`; `trigger` is reserved and cannot be a node's id or an edge's
target.

### `nodes`

An array of tables, optional and empty when omitted. Each node has:

| Key | Type | Meaning |
| --- | --- | --- |
| `id` | string | The node's name for edges; unique in the workflow. |
| `kind` | string | The kind name, which the registry turns into a node. |
| `params` | table | The kind's settings, read by that kind. Optional; defaults to an empty table. |
| `position` | table | `x` and `y` on the graph's canvas, both numbers. Optional; defaults to `0.0, 0.0`. |

### `edges`

An array of tables, optional and empty when omitted. Each edge has:

| Key | Type | Meaning |
| --- | --- | --- |
| `from` | string | A node id, or `"trigger"`. |
| `to` | string | A node id. |
| `port` | string | One of the source node's output ports, as its kind reports them. |

## Validation

Reading a file parses it whole, then validates it whole; a file that fails any check yields no workflow at all. Each
refusal names the file and the node or edge it is about:

- the file is not TOML, or has a key the file does not allow;
- there is no trigger, or there is more than one: `exactly one trigger is required, found ...`;
- two nodes have the same id: `node "<id>": duplicate id`;
- a node uses the reserved id `trigger`: `node "trigger": the id is reserved for the workflow's trigger`;
- an edge names a source or target node that does not exist: `edge "<from>" -> "<to>": no node "<id>"`;
- a node or trigger names a kind the registry does not know: `node "<id>": unknown node kind "<kind>"`;
- an edge leaves by a port the source node does not have: `node "<id>": edge to "<to>" leaves by port "<port>"`;
- the nodes form a cycle: `node "<id>": is part of a cycle ...`;
- a node cannot be reached from the trigger: `node "<id>": is unreachable from the trigger`.

A name that is not a safe file name is refused before a path is built, so a workflow name cannot reach outside
`workflows/`.

## Reading

`WorkflowDefinition::parse(text, file)` parses a file's text but does not validate its graph. Reading through the
harness configuration repository validates both: `HarnessRepo::workflow(name)` reads `workflows/<name>.toml` and returns
a `WorkflowDefinition`, or an error that names that file. `workflows::definition::read(root, name)` is the same read for
a repository root already at hand.

## The node registry

`workflows::nodes::node(kind, params)` is the extension point: it maps a kind name and its parameters to a `Node`, or
returns `NodeError::UnknownKind` for a kind no card has added yet. A node-kind card adds one arm to its match.

Every `Node` reports three things:

- `run(input)` does the node's work on the value that reached it and returns the value that leaves by one of its output
  ports;
- `ports()` returns its named `inputs` and `outputs`, whose output names an edge's `port` uses;
- `metadata()` returns its `kind`, a `title` and a one-sentence `description` for people.

No node kind is registered yet. Until a kind's card lands, a file naming it is refused.

## Node kinds

Each node kind has one section here and one page in [`workflows/`](workflows/README.md). This section is empty until the
first node kind lands.

- [`http`](workflows/http.md): calls a URL on an allowed host, with secrets by name, and outputs the status, headers and body.

## Extending this page

A node-kind card adds its section under [Node kinds](#node-kinds), a page `workflows/<kind>.md`, its entry in the
[node kind index](workflows/README.md), and its arm in the registry. It does not edit another kind's page.
