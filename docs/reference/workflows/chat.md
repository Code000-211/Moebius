# `chat`

Starts an agent chat. This is the one node an automation is stored as: an automation is a workflow with a [`schedule`
trigger](schedule-trigger.md) and one `chat` node.

```toml
[[nodes]]
id = "chat"
kind = "chat"
params = { agent = "Coder", prompt = "Triage the new issues." }
```

| Key | Type | Meaning |
| --- | --- | --- |
| `agent` | string | The agent the chat is with. Optional; omitted or empty means the Moebius agent. |
| `prompt` | string | The chat's first message; it must not be blank. Required. |
| `harness` | boolean | Whether the chat works in the harness configuration repository instead of a repository of its own. Optional; defaults to false. |

A run that reaches the node starts the chat from these parameters, in the repository the run belongs to; that is how
a workflow on a `schedule` trigger becomes an automation. The node has `in` and `out` ports so that its file reads and
validates as any workflow, and its own `run` passes the value it received through unchanged for a caller that only
reads the definition.
