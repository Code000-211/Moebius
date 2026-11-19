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

`store/assignees.rs` keeps who cards and inbox items are given to.

| Table | Holds |
|---|---|
| `card_assignees` | A card and a person it is given to. |
| `inbox_assignees` | An inbox item and a person it is given to; a trigger removes them with the item. |

`store/access.rs` keeps how people prove who they are: their access tokens and the sessions of browsers.

| Table | Holds |
|---|---|
| `access_tokens` | Person, name, the SHA-256 of the token, when it was made, last used (to the minute) and revoked. |
| `sessions` | The SHA-256 of the session id, person, the access token it was opened with (none for the server's token or a one-time code the server printed), when it started and when it expires. |

`store/favourites.rs` keeps what a person keeps for themselves in the sidebar.

| Table | Holds |
|---|---|
| `favourites` | The places a person pinned: kind (`project`, `view`, `card`, `page`), repository, key, title and position. |
| `recent_places` | The places they opened lately, the same way with when, newest first; the last 20 are kept. |

Neither a token nor a session id is stored as it is. Revoking a token deletes the sessions it opened; disabling a
person deletes theirs; a session is refused once it expires, and forgotten when the next one starts.

## Projects

`store/projects.rs` keeps the projects (ADR 0019).

| Table | Holds |
|---|---|
| `projects` | Name (unique regardless of case), key (unique: an uppercase letter and 1 to 9 uppercase letters or digits, such as `WEB`) and when it was made. A rename changes only the name. |
| `project_repositories` | The repositories of each project, with their position in the project's order. |

`repositories.home_project_id` is the project a repository's new issues land in, and `cards.project_id` the project
whose board a card is on. Adding a repository makes its home project in the same transaction, with the repository's
name and a key made from it (`key_for` in `store/projects.rs`: `web` is `WEB`, a second `web-` `WEB2`); a new card takes its
repository's home project; removing a repository removes its home project when nothing is left on it, and its key can then be taken again.

The migration that adds the tables makes one project per repository, with its name and id, as its home project, and
puts every card on it; a repository whose name an earlier one has in another case gets the project name `<name>
(<id>)`, since project names are unique in any case. It gives each a provisional key, `#<id>`, which no project can choose, because SQL cannot make a
key from a name; opening the store replaces each with a key made from the name, in the order the projects were made.

## Workflow runs

`store/workflows.rs` keeps what a workflow run and each of its nodes did; the engine writes them and the web app reads
them.

| Table | Holds |
|---|---|
| `workflow_runs` | Workflow name, repository, trigger kind, status (`queued`, `running`, `waiting`, `succeeded`, `failed`, `cancelled`), the run's value as JSON text, when it was stored, started and finished, and its error. |
| `workflow_node_runs` | Run id, node id, attempt, status, input and output as JSON text, when the attempt started and finished, and its error. An attempt is unique per run and node. |

The JSON in `value`, `input` and `output`, and the text of an error, is redacted with `blobs::redact` before it is
stored. A run's start is set when it first becomes `running`, and a run's or a node's finish when it first ends.
