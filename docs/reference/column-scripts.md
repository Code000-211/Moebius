# Column scripts

A column script is the behaviour behind one column of a board, written in Luau. This page is for anyone who writes or
changes a script: a person, or an agent such as the Moebius agent. It lists everything a script can read and do. The
code is in `crates/moebius-core/src/board/` (`api.rs` for the `moebius` table, `card.rs` for the `card` table); the default scripts are in `crates/moebius-core/workflow/`.

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
| `card.confirmed` | boolean | True for a card made from a grilling a person confirmed (see `desktop-api.md`, grillings); the person has decided on it, so it needs no approval. |
| `card.trusted` | boolean | True when the issue may move on without approval: it was made by a person in the desktop app on a repository without a forge, or its author is one of the repository's trusted GitHub logins (compared without regard to case). |
| `card.test_first` | boolean | True when the card's repository works test first: its default branch has a file `.moebius/test-first`. See Test first. |
| `card.blocked_by` | table (array of numbers) | The issue numbers the card is blocked by that are not done yet. A number counts as not done when it has no card or its card is not in `Done`. Empty (never nil) when nothing blocks the card. The numbers come from "blocked by #12" lines in the issue's body on GitHub. |
| `card.rounds` | number | How many runs with purpose `code` the card has had since it was last retried, whatever their status (a running one counts), and how many rounds of attempts of which none passed (see Attempts). |
| `card.reviews` | number | How many review rounds the card has had since it was last retried, whatever their status: the runs with purpose `review` and those with purpose `review-standards` (one per round of the two-axis review). |
| `card.tokens` | number | The input and output tokens the card's runs used since it was last retried (cached tokens are not added; the teacher's count too). |
| `card.stopped` | boolean | True when the card's latest run was stopped by a person and the card has not been retried since. |
| `card.review` | table or nil | The latest review of the card's work: the newest `review` run that `succeeded` after the card's latest `code` run, or a `review-spec` run that `succeeded` after a `review-standards` run that `succeeded` (the two axes of one round). nil when there is none, so a new `code` run clears it. |
| `card.review.verdict` | string | `approve` or `changes`. Read from the review run's summary (see below). |
| `card.review.findings` | string | The summary without its verdict line; for a review with no verdict, the text `The review gave no verdict.`; for a `changes` verdict without findings, `The review asked for changes but gave no findings.` |
| `card.review.judgements` | string | For a two-axis review, the judgement calls of both axes, each with its evidence; empty for a `review` run and when there are none. |
| `card.review.standards`, `card.review.spec` | table | Only for a two-axis review: that axis's `verdict` (`approve` or `changes`), `findings` (its blocking findings), `judgements` (its judgement calls) and `dropped` (how many findings were dropped for lacking evidence). |
| `card.note` | string or nil | Why the card was sent back to coding: the findings of a review, the failing checks or a conflict, as the script that moved the card wrote them (the second argument of `moebius.move`). A `code` run that succeeds clears it; one that fails keeps it, so the retry gets it too. |
| `card.change` | table or nil | The card's open change. nil until it is published. |
| `card.change.branch` | string | The branch the change lives on, `moebius/card-<number>`. |
| `card.change.base` | string | What the change merges into: the default branch, as `main` without a forge and `origin/main` on GitHub. |
| `card.change.pull_request` | number or nil | The number of the pull request on GitHub. |
| `card.change.checks` | string | `pending`, `success`, `failure` or `none`, for the change's current commit. See Checks and conflicts. |
| `card.change.failed_checks` | string or nil | On GitHub, the failing check runs, one per line; nil unless `checks` is `failure`. |
| `card.change.check_output` | string or nil | Without a forge, the end of the output of the repository's `.moebius/checks`, once they ended for the current commit. |
| `card.change.conflict` | string or nil | The files that conflict with `card.change.base`, one per line; nil when the change merges cleanly. |
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

Only `card.run`, `card.run.summary`, `card.review`, `card.selection`, `card.note`, `card.change` and its `pull_request`, `failed_checks`, `check_output` and `conflict`, and `model` and `result` of an attempt, can be nil. Check them before you read their fields.

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

`moebius.start_run(agent, purpose, note, runner)` returns the run's id (a number).
: Starts a run of the agent named `agent` (an `[agents.<name>]` table of the configuration) for this card in a new
worktree on a new branch `moebius/run-<id>`. The branch starts at the branch of the card's latest successful `code`
run, so the run sees the work so far, or at the repository's default branch when the card has none. `purpose` is any
string; the default workflow uses `code`, `review-standards` and `review-spec`, and attempts (see Attempts) use `attempt` and `review-select`. Only `code` runs count as `card.rounds`, and only
`review` and `review-standards` runs as `card.reviews`. A `code` run gets the prompt for making the change and commits what the agent leaves; it fails when
the agent changed nothing. A `review` run gets the prompt for reviewing the change on its branch and ending with
the verdict line; a `review-standards` run the prompt for the standards axis (the repository's `CONTRIBUTING.md`,
`CLAUDE.md`, `AGENTS.md` and the optional rubric `.moebius/review.md`, and the fixed smell baseline, always judgement
calls) and a `review-spec` run the prompt for the spec axis (what the issue asked that is missing, not asked for or
looks wrong), both asking for findings with evidence; nothing it changes is kept, and changing nothing is no failure. Any other purpose is prompted like
`code`. `note` is optional text (a string or nil) added to the end of the prompt under the heading `Notes from the
last round:`; the default workflow passes `card.note`. The call returns once the run is started, not when it ends. `runner` is optional (a string or nil): the name of a `[[runners]]` entry (see
[configuration](configuration.md#runners)) that the run goes to over SSH in place of the agent's own `runner`; without
it the agent's runner is used, and without that the run is on the server. Raises an error
when the agent does not exist (`no agent named <agent>`), the runner does not exist (`there is no runner named <name>`)
or is full (`the runner <name> is busy: ...`; no run is made then), the worktree cannot be made or the harness cannot be
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

`moebius.reproduction()` returns a table or nil.
: What the card's change says of its reproduction test in `.moebius/reproduce` (see Test first), read from the change's
latest commit: `command` (a string) and `files` (an array of strings) for a test, or `untestable` (a string, the
reason) and an empty `files` for a change that cannot be tested this way. nil when the change has no such file, or has
it exactly as the commit the change starts from has it (a file an earlier change left says nothing about this one).
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
`moebius.start_run`; when one run cannot be started after others were, the error names the runs that were started, and
they stay attempts of the card. The harness refuses a model it cannot give (`this harness cannot choose the model of
a run`). The time this call takes does not count against the time limit.

`moebius.check_attempt(run)` returns a table.
: Judges the attempt `run` (a number from `card.attempts`) against what the card is held to, as `moebius.run_test` does
a test: on a thread of its own, answering at once with `status` `pending` while it goes, then `passed` or `failed`
with the same answer on every later call, and `output`, why it failed. An attempt whose run did not succeed has failed
(`The run failed.`) without more. Otherwise the repository's own checks (`.moebius/checks` on the default branch, run
on the attempt's commit as for a change; none on a repository with a forge) must succeed, and when `card.test_first`
the attempt's `.moebius/reproduce` must name a test that fails on the commit the attempt starts from, with the test's
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

`moebius.offer_change(judgements)` returns the change's id (a number). `judgements` is optional text, which is stored in
the inbox item as `data.judgements` for the person to read.
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
| `no column "<name>"` | `moebius.move` with an unknown column. |
| `no agent named <name>` | `moebius.start_run` with an agent that is not configured. |
| `there is no runner named <name>` | `moebius.start_run` with a runner that is not configured. |
| a message starting `the runner` and containing `is busy` | `moebius.start_run` on a runner that has `max_runs` runs going; no run was made, so a script may try again on a later tick. |
| `the card has no code run to publish` | `moebius.publish_change` or `moebius.offer_change` on a card without a successful `code` run. |
| `the card has no change to check` | `moebius.run_checks` before the change is published. |
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
  spent inside `moebius.decide`, `moebius.start_run`, `moebius.publish_change`, `moebius.run_checks`,
  `moebius.reproduction`, `moebius.run_test`, `moebius.run_test_at_base` and `moebius.offer_change` is added back to the deadline, so a slow decider, harness start or push does not fail a
  script, nor does the time Moebius takes to describe the card (`card.change` needs git).
- Memory: 16 MiB for one run of a script.

Both limits are fixed; the configuration cannot change them.

## Checking a script

`moebius check-script <file> [--column <name>] [--title <text>] [--author <login>]` loads the file in the sandbox above
and runs its `on_card` once against a made-up card, then prints what the script did. It needs no server, no
configuration and no repository, and it stores and starts nothing.

| Option | Default | Meaning |
|---|---|---|
| `<file>` | required | The script, such as `.moebius/ready.luau`. |
| `--column` | `Backlog` | The column the card is in: one of the six columns, spelled as in the table at the top. |
| `--title` | `A card to check the script with` | The card's `title`. |
| `--author` | `person` | The card's `author`. `card.trusted` is true only for `person`, as on a repository without a forge. |

The made-up card has `id` and `number` 1, an empty `body`, `blocked_by` empty, `rounds`, `reviews` and `tokens` 0, and
`stopped` and `test_first` false, and `attempts` empty. It has no `note`, `change`, `run`, `review` or `selection`. The `moebius` table has every constant and function of
this page, but records its calls instead of acting:

- `moebius.decide` takes the fail-closed option and says so; it raises an error when that is not one of the options.
- `moebius.start_run` returns 1, then 2 and so on. `moebius.start_runs` returns the next `n` numbers, `moebius.check_attempt` returns
  `status` `pending` with an empty `output`. `moebius.has_agent` says yes for every name.
- `moebius.publish_change` and `moebius.run_checks` return a change on branch `moebius/card-1` whose `checks` is `none`.
  `moebius.offer_change` returns 1. `moebius.record_outcome` returns 0. `moebius.reproduction` returns nil, and
  `moebius.run_test` and `moebius.run_test_at_base` return `status` `pending` with an empty `output`.
- `moebius.agent_budget` returns nil, `moebius.tokens_today` and `moebius.runs_active` return 0, and the limits are the
  defaults.
- `moebius.move` raises `no column "<name>"` for a column that does not exist.

The output is one line for each call that does something, in the order of the calls, then the exit status 0:

| Line | Call |
|---|---|
| `moved the card to <column>`, with ` (note: <note>)` when there is one | `moebius.move` |
| `waited: <reason>` | `moebius.wait` |
| `started a run of <agent> for <purpose>`, then ` on <runner>` when a runner is given, with the note as above | `moebius.start_run` |
| `started <n> attempts of <agent>`, with the note as above | `moebius.start_runs` |
| `started judging attempt <run>` | `moebius.check_attempt` |
| `picked attempt <run>`, then `: <reason>` when there is one, or `settled the attempts: none passed` | `moebius.settle_attempts` |
| `asked a person (<kind>) and waited: <reason>` | `moebius.ask` |
| `put the card in the inbox as stuck: <reason>` | `moebius.stuck` |
| `decided <kind>: ... the fail-closed option "<option>" was taken` | `moebius.decide` |
| `published the change`, `started the checks of the change`, `offered the change to a person, and the card moved to Ready to merge` | `moebius.publish_change`, `moebius.run_checks`, `moebius.offer_change` |
| `recorded the outcome "<outcome>"` | `moebius.record_outcome` |
| `started the test "<command>" on the change` or `on the base commit` | `moebius.run_test`, `moebius.run_test_at_base` |

A script that calls none of them prints `The script did nothing: it did not move, wait, ask or start anything.` A script
that fails (see Errors), a file that cannot be read and a column that does not exist print the message on standard
error, starting with `moebius: `, and exit with status 1. The check shows one path through the script, the one the made-up
card takes: change `--title`, `--author` and `--column` to try the others. What the real card holds (its runs, its change,
its blockers, its tokens) is not made up, so a script that reads them must also be tried on a real card.

## Repository overrides

A repository replaces the script of a column by committing a file `.moebius/<script>` (for example
`.moebius/ready.luau`) to its default branch. The server reads it with `git show <ref>:.moebius/<script>` on every run
of the script:

- `<ref>` is the default branch of the clone for a repository without a forge, and `origin/<default branch>` for a
  repository on GitHub, which the poll keeps up to date. Moebius fetches the source repository of a clone without a
  forge every 30 seconds and fast-forwards the clone's default branch when it can; it cannot once a change merged
  here puts the branch ahead of the source.
- Only committed content counts. A change in the working tree of the clone is not read, and neither is any other
  branch.
- If the file does not exist, the built-in script runs.
- An override replaces a whole script. There is no way to call the built-in one from it.

A script from a repository runs with the same sandbox and limits as a built-in one.

The desktop API reads, saves and removes an override: `GET`, `PUT` and `DELETE` of
[`/v1/repositories/{id}/scripts/{column}`](desktop-api.md#get-v1repositoriesidscriptscolumn). A save checks the script
like `moebius check-script`, commits it to the default branch of the clone and, on GitHub, pushes it.

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
  `none`. The command runs with the server's environment cleared but for `PATH`, `HOME`, `USER`, `LANG` and `TMPDIR`,
  so that the change's code, which the checks run, sees no token or key of the server.

`card.change.conflict` lists the files that keep the change from merging cleanly into `card.change.base` (found with
`git merge-tree`; nothing is checked out), or is nil when it merges cleanly.

## Attempts

A card that is hard to get right can be tried several times at once, and the best attempt kept. Sampling a model many
times solves far more tasks than one attempt does; the gain is only as large as the choice among the samples is good,
and a choice by execution (tests and checks) together with a choice by a model is better than either alone. A column
script does this with `moebius.start_runs`, `moebius.check_attempt` and `moebius.settle_attempts`; the
[workflows guide](../guide/workflows.md#a-hard-card-several-attempts) shows a complete `code.luau`. The default
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
`attempt_judged` and `attempts_settled` entries in `GET /v1/cards/{id}/history`; the desktop app's live view and its
card page show which attempts passed, which was picked and why.

## Test first

A repository works test first when its default branch has a file `.moebius/test-first` (its content does not matter);
`card.test_first` is then true. The Coder's prompt then tells it to write a test that reproduces the issue before it
changes any code, see it fail, make the change and see it pass, and to name the test in `.moebius/reproduce`, committed
with the change:

```
# Lines that are empty or start with # are skipped.
cargo test --test issue_42
tests/issue_42.rs
```

The first line is the command that runs only the reproduction test; every further line is a file, relative to the
repository's root, that holds it. A change that cannot be tested this way, such as one of the documentation or the
configuration only, has the single line `untestable: <why>` instead (more lines after it belong to the reason).

`moebius.reproduction()` reads the file. The default `review.luau` then runs the command with
`moebius.run_test_at_base(command, files)`, where it must fail, and with `moebius.run_test(command)`, where it must pass.
Until both have ended the card waits with `running the reproduction test`. When either does not hold, or the change has
no `.moebius/reproduce`, the card goes back to `In progress` with a note that starts `Test first:` and says which did
not hold, with the output. A change that says it is untestable goes on to the review, whose run is given the reason to
judge. All of this comes after the checks and conflicts and before the review.

## The default workflow

| Script | What it does |
|---|---|
| `backlog.luau` | If `card.trusted` or `card.confirmed`, moves the card to `Ready`. Otherwise asks a person (`moebius.ask("approve_issue", ...)`) and waits; approving the issue in the desktop API moves the card to `Ready`. |
| `ready.luau` | Waits while `card.blocked_by` is not empty, naming the issues. Waits with `the daily token budget is spent` when `moebius.day_budget` is set and `moebius.tokens_today()` has reached it. Waits while `moebius.runs_active() >= moebius.run_limit`. Otherwise moves the card to `In progress`. |
| `code.luau` | Waits while the latest run is `starting` or `running`. Moves the card to `In review` when the latest run is a `code` run that `succeeded` and the card has no `card.note`. Otherwise marks the card stuck with `stopped by a person` when `card.stopped`, with `the round limit is reached` when `card.rounds >= moebius.round_limit`, and with `the token budget of N is spent` when `card.tokens` has reached `moebius.agent_budget("Coder")`; waits with `the daily token budget is spent` when the day's budget is spent; and else starts a `Coder` run with purpose `code` and `card.note` as its note: the first round, a retry after a `code` run that `failed` or was `stopped`, or the next round after the card was sent back. |
| `review.luau` | Waits while the latest run is going, and moves the card back to `In progress` when its latest run is neither a succeeded `code` run nor a review run (`review`, `review-standards` or `review-spec`). Then, in this order: publishes the change (`moebius.publish_change`) and starts its own checks (`moebius.run_checks`); waits while `card.change.checks` is `pending`; sends the card back to `In progress` when they are `failure`, with the note `The checks failed:` and the failing checks or the output; sends it back when `card.change.conflict` is set, with the conflicting files and the instruction to merge `card.change.base` into its branch and resolve them; when `card.test_first`, waits for the reproduction test and sends the card back when it does not hold (see Test first); without an agent `Reviewer` (`moebius.has_agent`), or when `card.review.verdict` is `approve`, offers the change; offers the change with `card.review.judgements` when the verdict is `approve`; sends the card back with `card.review.findings` as the note when it is `changes`; otherwise reviews along two axes: starts a run with purpose `review-standards` and, once it succeeded, one with purpose `review-spec` (given the untestable reason of test first as its note). The agent of each axis is the agent `Standards` or `Spec` when the configuration has one, else the `Reviewer`. A review run that `failed` restarts the round with the Standards axis, until `card.reviews >= moebius.round_limit`, when the card is marked stuck with `the round limit is reached`. The card is marked stuck with `stopped by a person` when `card.stopped`, before anything else, and with `the token budget of N is spent` when `card.tokens` has reached `moebius.agent_budget("Reviewer")`; it waits with `the daily token budget is spent` while the day's budget is spent, instead of starting a review. |
| `merge.luau` | Waits for a person to merge the change. The merge happens through the desktop API or, on GitHub, on the pull request. When the change no longer merges cleanly (the default branch moved), sends the card back to `In progress` with the conflict as the note; that resolves the change's inbox item. |
| `done.luau` | Calls `moebius.record_outcome("merged")`, so every judgement made for the card learns how it ended. |

The default workflow needs an `[agents.Coder]` table in the configuration, because `code.luau` starts the agent
`Coder`. An `[agents.Reviewer]` table is optional: with it every change is reviewed along two axes before it is offered, without
it changes are offered unreviewed. `[agents.Standards]` and `[agents.Spec]` tables are optional too: they give an axis
its own agent (another model, or a model class), where the Reviewer would otherwise review it.

## Example

[`examples/scripts/ready-large-cards-alone.luau`](../../examples/scripts/ready-large-cards-alone.luau) is a `ready.luau`
override. It is tested in `crates/moebius-core/tests/examples.rs`.
