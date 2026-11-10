# Workflow examples

Each file is a workflow of the harness configuration repository, kept under its `workflows/` folder as
`workflows/<name>.toml`. Save one with `moebius workflow save <name> <file>`, or through the app.

| File | What it shows |
| --- | --- |
| `release-preview.toml` | A manual trigger, a code node that shapes the value, and a condition with one branch per channel. |
| `nightly-triage.toml` | A schedule trigger, an agent node that leaves a report, and an HTTP node that posts it. |
| `approved-publish.toml` | An approval node: the run waits for a person before the HTTP node publishes. |

The HTTP nodes name `api_base` as an input, so a run can point them at a test service. `tests/examples.rs` starts each
workflow: it gives the manual ones a local fake HTTP server, and the scheduled one a fake `pi-bolt` harness.
