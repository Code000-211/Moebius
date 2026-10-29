# The default workflow, and where a person steps in

## When you want this

You want to know what Moebius does with an issue by itself, why a card is where it is, what a person can do to it and
which setting changes which behaviour. The scripts behind the columns are in `crates/moebius-core/workflow/`; this
page describes what they do. The exact functions they call are in the
[column script reference](../reference/column-scripts.md), and the routes a person uses are in the
[desktop API reference](../reference/desktop-api.md).

## The path of a card

A card is an issue on the board of one repository. It moves through six columns, each with a script:

```
Backlog -> Ready -> In progress -> In review -> Ready to merge -> Done
```

| Column | Script | What it does |
|---|---|---|
| `Backlog` | `backlog.luau` | Moves a trusted card, or one made from a confirmed grilling, to `Ready`. Any other card waits in the inbox as `approve_issue` until a person approves it. |
| `Ready` | `ready.luau` | Waits while issues block the card, while the daily token budget is spent and while the run limit is reached. Then moves the card to `In progress`. |
| `In progress` | `code.luau` | Starts a run of the agent `Coder` for each round, waits while it goes, and moves the card to `In review` when a round has succeeded. |
| `In review` | `review.luau` | Publishes the change, waits for its checks, sends the card back when they fail or the change conflicts, has the agent `Reviewer` read it (when there is one) and offers the change to a person. |
| `Ready to merge` | `merge.luau` | Waits for a person to merge. Sends the card back when the change no longer merges cleanly. |
| `Done` | `done.luau` | Records how every decision made for the card ended. |

A card is trusted when its issue was made through the desktop API on a repository without a forge (its author is
`person`), or when its author is one of the repository's `trusted` logins (GitHub logins, GitLab usernames, Forgejo
logins). On GitHub, GitLab and Forgejo the poll creates the cards, and an issue that is closed there moves its card to `Done`. The board's
header names the trusted logins, and its pencil edits them (`PUT /v1/repositories/{id}`); a card's page has a pencil
next to its title that edits the title and body in place (`PUT /v1/cards/{id}`), on a forge the issue too, where
"blocked by #n" in the body sets its blockers.

The board engine runs the script of every card's column on every tick (`board.tick_ms`, two seconds by default). When a
script moves the card, the script of the next column runs at once. [Column scripts](column-scripts.md) says how to
change any of them for one repository.

## What each step does

### Backlog

A trusted card moves on. For any other, `moebius.ask("approve_issue", ...)` adds one inbox item and the card waits for
a person (see below).

### Ready

In this order: `waiting for #12, #13` while issues block the card (the numbers come from `blocked by #n` lines in the
body of a GitHub issue, and an issue counts as done when its card is in `Done`); `the daily token budget is spent` when
`budgets.day_tokens` is set and used up; `the run limit is reached` when `run_limit` runs of cards are going; else the
card moves. Chats do not count against `run_limit`.

### In progress: the Coder

Each round is a `code` run of the agent `Coder` in a new worktree, on a branch `moebius/run-<id>`. The branch starts at
the branch of the card's latest successful `code` run, so each round builds on the last; the first round starts at the
default branch. The prompt is the issue, what to do, and after a send-back the notes of the last round under the
heading `Notes from the last round:`. When the run ends, Moebius commits what the agent left in the worktree. A round
that changed nothing has failed. When a round has succeeded and the card has no note, the card moves to `In review`.

A round that failed is followed by another. Three limits keep that from going on for ever. Each marks the card stuck,
which is an inbox item of kind `stuck`, instead of starting a run:

| Limit | Setting | Reason shown |
|---|---|---|
| Too many rounds | `board.round_limit` (default 5), counted since the last retry | `the round limit is reached` |
| The card used up the agent's tokens | `agents.Coder.token_budget` | `the token budget of N is spent` |
| A person stopped the run | `POST /v1/runs/{id}/stop` | `stopped by a person` |

While `budgets.day_tokens` is spent no round starts, and the card waits with `the daily token budget is spent`. The
router refuses model requests with HTTP 429 `budget_exceeded` over either budget, so a run that is going stops making
progress too.

### In review: checks, conflicts and the review loop

1. The change is published on a branch `moebius/card-<number>`, the same for all rounds of the card. On GitHub the first
   publish also opens a pull request that says `Closes #<number>`, on GitLab a merge request, on Forgejo a pull request; later ones push
   to it.
2. The checks run. On GitHub they are the check runs of the pull request, found by the poll, on GitLab the latest
   pipeline of the merge request's head commit, on Forgejo the commit statuses of the pull request's head commit (such
   as those Forgejo Actions reports); a repository with none
   gets `none` two minutes after the publish. Without a forge they are the command in `.moebius/checks` on the default
   branch, run in a throwaway worktree of the change and stopped after `checks.timeout_seconds`; without that file
   there are none. While they are pending the card waits with `waiting for the checks`.
3. Failing checks send the card back to `In progress` with a note that starts `The checks failed:` and gives the
   failing checks or the end of the output.
4. A change that does not merge cleanly into the default branch sends the card back with a note that names the
   conflicting files and says to merge the default branch into the branch and resolve them.
5. Without an agent `Reviewer` the change is offered now. With one, a `Reviewer` run (purpose `review`) reads the change
   in a worktree on the code branch; nothing it changes is kept. Its answer must end with a line `VERDICT: approve` or
   `VERDICT: changes` (see [Agents](agents.md)). `approve` offers the change. `changes` sends the card back with the
   findings as its note. An answer without a verdict counts as `changes`.
6. A review run that failed is started again, until `card.reviews` reaches `board.round_limit` and the card is marked
   stuck. A stopped run, or a spent token budget of the Reviewer, also marks it stuck. While the daily budget is
   spent no review starts.

In a repository that works **test first**, a step comes between 4 and 5: the change's reproduction test must hold
before anyone reviews the change. See [Test first](#test-first) below.

The loop of coding, checks and review ends when the change is offered or when the Coder's rounds reach the limit.
A send-back from any of steps 3 to 5 is a new `code` round with the reason in its prompt, on top of the last round's
branch.

### Test first

Tests generated from the issue and used as a gate are the review step with the best evidence: in a study on SWE-bench
Verified they raised the share of correct changes among those let through from 61% to 92% (Otter, arXiv 2502.05368).
A test that runs says more than a reviewer's opinion. A repository turns it on by committing an empty file
`.moebius/test-first` to its default branch. Then:

1. The Coder is told the rule of red, then green: before it changes any code it writes a test that reproduces the issue
   in a file of its own and sees it fail, then makes the change and sees it pass. It names the test in
   `.moebius/reproduce`, committed with the change: the command that runs only that test on the first line, the files
   that hold the test on the next ones. A change that cannot be tested this way (documentation, configuration) writes
   the single line `untestable: <why>` instead.
2. In review, after the checks and the conflict check, Moebius runs the command twice, each in a throwaway worktree
   with the time limit and cleared environment of the repository's checks: on the commit the change starts from with
   the test's files added, where it must fail, and on the change, where it must pass. The card waits with
   `running the reproduction test` meanwhile.
3. When the test passes without the fix, fails with it, cannot run, or the change names no test, the card goes back to
   `In progress` with a note that starts `Test first:` and says which did not hold, with the test's output.
4. A change that says it is untestable goes on to the review, and the reviewer is given the reason to judge.

The file formats and the functions behind this are in the [column script reference](../reference/column-scripts.md#test-first).
A repository can also write its own gate. This `review.luau` reviews by the reproduction test alone, without a
Reviewer, and offers a change that says it is untestable for the person to judge when they merge:

<!-- script: review.luau -->
```luau
function on_card(card)
  local run = card.run
  if run == nil or run.purpose ~= "code" or run.status ~= "succeeded" then
    moebius.move("In progress")
    return
  end
  moebius.publish_change()
  local change = moebius.run_checks()
  if change.checks == "pending" then
    moebius.wait("waiting for the checks")
    return
  elseif change.checks == "failure" then
    moebius.move("In progress", "The checks failed:\n" .. (change.failed_checks or change.check_output or ""))
    return
  end

  local test = moebius.reproduction()
  if test == nil then
    moebius.move("In progress", "Name the test that reproduces the issue in .moebius/reproduce.")
    return
  end
  if test.untestable ~= nil then
    moebius.offer_change()
    return
  end
  local before = moebius.run_test_at_base(test.command, test.files)
  local after = moebius.run_test(test.command)
  if before.status == "pending" or after.status == "pending" then
    moebius.wait("running the reproduction test")
  elseif before.status ~= "failed" then
    moebius.move("In progress", "The test must fail without your fix, but it did not:\n" .. before.output)
  elseif after.status ~= "passed" then
    moebius.move("In progress", "The test must pass with your fix, but it did not:\n" .. after.output)
  else
    moebius.offer_change()
  end
end
```

### Ready to merge

Offering the change adds an inbox item `ready_to_merge` with `{change, branch}`, publishes the event `change_offered` and
moves the card here. `merge.luau` waits with `waiting for a person to merge`. If the default branch moves so that the
change no longer merges, it sends the card back to `In progress` with the conflict as the note, and the inbox item
is resolved.

### Done

A merged change moves its card to `Done`, resolves its inbox items and publishes `change_merged`. `done.luau` then sets
the outcome `merged` on every decision recorded for the card, so that each learns how it ended.

## What a person can do

Each action is recorded as a decision whose decider is `person`. Bodies are JSON objects; send `{}` when you have nothing
to say. See the [desktop API](desktop-api.md) for a script that does all of this.

| Action | When it works | Call | What happens |
|---|---|---|---|
| Approve an issue | The card is in `Backlog`. | `POST /v1/cards/{id}/approve` `{"reason": "..."}` | The card's inbox items are resolved and the card moves to `Ready`. |
| Retry a card | The card has an open `stuck` item, or a `change_closed` item because its pull request was closed without a merge. | `POST /v1/cards/{id}/retry` `{"reason": "..."}` | The item is resolved and the card's rounds, reviews and tokens count from zero. The board starts work again. A card with a closed pull request goes back to `In progress` with the note `The pull request was closed without a merge.`, and its next publish opens a new pull request. |
| Reject a change | The change is open and its card is in `Ready to merge`. | `POST /v1/changes/{id}/reject` `{"reason": "..."}` | The card goes back to `In progress`, the reason is its note (`A person rejected the change.` without one), and the next Coder round is told. The change stays open. |
| Merge a change | The change is `open`. | `POST /v1/changes/{id}/merge` `{"reason": "..."}` | The default branch of the clone gets a merge commit (without a forge) or the pull request is merged (GitHub). The answer is `{"commit": ...}`. The card moves to `Done`. |
| Steer a run | The run is running. | `POST /v1/runs/{id}/steer` `{"message": "..."}` | The agent gets the message while it works. The run stores a `steered` event. |
| Stop a run | The run is running. | `POST /v1/runs/{id}/stop` `{"reason": "..."}` | The agent is asked to abort and the run ends `stopped`. The card is then stuck with `stopped by a person` until a person retries it. |

On GitHub a person can also merge the pull request there; the poll notices and the card moves to `Done`.

## How you see what is going on

- `GET /v1/snapshot`: every card's column, the changes, the inbox, and `waiting`, the reason each waiting card waits.
- `GET /v1/cards/{id}/history`: moves (with the note), runs starting and ending, decisions and inbox items of one card.
- `GET /v1/cards/{id}/runs`, `GET /v1/runs/{id}` and `GET /v1/runs/{id}/events`: the runs and what each did.
- `GET /v1/events`, a WebSocket: everything as it happens, including `board_error`, `card_waiting` and `run_event`.

## What can go wrong and how you see it

| What happens | How you see it |
|---|---|
| An issue from outside does not start. | The card stays in `Backlog` with an `approve_issue` item in the inbox. A person approves it. |
| A card sits in `Ready`. | `waiting` in the snapshot says why: blockers, the daily budget or the run limit. |
| The Coder cannot be started. | A `board_error` event with `no agent named Coder`. Add `[agents.Coder]` to `moebius.toml`. |
| A run fails. | The run's status is `failed`, and its events include a `moebius_error` that says why (`GET /v1/runs/{id}/events`). The next tick starts another round until a limit is reached. |
| A card is stuck. | An inbox item `stuck` with the reason. `POST /v1/cards/{id}/retry` gives it a fresh start. |
| A tool call of the agent was refused. | A `tool_check` run event with `decision` `refuse` (see [Rules](rules.md)). The run goes on. |
| A pull request was closed on GitHub without a merge. | The change is `closed`, an inbox item `change_closed` appears and the `change_closed` event is published. The card stays in `Ready to merge` until a person retries it with `POST /v1/cards/{id}/retry`: it goes back to `In progress`, and its next publish opens a new pull request. |
| The server stopped while a run was going. | At the next start the run is marked `failed` and the board starts the card's work again. |

A script that fails does not move the card; see [Column scripts](column-scripts.md).
