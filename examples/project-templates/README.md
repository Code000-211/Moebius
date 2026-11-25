# Templates for a repository with a measured merge bar

A project override for a repository whose changes to hot paths are merged only with a measurement, such as a GPU inference
engine. It is written after the way such a repository describes its work (an item ID such as `K-47`, a `[measured]` or
`[needs testing]` tag on the title, a table of numbers before and after); change the headings and the hints to the
repository's own words.

Copy the two files to `projects/<project name>/templates/` of the harness configuration repository:

- [`pull-request.md`](pull-request.md): the pull request template. It keeps the four default sections, so the `Fixes #N` line, the
  `Measurement` state and the tests checklist are read as the shipped template reads them, and adds an optional
  `Measurement table` section.
- [`issue.md`](issue.md): the issue template, made into the issue form `.github/ISSUE_TEMPLATE/moebius.yml`.

Then `"$MOEBIUS" templates <repository> --sync` opens the pull request that puts the copies in `.github/` (it waits for the
owner's click). A pull request's fields are claims, never decisions: `Measurement: not needed` does not lift
`require_measurement`, which only an accepted measurement or the `no-measurement` label that a person sets does.
See [templates](../../docs/reference/templates.md). Tested by `crates/moebius-core/tests/examples.rs`.
