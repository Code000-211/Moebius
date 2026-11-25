# Templates for a repository with a measured merge bar

A project override for a repository whose changes to hot paths are merged only with a measurement, such as a GPU inference
engine. It is written after the way such a repository describes its work: an item ID such as `K-47`, a pull request title
`[measured] K-47: <subject>` or `[needs testing] K-47: <subject>` with the tag first, a merge bar of three things (parity,
the targeted kernel faster in ms, end-to-end not worse), the GPU-less checks of a C++/CMake project, and a measurement that
lives in a committed file, `notes/backlog/measurements/<item>.md`. Change the headings and the hints to the repository's own
words.

Copy the two files to `projects/<project name>/templates/` of the harness configuration repository:

- [`pull-request.md`](pull-request.md): the pull request template. It keeps the four default sections, so the `Fixes #N` line, the
  `Measurement` state and the tests checklist are read as the shipped template reads them. `Measurement` holds `measured` and
  the path of the committed file, `needs testing` or `not needed` and the reason; the body holds no numbers.
- [`issue.md`](issue.md): the issue template, made into the issue form `.github/ISSUE_TEMPLATE/moebius.yml`.

Then `"$MOEBIUS" templates <repository> --sync` opens the pull request that puts the copies in `.github/` (it waits for the
owner's click). `/done` goes on the issue, not on the pull request. The review counts no measurement from the pull request body: the `Measurement` section only says where the file is, and a
`measured` there is a claim for the Reviewer to check. A pull request's fields are never decisions: `not needed` does not lift
`require_measurement`, which only an accepted measurement or the `no-measurement` label that a person sets does.
See [templates](../../docs/reference/templates.md). Tested by `crates/moebius-core/tests/examples.rs`.
