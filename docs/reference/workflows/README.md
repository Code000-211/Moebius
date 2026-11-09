# Workflow node kinds and trigger kinds

One page per kind, linked from the [workflow reference](../workflows.md). A node-kind card adds its page here and
does not edit another kind's page.

Node kinds:

- [`agent`](agent.md): runs an agent of the harness configuration in a worktree the run owns, on a branch named
  for the run.
- [`code`](code.md): runs Luau that reshapes the value and outputs what it returns.
- [`condition`](condition.md): sends the value down the first port whose expression is true, or a fallback port.
- [`for_each`](for-each.md): runs a sub-path once per element of a list and gathers the outputs.
- [`approval`](approval.md): pauses the run for a person, who approves the value, replaces it, or rejects it.
- [`http`](http.md): one HTTP request to an allowed host, with secrets by name.
- [`merge`](merge.md): joins two branches and emits what arrived by each side.
- [`chat`](chat.md): starts an agent chat; the node an automation is stored as.
- [`publish`](publish.md): offers the run's branch as a change for review, checks and merge.

The folder also holds the trigger kinds, which are not node kinds:

- [`manual`](manual-trigger.md): the trigger a person starts by hand, with the typed inputs it names.
- [`schedule`](schedule-trigger.md): starts a run when a cron comes due, in the server's local time.
- [`webhook`](webhook-trigger.md): a path on the server whose deliveries start runs of the workflow.

And the page for cancelling a run, which is not a node kind either:

- [`cancel`](cancel.md): ends a run that has not ended, at each status it can be in, and leaves its worktree and branch as they are.
