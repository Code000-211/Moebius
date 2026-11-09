# Workflow node kinds

One page per node kind, linked from the [workflow reference](../workflows.md). A node-kind card adds its page here and
does not edit another kind's page.

- [`agent`](agent.md): runs an agent of the harness configuration in a worktree the run owns, on a branch named
  for the run.
- [`code`](code.md): runs Luau that reshapes the value and outputs what it returns.
- [`condition`](condition.md): sends the value down the first port whose expression is true, or a fallback port.
- [`for_each`](for-each.md): runs a sub-path once per element of a list and gathers the outputs.
- [`approval`](approval.md): pauses the run for a person, who approves the value, replaces it, or rejects it.
- [`http`](http.md): one HTTP request to an allowed host, with secrets by name.
- [`merge`](merge.md): joins two branches and emits what arrived by each side.

The folder also holds the trigger kinds, which are not node kinds:

- [`manual`](manual-trigger.md): the trigger a person starts by hand, with the typed inputs it names.
- [`webhook`](webhook-trigger.md): a path on the server whose deliveries start runs of the workflow.
