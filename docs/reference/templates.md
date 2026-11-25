# Templates for pull requests and comments

The exact contracts of the forms of what is written on a forge ([ADR
0034](../adr/0034-templates-and-agents-that-work-on-the-forge.md)): the pull request template, the comment templates,
the replies of the server and where the templates live. How to use them is in [Helping on someone else's
repository](../guide/working-with-others.md#templates-for-pull-requests-and-comments). The code is `app/forms/` and
`store/forms.rs` of `moebius-core`.

## The pull request template

A Markdown file. The first line is `Fixes #N`; then a section for each `### Heading`; the text below a heading is its
hint, written as an HTML comment. The shipped template, also the content of `.github/PULL_REQUEST_TEMPLATE.md`:

```
Fixes #N

### Item
<!-- The issue or backlog item this is for, in one line. -->

### What the change does
<!-- Say what changed, in sentences a reviewer can check. -->

### Tests I ran
<!-- A checklist with numbers, one test run each:
- [x] `cargo test`: 412 passed, 0 failed -->

### Measurement
<!-- One of `measured`, `needs testing` or `not needed`, then the reason. -->
```

- **A project's template** is `projects/<project>/templates/pull-request.md` in the harness configuration repository. It
  may rename the headings, drop them and add sections of its own. A heading that ends in `(optional)` may be left empty;
  every other section is required. A file with no section (no line that starts `###`, `##` or `####`), or one that has
  a heading twice, is not used: the shipped template is, and `GET /v1/repositories/{id}/templates` says why in
  `problem`.
- **A heading** is any ATX heading of level 2 to 4, with or without a trailing `:`; the same words in any case are the same
  heading. Headings inside a fenced code block are text.

### How a body is read

`app::forms::pull::parse` reads a body by the headings of the template. The fields it gives, which are what `card.change`
exposes to [column scripts](column-scripts.md#the-card-argument) and the Reviewer is given as data:

| Field | Holds |
|---|---|
| `fixes` | the numbers `Fixes #N`, `Closes #N` and `Resolves #N` name before the first heading |
| `fields` | the text of each section by the slug of its heading (lower case, each run of other characters one `_`: `Tests I ran` is `tests_i_ran`), for the template's headings and any other the body has; a section with no text is left out |
| `tests` | the lines `- [x] text` or `- [ ] text` of the `Tests I ran` section, each with `done` and `text` |
| `measurement` | `{state, reason}` when the `Measurement` section starts with `measured`, `needs testing` or `not needed` (any case), the rest being the reason |
| `missing` | what the body lacks, in order: `Fixes #N`, then each required heading, as the template writes it, whose section is absent, empty (HTML comments do not count as text), or, for `Tests I ran`, without a checklist line, or, for `Measurement`, without one of the three states |

### The reply for a missing section

A pull request that the server follows (someone else's, for an issue that has a card) whose body lacks something is
answered **once** on the pull request, where the repository has `commands` and `answer_on_forge` on, with the reply
`## Missing` ([below](#the-servers-replies)) and `About: pull request`. The server does not guess what the pull request
meant: the Reviewer reads it with the fields it has. The fields are read again at every poll, so a body that is mended is
read as mended.

### Moebius's own pull requests

Written from the same template, by `Fixes #N` (or `Part of <KEY>-<number> (issue #N in <repository>)` for a change in
another repository of the card) and the sections of the Coder's report, which the Coder is asked for in the template's
headings. A section the report has none for is `Not reported.`; `Item` is `#<number>: <title>`; text before the
report's first heading counts as `What the change does`. What the Coder says in `## Review answer` is not part of the
body. Each section is cut at 6,000 characters, and the body is redacted.

## The issue template

A Markdown file of sections like the pull request template, without the first line: the shipped one has `### Item (optional)`,
`### What is wanted` and `### How we will know it is done`. A project replaces it with
`projects/<project>/templates/issue.md`. The repository's copy is a **GitHub issue form**,
`.github/ISSUE_TEMPLATE/moebius.yml`, made from the sections: the first is a one-line `input`, the others are `textarea`s, a
required section is `validations: required: true`. GitHub writes an answered form into the issue body as `### <label>` and the
answer, so the body is read by the same headings: `card.issue.fields` (by slug) and `card.issue.missing` in
[column scripts](column-scripts.md#the-card-argument). An issue that a backlog sync made keeps the backlog's own format and
has no `card.issue`. Nothing is replied for a missing issue section.

## The comment templates

Each kind of comment starts with a fixed first line that names it (the first line with words, in any case). `read`
(`app::forms::comments`) gives its kind, its fields, what it lacks and whether it is in an old form.

| Kind | First line | Fields | Lacks |
|---|---|---|---|
| Measurement | `## Measurement` | the form of [the measurement](factory.md#measurement): `before <sha>`, `after <sha>`, `GPUs:`, `Power cap:`, `ROCm:` or `Driver:`, `Container:`, `Runs before:`, `Runs after:` | what [`problems`](factory.md#measurement) names, as before |
| Done | `/done`, or `## Done` | the sections `### What was done`, `### Tests run`, `### Open points` | each of the three that is absent or empty, when the comment has a heading or starts `## Done` |
| Review answer | `## Review answer` | one list line (`1. …`, `- …`) for each finding of the last review | `answer N` for each finding with no line |
| Take | `/take` | none | nothing |
| Release | `/release` | none | nothing |

- **The old forms work.** `/done` with plain words after it, on any line of a comment where a command may stand (ADR
  0028), is a `/done` with no fields to lack; its words are the reason. `/take` and `/release` take any words after them.
  A measurement is read by the parser of ADR 0033, unchanged.
- **A malformed comment is refused**, with the reply `## Missing` naming what it lacks, and nothing is done with it: a
  `/done` or `## Done` with sections it lacks does not ask the owner to look, a review answer without a line for each
  finding is not kept. A measurement is refused as ADR 0033 says, with its sentences about commits and runs.
- **A `## Done` comment** is a `/done`: its first line replaces the command, and the owner's question holds what was done,
  the tests and the open points, cut at 500 characters.
- **A `## Review answer`** is read where the author of a followed pull request writes it on the issue; the Reviewer's next
  round is given it, fenced, with the instruction to check each claim. For Moebius's own pull request the answer is
  written by the server from the Coder's report, below.

The template of each kind is a file, `<name>.md`: `measurement`, `done`, `review-answer`, `take`, `release`. A project
replaces one with `projects/<project>/templates/<name>.md`; the repository's copy is `.github/moebius/<name>.md`.

## The server's replies

The server's own replies that these templates produce are in the same form as a comment: a first line that names the
reply, then `Name: value` lines, a blank line, and words for a person.

| First line | Fields | Written when |
|---|---|---|
| `## Missing` | `About` (`pull request` or `comment`), `Kind` (the kind or the first line), `Missing` (what is lacking, separated by commas), `Template` (the path of the template in the repository) | a body or a comment lacks what its template requires |
| `## Posted` | `About` (`issue` or `comment`), `Kind`, `Detail` | the issue is told where the work is (`Kind: Pull request`, `Detail: #<number>`), or a review answer was read (`Detail: <n> answers`) |

`Reply::parse` reads a reply back into its name and fields. The older replies to commands (`/take`, `/release`, `/done`)
and the reply to a measurement keep their sentences.

## Where the templates live

| What | Moebius ships | A project's | The repository's copy |
|---|---|---|---|
| Pull request template | `PullTemplate::standard` | `projects/<project>/templates/pull-request.md` | `.github/PULL_REQUEST_TEMPLATE.md` |
| Issue template | `DEFAULT_ISSUE_TEMPLATE` | `projects/<project>/templates/issue.md` | `.github/ISSUE_TEMPLATE/moebius.yml` (an issue form) |
| A comment template | `Kind::template` | `projects/<project>/templates/<name>.md` | `.github/moebius/<name>.md` |

The project is the repository's home project. A template file is a project file of the harness configuration
repository, edited like the others there. The copies are kept by `POST /v1/repositories/{id}/templates/sync`
(`"$MOEBIUS" templates <repository> --sync`): it writes the files of the repository on a branch `moebius/templates`
from its default branch, and opens a pull request through the repository's forge with an Inbox item, `ready_to_merge`,
that says the pull request waits for the owner. `.github/` is a locked path of [the merge
gate](desktop-api.md#the-merge-gate), so the server never merges it. An open pull request of an earlier sync is moved to the
new files instead of another being opened; nothing is opened when the default branch has the files already (`{"change":
null}`).

## Working on the forge

Where a repository has `work_on_forge` (a [flow setting](desktop-api.md#get-v1repositoriesidflow), on by default) and its
forge has drafts (GitHub, GitLab and Forgejo):

| Step | What is written |
|---|---|
| The first successful `code` run | `moebius.publish_change()` opens the pull request, as a draft (GitHub's `draft` flag, `Draft: ` before a GitLab title, `WIP: ` before a Forgejo title), on the card's branch, with the body of the template. One pull request for all the rounds. The issue gets one `## Posted` reply with `Kind: Pull request` and the number, written where the repository answers on the forge and tried again at the next tick until it is. |
| Each later round | The branch is pushed and the title and body are written again. |
| The review | `moebius.post_review(card.review)` writes the verdict as a review of the pull request: `REQUEST_CHANGES` with a comment on each line `card.review.comments` names, or `APPROVE`, tied to the head that was read. Comments on lines the diff does not have go into the summary. A review written without a verdict starts its body with `**Approved.**` or `**Changes requested.**` on every forge. On GitLab the approval is made first, so a refused one has written nothing before the retry. Once for each round (the number of successful `code` runs) and head. A forge that refuses a verdict on the request of its own user takes it as a review that only comments (`COMMENT`), whose body starts with `Approved.` or `Changes requested.`; one that refuses the lines takes it without them, in the summary. |
| The round after a review that asked for changes | The Coder's `## Review answer` section is written as a `## Review answer` comment on the pull request, one numbered line for each finding (as many as `card.review.count`), `(finding N was not answered)` for one it gave no line; once for each head. The Reviewer's next prompt gets it, fenced. |
| Approval | `moebius.offer_change` takes the pull request out of draft (GitHub `markPullRequestReadyForReview`, the prefix of a title taken off) and offers it as before; it merges through [the merge gate](desktop-api.md#the-merge-gate) as before. |

**What the Reviewer is given.** The review prompts of a pull request (Moebius's own and one someone else opened) get the
parsed fields, fenced as data and labelled as the author's claims, with what the body lacks, to check against the diff. The
review of a round after an answer is given the answer to its last review, and only that: once a later review has no answer to
be read, an older one is not shown.

**Fields are claims, never decisions.** What the author of a pull request writes (the `Measurement` state, the tests, any
section) is data for the Reviewer. It never satisfies [the merge gate](desktop-api.md#the-merge-gate), lifts
`require_measurement` or stands for the `no-measurement` label, which only a person sets; only an accepted measurement of a
registered measurer does. The same holds for the fields of a comment.

**The merge message.** When the server merges a change whose pull request body was read, the merge commit's title is the
first line of `Item` and its body is `What the change does`, then `Fixes #N` (for a change in the card's own repository), cut
and redacted; otherwise the message is as before (`Merge #<number>: <title>`). On GitHub the title is `commit_title` and the
rest `commit_message`.

**Branches.** Own work is on `moebius/card-<number>`, or `moebius/<ID>-card-<number>` when the issue was made from a backlog
item (`moebius/K-47-card-12`), so people and the binding of a pull request to its issue
([ADR 0033](../adr/0033-measurements-and-reviews-flow-without-the-owners-clicks.md)) see the item. A change that has a branch
keeps it.

**A card under way when `work_on_forge` turns on.** `moebius.publish_change()` runs at every tick of In review, so a card
with a pushed branch and no pull request gets its draft at the next tick, and a verdict it already has is written once. A
card in Ready to merge has a pull request that is not a draft and is not made one.

A review finding names a line with `FILE: <path>:<line>` after its `FINDING` line or its `EVIDENCE`; a range
`path:12-14` counts as its first line, and a line that is no place is ignored.

**What is written.** Plans, commits, tests with numbers, findings and waiting reasons: what happened. Never a prompt, the
output of a tool, a token, a cost or a link into Moebius. Everything is redacted (`blobs::redact`) before it is sent.

**Where the setting is off, or the forge has no drafts** (`none`), the pull request opens when the change is offered, with
the body of ADR 0019, and the Reviewer's verdict stays a note on the card.
