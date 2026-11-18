# The store

One SQLite connection behind a mutex (`Store`), opened at `<data_dir>/moebius.sqlite`, with its schema migrated at
start. One module per kind of record; `mod.rs` opens the store, holds the migrations, the local clock and the row
helpers, and re-exports every record type. Large content lives in `blobs`, and every value written through a record
goes through secret redaction first.

## People

`store/people.rs` keeps the people of the organisation (ADR 0018).

| Table | Holds |
|---|---|
| `people` | Handle (unique regardless of case, without the `@`), display name, role (`owner`, `member`, `viewer`), whether the person is disabled, and when they were added. A trigger refuses every delete: a person is only disabled. |

The migration that adds the table makes the first owner (id 1, handle `owner`) and gives them what a person did before:
`cards.author` of an issue written through Moebius (on a repository without a forge, or confirmed), `card_comments.author`
of a comment written through Moebius, `judgements.decider`, `cards.held_by` and `cards.claimed_by`, each `person`
before and `person:owner` after. A card or comment read from a forge keeps its author, even the login `person`.

## Workflow runs

`store/workflows.rs` keeps what a workflow run and each of its nodes did; the engine writes them and the web app reads
them.

| Table | Holds |
|---|---|
| `workflow_runs` | Workflow name, repository, trigger kind, status (`queued`, `running`, `waiting`, `succeeded`, `failed`, `cancelled`), the run's value as JSON text, when it was stored, started and finished, and its error. |
| `workflow_node_runs` | Run id, node id, attempt, status, input and output as JSON text, when the attempt started and finished, and its error. An attempt is unique per run and node. |

The JSON in `value`, `input` and `output`, and the text of an error, is redacted with `blobs::redact` before it is
stored. A run's start is set when it first becomes `running`, and a run's or a node's finish when it first ends.
