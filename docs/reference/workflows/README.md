# Workflow node kinds and trigger kinds

One page per kind, linked from the [workflow reference](../workflows.md). A node-kind card adds its page here and
does not edit another kind's page.

Node kinds:

- [`code`](code.md): runs Luau that reshapes the value and outputs what it returns.
- [`condition`](condition.md): sends the value down the first port whose expression is true, or a fallback port.
- [`for_each`](for-each.md): runs a sub-path once per element of a list and gathers the outputs.
- [`http`](http.md): one HTTP request to an allowed host, with secrets by name.
- [`merge`](merge.md): joins two branches and emits what arrived by each side.

Trigger kinds:

- [`manual`](manual-trigger.md): the trigger a person starts by hand, with the typed inputs it names.
- [`schedule`](schedule-trigger.md): starts a run when a cron comes due, in the server's local time.
