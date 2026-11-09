# Workflow node kinds

One page per node kind, linked from the [workflow reference](../workflows.md). A node-kind card adds its page here and
does not edit another kind's page.

- [`condition`](condition.md): sends the value down the first port whose expression is true, or a fallback port.
- [`for_each`](for-each.md): runs a sub-path once per element of a list and gathers the outputs.
- [`merge`](merge.md): joins two branches and emits what arrived by each side.

A trigger kind has its page here too, named `<kind>-trigger.md` and linked from the same reference.

- [`webhook`](webhook-trigger.md): a path on the server whose deliveries start runs of the workflow.
