# Templates for everything written on a forge, and agents that work there as people do

[0027](0027-working-with-people-on-other-servers-through-the-forge.md) to [0033](0033-measurements-and-reviews-flow-without-the-owners-clicks.md)
made the forge the place where helpers on other servers and Moebius meet. Two things were still left to guessing. What a
pull request or a comment says was found by looking for words, so a card and the Reviewer read what a person happened
to write; and Moebius's own agents did their work inside Moebius: the pull request opened only after an approving
review, the review was a note on the card, and the issue held what the work was doing. This part gives every pull
request and every comment a form that is read by its fixed headings, and has Moebius's own agents work on the forge the
way a developer does: commits, a draft pull request, reviews, an answer to each finding.

## Considered Options

- **Read a pull request by keywords** (`tested`, `measured`, a number near `passed`). What exists. A body that says it
  differently is read differently, and nobody can tell a writer what is expected.
- **A form with fields** (YAML front matter, a JSON block). Exact, and not what a person writes in a text box on a forge.
- **Fixed Markdown headings, read by heading.** A pull request template is the file the forge already shows to whoever
  opens a request, so people and agents fill the same one, and the server reads the text under each heading.
- **Keep the agents' work in Moebius and mirror a summary to the forge.** Two histories that can disagree.
- **Let the pull request hold the history, and the issue only its status.** One place for what happened, in the tool
  people already read.

## Decision

- **The pull request template is Markdown with fixed headings** (`app::forms::pull`). The first line is `Fixes #N`; then
  `### Item`, `### What the change does`, `### Tests I ran` (a checklist with numbers), `### Measurement` (`measured`,
  `needs testing` or `not needed`, then a reason). The template is a file, and so is a project's: a project replaces it
  with `projects/<project>/templates/pull-request.md` in the harness configuration repository, with its own headings and
  sections of its own; a heading that ends in `(optional)` may be left empty. A file without a section is not a
  template and the shipped one is used, which the Settings page says.
- **A pull request body is read into fields** by those headings, keyed by the heading's slug (`tests_i_ran`), with the
  numbers `Fixes` names, the tests as a checklist, the measurement as a state and a reason, and `missing`, the required
  sections that are absent or empty (a section that holds only the template's hint is empty). The fields are kept on the
  change, given to the Reviewer as data, and exposed to column scripts as `card.change.fields`, `.missing` and
  `.measurement`.
- **A pull request that lacks a required section is told once**, in a reply that names it, on a repository where
  commands and answers are on. It is not guessed at: the Reviewer reads it as it is, with the fields it has.
- **A comment template for every kind of comment Moebius reads or writes** (`app::forms::comments`). Each starts with a
  fixed first line that names it: `## Measurement`, `/done` (or `## Done`), `## Review answer`, `/take`, `/release`.
  Each has documented fields; `Measurement` is the form of ADR 0033, read by the code that read it, and accepts what it
  accepted. A malformed comment is answered with the field it lacks and does nothing. The old forms still work: `/done`
  followed by plain words, a `/take` or `/release` with any words after it.
- **The server's own replies use the same forms**: `## Missing` (with `About`, `Kind`, `Missing`, `Template`) and
  `## Posted` (with `About`, `Kind`, `Detail`), so a person or another server can parse them. The older plain replies to
  commands keep their wording.
- **Moebius keeps the repository's copies of the templates current** in `.github/PULL_REQUEST_TEMPLATE.md` (where the
  forge looks) and `.github/moebius/<kind>.md`, by opening a pull request. `.github/` is a locked path of the merge gate
  (ADR 0031), so that pull request waits for the owner's click, which its Inbox item says. `POST
  /v1/repositories/{id}/templates/sync` and `"$MOEBIUS" templates <repository> --sync` open it on demand.
- **Everything posted is results, not internals**: plans, commits, tests with numbers, findings and waiting reasons. Never
  a prompt, the output of a tool, a token, a cost or a link into Moebius. All of it goes through `blobs::redact` first.
- **The agents work on the forge** where a repository has `work_on_forge` (a flow setting, on by default) and its forge has
  drafts:
  1. After the first successful `code` run the pull request opens at once, as a draft, with `Fixes #N` and a body written
     from the template out of the Coder's own report, which is asked for in the template's headings. One pull request
     for all the rounds of a card; its body is written again after each round.
  2. The Coder commits its own work, with messages that say what it did. What it leaves uncommitted is committed by
     Moebius as before.
  3. The Reviewer's verdict is a review of the pull request: changes requested with a comment on each line a finding
     names (`FILE: <path>:<line>`), those outside the diff in the summary, or an approval; tied to the head that was read
     and written once for each round and head. A forge that refuses a verdict on the request of its own user, as GitHub
     does for the user who opened it, takes the review as one that only comments, with the verdict on its first line.
  4. The next round answers each finding in a `## Review answer` comment, one numbered line for each; the Reviewer's next
     prompt gets it, as data to check.
  5. On approval the pull request leaves draft (GitHub's `markPullRequestReadyForReview`, a GitLab title without
     `Draft:`, a Forgejo title without `WIP:`) and merges through the merge gate as before.
  6. The issue holds only the status label and the link to the pull request.
  On a forge without drafts, and where the setting is off, the pull request opens when the change is offered, as before.

- **Fields are claims.** Nothing a pull request or comment author writes satisfies the merge gate, lifts
  `require_measurement` or stands for `no-measurement`; the Reviewer reads it as data.
- **An issue template too**, kept as a GitHub issue form in `.github/ISSUE_TEMPLATE/moebius.yml` and read by the same
  headings (`card.issue`); backlog-sync issues keep their format.
- **The merge message** comes from the pull request's fields (the item as the subject, what the change does, `Fixes #N`), and
  **branches** name the backlog item when the issue has one (`moebius/K-47-card-12`).
- **A card under way** when the setting turns on gets its draft at the next tick; one in Ready to merge is not reopened.

## Consequences

- A repository on GitHub, GitLab or Forgejo gets a pull request at the end of the first round instead of at the end of the
  review, so the forge's checks start earlier and a person can read the work as it grows. A change that fails its review
  is a draft that was open for a while.
- A pull request that follows the template can be judged by its fields; a project that wants other sections only writes
  them into its template.
- The review on the forge is the one place a person reads the verdict; it is not repeated on the issue.
- Not done here: a sandboxed lane that runs the tests of a change before it is a draft.
