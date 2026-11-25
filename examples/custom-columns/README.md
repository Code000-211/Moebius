# Custom columns

A project whose In review is split in two: `Ready for testing`, where the card waits for the measurement to be written
in the issue under `## Measurement`, and `Being measured`, where a person's label decides whether the change is offered
or goes back to coding. Both count as In review.

- [`kanban.toml`](kanban.toml): the two columns, both after In review, in the order of the file.
- [`testing.luau`](testing.luau): the script of `Ready for testing`.
- [`measured.luau`](measured.luau): the script of `Being measured`.

Copy the three files to `projects/<project name>/` of the harness configuration repository, and commit them to its
default branch. Try a script first with `moebius check-script testing.luau --kanban kanban.toml --column "Ready for
testing"`. The columns, the class and the checks are in the [column script
reference](../../docs/reference/column-scripts.md#custom-columns), and the worked example is
[Custom columns](../../docs/guide/custom-columns.md). Tested by `crates/moebius-core/tests/examples.rs`.
