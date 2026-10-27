# Column scripts

A column script is the behaviour behind one column of a board, written in Luau. This page is for anyone who writes or
changes a script: a person, or an agent such as the Moebius agent. It lists everything a script can read and do. The
code is in `crates/moebius-core/src/board.rs`; the default scripts are in `crates/moebius-core/workflow/`.

## How a column maps to a script

Every board has the same six columns, in this order. Each column has one script file.

| Column | Script |
|---|---|
| `Backlog` | `backlog.luau` |
| `Ready` | `ready.luau` |
| `In progress` | `code.luau` |
| `In review` | `review.luau` |
| `Ready to merge` | `merge.luau` |
| `Done` | `done.luau` |

Column names are exact: capital first letter, one space between words. You cannot add, remove or rename columns yet.

The board engine runs the script of a card's column for every card on every tick (`board.tick_ms`, default 2 seconds).
After a script moved the card, it runs the script of the new column at once, and so on until a script leaves the card
where it is (at most twelve more runs in one tick). A card in `Done` is skipped once every judgement
made for it has an outcome.

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
| `card.author` | string | Who created the issue: `person` for an issue made in the desktop app, otherwise the GitHub login. |
| `card.column` | string | The column the card is in. |
| `card.trusted` | boolean | True when the issue may move on without approval: it was made by a person in the desktop app on a repository without a forge, or its author is one of the repository's trusted GitHub logins (compared without regard to case). |
| `card.blocked_by` | table (array of numbers) | The issue numbers the card is blocked by that are not done yet. A number counts as not done when it has no card or its card is not in `Done`. Empty (never nil) when nothing blocks the card. The numbers come from "blocked by #12" lines in the issue's body on GitHub. |
| `card.rounds` | number | How many runs with purpose `code` the card has had, whatever their status (a running one counts). |
| `card.reviews` | number | How many runs with purpose `review` the card has had, whatever their status. |
| `card.review` | table or nil | The latest review of the card's work: the newest `review` run that `succeeded` after the card's latest `code` run. nil when there is none, so a new `code` run clears it. |
| `card.review.verdict` | string | `approve` or `changes`. Read from the review run's summary (see below). |
| `card.review.findings` | string | The summary without its verdict line; for a review with no verdict, the text `The review gave no verdict.` |
| `card.note` | string or nil | Why the card was sent back to coding: the findings of a review, the failing checks or a conflict, as the script that moved the card wrote them (the second argument of `moebius.move`). A `code` run that succeeds clears it; one that fails keeps it, so the retry gets it too. |
| `card.change` | table or nil | The card's open change. nil until it is published. |
| `card.change.branch` | string | The branch the change lives on, `moebius/card-<number>`. |
| `card.change.base` | string | What the change merges into: the default branch, as `main` without a forge and `origin/main` on GitHub. |
| `card.change.pull_request` | number or nil | The number of the pull request on GitHub. |
| `card.change.checks` | string | `pending`, `success`, `failure` or `none`, for the change's current commit. See Checks and conflicts. |
| `card.change.failed_checks` | string or nil | On GitHub, the failing check runs, one per line; nil unless `checks` is `failure`. |
| `card.change.check_output` | string or nil | Without a forge, the end of the output of the repository's `.moebius/checks`, once they ended for the current commit. |
| `card.change.conflict` | string or nil | The files that conflict with `card.change.base`, one per line; nil when the change merges cleanly. |
| `card.run` | table or nil | The card's latest run (the one with the highest id). nil when the card has had no run. |
| `card.run.id` | number | The run's id. |
| `card.run.agent` | string | The agent's name. |
| `card.run.purpose` | string | The purpose the run was started with. |
| `card.run.status` | string | One of `starting`, `running`, `succeeded`, `failed`, `stopped`. |
| `card.run.summary` | string or nil | The agent's final answer: the text of the last assistant message Pi reports in its `agent_end` event. nil until the run has one. |

The verdict of a review is read from the end of the summary of a `review` run. A last line `VERDICT: approve` or
`VERDICT: changes` (the word in any case) gives the verdict, and the lines before it are the findings. A summary
without such a line, or with any other word, is `changes`: work that was not reviewed is not approved.

Only `card.run`, `card.run.summary`, `card.review`, `card.note`, `card.change` and its `pull_request`, `failed_checks`, `check_output` and `conflict` can be nil. Check them before you read their fields.

## The `moebius` table

Arguments are positional. A wrong number or type of argument raises an error (see Errors).

### Constants

| Name | Type | Meaning |
|---|---|---|
| `moebius.run_limit` | number | How many runs may go at once across all boards (`run_limit`). |
| `moebius.round_limit` | number | How many `code` rounds a card gets (`board.round_limit`). |

### Functions

`moebius.decide(kind, context, options, fail_closed)` returns `choice, reason`.
: Asks a decision point and records a judgement. `kind` is a string, `context` a table of string keys to string values,
`options` an array of strings, `fail_closed` the option to take when nobody can answer. Only the kind `tool_call` has a
decider on a server today, so any other kind returns `fail_closed` with a reason that starts with `refused: no decider
answers`. Returns two strings. The time this call takes does not count against the script's time limit. Raises an error
when the judgement cannot be stored.

`moebius.start_run(agent, purpose, note)` returns the run's id (a number).
: Starts a run of the agent named `agent` (an `[agents.<name>]` table of the configuration) for this card in a new
worktree on a new branch `moebius/run-<id>`. The branch starts at the branch of the card's latest successful `code`
run, so the run sees the work so far, or at the repository's default branch when the card has none. `purpose` is any
string; the default workflow uses `code` and `review`. Only `code` runs count as `card.rounds`, and only `review` runs
as `card.reviews`. A `code` run gets the prompt for making the change and commits what the agent leaves; it fails when
the agent changed nothing. A `review` run gets the prompt for reviewing the change on its branch and ending with
the verdict line; nothing it changes is kept, and changing nothing is no failure. Any other purpose is prompted like
`code`. `note` is optional text (a string or nil) added to the end of the prompt under the heading `Notes from the
last round:`; the default workflow passes `card.note`. The call returns once the run is started, not when it ends. Raises an error
when the agent does not exist (`no agent named <agent>`), the worktree cannot be made or the harness cannot be
started. The time this call takes does not count against the time limit. It does not check `run_limit`; a script
that wants the limit checks `moebius.runs_active()` first.

`moebius.has_agent(name)` returns a boolean.
: True when an `[agents.<name>]` table of the configuration exists.

`moebius.move(column, note)`
: Moves the card to `column` when the script returns. Raises an error `no column "<column>"` for a name that is not one
of the six. Moving to the card's own column does nothing. The last call wins when a script calls it twice. `note` is
optional text (a string or nil) that is stored on the card as `card.note` when it moves, to say why it went back; the
default `code.luau` gives it to the next `Coder` run. A card that moves out of `Ready to merge` has its inbox items
resolved.

`moebius.wait(reason)`
: Leaves the card where it is and records `reason` as why. The last call wins. The server keeps the card's current reason
in memory (it is gone after a restart until the next tick), publishes `card_waiting` when it changes, and shows it in
the snapshot's `waiting` (see the [desktop API](desktop-api.md)). A card that moves, or whose script does not wait, has
no reason.

`moebius.publish_change()` returns the card's change as a table like `card.change`.
: Publishes the work of the card's latest successful `code` run on the change's branch, `moebius/card-<number>`: the
branch is pointed at that run's last commit and the forge is told. The first time this creates the change (on GitHub:
pushes the branch and opens a pull request whose body says `Closes #<number>`); later it updates it (on GitHub: a push).
It does nothing when that commit is already published, so a script may call it on every tick. Publishing records the
time, which starts the grace period of `card.change.checks`, and publishes `change_updated`. Raises an error `the card has no code
run to publish` when the card has no successful `code` run, and an error with the forge's message when the push or
pull request fails. Unlike `card.change`, which is a snapshot from before the script ran, the returned table is current.
The time this call takes does not count against the time limit.

`moebius.run_checks()` returns the card's change as a table like `card.change`.
: Starts the repository's own checks (see Checks and conflicts) on the change's current commit, unless they already ran
or are running for it. It returns at once; the returned `checks` is `pending` while they run. It does nothing, and
`checks` stays as it is, on a repository with a forge, or without `.moebius/checks` on its default branch. Raises an
error `the card has no change to check` before the change is published. The time this call takes does not count
against the time limit.

`moebius.offer_change()` returns the change's id (a number).
: Publishes the change as `moebius.publish_change` does and offers it to a person: it adds an item `ready_to_merge` to
the inbox with `{change, branch}` as its data, publishes `change_offered`, and moves the card to `Ready to merge` at
once (not when the script returns). Raises the errors of `moebius.publish_change`. Call it last in a script. A person
merges the branch `moebius/card-<number>`. The time this call takes does not count against the time limit.

`moebius.ask(kind, reason)`
: Puts the card in the inbox for a person, with `kind` as the item's kind, the card's title as its title and
`{reason = reason}` as its data. It does this once: nothing is added while an item of that kind is open for the card.
Also does what `moebius.wait(reason)` does. Items for a card are resolved when a person approves the issue, when its
change is merged and when its change is closed.

`moebius.stuck(reason)`
: The same as `moebius.ask("stuck", reason)`.

`moebius.record_outcome(outcome)` returns a number.
: Sets `outcome` (a string such as `merged`) on every judgement of the card that has none, and returns how many it set.

`moebius.runs_active()` returns a number.
: How many runs have not ended, across all boards.

## Errors

A script fails when it does not compile, has no `on_card` function, raises an error, or runs out of time or memory.
The card stays where it is, and the server publishes the event `board_error` with `{card, message}`. The message
starts with the script's file name. The next tick tries again. An override with an error does not fall back to the
default script.

Error messages that come from Moebius:

| Message | Cause |
|---|---|
| `the script ran out of time` | The time limit below was reached. |
| a message containing `memory` | The memory limit below was reached. |
| `no column "<name>"` | `moebius.move` with an unknown column. |
| `no agent named <name>` | `moebius.start_run` with an agent that is not configured. |
| `the card has no code run to publish` | `moebius.publish_change` or `moebius.offer_change` on a card without a successful `code` run. |
| `the card has no change to check` | `moebius.run_checks` before the change is published. |

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
  spent inside `moebius.decide`, `moebius.start_run`, `moebius.publish_change`, `moebius.run_checks` and
  `moebius.offer_change` is added back to the deadline, so a slow decider, harness start or push does not fail a
  script, nor does the time Moebius takes to describe the card (`card.change` needs git).
- Memory: 16 MiB for one run of a script.

Both limits are fixed; the configuration cannot change them.

## Repository overrides

A repository replaces the script of a column by committing a file `.moebius/<script>` (for example
`.moebius/ready.luau`) to its default branch. The server reads it with `git show <ref>:.moebius/<script>` on every run
of the script:

- `<ref>` is the default branch of the clone for a repository without a forge, and `origin/<default branch>` for a
  repository on GitHub, which the poll keeps up to date.
- Only committed content counts. A change in the working tree of the clone is not read, and neither is any other
  branch.
- If the file does not exist, the built-in script runs.
- An override replaces a whole script. There is no way to call the built-in one from it.

A script from a repository runs with the same sandbox and limits as a built-in one.

## Checks and conflicts

A change is **published** on a branch of its own, `moebius/card-<number>`, which the card keeps for all its rounds: each
round's work builds on the last, so the branch only moves forward. On GitHub publishing pushes the branch and the first
time opens the pull request; later it updates it with a push. Without a forge there is nothing to push.

`card.change.checks` says how the checks of the change's current commit stand:

- On GitHub, the poll records the check runs of the pull request's head commit. Checks recorded for another commit do
  not count. Until they are recorded for the published commit, `checks` is `pending` for 2 minutes after the publish
  (the checks have not started yet), and then `none`, so that a repository without CI does not wait for ever.
  `card.change.failed_checks` lists the failing check runs, one per line: the name, how it ended, what it says and
  where to read more, such as `lint: failure, 3 warnings (https://ci.example/runs/1)`.
- Without a forge, a repository may have a file `.moebius/checks` on its default branch: a shell command. It is read
  from the default branch, never from the change, so that the change cannot decide how it is checked. `moebius.run_checks()`
  runs it with `sh -c` in a throwaway worktree of the change's commit, on a thread of its own, and stops it after
  `checks.timeout_seconds` (see the [configuration](configuration.md)). While it runs `checks` is `pending`, then
  `success` (exit status 0) or `failure`, and `card.change.check_output` holds the last 4 KB of its output, standard
  output and error together, with secrets removed. It runs once for each published commit. Without the file `checks` is
  `none`.

`card.change.conflict` lists the files that keep the change from merging cleanly into `card.change.base` (found with
`git merge-tree`; nothing is checked out), or is nil when it merges cleanly.

## The default workflow

| Script | What it does |
|---|---|
| `backlog.luau` | If `card.trusted`, moves the card to `Ready`. Otherwise asks a person (`moebius.ask("approve_issue", ...)`) and waits; approving the issue in the desktop API moves the card to `Ready`. |
| `ready.luau` | Waits while `card.blocked_by` is not empty, naming the issues. Waits while `moebius.runs_active() >= moebius.run_limit`. Otherwise moves the card to `In progress`. |
| `code.luau` | Waits while the latest run is `starting` or `running`. Moves the card to `In review` when the latest run is a `code` run that `succeeded` and the card has no `card.note`. Otherwise marks the card stuck with `the round limit is reached` when `card.rounds >= moebius.round_limit`, and else starts a `Coder` run with purpose `code` and `card.note` as its note: the first round, a retry after a `code` run that `failed` or was `stopped`, or the next round after the card was sent back. |
| `review.luau` | Waits while the latest run is going, and moves the card back to `In progress` when its latest run is neither a succeeded `code` run nor a `review` run. Then, in this order: publishes the change (`moebius.publish_change`) and starts its own checks (`moebius.run_checks`); waits while `card.change.checks` is `pending`; sends the card back to `In progress` when they are `failure`, with the note `The checks failed:` and the failing checks or the output; sends it back when `card.change.conflict` is set, with the conflicting files and the instruction to merge `card.change.base` into its branch and resolve them; without an agent `Reviewer` (`moebius.has_agent`), or when `card.review.verdict` is `approve`, offers the change; sends the card back with `card.review.findings` as the note when the verdict is `changes`; starts a `Reviewer` run with purpose `review` otherwise. A `review` run that `failed` or was `stopped` is started again, until `card.reviews >= moebius.round_limit`, when the card is marked stuck with `the round limit is reached`. |
| `merge.luau` | Waits for a person to merge the change. The merge happens through the desktop API or, on GitHub, on the pull request. When the change no longer merges cleanly (the default branch moved), sends the card back to `In progress` with the conflict as the note; that resolves the change's inbox item. |
| `done.luau` | Calls `moebius.record_outcome("merged")`, so every judgement made for the card learns how it ended. |

The default workflow needs an `[agents.Coder]` table in the configuration, because `code.luau` starts the agent
`Coder`. An `[agents.Reviewer]` table is optional: with it every change is reviewed before it is offered, without it
changes are offered unreviewed.

## Example

[`examples/scripts/ready-large-cards-alone.luau`](../../examples/scripts/ready-large-cards-alone.luau) is a `ready.luau`
override. It is tested in `crates/moebius-core/tests/examples.rs`.
