# Workflows belong to a project

A workflow was one file of the harness configuration repository for the whole server, at `workflows/<name>.toml`, beside
`agents/`, `skills/` and `mcp.toml` ([`reference/workflows.md`](../reference/workflows.md), "The file lives at
`workflows/<name>.toml` under the harness configuration repository's root"; `workflows::definition::DIRECTORY`; step 12i
of [`architecture.md`](../architecture.md), which reads every file there for the schedule). Its run named a repository
and nothing else tied it to a project. Since [0019](0019-a-project-is-a-board-over-any-repositories.md) a project is
where a team's work is, and the owner decided that workflows are built and run in a project's Workflows tab. This record
says what that means for the files, the store and the run. It extends 0019, whose "settings split by what they are
about" now has a third kind, and it amends 0019's list of what `projects/<name>/` holds: a project's folder holds its
column scripts and its workflows. It is one step of the restructure of the web app's navigation (issue 690).

## Considered Options

- **Workflows stay server-wide**, and the project's tab shows a filtered view of their runs.
- **A workflow lists the projects it may run in**, one file for many projects.
- **A workflow is owned by exactly one project.**
- For the repository of a run: **any repository of the server**, as before, **one of the project's repositories**, or
  **one named in the file only**.

## Decision

- **A workflow definition is owned by exactly one project**, at `projects/<project name>/workflows/<name>.toml` of the
  harness configuration repository. Names are unique within a project, and two projects may each have `release`. A
  workflow saved for one project is not seen by another. The API addresses a workflow as (project, name).
- **A run records `project_id` and `repository_id`.** `workflow_runs.project_id` is nullable and indexed, added to the
  table without touching a row: a run from before this record has none until the migration of the files gives it one.
  `repository_id` stays `NOT NULL`. `project_id` is a plain value, not a reference.
- **The repository of a run is one of the project's repositories.** A manual trigger's repository input and the
  `repository` parameter of a schedule or a webhook are checked against that set. Left out, a run belongs to the
  project's only repository when it has exactly one; a project with no repository runs in its own files repository
  ([0019](0019-a-project-is-a-board-over-any-repositories.md)). A run that names a repository outside the set, or names
  none where the project has more than one, is refused before anything is stored, with a message that lists the choices:
  - `the repository "<name>" is not one of the repositories of the project <project>; a run of its workflows belongs to one of: <a>, <b>`
  - `the project <project> has more than one repository, so the run must name one of: <a>, <b>`

  The choices are the project's repositories in the order it names them, or its files repository.
- **Automations are owned by a project too.** An automation is a workflow with a schedule trigger and a chat node, so it
  is kept in its project's folder and its store row (`automations.project_id`, with its workflow file name) says which
  project it belongs to. A schedule remembers the minute it last fired at under (project, workflow), by the project's
  id, so renaming a project leaves it alone.
- **`run_limit` and `round_limit` stay global** in `moebius.toml`.
- **Deleting a project is refused while it owns workflows or has a workflow run queued, running or waiting.** Finished
  runs keep `project_id` as a plain value; no cascade removes them, and a run whose project is gone shows it as
  deleted. **Renaming a project moves its workflows** with the rest of its folder, in the one commit that moves it.

## Consequences

- The store change is additive: a nullable column and an index on `workflow_runs`; `workflow_schedules` is keyed by
  (project, workflow) and `automations` gets a nullable `project_id`. Rows from before are server-wide (project 0 for a
  schedule, none for the rest) until the migration of the workflow files (a later issue) moves each file and re-keys its
  rows in the same step, carrying the minute a schedule last fired at over so that nothing fires twice.
- An older server cannot read `projects/<project>/workflows/`: after a downgrade it sees no workflows.
- Until that migration and the API of this record land, the server-wide files still run as before, with no project.
- A proposal of the Retro may change a project's workflow file, like its column scripts.
- Packages that install a workflow take a target project; a webhook's URL includes the project, and an old URL keeps
  answering by the workflow's name when only one project has it, with a conflict that lists the choices when several do.
  Both come with the API of a later issue.
- Fleet shows the runs of every project with the project's name; there is no second, cross-project place for
  definitions.
