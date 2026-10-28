---
name: moebius-workflows
description: Write or change Moebius column scripts (Luau) and whole workflows in a repository's .moebius/ directory. Use it when the person wants cards of a repository treated differently - held back, approved first, started with another agent, given up sooner, checked or reviewed another way - or asks for a column script, a workflow, .moebius/checks or .moebius/rules.md.
---

# Column scripts and workflows

Goal: a repository's cards do what the person wants, with the smallest change to the default workflow, checked before
it is offered.

## What a workflow is

Every board has six columns, each run by one Luau script; a repository replaces a column's script with its own file:

| Column | File in the repository |
|---|---|
| `Backlog` | `.moebius/backlog.luau` |
| `Ready` | `.moebius/ready.luau` |
| `In progress` | `.moebius/code.luau` |
| `In review` | `.moebius/review.luau` |
| `Ready to merge` | `.moebius/merge.luau` |
| `Done` | `.moebius/done.luau` |

A whole workflow is a set of these files, plus `.moebius/checks` (the command that checks a change, without a forge) and
`.moebius/rules.md` (rules for this repository's runs). Moebius reads them from the default branch, so they work once
the person merges your change.

## Steps

1. Get the script the column runs now; an override replaces the whole script, so start from it:
   `"$MOEBIUS" api GET /v1/repositories/<id>/scripts/Ready --json` gives its `text` (the repository id is in
   `"$MOEBIUS" repos`).
2. Write `.moebius/<file>.luau` in your worktree with the one change. Keep `function on_card(card)`; keep what the
   default does that the person did not ask to change (blockers, budgets, the run limit, the round limit).
3. Check every path:
   `"$MOEBIUS" check-script .moebius/ready.luau --column Ready`, again with `--title "[large] ..."` or
   `--author someone` for the other branches. It prints what the script did (`moved the card to In progress`,
   `waited: ...`) or the error, with exit status 1.
4. Leave the file in the worktree; it is offered as a change when the chat ends. In a chat without the repository,
   save it with `"$MOEBIUS" api PUT /v1/repositories/<id>/scripts/Ready` instead (see the `moebius-configure` skill).

## What a script can use

`card` is read only: `id`, `number`, `title`, `body`, `author`, `column`, `trusted`, `confirmed`, `blocked_by`,
`rounds`, `reviews`, `tokens`, `stopped`, `note`, and the tables `run`, `review` and `change`, which can be nil. The
`moebius` table acts: `move(column, note)`, `wait(reason)`, `ask(kind, reason)`, `stuck(reason)`,
`start_run(agent, purpose, note)`, `has_agent(name)`, `agent_budget(name)`, `runs_active()`, `tokens_today()`,
`decide(kind, context, options, fail_closed)`, `publish_change()`, `run_checks()`, `offer_change()`,
`record_outcome(outcome)` and the constants `run_limit`, `round_limit`, `day_budget`. Check every other name in
`reference/column-scripts.md` of the `moebius` skill; what it does not list does not exist. A script keeps no state between ticks, sees only its own card and has no
files, network or `require`.

## Example: hold large cards until nothing else runs

The default `ready.luau` with one change: a card whose title says `[large]` starts only when no other run is going.

<!-- column: Ready -->
```luau
function on_card(card)
  if #card.blocked_by > 0 then
    local numbers = {}
    for _, number in ipairs(card.blocked_by) do
      table.insert(numbers, "#" .. number)
    end
    moebius.wait("waiting for " .. table.concat(numbers, ", "))
    return
  end
  if moebius.day_budget ~= nil and moebius.tokens_today() >= moebius.day_budget then
    moebius.wait("the daily token budget is spent")
    return
  end
  local large = string.find(card.title, "[large]", 1, true) ~= nil
  local limit = moebius.run_limit
  if large then
    limit = 1
  end
  if moebius.runs_active() >= limit then
    moebius.wait(large and "a [large] card starts when no other run is going" or "the run limit is reached")
    return
  end
  moebius.move("In progress")
end
```

Try both paths: `"$MOEBIUS" check-script .moebius/ready.luau --column Ready` and
`"$MOEBIUS" check-script .moebius/ready.luau --column Ready --title "[large] Rewrite the parser"`.

## What can go wrong

- `no column "In Progress"`: column names are exact (`In progress`).
- A card that waits with no reason: call `moebius.wait(reason)` on every path that leaves it.
- Runs that pile up: `start_run` ignores the run limit; wait while `card.run.status` is `starting` or `running`.
- `moebius.decide` takes the fail-closed option in `check-script` and wherever no plugin answers the kind.
- The override does nothing: it is not merged yet, or not on the default branch.
- A script error on the server shows as the card's `waiting` reason `script error: ...` in `"$MOEBIUS" card <id>`.

## Details

In the `moebius` skill: `guide/column-scripts.md` (more examples, how to test a script), `reference/column-scripts.md`
(every field and function, the limits), `guide/workflows.md` (what each default script does) and `guide/rules.md`.
