# Workflows

A workflow is a graph kept as a file in the harness configuration repository: one trigger starts it, nodes do the work,
and edges carry a value from one node's output port to another node. This page specifies that file exactly. Each node
kind has a section here and a page of its own in [`workflows/`](workflows/README.md); the registry in
`crates/moebius-core/src/workflows/nodes/mod.rs` has one match arm per kind. A parameter may be filled from an earlier
node with an expression: see [Expressions](workflows/expressions.md).

The file lives at `workflows/<name>.toml` under the harness configuration repository's root, beside `agents/`, `skills/`
and `mcp.toml`. The `<name>` is the file's stem and may contain only letters, digits, `-` and `_`.

The example below shows the file's shape, with placeholder kind names: a real file must name one of the kinds under
[Node kinds](#node-kinds).

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
| `kind` | string | The trigger's kind. `manual` is the trigger a person starts by hand and is read by itself, from [its page](workflows/manual-trigger.md); every other kind is resolved by the same registry that resolves a node's `kind`. |
| `params` | table | The kind's settings, read by that kind. Optional; defaults to an empty table. For `manual` these are the inputs a person gives the run. |

An edge starts at the trigger with `from = "trigger"`; `trigger` is reserved and cannot be a node's id or an edge's
target. A value leaves a trigger by its `out` port, which is what the edges leaving the trigger name.

The triggers that exist:

- [`manual`](workflows/manual-trigger.md): a person starts each of its runs, with the inputs the trigger names.

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
- a node names a kind the registry does not know, or a trigger names a kind that is neither `manual` nor one the
  registry knows: `node "<id>": unknown node kind "<kind>"`;
- a `manual` trigger's parameters are not its inputs: a message that names the input that is not one, as its
  [page](workflows/manual-trigger.md) spells them out;
- an edge leaves by a port the source node does not have: `node "<id>": edge to "<to>" leaves by port "<port>"`;
- a node that routes, so chooses its output port (see `routes()` under [the node registry](#the-node-registry)),
  reports a port with no edge leaving by it, `fallback` included: `node "<id>": port "<port>" has no edge leaving by
  it`;
- the nodes form a cycle: `node "<id>": is part of a cycle ...`;
- a node cannot be reached from the trigger: `node "<id>": is unreachable from the trigger`.

A name that is not a safe file name is refused before a path is built, so a workflow name cannot reach outside
`workflows/`.

## Reading

`WorkflowDefinition::parse(text, file)` parses a file's text but does not validate its graph. Reading through the
harness configuration repository validates both: `HarnessRepo::workflow(name)` reads `workflows/<name>.toml` and returns
a `WorkflowDefinition`, or an error that names that file. `workflows::definition::read(root, name)` is the same read for
a repository root already at hand.

## Executing a run

`App::start_workflow_run` is the one door a trigger has into a run. It takes a `WorkflowSource` (a
`WorkflowDefinition`, or the name of one in the harness configuration repository), the repository the run belongs to,
and the value the trigger starts it with. The definition is validated before anything is stored, so a graph with an
unresolved edge, a cycle, an unreachable node or a kind nobody registered cannot start.

The run is stored before the call returns, with status `queued`, and `start_workflow_run` returns that row. The nodes
then run on a thread of their own, under these limits, which a workflow sets at the top of its file and which have
defaults when it does not:

| Key | Default | What it limits |
|---|---|---|
| `node_timeout_seconds` | `300` | How long one node may run. A node still running after this fails the run. |
| `max_nodes` | `100` | How many nodes one run may execute. Asking for one more fails the run. |

A run moves from `queued` to `running` and ends `succeeded` or `failed`. Each node is one `workflow_node_runs` row,
stored with its input before the node starts and its output (or error) after it finishes, so a run's nodes are readable
while it is still going. A node gets the trigger's value, the value its predecessor returned, and the values of the
nodes that ran before it; it returns one value for the edge that leaves it and, when it chooses, the name of the output
port that edge leaves by. When no edge leaves the node that just ran, the run is `succeeded`; when a node names a port
with no edge, the run is `failed` with `node "<id>" named port "<port>", which has no edge leaving by it`. A node's error
ends the run `failed` with that error, and no later node runs. A timeout or a reached node ceiling also ends it
`failed`, with an error that says which limit was reached.

`workflows::run::execute` is the same executor without the store: it takes the definition, the starting value, the
`Limits`, a `Runner` and an `Observer`. It reports each node to the observer before and after it runs, so the caller
can record the step. The `Runner` trait runs one node: `run(node, input)` gives it the node and the value that reached
it; `run_routed(node, scope)` gives it, in a `Scope`, the trigger's value, the input and the values of the nodes that
ran before, and takes back a `Routed`, the value and the port the node named, if it named one. A run that cannot go on
ends with a `workflows::run::Error` (a node's `NodeError`, a timeout, the ceiling, a port with no edge, an edge to an
unknown node, or a step the observer could not record); its text is the run's `error`.

## The node registry

`workflows::nodes::node(kind, params)` is the extension point: it maps a kind name and its parameters to a `Node`, or
returns `NodeError::UnknownKind` for a kind no card has added yet. A node-kind card adds one arm to its match.

Every `Node` reports five things:

- `run(input)` does the node's work on the value that reached it and returns the value that leaves by one of its output
  ports;
- `run_routed(scope)` does the same work as a run asks it to: it reads the trigger's value, the node's input and the
  values of the nodes that ran before it from a `Scope`, and returns a `Routed`, the value that leaves the node and,
  when the node chooses which output port it leaves by, the name of that port, so the executor follows the edge that
  leaves by it. A node that chooses no port needs only `run`;
- `ports()` returns its named `inputs` and `outputs`, whose output names an edge's `port` uses;
- `routes()` says whether the node names the port its value leaves by, in the `port` of the `Routed` above. A node that
  names one returns `true`, and validation then refuses it for a port with no edge leaving by it; a node whose value
  leaves by its first edge whatever it returns keeps the default, `false`;
- `metadata()` returns its `kind`, a `title` and a one-sentence `description` for people.

Only the kinds listed under [Node kinds](#node-kinds) are registered; a file naming any other kind is refused.

## Node kinds

Each node kind has one section here and one page in [`workflows/`](workflows/README.md).

- [`condition`](workflows/condition.md): sends the value down the first port whose expression is true.
- [`for_each`](workflows/for-each.md): runs a sub-path once per element of a list and gathers the outputs.
- [`merge`](workflows/merge.md): joins two branches and emits what arrived by each side.

## Extending this page

A node-kind card adds its section under [Node kinds](#node-kinds), a page `workflows/<kind>.md`, its entry in the
[node kind index](workflows/README.md), and its arm in the registry. It does not edit another kind's page.
