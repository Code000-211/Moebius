# Column scripts

A column script is the behaviour behind one column of a board, written in Luau. This page is for anyone who writes or
changes a script: a person, or an agent such as the Moebius agent. It lists everything a script can read and do. The
code is in `crates/moebius-core/src/board/` (`api.rs` for the `moebius` table, `card.rs` for the `card` table); the default scripts are in `crates/moebius-core/kanban/`.

## How a column maps to a script

Every board has six built-in columns, in this order, and a project may add columns of its own among them (see
[Custom columns](#custom-columns)). Each column has one script file.

| Column | Script |
|---|---|
| `Backlog` | `backlog.luau` |
| `Not started` | `ready.luau` |
| `In progress` | `code.luau` |
| `In review` | `review.luau` |
| `Ready to merge` | `merge.luau` |
| `Done` | `done.luau` |

The six built-in columns cannot be renamed or removed, and Backlog is always first and Done always last. Their names are
exact: capital first letter, one space between words. A custom column's name is as its project wrote it, and wherever a
column is named (the argument of `moebius.move`, `--column` of `check-script`, `{column}` of the scripts routes, the
`columns` filter of a view) it is found in any case.

`Not started` was called `Ready` before. `Ready` is still accepted as its name wherever a column is named: the argument
of `moebius.move`, `--column` of `check-script`, and `{column}` of the scripts routes of the desktop API. The column
itself is always called `Not started`: `card.column`, the `card_moved` event and every response give that name, so a
script that compares `card.column` with `"Ready"` must compare it with `"Not started"`.

The board engine runs the script of a card's column for every card on every tick (`board.tick_ms`, default 2 seconds).
After a script moved the card, it runs the script of the new column at once, and so on until a script leaves the card
where it is (at most twelve more runs in one tick). A card in `Done` is skipped once every judgement
made for it has an outcome. A card that follows a pull request someone else opened (see [Working with people on other
servers](desktop-api.md#working-with-people-on-other-servers)) has no script run while that request is open, as a held card has none.

A tick leaves a card alone when its last script did nothing (the card stayed in its column and no run started), the
card has no open change, nothing at all was written to the store since that script began (a card, label, run, change,
inbox item, comment or setting a person or the board changed) and the limits scripts see (`moebius.run_limit`,
`moebius.round_limit`, `moebius.day_budget`) are the same. A card that is skipped is still settled when it has waited
a minute, so a script that reads the clock (`moebius.now()`, a due date, `card.moved_at`), the project's script files
or the forge's checks notices within a minute. A card with an open change is settled at every tick, since its checks
and conflicts come from git and the forge. So a script must not rely on running at every tick: it runs when what it
reads changed, and at least once a minute. A script's compiled form is kept per script text, and a project's script
files are read once per tick, not once per card.

Each run of a script is a fresh Luau state. Nothing is kept between runs: global variables a script sets are gone on
the next tick. Keep state on the card, in the store or in the inbox, through the functions below.

A script defines one global function, `on_card(card)`. Its return value is ignored. What the script did takes effect
when `on_card` returns, except where a function below says otherwise.

```luau
function on_card(card)
  moebius.wait("nothing to do")
end
```

## The `card` argument

`card` is a read-only snapshot of the card taken just before the script runs.

| Field | Type | Meaning |
|---|---|---|
| `card.id` | number | The card's id in the store. |
| `card.number` | number | The issue number: on GitHub the issue's number, otherwise a counter per repository starting at 1. |
| `card.title` | string | The issue's title. |
| `card.body` | string | The issue's body; an empty string when there is none. |
| `card.author` | string | Who created the issue: `person:<handle>` for an issue a known person made in the web app (an issue from before people belongs to the first owner, `person:owner`), `person` for one made without a known person, otherwise the forge login. |
| `card.column` | string | The column the card is in. |
| `card.column_class` | string | The built-in column the card's column counts as: the column itself for a built-in one, and for a custom column the `counts_as` of its project's `kanban.toml` (see [Custom columns](#custom-columns)). One of the six names. Read-only. |
| `card.confirmed` | boolean | True for a card made from a grilling a person confirmed (see `desktop-api.md`, grillings); the person has decided on it, so it needs no approval. |
| `card.trusted` | boolean | True when the issue may move on without approval: it was made by a person in the web app on a repository without a forge, or its author is one of the repository's trusted GitHub logins (compared without regard to case). |
| `card.test_first` | boolean | True when the card's repository works test first: its project has a `test-first` file in its settings. See Test first. |
| `card.blocked_by` | table (array of numbers) | The issue numbers the card is blocked by that are not done yet. A number counts as not done when it has no card or its card is not in `Done`. Empty (never nil) when nothing blocks the card. The numbers come from "blocked by #12" lines in the issue's body on a forge, from the forge's own dependencies (GitHub's issue dependencies, GitLab's `is_blocked_by` links, Forgejo's dependencies) and from what was added through the API (see [the issue tracker](desktop-api.md#the-issue-tracker)). A closed issue is a card in `Done`, so its blocked issues stop waiting. |
| `card.parent` | number or nil | The issue number of the card's parent issue, or nil. |
| `card.children` | table (array of tables) | The card's sub-issues, oldest first; empty (never nil) when it has none. Each has `id` (the card's id), `number`, `title`, `state` (`open` or `closed`) and `column`. Only issues a person can have vouched for are listed: those of a trusted author, those a person approved and those of a confirmed grilling. An issue of anyone else that says "Part of #n" is shown under its parent but is not one of its `children`, so it cannot hold the parent back or close it. |
| `card.moved_at` | number | When the card last moved, was edited or was released from a hold, in Unix seconds. Compare it with `moebius.now()`: the default `backlog.luau` and `ready.luau` hold a card whose `moved_at` is more than 30 days old, and `done.luau` archives one more than 14 days old. |
| `card.merged` | boolean | True when a change of the card was merged. A card is in `Done` because its change was merged, or because its issue was closed (by a person, a script or the forge), which is when this is false. |
| `card.labels` | table (array of strings) | The issue's labels, in order; empty when it has none. |
| `card.claimed_by` | string or nil | Who claimed the issue (`person`, `person:<handle>`, `agent:<name>` or a name a session gave), or nil. |
| `card.taken_by` | string or nil | The forge login the issue is assigned to when that is none of this server's people's logins on that forge (compared without regard to case), without the `@`: someone on another server took the issue. nil otherwise. Read-only. `moebius.start_run` and `moebius.start_runs` start nothing for such a card, see below. |
| `card.assignees` | table (array of strings) | The handles of the people the card is given to (see [Assignees](desktop-api.md#assignees)), without the `@`, in the order the people were added; empty (never nil) when it is no one's. A person who was disabled after being assigned stays in the list. Read-only. |
| `card.priority` | string | How soon the card should be done: `urgent`, `high`, `medium`, `low` or `none`; never nil. It is the card's `priority:` label (see [Priority and due date](desktop-api.md#priority-and-due-date)). Read-only. |
| `card.due` | string or nil | The day the card is due as `YYYY-MM-DD` (it sorts as text), from its `due:` label; nil when it has none. Read-only. |
| `card.kind` | string or nil | The name of the card's kind of issue (see [Kinds of issues](desktop-api.md#kinds-of-issues)), or nil while it has none. Read-only. |
| `card.fields` | table | The values set for the fields of the card's kind, by field name: a number for a `number` field, a string for the others (a `person` field is a handle without the `@`, a `date` field `YYYY-MM-DD`); a field with no value is nil. An empty table (never nil) when none is set or the card has no kind. Read-only. |
| `card.cycle` | text or nil | The name of the open cycle of its project the card is in (see [Cycles](desktop-api.md#cycles)); nil when it is in none. Read-only. |
| `card.estimate` | number or nil | The card's estimate in points, a whole number from 1 to 1000, from its `estimate:` label; nil when it has none. A person sets it, or takes the Moebius agent's proposal (see [Estimates](desktop-api.md#estimates)). Read-only. |
| `card.rounds` | number | How many runs with purpose `code` the card has had since it was last retried, whatever their status (a running one counts), and how many rounds of attempts of which none passed (see Attempts). |
| `card.reviews` | number | How many review rounds the card has had since it was last retried, whatever their status: the runs with purpose `review` and those with purpose `review-standards` (one per round of the two-axis review). |
| `card.tokens` | number | The input and output tokens the card's runs used since it was last retried (cached tokens are not added; the teacher's count too). |
| `card.held` | table or nil | Set while a person or the Moebius agent holds the card (`POST /v1/cards/{id}/block` in `desktop-api.md`): `held.reason` (string or nil), `held.by` (`person`, `person:<handle>` or `agent:Moebius`) and `held.at` (Unix seconds). Read-only. The engine skips a held card before its script is called, so a script never runs for one and sees `card.held` only as nil; the dry run of `check-script` makes up a card that is not held. |
| `card.stopped` | boolean | True when the card's latest run was stopped by a person and the card has not been retried since. |
| `card.review` | table or nil | The latest review of the card's work: the newest `review` run that `succeeded` after the card's latest `code` run, or a `review-spec` run that `succeeded` after a `review-standards` run that `succeeded` (the two axes of one round). nil when there is none, so a new `code` run clears it. |
| `card.review.verdict` | string | `approve` or `changes`. Read from the review run's summary (see below). |
| `card.review.findings` | string | The summary without its verdict line; for a review with no verdict, the text `The review gave no verdict.`; for a `changes` verdict without findings, `The review asked for changes but gave no findings.` |
| `card.review.judgements` | string | For a two-axis review, the judgement calls of both axes, each with its evidence; empty for a `review` run and when there are none. |
| `card.review.count` | number | How many findings `findings` holds, counted from the review: the blocking findings of a review along two axes, the comments on lines of a review of a pull request someone else opened, and 0 for any other `review` run. The server answers that many findings in the next round's `## Review answer`. |
| `card.review.comments` | table (array of tables) | The findings of the review that name a line (`FILE: <path>:<line>` after a finding), each with `path`, `line` (of the file as the change leaves it) and `body`; empty when none does. `moebius.post_review` writes them as comments on the pull request. |
| `card.review.standards`, `card.review.spec` | table | Only for a two-axis review: that axis's `verdict` (`approve` or `changes`), `findings` (its blocking findings), `judgements` (its judgement calls) and `dropped` (how many findings were dropped for lacking evidence). |
| `card.note` | string or nil | Why the card was sent back to coding: the findings of a review, the failing checks or a conflict, as the script that moved the card wrote them (the second argument of `moebius.move`). A `code` run that succeeds clears it; one that fails keeps it, so the retry gets it too. |
| `card.repositories` | table (array of strings) | The names of the repositories the card works in, in the order it names them: its own issue's repository first (the forge's for an issue of a forge; the project's files for a card of a project without repositories), then those a person or the Moebius agent named (`PUT /v1/cards/{id}/repositories` in `desktop-api.md`) and those a `code` run asked for (see Cards over several repositories). Never empty. Read-only. |
| `card.changes` | table (array of tables) | The card's open changes, one per repository of `card.repositories` that has one, in that order; empty (never nil) until the first is published. |
| `card.changes[i].repository` | string | The name of the repository the change is in. |
| `card.changes[i].branch` | string | The branch the change lives on: `moebius/card-<number>` in the card's own repository, `moebius/card-<KEY>-<number in the project>` (such as `moebius/card-WEB-24`) in another. |
| `card.changes[i].base` | string | What the change merges into: the repository's default branch, as `main` without a forge and `origin/main` on GitHub. |
| `card.changes[i].pull_request` | number or nil | The number of the pull request on GitHub. |
| `card.changes[i].draft` | boolean | Whether the pull request is still a draft: Moebius opens its own as one after the first round where the repository works on the forge (`work_on_forge`), and takes it out of draft when the review approves ([ADR 0034](../adr/0034-templates-and-agents-that-work-on-the-forge.md)). |
| `card.changes[i].fields` | table | What the pull request's body says, read by the pull request template: the text of each section by the slug of its heading (`item`, `what_the_change_does`, `tests_i_ran`, `measurement`, and a project's own), empty until the body was read. See [templates](templates.md#how-a-body-is-read). |
| `card.changes[i].missing` | table (array of strings) | The required parts of the pull request template the body lacks, as the template writes them (`Fixes #N`, a heading); empty when it has them all. |
| `card.changes[i].measurement` | string or nil | `measured`, `needs testing` or `not needed`, as the `Measurement` section starts; nil when it does not. |
| `card.issue` | table or nil | What the issue's body says, read by the repository's [issue template](templates.md#the-issue-template): `card.issue.fields` is the text of each section by the slug of its heading, `card.issue.missing` the required sections it lacks. nil for an issue a backlog sync made. |
| `card.change` | table or nil | The first of `card.changes`, the change of the card's own repository, so a script reads `card.change.fields` and `card.change.missing`; nil until the card has a change. |
| `card.changes[i].checks` | string | `pending`, `success`, `failure` or `none`, for the change's current commit. See Checks and conflicts. |
| `card.changes[i].failed_checks` | string or nil | On GitHub, the failing check runs and commit statuses, one per line; nil unless `checks` is `failure`. |
| `card.changes[i].check_output` | string or nil | Without a forge, the end of the output of the repository's `checks`, once they ended for the current commit. |
| `card.changes[i].conflict` | string or nil | The files that conflict with the change's `base`, one per line; nil when the change merges cleanly. |
| `card.attempts` | table (array) | The card's attempts that have not been settled, oldest first; empty (never nil) when it has none. See Attempts. |
| `card.attempts[i].run` | number | The attempt's run id; its branch is `moebius/run-<run>`. |
| `card.attempts[i].agent` | string | The agent of the attempt's run. |
| `card.attempts[i].model` | string or nil | The model or model class the attempt was started with in place of its agent's; nil when it used its agent's. |
| `card.attempts[i].status` | string | The status of the attempt's run: `starting`, `running`, `succeeded`, `failed` or `stopped`. |
| `card.attempts[i].result` | string or nil | `passed` or `failed` once `moebius.check_attempt` has judged the attempt; nil before. |
| `card.attempts[i].output` | string | Why the attempt failed: the end of the output of the checks or the test that did not hold. Empty for one that passed. |
| `card.selection` | table or nil | The latest run with purpose `review-select` started after the open attempts, if there is one. |
| `card.selection.run` | number | The selection run's id. |
| `card.selection.status` | string | The status of the selection run. |
| `card.selection.pick` | number or nil | The run id on the last line `PICK: <run id>` of the summary of a selection run that `succeeded`; nil while it goes, when it failed and when it names none. |
| `card.selection.reason` | string | The summary of a selection run that `succeeded` without its last line, the reviewer's reasons; empty otherwise. |
| `card.run` | table or nil | The card's latest run (the one with the highest id) that is not an attempt that was not picked (see Attempts). nil when the card has had no run. |
| `card.run.id` | number | The run's id. |
| `card.run.agent` | string | The agent's name. |
| `card.run.purpose` | string | The purpose the run was started with. |
| `card.run.status` | string | One of `starting`, `running`, `succeeded`, `failed`, `stopped`. |
| `card.run.summary` | string or nil | The agent's final answer: the text of the last assistant message Pi reports in its `agent_end` event. nil until the run has one. |

The verdict of a review is read from the end of the summary of a `review` run. A last line `VERDICT: approve` or
`VERDICT: changes` (the word in any case) gives the verdict, and the lines before it are the findings. A summary
without such a line, or with any other word, is `changes`: work that was not reviewed is not approved. A `changes` verdict
with no findings before it has the findings `The review asked for changes but gave no findings.`

Only `card.held`, `card.run`, `card.run.summary`, `card.review`, `card.selection`, `card.note`, the `pull_request`, `failed_checks`, `check_output` and `conflict` of a change, and `model` and `result` of an attempt, can be nil. Check them before you read their fields.

## The `moebius` table

Arguments are positional. A wrong number or type of argument raises an error (see Errors).

### Constants

| Name | Type | Meaning |
|---|---|---|
| `moebius.run_limit` | number | How many runs may go at once across all boards (`run_limit`). |
| `moebius.round_limit` | number | How many `code` rounds a card gets (`board.round_limit`). |
| `moebius.day_budget` | number or nil | The most tokens all runs may use in a day (`budgets.day_tokens`); nil without one. |

### Functions

`moebius.decide(kind, context, options, fail_closed)` returns `choice, reason`.
: Asks a decision point and records a judgement. `kind` is a string, `context` a table of string keys to string values,
`options` an array of strings, `fail_closed` the option to take when nobody can answer. Only the kind `tool_call` has a
decider on a server today, so any other kind returns `fail_closed` with a reason that starts with `refused: no decider
answers`. Returns two strings. The time this call takes does not count against the script's time limit. Raises an error
when the judgement cannot be stored.

A review along two axes is made of two runs, with purposes `review-standards` and `review-spec`, each ending with
findings and a last line `VERDICT: approve` or `VERDICT: changes` (it only shows that the reviewer finished; without it
the axis is `changes` with the finding `The Standards review gave no verdict.`). A finding is a line
`FINDING blocking: <what is wrong>` or `FINDING judgement: <what is doubtful>`, followed by a line
`EVIDENCE: <the lines of the diff, or the line of the issue, that show it>` (evidence may run over several lines).
Moebius drops a finding without evidence, and counts it in `dropped`. A finding whose text begins `Possible` (the
smells of the fixed baseline of the Standards axis) is a judgement call whatever it says. An axis is `changes` when it
has a blocking finding with evidence and `approve` otherwise; the review is `changes` when either axis is, and then
`card.review.findings` holds the blocking findings of both under the heading `Standards (...)` or `Spec (...)`, each
with `Evidence:`. Judgement calls never send the card back; `card.review.judgements` holds them under the same headings.

`moebius.start_run(agent, purpose, note, runner)` returns the run's id (a number), or nil when `run_limit` runs of cards are going: it then starts no run and the card waits with `the run limit is reached`, so a script that has nothing more to do returns at once (`if moebius.start_run(...) == nil then return end`) and is run again when a run ends. It also returns nil and starts nothing, with no error, for a card this server leaves alone: one whose `card.taken_by` is set, which waits with `taken by @<login>`, or, in a repository whose `agents_take` is `labelled` (see [`PUT /v1/repositories/{id}`](desktop-api.md#put-v1repositoriesid)), one whose issue lacks the label, that nobody of this server took and that has no run yet, which waits with `waiting for the label "<label>" (this repository's agents take only labelled issues nobody took)`. When it starts a run, the person it is for is assigned to the issue on the forge (see [Working with people on other servers](desktop-api.md#working-with-people-on-other-servers)). Only runs of cards count; chats and workflow runs do not.
: Starts a run of the agent named `agent` (an `[agents.<name>]` table of the configuration) for this card in a new
worktree on a new branch `moebius/run-<id>`. The branch starts at the branch of the card's latest successful `code`
run, so the run sees the work so far, or at the repository's default branch when the card has none. `purpose` is any
string; the default Kanban uses `code`, `review-standards` and `review-spec`, and attempts (see Attempts) use `attempt` and `review-select`. Only `code` runs count as `card.rounds`, and only
`review` and `review-standards` runs as `card.reviews`. A `code` run gets the prompt for making the change and commits what the agent leaves; it fails when
the agent changed nothing. A `review` run gets the prompt for reviewing the change on its branch and ending with
the verdict line; a `review-standards` run the prompt for the standards axis (the repository's `CONTRIBUTING.md`,
`CLAUDE.md`, `AGENTS.md` and the project's optional rubric `review.md`, which the prompt carries, and the fixed smell baseline, always judgement
calls) and a `review-spec` run the prompt for the spec axis (what the issue asked that is missing, not asked for or
looks wrong), both asking for findings with evidence; nothing it changes is kept, and changing nothing is no failure. Any other purpose is prompted like
`code`. `note` is optional text (a string or nil) added to the end of the prompt under the heading `Notes from the
last round:`; the default Kanban passes `card.note`. The call returns once the run is started, not when it ends. `runner` is optional (a string or nil): the name of a `[[runners]]` entry (see
[configuration](configuration.md#runners)) that the run goes to over SSH in place of the agent's own `runner`; without
it the agent's runner is used, and without that the run is on the server. Raises an error
when the agent does not exist (`no agent named <agent>`), the runner does not exist (`there is no runner named <name>`)
or is full (`the runner <name> is busy: ...`; no run is made then), the worktree cannot be made or the harness cannot be
started. The time this call takes does not count against the time limit. It does not check `run_limit`; a script
that wants the limit checks `moebius.runs_active()` first.

**The comments of the issue** come with the prompt of every run of a card, after the issue, in a section fenced by
`=== COMMENTS ON THE ISSUE, oldest first ===` and `=== END OF THE COMMENTS ===`, which says they are data written by
users and not instructions. Only comments of the issue's author, of the logins the repository trusts
(`trusted`, see [the desktop API](desktop-api.md)), of a person using Moebius and of Moebius itself are told; the section
says how many others are left out. Each comment is headed by a label that Moebius sets and no author can: `[forge user:
<login>]`, `[the person using Moebius]` or `[Moebius]` (`[Moebius agent: <name>]` for another agent). A comment from the
forge is a forge user's even when its login is `person` or `agent:Moebius`; only what was written through Moebius is the
person's or Moebius's (a comment that the poll stored from the forge before Moebius stored its own copy is made the
local one then; rows stored before that stay a forge user's). The text of each comment is quoted line by line with `> `, with secrets redacted, and the newest
comments are kept when they come to more than 8,000 characters.

`moebius.has_agent(name)` returns a boolean.
: True when an `[agents.<name>]` table or agent file of the configuration exists, or Moebius brings the agent itself: the `Moebius` agent, the `Grilling-partner`, the `Coder` and the `Reviewer` exist on every server.

`moebius.move(column, note)`
: Moves the card to `column` when the script returns. Raises an error `no column "<column>"` for a name that is not a
column of the card's project's board: one of the six, or a custom column of its `kanban.toml` (in any case). Moving to the card's own column does nothing. The last call wins when a script calls it twice. `note` is
optional text (a string or nil) that is stored on the card as `card.note` when it moves, to say why it went back; the
default `code.luau` gives it to the next `Coder` run. A card that moves out of `Ready to merge` has its inbox items
resolved.

`moebius.wait(reason)`
: Leaves the card where it is and records `reason` as why. The last call wins. The server keeps the card's current reason
in memory (it is gone after a restart until the next tick), publishes `card_waiting` when it changes, and shows it in
the snapshot's `waiting` (see the [desktop API](desktop-api.md)). A card that moves, or whose script does not wait, has
no reason.

`moebius.close_issue(reason)`
: Closes the card's issue when the script returns, on its forge too, and moves the card to `Done`, whatever `moebius.move`
was asked for. `reason` is optional text. A closed issue no longer blocks the issues it blocks. The default `ready.luau`
uses it to close a parent whose last sub-issue is done. If the forge refuses, the card stays where it is and waits with
the reason; the forge is told once as a `forge_error` (once however often the board tries again) and is not asked again
for ten minutes (nor at every tick), unless a person closes the issue or the poll sees it closed first. An issue that
was reopened (by a person, or on the forge) is not closed again by this call for the sub-issues that were done already:
it stays where it is, waiting, until one of its sub-issues closes after the reopening.

`moebius.now()` returns a number.
: The time in Unix seconds, as `card.moved_at` is given.

`moebius.archive(card)`
: Archives the card when the script returns: it leaves the board (the snapshot of the desktop API and the board's tick
leave it out, so no script runs for it again) and stays in the store, in the issue list (`GET /v1/repositories/{id}/issues`,
filtered by `archived`) and in the analytics. `card` must be the table the script was given; another card is an
error, because a script acts on its own card only. Nothing is moved or deleted: the card stays in its column, and a
move (a person's, or the forge reopening its issue) brings it back to the board. The default `done.luau` calls it for a
card that has been done for more than 14 days.

`moebius.hold(reason)`
: Holds the card when the script returns, as a person does with `POST /v1/cards/{id}/block`: its running runs are
stopped, the decision is recorded (kind `block_card`) and the holder is `script:<column>` (`card.held.by` is not seen by
a script, which never runs for a held card). The card stays in its column, waiting with `reason`, until a person
releases it, which starts `card.moved_at` again.

`moebius.notify(kind, text)`
: Puts the card in the inbox with `kind` as the item's kind, the card's title as its title and `{reason = text}` as its
data, given to the people the card is given to (`card.assignees`), once while an item of that kind is open for the card.
Unlike `moebius.ask`, the card does not wait. The default scripts use the kind `stale`.

`moebius.set_label(label, present)`
: Gives the card's issue the label (`present` true) or takes it away (`present` false) when the script returns, on its forge
too; the issue's other labels stay. Taking away matches the name in any case. Nothing is sent when the issue is as asked,
so a script may call it at every tick. A label the forge does not know is made by it (GitHub does). If the forge refuses,
the card goes on, because a label is a mark and not a step: the forge is told once as a `forge_error` and is not asked
again for ten minutes. The default `code.luau` and `review.luau` give the label `in-progress` and the other default
scripts take it away, so an issue carries it exactly while its card is in `In progress` or `In review` and not stuck.

`moebius.publish_change()` returns the card's changes as an array like `card.changes`.
: Publishes the work of the card's latest successful `code` run on the branch of its change in each repository the card
works in: the branch is pointed at that run's last commit there and pushed to that repository's forge. The card's own
repository always has a change, on `moebius/card-<number>`, unless that change was merged and no commit was made in it
since (the card went back for another repository): a pull request with no commits is not opened. Another repository has one, on
`moebius/card-<KEY>-<number in the project>`, once a `code` run left commits in it beyond its default branch. The first
time this creates the change, without a pull request where the repository does not work on the forge: the pull request then
opens when the change is offered (`moebius.offer_change`), so not before the review approved it. Where it does (the setting
`work_on_forge`, on by default for GitHub, GitLab and Forgejo, see [templates](templates.md#working-on-the-forge)) the
pull request opens now, as a draft, with a body written from the pull request template. Once a change has a pull request, publishing a later
round updates it: a push, and the title and body say what the branch does now.
It does nothing to a change whose commit is already published, so a script may call it on every tick. Publishing records
the time, which starts the grace period of the change's `checks`, and publishes `change_updated`. Raises an error `the
card has no code run to publish` when the card has no successful `code` run, and an error with the forge's message when
a push or pull request fails. Unlike `card.changes`, which is a snapshot from before the script ran, the returned array
is current. The time this call takes does not count against the time limit.

`moebius.open_pull_request()` returns the card's changes as an array like `card.changes`.
: Publishes the changes as `moebius.publish_change` does and, for each that has no pull request yet, opens it at once
(on GitHub, with a body that says `Closes #<number>` and what the change does, without the review's `judgements`,
which only `moebius.offer_change` attaches). A review script that calls it
when the latest `code` run ended lets the forge run its checks on the pull requests while the review of the card still
reads the changes: the poll reads the checks of each pull request's head commit, and the grace period of
`checks` applies, so the script can wait for them and offer the changes only when they are green (see
Checks and conflicts). On a forge without pull requests it does nothing beyond the publish,
and it does nothing at all when the commit already has one, so a script may call it on every tick. Raises the errors
of `moebius.publish_change`. The returned array is current. The time this call takes does not count against the time
limit.

`moebius.run_checks()` returns the card's changes as an array like `card.changes`.
: Starts the repository's own checks (see Checks and conflicts) on each change's current commit, with the `checks` of
that change's repository, unless they already ran or are running for it. It returns at once; a returned `checks` is
`pending` while they run. It does nothing to a change, whose `checks` stays as it is, in a repository with a forge, or
without a `checks` file in the repository's settings. Raises an error `the card has no change to check` before the
first change is published. The time this call takes does not count against the time limit.

`moebius.reproduction()` returns a table or nil.
: What the card's code runs say of the reproduction test of its change (see Test first), read from the `REPRODUCE:`
lines of the summary of the card's latest succeeded `code` run that has any: `command` (a string) and `files` (an array of strings) for a test, or `untestable` (a string, the
reason) and an empty `files` for a change that cannot be tested this way. nil when no succeeded `code` run of the card
names a test.
Raises an error `the card has no change to test` before the change is published. The time this call takes does not
count against the time limit.

`moebius.run_test(command)` returns a table.
: Runs `command` on the change's current commit, as the repository's checks run (see Checks and conflicts): with `sh -c`
in a throwaway worktree, on a thread of its own, with the cleared environment and stopped after
`checks.timeout_seconds`. It returns at once with how the run stands: `status` is `pending` while it runs, then `passed`
(exit status 0), `failed`, or `error` when it could not run; `output` is the last 4 KB of what it printed with secrets
removed, or why it could not run. A command runs once for each commit of the change; later calls return its result.
The result is kept in the server's memory, so after a restart the command runs again. Raises an error `the card has no
change to test` before the change is published. The time this call takes does not count against the time limit.

`moebius.run_test_at_base(command, files)` returns a table like `moebius.run_test`.
: The same, but on the commit the change starts from (where it leaves the default branch, `git merge-base`) with the
files `files` (an array of paths, optional) added as the change's current commit has them. A path the change does not
have makes the run an `error`. Its result is kept apart from `moebius.run_test`'s for the same command.

`moebius.start_runs(agent, n, opts)` returns an array of run ids (numbers).
: Starts `n` runs of the agent `agent` with the purpose `attempt` (see Attempts), `n` from 1 to 8. Each is a run as
`moebius.start_run` makes it, in a worktree and on a branch `moebius/run-<id>` of its own, all starting at the same
commit: the branch of the card's latest successful `code` run, or the default branch. The run is told what a `code`
run is told. `opts` is optional, a table with these keys, all optional: `models`, an array of strings, each a model or
a model class (see [router](router.md)), used in turn, the first for the first attempt and so on round again, in place
of the agent's own `model`; `note`, text added to the prompt of every attempt as `moebius.start_run` adds its note;
`runner`, the name of a runner, as for `moebius.start_run`. The attempts are recorded as one group, in `card.attempts`.
The call returns once the runs are started. Raises an error for another `n`, and for the errors of
`moebius.start_run`, but attempts start only while the run limit has a free slot: as many as fit, and none (an empty array, the card waiting with `the run limit is reached`) when none does. For a card this server leaves alone (see `moebius.start_run`) it starts none and returns an empty array, the card waiting with the same reason. When one run cannot be started after others were, the error names the runs that were started, and
they stay attempts of the card. The harness refuses a model it cannot give (`this harness cannot choose the model of
a run`). The time this call takes does not count against the time limit.

`moebius.check_attempt(run)` returns a table.
: Judges the attempt `run` (a number from `card.attempts`) against what the card is held to, as `moebius.run_test` does
a test: on a thread of its own, answering at once with `status` `pending` while it goes, then `passed` or `failed`
with the same answer on every later call, and `output`, why it failed. An attempt whose run did not succeed has failed
(`The run failed.`) without more. Otherwise the repository's own checks (the project's `checks`, run
on the attempt's commit as for a change; none on a repository with a forge) must succeed, and when `card.test_first`
the attempt's summary must name a test with `REPRODUCE:` lines that fails on the commit the attempt starts from, with the test's
files added, and passes on the attempt, or say `untestable: <why>`; a reason is judged by the review later. The result
is stored with the attempt (`card.attempts[i].result`) and published as the event `attempt_judged`. Raises an error
`run <n> is not an attempt` for another run. The time this call takes does not count against the time limit.

`moebius.settle_attempts(run, text)` returns nothing.
: Settles the card's open attempts, which ends the round. With `run`, the id of an open attempt whose `result` is
`passed`, that attempt is picked: its run's purpose becomes `code`, so that it is the card's latest successful `code`
run and every later run builds on its branch, the card's `note` is cleared, and `text` is recorded as the reason it was
picked. Without `run` (nil), none passed: the round counts in `card.rounds` as a failed one, and `text` (a default when
it is empty) becomes the card's `note`, which the next round is told. The other attempts keep their branches. Publishes
the event `attempts_settled` with `{card, picked, runs}`. Raises an error `the card has no open attempts`, `attempt <n>
did not pass, so it cannot be picked` or `run <n> is not an open attempt of the card`.

`moebius.post_review(review)` returns the address of the review on the forge, or nil.
: Writes `review`, a table with `verdict` (`approve` or `changes`), `findings`, `judgements` and `comments` as `card.review` has them, on the pull request of the card's change in its own repository as a review of it: `REQUEST_CHANGES` with a comment on each of `comments` that is a line of the diff, or `APPROVE`, tied to the commit that was read; comments on other lines go into the summary; a forge that refuses a verdict on the request of its own user takes the review as one that only comments. Once for each round and head: a second call finds the first and answers nil. Does nothing, and answers nil, where the repository does not work on the forge (`work_on_forge`, [templates](templates.md#working-on-the-forge)) or the change has no pull request yet. A forge that refuses the review is reported as `forge_error` and the card goes on. The time this call takes does not count against the time limit. The default `review` script calls it before it offers the change or sends the card back.

`moebius.offer_change(judgements)` returns the id (a number) of the change in the card's own repository (the first change when that one was merged and the card went back for another repository). It raises an error `the card has nothing new to offer` when every change of the card is merged. `judgements`
is optional text, which is stored in each inbox item as `data.judgements` for the person to read.
: Publishes the changes as `moebius.publish_change` does and opens each one's pull request if it has none (on GitHub,
with a body that says `Closes #<number>`, or `Part of <KEY>-<number in the project> (issue #<number> in <repository>)`
for a change in another repository than the card's own, what the code run said it did, the files it changed and, when
`judgements` is given, what the review found), then offers them to a person: it adds an item `ready_to_merge` to the inbox for each
change, with `{change, branch}` as its data, publishes `change_offered` for each, and moves the card to `Ready to
merge` at once (not when the script returns). Raises the errors of `moebius.publish_change`. Call it last in a script.
A person merges the branch `moebius/card-<number>`, and the branch of each other change. The time this call takes does
not count against the time limit.

`moebius.merge_change(reason)` returns two values: the commit (a string) of the first merge it made, which is the
card's own repository's unless that was merged before, and nil; or nil and why a merge failed (a string). `reason` is
optional text and becomes the reason of each merge judgement.
: Merges the card's open changes on their forges, one after another in the order of `card.changes`, and moves the card
to `Done` at once (not when the script returns), which resolves the changes' inbox items. There is no merge across
forges at once, so a merge that fails stops there (ADR 0019): the changes before it stay merged, the changes from it on
stay open, and the card stays where it is. It waits with the reason `the merge in <repository> failed: <the error>`,
which the second value is too, and is marked with an inbox item `merge_stopped` (see `InboxItem` in `desktop-api.md`)
that names the repositories whose changes are merged and those whose changes are still open; the same change failing
again keeps the item. A change that failed is not tried again at once: until the item's `retry_at` (30 seconds after
the first failure, twice as long after each failure in a row, at most ten minutes) a call returns nil and the last
reason without asking the gate or the forge and without recording a judgement; a new head of the change, or the item
resolved by a person, has it tried at the next call. A change whose pull request the server opened as a draft (where
the agents work on the forge) is taken out of draft before the forge is asked to merge it. A later merge of the rest, by this function or by a person, finishes the card. The judgement is recorded as the column's script
(`script:<column>`, see [the decision model](decision-model.md)), not as a person. Raises `the card has no open change to
merge` when the card has no change. A script is not a person, so the [merge gate](desktop-api.md#the-merge-gate) holds its merges to the
repository's locked paths, hold and pause, and the rest: a refusal is the second value (the reason of the gate, such as `the change touches .github/workflows/ci.yml, which is locked by .github/**: only a person merges it`), the card goes to `Ready to merge`, and an inbox item `merge_refused` names the reason; no `merge_stopped` item is made for it. A repository that merges
without a person overrides `merge.luau` with a call to this function as its last statement; the default `merge.luau`
waits for a person instead. The time this call takes does not count against the time limit.

`moebius.lane_job()` returns two values: the card's latest job of the [test lane](lanes.md) for its change's head, as a table with `id`, `state`, `lane`, `head`, `step`, `failed_step` and `summary`, or nil when no job can be queued; and why not, as a string. `state` is `queued`, `running`, `measured`, `passed`, `failed`, `errored` or `cancelled`, and `step` is the step a running job is at.
: Asks the lane the repository uses for a job for the card's change at its head: a followed pull request needs a round of the Reviewer that ended `approve` or `approve_code` for that head, and a change of Moebius's own needs the card's latest review to approve. A job is queued, unless one is there for this head already (then that one is returned, with the reason `a job for this version is there already`) or something stops it: no lane for the repository, a lane that is not configured, no open change, no valid `lane.toml` in the project, a frozen repository, a pause of all agents, a held card, a locked path, no verdict yet. A job that is queued is started by the lane's own pass, not by the script. The call does not wait for the job: a script calls it at each tick and reads `state`. When a job of a change of Moebius's own fails, the lane moves the card to `In progress` with the failure as its note. See [the lane](lanes.md#triggering).

`moebius.ask(kind, reason)`
: Puts the card in the inbox for a person, with `kind` as the item's kind, the card's title as its title and
`{reason = reason}` as its data. It does this once: nothing is added while an item of that kind is open for the card.
Also does what `moebius.wait(reason)` does. Items for a card are resolved when a person approves the issue, when its
change is merged and when its change is closed.

`moebius.stuck(reason)`
: The same as `moebius.ask("stuck", reason)`.

`moebius.record_outcome(outcome)` returns a number.
: Sets `outcome` (a string such as `merged` or `closed`) on every judgement of the card that has none, and returns how many it set.

`moebius.runs_active()` returns a number.
: How many runs of cards have not ended, across all boards. Chats (runs without a card) are not counted, so they do
not use up `moebius.run_limit`.

`moebius.agent_budget(name)` returns a number or nil.
: The token budget of the agent `name` for one card (`agents.<name>.token_budget`); nil when it has none or there is no such
agent. A script compares it with `card.tokens`.

`moebius.tokens_today()` returns a number.
: The input and output tokens all runs have used since the start of the server's local day. A script compares it with
`moebius.day_budget`.

## Errors

A script fails when it does not compile, has no `on_card` function (`the script does not define a function on_card(card)`),
raises an error, or runs out of time or memory.
The card stays where it is, and the server publishes the event `board_error` with `{card, message}`. The message
starts with the script's file name. The next tick tries again. An override with an error does not fall back to the
default script.

Error messages that come from Moebius:

| Message | Cause |
|---|---|
| `the script ran out of time` | The time limit below was reached. |
| a message containing `memory` | The memory limit below was reached. |
| `no column "<name>"` | `moebius.move` with a column the card's board does not have. |
| `no agent named <name>` | `moebius.start_run` with an agent that is not configured. |
| `there is no runner named <name>` | `moebius.start_run` with a runner that is not configured. |
| a message starting `the runner` and containing `is busy` | `moebius.start_run` on a runner that has `max_runs` runs going; no run was made, so a script may try again on a later tick. |
| `the card has no code run to publish` | `moebius.publish_change` or `moebius.offer_change` on a card without a successful `code` run. |
| `the card has nothing new to offer: its changes are merged` | `moebius.offer_change` on a card all of whose changes are merged and that has no new commits. |
| `the card has no change to check` | `moebius.run_checks` before the change is published. |
| `the card has no open change to merge` | `moebius.merge_change` on a card without a change. |
| `the card has no change to test` | `moebius.reproduction`, `moebius.run_test` or `moebius.run_test_at_base` before the change is published. |
| `n is 1 to 8 attempts` | `moebius.start_runs` with another number of attempts. |
| `run <n> is not an attempt` | `moebius.check_attempt` with a run that is not an attempt. |
| `the card has no open attempts`, `attempt <n> did not pass, so it cannot be picked`, `run <n> is not an open attempt of the card` | `moebius.settle_attempts`. |

## The sandbox

Scripts run in Luau's sandbox mode with the following changed or removed:

- Removed: `require`, `dofile`, `loadfile`, `load`, `loadstring`, `getfenv`, `setfenv`. There is no `io`, no `package`,
  no network and no way to start a process.
- `os` has only `clock`, `date`, `difftime` and `time`: no `execute`, `remove` or `getenv`. `debug` has only `info` and
  `traceback`.
- Global tables and the standard libraries are read-only, so a script cannot change `string` or `table`.

What is left is the base functions (`assert`, `error`, `pairs`, `ipairs`, `pcall`, `select`, `setmetatable`, `tonumber`,
`tostring`, `type`, `typeof`, `unpack` and the like), `print` (it writes to the server's standard output), `string`,
`table`, `math`, `bit32`, `utf8`, `coroutine`, `buffer`, `vector`, `os` and `debug` as above, and `moebius`.

## Limits

- Time: 2 seconds for one run of a script, loading included. Only the time a script itself spends counts. The time
  spent inside `moebius.decide`, `moebius.start_run`, `moebius.publish_change`, `moebius.open_pull_request`,
  `moebius.run_checks`, `moebius.reproduction`, `moebius.run_test`, `moebius.run_test_at_base`, `moebius.post_review`, `moebius.offer_change`
  and `moebius.merge_change` is added back to the deadline, so a slow decider, harness start, push or merge does not fail a
  script, nor does the time Moebius takes to describe the card (`card.changes` needs git).
- Memory: 16 MiB for one run of a script.

Both limits are fixed; the configuration cannot change them.

## Checking a script

`moebius check-script <file> [--column <name>] [--kanban <file>] [--title <text>] [--author <login>]` loads the file in the sandbox above
and runs its `on_card` once against a made-up card, then prints what the script did. It needs no server, no
configuration and no repository, and it stores and starts nothing.

| Option | Default | Meaning |
|---|---|---|
| `<file>` | required | The script, such as `ready.luau` (the file name in a project's settings). |
| `--column` | `Backlog` | The column the card is in: one of the six columns, spelled as in the table at the top (`Ready` for `Not started` too), or a custom column of the board given with `--kanban`. |
| `--kanban` | none | A project's `kanban.toml` (see [Custom columns](#custom-columns)). The made-up card's board is then the one it defines: `moebius.move` accepts its columns, the card can be in one of them, and `card.column_class` is its class. A file that is not valid is refused with the message of the parser, as a save would be. |
| `--title` | `A card to check the script with` | The card's `title`. |
| `--author` | `person:owner` | The card's `author`. `card.trusted` is true only for a person (`person:<handle>`, or `person` on a record from before people), as on a repository without a forge. |

The made-up card has `column_class` (its column's class), `id` and `number` 1, an empty `body`, `blocked_by`, `children` and `labels` empty, no `parent`, `claimed_by` or `taken_by`, `assignees` empty, `priority` `none`, no `due`, no `estimate`, `kind` nil, `fields` empty, `merged` false, `rounds`, `reviews` and `tokens` 0, and
`stopped` and `test_first` false, and `attempts` empty. It has no `note`, `change`, `run`, `review` or `selection`. The `moebius` table has every constant and function of
this page, but records its calls instead of acting:

- `moebius.decide` takes the fail-closed option and says so; it raises an error when that is not one of the options.
- `moebius.start_run` returns 1, then 2 and so on. `moebius.start_runs` returns the next `n` numbers, `moebius.check_attempt` returns
  `status` `pending` with an empty `output`. `moebius.has_agent` says yes for every name.
- `moebius.publish_change` and `moebius.run_checks` return a change on branch `moebius/card-1` whose `checks` is `none`.
  `moebius.post_review` returns nil and `moebius.offer_change` returns 1 and `moebius.merge_change` returns a made-up commit (forty zeros). `moebius.record_outcome` returns 0. `moebius.reproduction` returns nil, and
  `moebius.run_test` and `moebius.run_test_at_base` return `status` `pending` with an empty `output`.
- `moebius.agent_budget` returns nil, `moebius.tokens_today` and `moebius.runs_active` return 0, and the limits are the
  defaults.
- `moebius.move` raises `no column "<name>"` for a column the board (the six, or the one `--kanban` defines) does not
  have.

The output is one line for each call that does something, in the order of the calls, then the exit status 0:

| Line | Call |
|---|---|
| `moved the card to <column>`, with ` (note: <note>)` when there is one | `moebius.move` |
| `waited: <reason>` | `moebius.wait` |
| `closed the issue and moved the card to Done`, then `: <reason>` when there is one | `moebius.close_issue` |
| `gave the issue the label "<label>"` or `took the label "<label>" from the issue` | `moebius.set_label` |
| `started a run of <agent> for <purpose>`, then ` on <runner>` when a runner is given, with the note as above | `moebius.start_run` |
| `started <n> attempts of <agent>`, with the note as above | `moebius.start_runs` |
| `started judging attempt <run>` | `moebius.check_attempt` |
| `picked attempt <run>`, then `: <reason>` when there is one, or `settled the attempts: none passed` | `moebius.settle_attempts` |
| `asked a person (<kind>) and waited: <reason>` | `moebius.ask` |
| `put the card in the inbox as stuck: <reason>` | `moebius.stuck` |
| `archived the card`, `held the card: <reason>`, `told the card's people (<kind>): <text>` | `moebius.archive`, `moebius.hold`, `moebius.notify` |
| `decided <kind>: ... the fail-closed option "<option>" was taken` | `moebius.decide` |
| `published the change`, `started the checks of the change`, `offered the change to a person, and the card moved to Ready to merge`, `merged the change, and the card moved to Done` | `moebius.publish_change`, `moebius.run_checks`, `moebius.offer_change`, `moebius.merge_change` |
| `wrote the review (<verdict>) on the pull request of the change` | `moebius.post_review` |
| `recorded the outcome "<outcome>"` | `moebius.record_outcome` |
| `started the test "<command>" on the change` or `on the base commit` | `moebius.run_test`, `moebius.run_test_at_base` |

A script that calls none of them prints `The script did nothing: it did not move, wait, ask or start anything.` A script
that fails (see Errors), a file that cannot be read and a column that does not exist print the message on standard
error, starting with `moebius: `, and exit with status 1. The check shows one path through the script, the one the made-up
card takes: change `--title`, `--author` and `--column` to try the others. What the real card holds (its runs, its change,
its blockers, its tokens) is not made up, so a script that reads them must also be tried on a real card.

## Project overrides

A project replaces the script of a column by having a file `<script>` (for example `ready.luau`) in its settings, the
files of `projects/<project name>/` in the [harness configuration repository](harness-configuration.md#project-settings).
The script that runs for a card is the one of the project whose board the card is on, so two projects that share a
repository can work differently; what is about the code (the checks, test first, the rules) is the repository's, the
same in every project. Nothing is read from a repository's own `.moebius/` any more, nor from a column script in
`repos/<repository name>/`, where scripts were kept before projects and from where the server moves them to the
repository's home project. The server reads the file on every run of the script:

- It is read from the default branch of the harness configuration repository's clone, which the server fetches every 30
  seconds and fast-forwards. A change in another branch is not read.
- If the file does not exist, or the server has no harness configuration repository (`[harness]`), the built-in script
  runs.
- An override replaces a whole script. There is no way to call the built-in one from it.

A script from a project runs with the same sandbox and limits as a built-in one.

The desktop API reads, saves and removes an override: `GET`, `PUT` and `DELETE` of
[`/v1/projects/{id}/scripts/{column}`](desktop-api.md#get-v1projectsidscriptscolumn). A save checks the script
like `moebius check-script`, commits it to the harness configuration repository and pushes it to its `origin`.

## Custom columns

A project may add columns of its own to its board. They are defined in `projects/<project name>/kanban.toml` of the
[harness configuration repository](harness-configuration.md#project-settings), and each has a script of its own
([ADR 0030](../adr/0030-custom-board-columns-per-project.md)). The guide has a worked example:
[Custom columns](../guide/custom-columns.md).

```toml
[[column]]
name = "Ready for testing"
script = "testing.luau"
after = "In review"
counts_as = "In review"
```

Every `[[column]]` has these keys, and no other key is accepted:

| Key | Required | Meaning |
|---|---|---|
| `name` | yes | The column's name, as the board shows it: 1 to 40 characters, without a control character, trimmed. Unique among the file's columns without regard to case, and not the name of a built-in column in any case (`Ready` is the alias of `Not started` and is taken too). |
| `script` | no | The column's script, a file of the project's folder: a plain file name ending in `.luau`, of letters, digits, `-`, `_`, `.` and spaces, not starting or ending with a dot or a space, without `..` and without a folder. Left out, the column runs the script file of its class (`review.luau` for a column that counts as `In review`), so a project's own `review.luau` serves it too. |
| `after` | one of `after` and `counts_as` | The column this one comes after: a built-in column other than `Done`, or another custom column of the file. Left out, the column comes after the built-in column that its `counts_as` names. |
| `counts_as` | no | The column's class: one of `Backlog`, `Not started`, `In progress`, `In review`, `Ready to merge` or `Done`, spelled exactly so. Left out, the class of the column it comes after (the class of the built-in column, or the class the other custom column has). |
| `lane` | no | `true` makes a card that enters the column queue a job for the project's [test lane](lanes.md) (default `false`). Only for a column whose class is `In review`. |

**Order.** The board is Backlog, then each built-in column followed by the custom columns that come after it, and Done
last. A custom column that comes after another custom column comes right behind it, before the later columns that come
after the same built-in one. Columns that come after the same column keep the order they have in the file, so two
columns that are both `after = "In review"` are `In review`, the first of them, the second. A file with no `[[column]]`
is the six built-in columns, as no file is.

**The class.** A card in a custom column counts exactly as the built-in column that is its class wherever built-in logic
decides something from the column: whether the issue is closed, whether the card still blocks the cards that wait for
it (a card whose class is `Done` is done), the burn-up of a cycle and the figures of the analytics and of milestones, the
open cards of a project, `/done` and the other commands, the limits of review rounds, a merge from `Ready to merge`, the
reviews of followed pull requests, the loops and the status of a card. A script reads it as `card.column_class`. The
class is for reading only: what Moebius writes still goes through scripts. A card that enters a custom column that counts
as `Done` is not closed by that, nor is its issue, unless the column's script closes it (`moebius.close_issue`); when the
engine applies a close, the card goes to the built-in `Done`; and the moves that Moebius makes to a built-in column (the
merge to `Done`, a reopened issue to `Backlog`) are to that column.

**What runs.** The script of a custom column runs for its cards as a built-in one does, with the same sandbox and
limits, and its file is read from the project's settings on every run. A custom column whose script file is not there
runs the script of its class, and the card's waiting text says so. A column the file does not have (but a card is in)
runs the script of the class it had, with the notice below.

**Checks.** A file is refused as a whole, and the message starts with `kanban.toml:` and has every mistake in it,
separated by `; `. The classes of mistake are:

| Message | Cause |
|---|---|
| `kanban.toml: ` and the parser's message | The text is not TOML, or a `[[column]]` has a key that is not one of the four above, a value of the wrong type or no `name`. |
| `column <n> has no name` | An empty `name`. |
| `column <n> ("<name>"): a name has at most 40 characters and no control character` | A name that is too long or has a control character. |
| `column <n> ("<name>"): the name is taken by a built-in column ("Ready" is the alias of "Not started")` | A name that is a built-in column's, in any case. |
| `column <n> ("<name>"): the name is used twice (names are compared without case)` | Two columns with the same name. |
| `column <n> ("<name>"): script "<file>" must be a plain file name ending in .luau, without a folder` | A script name that is not safe. |
| `column <n> ("<name>"): counts_as "<class>" is none of Backlog, Not started, In progress, In review, Ready to merge, Done` | A class that is not one of the six. |
| ``column "<name>": give `after` (the column it comes after) or `counts_as` `` | A column with neither key. |
| `column "<name>": after "<column>" names no column` | An `after` that is no built-in or custom column. |
| `column "<name>": nothing comes after Done` | `after = "Done"`. |
| ``column "<name>": the `after` columns lead round in a circle`` | Custom columns that come after each other in a loop. |
| ``column "<name>": `lane = true` is for a column that counts as In review`` | A column marked `lane` whose class is another. |

**Saving it.** A save is refused with `422` when the file is not valid (the messages above), and with `409` when it would
remove a column while cards are in it: `kanban.toml would remove the column "<name>" while cards are in it (#12, #15);
move them to another column first`, which names up to ten cards of the first such column. Removing the file removes all
the custom columns, and is refused the same way. The same two checks hold wherever the file changes: a save through the
API, the Moebius agent's edits and the merge of a proposal that changes it (the merge is refused and the proposal
stays). A file that is committed by hand to the repository and fetched is not checked before it is read, so this is what
happens to a board whose file is wrong when it is read:

- **A file that is not valid** leaves the columns last known in force (the six built-in ones before there was a valid
  file), and the project's `columns_problem` says what is wrong. The cards keep running their scripts, and nothing fails.
- **Cards in a column the file has lost** (it was removed or renamed by hand) are not on the board's columns any more,
  but each runs the script of the class that column had, and its waiting text says that the project's `kanban.toml` no
  longer has the column. `columns_problem` names the columns and the classes. Move the cards to a column of the board, or
  put the column back, and the notice goes. The cards still count as the class they had until then.
- A card in a column nobody ever defined (moved by hand in the store) counts as `Backlog`, runs the script of
  `Backlog`, and is told so.

A project with no `kanban.toml` has the six built-in columns, as before. `GET /v1/projects/{id}` gives the board's
`columns` and the `columns_problem`, and `GET /v1/snapshot` gives each card its `column_class` (see the [desktop
API](desktop-api.md)). The file is read from the default branch of the harness configuration repository's clone, like
the scripts, and the board built from it is kept for as long as the text is the same.

## Checks and conflicts

A change is **published** on a branch of its own, `moebius/card-<number>`, which the card keeps for all its rounds: each
round's work builds on the last, so the branch only moves forward. On GitHub publishing pushes the branch and the first
time opens the pull request; later it updates it with a push. Without a forge there is nothing to push. A card over
several repositories has one change in each (see Cards over several repositories), each checked as this section says
with what its own repository has.

The `checks` of a change in `card.changes` say how the checks of the change's current commit stand:

- On GitHub, the poll records the check runs and commit statuses of the pull request's head commit. Checks recorded for another commit do
  not count. Until they are recorded for the published commit, `checks` is `pending` for 2 minutes after the publish
  (the checks have not started yet), and then `none`, so that a repository without CI does not wait for ever.
  The change's `failed_checks` lists the failing check runs and statuses, one per line: the name, how it ended, what it says and
  where to read more, such as `lint: failure, 3 warnings (https://ci.example/runs/1)`.
- Without a forge, a repository may have a file `checks` in its settings: a shell command. It is read from the settings,
  never from the change, so that the change cannot decide how it is checked. `moebius.run_checks()`
  runs it with `sh -c` in a throwaway worktree of the change's commit, on a thread of its own, and stops it after
  `checks.timeout_seconds` (see the [configuration](configuration.md)). While it runs `checks` is `pending`, then
  `success` (exit status 0) or `failure`, and the change's `check_output` holds the last 4 KB of its output, standard
  output and error together, with secrets removed. It runs once for each published commit. Without the file `checks` is
  `none`. The command runs with the server's environment cleared but for `PATH`, `HOME`, `USER`, `LANG` and `TMPDIR`,
  so that the change's code, which the checks run, sees no token or key of the server.

The change's `conflict` lists the files that keep it from merging cleanly into its `base` (found with
`git merge-tree`; nothing is checked out), or is nil when it merges cleanly. It is found when a script first reads it,
not when the card is read, and the answer is kept for as long as the heads of `base` and of the change's branch stay the
same, so a script that reads it at every tick starts git only when one of them moves.

## Cards over several repositories

A card works in the repositories of `card.repositories` (ADR 0019): its own issue's, then those a person or the Moebius
agent named with `PUT /v1/cards/{id}/repositories` (see `desktop-api.md`), from the repositories of the card's project
and its files. A run of the card, started by `moebius.start_run` or `moebius.start_runs`, gets a worktree of each, on its
branch `moebius/run-<id>` of each repository, each starting at that repository's branch of the card's latest successful
`code` run that worked in it, or at its default branch. The harness starts in the worktree of the card's own repository;
the prompt of a `code` run or an attempt names where the others are and which other repositories of the project it may
ask for. The tool-call check lets the run write in each of its worktrees and nowhere else (see the context key
`worktrees` in `plugin-api.md`). A run over several repositories runs on the server: `moebius.start_run` raises an
error for one that names a runner. When the run ends, what it left in each worktree is committed; a `code` run that
changed nothing in any of them has failed.

A `code` run that finds it needs another repository says so in its summary, with a line `REPOSITORY: <name>` for each.
When it succeeds and names a repository of the project that the card does not work in yet, that repository is added to
the card after the others, and the card's note says so: `card.note` is set although the run succeeded, so the default
`code.luau` starts the next round, which has a worktree of it. A name that is no repository of the project, alone, sets
a note that says which the run may ask for. A name the card already works in is passed over. A run that asks succeeds even when it changed nothing,
since it could not do the work without the repository. The lines are left out of the pull request's body. Only a
`code` run asks: an attempt is told where its worktrees are, not that it may ask.

The tool-call check reads the `rules.md` of the card's own repository for every call of the run, whichever worktree it
writes in. A change in another repository than the card's own is merged with the message `Merge <KEY>-<number>:
<title>`, and its pull request names the card instead of closing an issue. Every pull request of such a card has the
line `One of the changes of <KEY>-<number> in <repositories>, which Moebius merges together, in that order.`

A review run (`review`, `review-standards`, `review-spec`) of such a card gets a worktree of each repository too, each
on the branch of the card's change there, and its prompt names each with the command that shows its change (`git -C
<worktree> diff <base>...HEAD`): the Reviewer sees every diff of the card at once, and its findings send the whole card
back. The standards axis judges each repository's change by that repository's own standards and its own `review.md`
(see `harness-configuration.md`); the spec axis judges the issue over all the changes together; a single `review`
does both.

The changes of a card are merged in the order of `card.repositories`. A change is not merged while one before it is
open: `POST /v1/changes/{id}/merge` refuses it with `422`, and `moebius.merge_change` merges them in that order. A
person's merge that fails while another change of the card is merged marks the card the same way. A
card is done once its last open change is merged: merging one change of several resolves only that change's inbox item.
A merge that fails stops there and marks the card (see `moebius.merge_change`).

**If you upgrade:** `card.change` is now `card.changes`, and `moebius.publish_change`, `moebius.open_pull_request` and
`moebius.run_checks` return an array. A project script written for one change raises an error that says so (reading
`card.change`, or a field such as `checks` on the array), and the card stays where it is until the script is changed;
loop over the array, as the default scripts do.

## Attempts

A card that is hard to get right can be tried several times at once, and the best attempt kept. Sampling a model many
times solves far more tasks than one attempt does; the gain is only as large as the choice among the samples is good,
and a choice by execution (tests and checks) together with a choice by a model is better than either alone. A column
script does this with `moebius.start_runs`, `moebius.check_attempt` and `moebius.settle_attempts`; the
[Kanban guide](../guide/kanban.md#a-hard-card-several-attempts) shows a complete `code.luau`. The default
`code.luau` does not use attempts: a repository opts in with its own.

- An **attempt** is a run with the purpose `attempt`. It is prompted, committed and judged as a failed run as a `code`
  run is, but it is not a `code` run: `card.rounds` does not count it, `card.run` and the branch the next run builds on
  do not follow it, and `card.review` ignores it. Its tokens count in `card.tokens` and so against
  `agents.<name>.token_budget`, which is one budget for all the attempts of a card together; the router refuses the
  requests of every attempt of the card once the budget is spent.
- The attempts started by one call are a **group**. Until a group is settled its attempts are in `card.attempts`.
- A succeeded attempt is **judged** by `moebius.check_attempt`: the checks and, test first, the reproduction test.
- When every attempt of the group has been judged the script **settles** it. With no attempt that passed, the round is a
  failed one (it counts in `card.rounds`, so `board.round_limit` bounds the cost) and the next round is told why. With
  exactly one that passed, that one is picked, without a selection run. With several, a run with the purpose
  `review-select` of the Reviewer is started with the passing attempts listed in its `note`, one per line, as
  `- run <id>: branch moebius/run-<id>`. It is told to compare the diffs (`git diff <base>...<branch>`), give its reasons
  for each and end with a last line `PICK: <run id>`. Like a review run it changes nothing that is kept and
  changing nothing is no failure. `card.selection` holds the pick and the reasons, and the script calls
  `moebius.settle_attempts` with them. The text is kept with the picked attempt as its reason.
- The picked attempt becomes the card's `code` run. The attempts that were not picked keep their branches
  `moebius/run-<id>` in the repository's clone until the card is done.

The desktop API shows the latest group of a card as `attempts` in its board entry (`GET /v1/snapshot`) and as
`attempt_judged` and `attempts_settled` entries in `GET /v1/cards/{id}/history`; the web app's live view and its
card page show which attempts passed, which was picked and why.

## Test first

A repository works test first when its project has a file `test-first` in its settings (its content does not matter);
`card.test_first` is then true. The Coder's prompt then tells it to write a test that reproduces the issue before it
changes any code, see it fail, make the change and see it pass, and to name the test at the end of the final summary of
every run, in lines the server reads:

```
REPRODUCE: cargo test --test issue_42
REPRODUCE-FILE: tests/issue_42.rs
```

The `REPRODUCE:` line is the command that runs only the reproduction test; each `REPRODUCE-FILE:` line in the same block of lines (up to a blank line, above or below the `REPRODUCE:` line; those under an
earlier `REPRODUCE:` line are that line's) is a file, relative to the repository's root, that holds it. The last `REPRODUCE:` line of a summary counts. A change that
cannot be tested this way, such as one of the documentation or the configuration only, has the single line
`REPRODUCE: untestable: <why>` instead. Backticks around the command or a path are ignored. The summary is stored with
the run, so the test is stored with the attempt or the `code` run that made the change, and no file of the change names it.

`moebius.reproduction()` reads the lines. The default `review.luau` then runs the command with
`moebius.run_test_at_base(command, files)`, where it must fail, and with `moebius.run_test(command)`, where it must pass.
Until both have ended the card waits with `running the reproduction test`. When either does not hold, or no succeeded
`code` run of the card names a test, the card goes back to `In progress` with a note that starts `Test first:` and says
which did not hold, with the output. A change that says it is untestable goes on to the review, whose run is given the
reason to judge. All of this comes after the checks and conflicts and before the review.

## The default Kanban

| Script | What it does |
|---|---|
| `backlog.luau` | If `card.trusted` or `card.confirmed`, moves the card to `Not started`. Otherwise asks a person (`moebius.ask("approve_issue", ...)`) and waits; approving the issue in the desktop API moves the card to `Not started`. |
| `ready.luau` | For an issue with a label that starts `wayfinder:` (a [wayfinder](../guide/grilling.md#plan-a-big-piece-of-work-with-wayfinder) map or one of its tickets: a question, not a task), starts no work and closes nothing: it waits with `a wayfinder question, not a task: it stays until the wayfinder session closes it`, whatever its sub-issues, blockers and budget say, until the session closes the issue. For a parent (`card.children` is not empty) that has no run and no change of its own, starts no work of its own: it waits with `waiting for sub-issues #3, #4` while a sub-issue is open, and closes the issue (`moebius.close_issue`) once every sub-issue is closed. Otherwise, for a card given to a person (`card.assignees` is not empty), starts no work: it waits with `given to @lea, @tom` and stays in `Not started` until the assignees are changed. Otherwise waits while `card.blocked_by` is not empty, naming the issues. Waits with `the daily token budget is spent` when `moebius.day_budget` is set and `moebius.tokens_today()` has reached it. Waits while `moebius.runs_active() >= moebius.run_limit`. Otherwise moves the card to `In progress`. The board settles cards in the order urgent first, then due first, then oldest, so when the run limit leaves one slot, an urgent card gets it before an older low one. |
| `backlog.luau` | When `card.moved_at` is more than `STALE_AFTER_DAYS` (30) days old, tells the card's people (`moebius.notify("stale", ...)`) and holds the card with `stale: it has not moved for 30 days`; a person releases it, which resolves the `stale` item. Otherwise, if `card.trusted` or `card.confirmed`, moves the card to `Not started`. Otherwise asks a person (`moebius.ask("approve_issue", ...)`) and waits; approving the issue in the desktop API moves the card to `Not started`. |
| `ready.luau` | For an issue with a label that starts `wayfinder:` (a [wayfinder](../guide/grilling.md#plan-a-big-piece-of-work-with-wayfinder) map or one of its tickets: a question, not a task), starts no work and closes nothing: it waits with `a wayfinder question, not a task: it stays until the wayfinder session closes it`, whatever its sub-issues, blockers and budget say, until the session closes the issue. For a parent (`card.children` is not empty) that has no run and no change of its own, starts no work of its own: it waits with `waiting for sub-issues #3, #4` while a sub-issue is open, and closes the issue (`moebius.close_issue`) once every sub-issue is closed. Otherwise a card whose `card.moved_at` is more than `STALE_AFTER_DAYS` (30) days old is told to its people and held, as in `backlog.luau` (a wayfinder question and a parent waiting for its sub-issues are meant to wait, so they come before this and are never held as stale). Otherwise, for a card given to a person (`card.assignees` is not empty), starts no work: it waits with `given to @lea, @tom` and stays in `Not started` until the assignees are changed. Otherwise waits while `card.blocked_by` is not empty, naming the issues. Waits with `the daily token budget is spent` when `moebius.day_budget` is set and `moebius.tokens_today()` has reached it. Waits while `moebius.runs_active() >= moebius.run_limit`. Otherwise moves the card to `In progress`. |
| `code.luau` | Waits while the latest run is `starting` or `running`. Moves the card to `In review` when the latest run is a `code` run that `succeeded` and the card has no `card.note`. Otherwise marks the card stuck with `stopped by a person` when `card.stopped`, with `the round limit is reached` when `card.rounds >= moebius.round_limit`, and with `the token budget of N is spent` when `card.tokens` has reached `moebius.agent_budget("Coder")`; waits with `the daily token budget is spent` when the day's budget is spent; and else starts a `Coder` run with purpose `code` and `card.note` as its note: the first round, a retry after a `code` run that `failed` or was `stopped`, or the next round after the card was sent back. |
| `review.luau` | Waits while the latest run is going, and moves the card back to `In progress` when its latest run is neither a succeeded `code` run nor a review run (`review`, `review-standards` or `review-spec`). Then, in this order: publishes the changes (`moebius.publish_change`) and starts their own checks (`moebius.run_checks`); waits while the `checks` of any change are `pending`; sends the card back to `In progress` when those of a change are `failure`, with the note `The checks failed:` and the failing checks or the output; sends it back when a change's `conflict` is set, with the conflicting files and the instruction to merge its `base` into its branch and resolve them (for a card over several repositories, both notes name the change's repository); when `card.test_first`, waits for the reproduction test and sends the card back when it does not hold (see Test first); without an agent `Reviewer` (`moebius.has_agent`, which is true on every server because the Reviewer is built in), offers the change; offers the change with `card.review.judgements` when `card.review.verdict` is `approve`; sends the card back with `card.review.findings` as the note when it is `changes`; otherwise reviews along two axes: starts a run with purpose `review-standards` and, once it succeeded, one with purpose `review-spec` (given the untestable reason of test first as its note). The agent of each axis is the agent `Standards` or `Spec` when the configuration has one, else the `Reviewer`. A review run that `failed` restarts the round with the Standards axis, until `card.reviews >= moebius.round_limit`, when the card is marked stuck with `the round limit is reached`. The card is marked stuck with `stopped by a person` when `card.stopped`, before anything else, and with `the token budget of N is spent` when `card.tokens` has reached `moebius.agent_budget("Reviewer")`; it waits with `the daily token budget is spent` while the day's budget is spent, instead of starting a review. |
| `merge.luau` | Waits for a person to merge the changes. The merge happens through the desktop API or, on GitHub, on the pull request. When the forge's checks of a change's pull request failed, sends the card back to `In progress` with the note `The checks failed:` and the failing checks; when a change no longer merges cleanly (its default branch moved), sends it back with the conflict as the note (for a card over several repositories, both notes name the change's repository); that resolves the changes' inbox items. A repository whose changes should merge without a person overrides this script with one that calls `moebius.merge_change(reason)` as its last statement, once it has checked what it wants to hold. |
| `done.luau` | Calls `moebius.record_outcome("merged")` when `card.merged` (a change of the card was merged) and `moebius.record_outcome("closed")` otherwise, so every judgement made for the card learns how it ended. Archives the card (`moebius.archive`) once `moebius.now() - card.moved_at` is more than `ARCHIVE_AFTER_DAYS` (14) days. The board looks at the cards in `Done` whose outcomes are recorded every ten minutes, not at every tick. A project changes the numbers in its own copy of the script. A server that is upgraded to this version archives every card that has been done for longer than that, and holds every waiting card older than 30 days, at the first tick: `moved_at` of an existing card starts as the time of its last recorded move, or its creation. |

The default Kanban needs an `[agents.Coder]` table in the configuration, because `code.luau` starts the agent
`Coder`. An `[agents.Reviewer]` table is optional: with it every change is reviewed along two axes before it is offered, without
it changes are offered unreviewed. `[agents.Standards]` and `[agents.Spec]` tables are optional too: they give an axis
its own agent (another model, or a model class), where the Reviewer would otherwise review it.

## Example

[`examples/scripts/ready-large-cards-alone.luau`](../../examples/scripts/ready-large-cards-alone.luau) is a `ready.luau`
override. It is tested in `crates/moebius-core/tests/examples.rs`.

### Cards that change project or column set

- A card put on another project's board by a `project:<name>` label keeps its column if that board has a column of the
  same name; otherwise it moves to the built-in column of its class and the move is recorded.
- A closed pull request of someone else sends the card back to the column it was taken from if the board still has
  it, and to Not started if not.
- A script that closes the issue of a card already in a column that counts as Done leaves the card there.
