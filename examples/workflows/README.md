# Workflow examples

Each file is a workflow of a project, kept in the harness configuration repository as
`projects/<project>/workflows/<name>.toml`. Save one into a project with
`moebius workflow save --project <project> <name> <file>`, or through the app.

| File | What it shows |
| --- | --- |
| `release-preview.toml` | A manual trigger, a code node that shapes the value, and a condition with one branch per channel. |
| `nightly-triage.toml` | A schedule trigger, an agent node that leaves a report, and an HTTP node that posts it. |
| `approved-publish.toml` | An approval node: the run waits for a person before the HTTP node publishes. |
| `nightly-backlog-sync.toml` | A schedule trigger and one HTTP node that calls `POST /v1/projects/{id}/backlog/sync` each night (see [keeping the owner's work small](../../docs/guide/software-factory.md)). |

The HTTP nodes name `api_base` as an input, so a run can point them at a test service. `tests/examples.rs` starts each
workflow: it gives the manual ones a local fake HTTP server, and the scheduled one a fake `pi-bolt` harness.
