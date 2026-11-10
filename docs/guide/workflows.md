# Write a workflow

A workflow is a graph of steps kept in the harness configuration repository as `workflows/<name>.toml`. A trigger
starts it, nodes do its work, and edges carry each node's value to the next. The [workflow reference](../reference/workflows.md)
has the exact parameters of every trigger and node; this page builds one from a trigger to its last request.

## The file

A workflow has a `name`, a `description`, one `[[trigger]]`, any number of `[[nodes]]` and the `[[edges]]`
between them. A file with no nodes is a trigger that starts a run and does nothing else. An edge names the node it
leaves, the node it reaches, and the source's port it leaves by: every kind reports its output ports, and even a
single-output node's edge names the one it has. Every port of a router such as a condition must have an edge leaving
it, or reading the file fails.

Save the file with the app, `moebius workflow save <name> <file>`, or by committing it to the harness configuration
repository. Start a manual run with `moebius workflow run release-preview --repository <name> --input release=1.4.0
--input channel=stable`.

## A workflow in full

This is [`examples/workflows/release-preview.toml`](../../examples/workflows/release-preview.toml). A person gives a
version and a channel. The [code node](../reference/workflows/code.md) shapes the announcement, the
[condition](../reference/workflows/condition.md) sends it down the branch for the chosen channel, and an
[HTTP node](../reference/workflows/http.md) posts it to that channel's service.

<!-- workflow -->
```toml
# A person starts this workflow with a release's version and channel. A code node shapes the announcement, a
# condition sends it down the stable or the preview branch, and an HTTP node announces it where that channel lives.
name = "release-preview"
description = "Announce a release on the channel the person chose."

[[trigger]]
kind = "manual"
params = { inputs = [
    { name = "release", type = "text", required = true },
    { name = "channel", type = "choice", choices = ["stable", "preview"], default = "preview" },
    { name = "api_base", type = "text", default = "https://api.example.com" },
] }

[[nodes]]
id = "shape"
kind = "code"
params = { source = """
local version = input.release
local channel = input.channel
return {
    version = version,
    channel = channel,
    message = "Release " .. version .. " is out on " .. channel .. ".",
}
""" }

[[nodes]]
id = "branch"
kind = "condition"
params = { ports = [
    { name = "stable", when = "input.channel == 'stable'" },
    { name = "preview", when = "input.channel == 'preview'" },
] }

[[nodes]]
id = "announce-stable"
kind = "http"
params = { method = "POST", url = "{{ trigger.api_base }}/announce/stable", body = "{{ input }}" }

[[nodes]]
id = "announce-preview"
kind = "http"
params = { method = "POST", url = "{{ trigger.api_base }}/announce/preview", body = "{{ input }}" }

[[edges]]
from = "trigger"
to = "shape"
port = "out"

[[edges]]
from = "shape"
to = "branch"
port = "out"

[[edges]]
from = "branch"
to = "announce-stable"
port = "stable"

[[edges]]
from = "branch"
to = "announce-preview"
port = "preview"
```

Before a run of this example can post its announcement, `api_base`'s host must be allowed: an `http` node may call
only a host named in `[workflows] http_hosts` in `moebius.toml`, and that list is empty by default. For the default
`https://api.example.com`, add

```toml
[workflows]
http_hosts = ["api.example.com"]
```

or pass an `api_base` whose host is already in the list.

## What each part reads

The trigger's value is `trigger`. Every node after it also sees `input`, the value that reached it, and
`node("<id>")`, the value an earlier node produced (an [agent node](../reference/workflows/agent.md) cannot name
another node's value). A parameter is filled from those names by an expression in
`{{ }}`: `{{ trigger.api_base }}` is the input the person gave, and a whole expression as the HTTP node's `body` is
the object the code node returned. The [expressions page](../reference/workflows/expressions.md) has the grammar and
the sandbox.

A [manual trigger](../reference/workflows/manual-trigger.md) names the inputs a person fills in; a
[schedule trigger](../reference/workflows/schedule-trigger.md) starts its run from a cron expression; a
[webhook trigger](../reference/workflows/webhook-trigger.md) starts it from a request. Any of them can reach the same
nodes.

## Waiting for a person

A workflow that must not publish on its own puts an [approval node](../reference/workflows/approval.md) before the
node that publishes. The run waits, the app shows the prompt in the inbox, and the run continues only after a person
approves or edits the value, or fails closed with their reason. `examples/workflows/approved-publish.toml` is that
shape. The scheduled agent example, `examples/workflows/nightly-triage.toml`, shows an
[agent node](../reference/workflows/agent.md) whose report the next node posts.

The other node kinds are a [for-each](../reference/workflows/for-each.md) that runs its body once per item, a
[merge](../reference/workflows/merge.md) that gathers values from more than one branch, and a
[publish](../reference/workflows/publish.md) that offers the branch an agent node left as a change.
