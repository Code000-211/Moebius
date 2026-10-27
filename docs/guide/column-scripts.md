# Change what a column does for one repository

## When you want this

You want cards of one repository to be treated differently from the default: hold some of them back, ask a person
before others start, give up sooner, or send work to another agent. Behaviour belongs to a column, and a column's
behaviour is a Luau script. A repository replaces the script of a column with its own.

The exact contract is in [`docs/reference/column-scripts.md`](../reference/column-scripts.md): the `card` fields, the
`moebius` functions, the limits and the sandbox. This page shows how to use it.

## The steps

1. Pick the column. The scripts are `backlog.luau`, `ready.luau`, `code.luau` (the column `In progress`),
   `review.luau`, `merge.luau` and `done.luau`. [Workflows](workflows.md) says what each does.
2. Start from the default script, which is in `crates/moebius-core/workflow/<script>`. An override replaces the whole
   script and cannot call the built-in one, so what you do not copy is gone: a Ready override that forgets the
   blockers starts blocked cards. Every example below is a copy of the default with a change.
3. Write the file and keep the function `on_card(card)`. A script reads the `card` and acts through the `moebius`
   table: `moebius.move`, `moebius.wait`, `moebius.ask`, `moebius.stuck`, `moebius.start_run` and the rest. Each run of
   a script starts from nothing, so keep no state in globals.
4. Check it with `moebius check-script` (see [How to try a script](#how-to-try-a-script)) until it does what you want.
5. Commit it as `.moebius/<script>` on the repository's default branch (see below). Only committed content on that
   branch counts.
6. The script is read again on every run, so the next tick (`board.tick_ms`, two seconds by default) uses it. There is
   nothing to restart.

### Where the file goes

A repository on GitHub: push the file to the default branch. The poll fetches it within `github.poll_seconds`.

A repository without a forge: Moebius reads the scripts from its own clone of it. Every 30 seconds it fetches the source
repository, the one it was cloned from, and fast-forwards the clone's default branch, so commit to the source
repository and the file arrives. Once Moebius has merged a change into the clone, the clone's default branch is
ahead of the source and cannot fast-forward any more: from then on, commit in the clone. The snapshot says where it is
(`repository.path`):

```sh
CLONE=$(curl -s -H "Authorization: Bearer $TOKEN" $API/v1/snapshot | jq -r '.boards[0].repository.path')
mkdir -p "$CLONE/.moebius"
cp ready.luau "$CLONE/.moebius/ready.luau"
git -C "$CLONE" add .moebius/ready.luau
git -C "$CLONE" commit -m "Start large cards alone"
```

A file that is in the repository when you add it to Moebius is in the clone from the start. Changes that Moebius merges
go into the same branch of the clone, so keep nothing else uncommitted there. A fetch that fails is reported as a
`forge_error` event.

## Examples

Each example is a whole script. The test in `crates/moebius-core/tests/guide.rs` runs each against a card in its column
and checks that it does what the text around it says.

### Hold large cards until nothing else runs

A card whose title contains `[large]` starts only when no other run is going, so it has the machine to itself.
Everything else is the default `ready.luau`: wait for blockers, wait while the daily token budget is spent, wait for a
free run slot. The idea is the same as
[`examples/scripts/ready-large-cards-alone.luau`](../../examples/scripts/ready-large-cards-alone.luau).

<!-- script: ready.luau; test: large-cards-alone -->
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

  local limit = moebius.run_limit
  local large = string.find(card.title, "[large]", 1, true) ~= nil
  if large then
    limit = 1
  end
  if moebius.runs_active() >= limit then
    if large then
      moebius.wait("a [large] card starts when no other run is going")
    else
      moebius.wait("the run limit is reached")
    end
    return
  end
  moebius.move("In progress")
end
```

`string.find(title, "[large]", 1, true)` is a plain search: the last argument turns off patterns, so the brackets mean
themselves. `moebius.runs_active()` counts runs of cards on every board, not chats.

Try it: create two issues, one titled `[large] Rewrite the parser`. While the other has a run going, the large card
stays in `Ready` and the snapshot's `waiting` holds its reason. It moves on once no run is going. A large card can wait
a long time on a busy server, because ordinary cards keep taking the free slots.

### Ask a person before starting cards from an author

Issues from some authors, say a bot, should be read by a person first, even when the author is trusted. This
`backlog.luau` asks about the authors in a list and otherwise does what the default does.

<!-- script: backlog.luau; test: ask-about-authors -->
```luau
local read_first = { ["dependabot[bot]"] = true }

function on_card(card)
  if read_first[card.author] then
    moebius.ask("approve_issue", "a person reads the issues of " .. card.author .. " first")
  elseif card.trusted then
    moebius.move("Ready")
  else
    moebius.ask("approve_issue", "waiting for a person to approve the issue")
  end
end
```

`moebius.ask(kind, reason)` adds one inbox item for the card, once, and makes the card wait with the reason. A person
approves with `POST /v1/cards/{id}/approve`, which resolves all the card's inbox items and moves the card to `Ready`,
but works only on a card in `Backlog`. So an ask that a person answers belongs in the Backlog script. In other columns
use `moebius.wait`, or `moebius.stuck` when `POST /v1/cards/{id}/retry` is the answer.

On a repository without a forge, every issue made through the API has the author `person`, so this script changes
nothing there. On GitHub the author is the login, and the list compares it as written (case matters here, unlike
`card.trusted`).

### Give up sooner

The default `code.luau` marks a card stuck after `board.round_limit` rounds (five by default) of the Coder. This
version stops after two, whatever the configuration says, and never allows more than the configuration does. The
Reviewer's runs are counted by `review.luau`, which is not changed here.

<!-- script: code.luau; test: two-rounds -->
```luau
local ROUNDS = 2

function on_card(card)
  local run = card.run
  local limit = math.min(ROUNDS, moebius.round_limit)
  local budget = moebius.agent_budget("Coder")
  if run ~= nil and (run.status == "starting" or run.status == "running") then
    moebius.wait("coding")
  elseif run ~= nil and run.purpose == "code" and run.status == "succeeded" and card.note == nil then
    moebius.move("In review")
  elseif card.stopped then
    moebius.stuck("stopped by a person")
  elseif card.rounds >= limit then
    moebius.stuck("the round limit of " .. limit .. " is reached")
  elseif budget ~= nil and card.tokens >= budget then
    moebius.stuck("the token budget of " .. budget .. " is spent")
  elseif moebius.day_budget ~= nil and moebius.tokens_today() >= moebius.day_budget then
    moebius.wait("the daily token budget is spent")
  else
    moebius.start_run("Coder", "code", card.note)
    if card.rounds == 0 then
      moebius.wait("coding")
    else
      moebius.wait("coding, round " .. (card.rounds + 1))
    end
  end
end
```

A round is a `code` run, counted since the card was last retried, so a person's retry gives the card two more.
Try it with a Coder that fails: after the second failed run the snapshot's inbox has an item of kind `stuck` with the
reason `the round limit of 2 is reached`. Answer it with `POST /v1/cards/{id}/retry`.

### Wait for a dependency, except for urgent cards

The default `ready.luau` waits while `card.blocked_by` is not empty. On GitHub that list comes from lines such as
`blocked by #12` in the issue's body, and an issue counts as done when its card is in `Done`. Without a forge nothing
fills it, so no card there is blocked. This version lets a card whose title contains `[urgent]` start anyway.

<!-- script: ready.luau; test: urgent-skips-blockers -->
```luau
function on_card(card)
  local urgent = string.find(card.title, "[urgent]", 1, true) ~= nil
  if #card.blocked_by > 0 and not urgent then
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
  if moebius.runs_active() >= moebius.run_limit then
    moebius.wait("the run limit is reached")
    return
  end
  moebius.move("In progress")
end
```

Try it on GitHub: open an issue whose body says `blocked by #12` while #12 is open. Its card waits in `Ready` with
`waiting for #12`. Add `[urgent]` to the title and the next poll moves it on.

## How to try a script

**Check it with the command.** `moebius check-script` runs the script once against a made-up card and prints what it did,
or the error. It needs no server, no configuration and no repository, and it stores and starts nothing, so it is the
first thing to do after every change:

```sh
moebius check-script .moebius/ready.luau --column Ready
moebius check-script .moebius/ready.luau --column Ready --title "[large] Rewrite the parser" --author leon
```

The first prints, for the default `ready.luau`, `moved the card to In progress`. Pass `--column` with the column the
script belongs to, and `--title` and `--author` to take the other paths of the script: a title with `[large]`, an
author the script does not trust. A script that does not compile, raises an error, calls `moebius.move` with a column
that does not exist or runs too long prints the error and exits with status 1. The command makes up the card: it has
no runs, no change, no blockers and no tokens, `moebius.has_agent` says yes to every name and `moebius.decide` takes the
fail-closed option, so it cannot tell you what a decider would say or what a card with a history does; for that use
one of the ways below. Every line it can print is listed in the
[reference](../reference/column-scripts.md#checking-a-script).

**Run it in a test.** `crates/moebius-core/tests/guide.rs` and `crates/moebius-core/tests/examples.rs` show the
pattern, which needs no model and no Pi-Bolt:

1. Make a git repository on branch `main` with the script committed as `.moebius/<script>`.
2. Make a `Store::in_memory`, a `Host` and an `App`, and add the repository with `App::add_repository`.
3. Make an `Engine` with a harness of your own that implements `Runs` (it can start runs that never end and record what
   it was asked to start), `Workflow::default()` and `Limits`.
4. Create a card with `App::create_issue`, put it in the script's column with `App::move_card`, call
   `Engine::tick(card.id)` and read the `Step`: `moved`, `waiting` and `started_run`. The store tells the rest: the
   inbox, the runs, the judgements.

**Run it on a scratch repository.** Add a repository that is only for trying, commit the script as above, create an
issue and read `GET /v1/snapshot`. The card's column and its reason in `waiting` show what the script did within a
tick. Follow `GET /v1/events` (see the [desktop API](desktop-api.md)) to see `board_error` events.

**Print.** `print` in a script writes to the server's standard output.

## What can go wrong and how you see it

| What happens | How you see it |
|---|---|
| The script does not compile, has no `on_card`, raises an error or runs longer than 2 seconds. | The event `board_error` with `{card, message}` at every tick; the message starts with the script's file name. The card stays where it is, and its `waiting` in the snapshot says `script error: <message>` until the script runs cleanly again. The server does not fall back to the default script. The event is not stored: you see it only on a connected `GET /v1/events`. |
| The override has no effect. | It is not committed, not on the default branch, or in the wrong place (in a repository without a forge, it was committed to the source repository less than 30 seconds ago, or the clone has merged changes and no longer follows the source). The built-in script runs and nothing complains. Check with `git -C <clone> show <default branch>:.moebius/<script>`. |
| `no column "<name>"` | `moebius.move` got a name that is not exactly one of the six columns, such as `In Progress`. |
| `no agent named <name>` | `moebius.start_run` named an agent that is not in `moebius.toml`. Test with `moebius.has_agent(name)` first. |
| Runs pile up. | `moebius.start_run` does not look at `run_limit` and starts a run every time it is called. A script that calls it on every tick must wait while the card's latest run is going, as `code.luau` does. |
| A card waits and nobody knows why. | Call `moebius.wait(reason)` on every path that leaves the card where it is. The reason appears in the snapshot's `waiting` and in `card_waiting` events. |
| `the script ran out of time` | The limit is 2 seconds of the script's own time, and fixed. Time spent inside `moebius.decide`, `start_run`, `publish_change`, `run_checks` and `offer_change` does not count. |
| A held card is decided again every tick. | `moebius.decide` records a judgement on every call (see [Plugins](plugins.md)). |

The sandbox has no file system, network, processes or `require`, and a script sees only its own card: there is no way
to look at another card from a script.
