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
| `card.run` | table or nil | The card's latest run (the one with the highest id). nil when the card has had no run. |
| `card.run.id` | number | The run's id. |
| `card.run.agent` | string | The agent's name. |
| `card.run.purpose` | string | The purpose the run was started with. |
| `card.run.status` | string | One of `starting`, `running`, `succeeded`, `failed`, `stopped`. |

Only `card.run` can be nil. Check it before you read its fields.

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

`moebius.start_run(agent, purpose)` returns the run's id (a number).
: Starts a run of the agent named `agent` (an `[agents.<name>]` table of the configuration) for this card in a new
worktree on a new branch `moebius/run-<id>`. `purpose` is any string; the default workflow uses `code`, and only
`code` runs count as `card.rounds`. The call returns once the run is started, not when it ends. Raises an error
when the agent does not exist (`no agent named <agent>`), the worktree cannot be made or the harness cannot be
started. The time this call takes does not count against the time limit. It does not check `run_limit`; a script
that wants the limit checks `moebius.runs_active()` first.

`moebius.move(column)`
: Moves the card to `column` when the script returns. Raises an error `no column "<column>"` for a name that is not one
of the six. Moving to the card's own column does nothing. The last call wins when a script calls it twice.

`moebius.wait(reason)`
: Leaves the card where it is and records `reason` as why. The last call wins. The server keeps the card's current reason
in memory (it is gone after a restart until the next tick), publishes `card_waiting` when it changes, and shows it in
the snapshot's `waiting` (see the [desktop API](desktop-api.md)). A card that moves, or whose script does not wait, has
no reason.

`moebius.offer_change()` returns the change's id (a number).
: Offers the branch of the card's latest run (`moebius/run-<id>`) as a change through the repository's forge. On a
repository without a forge the change is the branch itself. On GitHub the branch is pushed and a pull request is opened
whose body says `Closes #<number>`. It adds an item `ready_to_merge` to the inbox with `{change, branch}` as its data,
publishes `change_offered`, and moves the card to `Ready to merge` at once (not when the script returns). Raises an
error `the card has no run to offer` when the card has had no run, and an error with the forge's message when the push
or pull request fails. Call it last in a script. The time this call takes does not count against the time limit.

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
| `the card has no run to offer` | `moebius.offer_change` on a card without runs. |

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
  spent inside `moebius.decide` and `moebius.start_run` is added back to the deadline, so a slow decider or a slow
  harness start does not fail a script. The time spent in the other functions counts, `moebius.offer_change` among
  them.
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

## The default workflow

| Script | What it does |
|---|---|
| `backlog.luau` | If `card.trusted`, moves the card to `Ready`. Otherwise asks a person (`moebius.ask("approve_issue", ...)`) and waits; approving the issue in the desktop API moves the card to `Ready`. |
| `ready.luau` | Waits while `card.blocked_by` is not empty, naming the issues. Waits while `moebius.runs_active() >= moebius.run_limit`. Otherwise moves the card to `In progress`. |
| `code.luau` | Starts a `Coder` run with purpose `code` when the card has no run or its latest run has another purpose, and waits. Waits while the latest run is `starting` or `running`. Offers the change when the run `succeeded`. When the run `failed` or was `stopped`: marks the card stuck with `the round limit is reached` when `card.rounds >= moebius.round_limit`, otherwise starts another `Coder` run. |
| `review.luau` | Offers the change at once. No review exists yet, and `code.luau` offers the change itself, so no card reaches this column today. |
| `merge.luau` | Waits for a person to merge the change. The merge happens through the desktop API or, on GitHub, on the pull request. |
| `done.luau` | Calls `moebius.record_outcome("merged")`, so every judgement made for the card learns how it ended. |

The default workflow needs an `[agents.Coder]` table in the configuration, because `code.luau` starts the agent
`Coder`.

## Example

[`examples/scripts/ready-large-cards-alone.luau`](../../examples/scripts/ready-large-cards-alone.luau) is a `ready.luau`
override. It is tested in `crates/moebius-core/tests/examples.rs`.
