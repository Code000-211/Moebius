# A test lane

The files of a project whose pull requests are built, tested and measured on a machine of the owner's before they merge:

- [`kanban.toml`](kanban.toml): the column `Ready for testing`, marked `lane = true`.
- [`lane.toml`](lane.toml): the job, for a GPU inference engine: build, tests, selftests and a measurement that runs the
  merge base and the head by turns.

Copy both to `projects/<project name>/` of the harness configuration repository and commit them to its default branch,
add a `[[lanes]]` entry to `moebius.toml` and give the repository the lane (see [the guide](../../docs/guide/test-lane.md)).
The keys are in the [reference](../../docs/reference/lanes.md#the-job-definition). Tested by
`crates/moebius-core/tests/examples.rs`.
