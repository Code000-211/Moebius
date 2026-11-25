# Add columns of your own to a project's board

## When you want this

The six columns say where a card is in the default flow, and a project's flow is sometimes longer: a change that has
been reviewed must still be tested, measured or signed off by someone, and for now that is a label or a line of waiting
text that nobody sees on the board. A custom column is a place on the board, with a script of its own, that a project
adds between Backlog and Done. This page builds one: a project whose In review is split into `Ready for testing`,
where the card waits for a person to write the measurement into the issue, and `Being measured`, where a label decides
what happens next.

The exact contract, with every key, every message and what happens to a board whose file is wrong, is in
[`docs/reference/column-scripts.md`](../reference/column-scripts.md#custom-columns). The decision is
[ADR 0030](../adr/0030-custom-board-columns-per-project.md).

## The steps

1. Decide which of the six built-in columns each new column is *like*. That is its **class**, `counts_as`. Everywhere
   Moebius decides something from a card's column (is it closed, does it still block others, does a round count, may it
   be merged) a card in a custom column counts exactly as a card in its class. A column with the class `In review` is in
   review for the board's status, the round limits and the open-card counts; one with the class `Done` is done for the
   cards it blocks and for the cycle's burn-up. A script sees the class as `card.column_class`.
2. Decide where it goes: `after` names the column it comes after, a built-in one (Backlog is always first and Done
   always last, so nothing comes after Done) or another custom column. Left out, `after` is the built-in column the
   class names, and `counts_as` is left out when it is the class of the column it comes after.
3. Write `kanban.toml`, one `[[column]]` for each, and a script for each column (a plain `.luau` file name; left out, the
   column runs the script of its class).
4. Check the scripts with `moebius check-script <script> --kanban kanban.toml --column "<column>"` and commit them
   to `projects/<project name>/` of the harness configuration repository, on its default branch. There is no editor for
   `kanban.toml` in the web app yet: it is a file of the repository, edited there, in a chat about the harness or by the
   Moebius agent. The project's Columns tab shows the custom columns read-only, and the script of each can be edited
   there.
5. Nothing to restart. The file is read again like the scripts, and the board of the project shows the new columns at
   its next read.

## The example

A project wants every change measured before it is offered to a person. In review stays what it is; two columns after it
hold the cards that wait for the measurement. Both come after In review and count as In review, and since columns that
come after the same column keep the order of the file, the board reads Backlog, Not started, In progress, In review,
Ready for testing, Being measured, Ready to merge, Done.

<!-- kanban -->
```toml
# projects/<project name>/kanban.toml
[[column]]
name = "Ready for testing"
script = "testing.luau"
after = "In review"

[[column]]
name = "Being measured"
script = "measured.luau"
after = "In review"
```

`counts_as` is left out, so each column counts as In review, the class of the column it comes after. A person moves a card
into `Ready for testing` when the review is done (a project may also let its own `review.luau` do it). Its script waits
until the issue holds a `## Measurement` section, which a person writes, and then moves the card on:

<!-- script: testing.luau; test: wait-for-measurement -->
```luau
-- Ready for testing: the change waits here until a person has written the measurement into the issue, under a
-- heading `## Measurement`. Then the card goes on to Being measured.
local MARKER = "## Measurement"

function on_card(card)
  if string.find(card.body, MARKER, 1, true) == nil then
    moebius.wait("waiting for a " .. MARKER .. " section in the issue")
    return
  end
  moebius.move("Being measured", "the measurement is written in the issue")
end
```

A script cannot read the comments of an issue, only its body and labels, which is why the measurement goes into the
body. `moebius.move` takes the name of any column of the card's board, in any case, so the script can name
`Being measured`; on a board without that column it raises `no column "Being measured"`.

`Being measured` decides from labels a person gives the issue. A good measurement offers the change to a person, as the
default review does; a bad one sends the card back to coding with a note:

<!-- script: measured.luau; test: measured-labels -->
```luau
-- Being measured: a person gives the issue the label `measured` when the numbers are good, or `regressed` when they
-- are not. A good measurement offers the change to a person as the default review does; a bad one sends the card back
-- to coding with a note.
local function has_label(card, label)
  for _, own in ipairs(card.labels) do
    if own == label then
      return true
    end
  end
  return false
end

function on_card(card)
  if has_label(card, "regressed") then
    moebius.move("In progress", "The measurement regressed. Find out why, and fix it.")
  elseif has_label(card, "measured") then
    moebius.publish_change()
    moebius.offer_change()
  else
    moebius.wait("being measured: waiting for the label measured or regressed")
  end
end
```

The files are also in [`examples/custom-columns/`](../../examples/custom-columns/), where a test runs them.

## What counts as what

A card in `Ready for testing` or `Being measured` counts as In review, so:

- the project's open-card count, the analytics and the board's status treat it as a card in review, and a script that
  cares reads `card.column_class`, not the column's name;
- it is not done, and what it blocks keeps waiting; a card that has `Done` as its class (a column `Shipped` after
  `Ready to merge`, say) is done for those cards, for the burn-up of its cycle and for the analytics.

What the class does not do is write. A card that is moved into a column that counts as `Done` is not closed by that, and
its issue stays open, unless the column's script closes it with `moebius.close_issue`; the moves that Moebius itself
makes to a built-in column (the merge to `Done`, a reopened issue to `Backlog`) go to that column. A column that is
like `Done` for the people who read the board and not for the issue is deliberate: what changes the issue is written in
the script a person can read.

## Changing the columns later

- The six built-in columns cannot be renamed or removed.
- A column with cards in it cannot be removed or renamed: the save of `kanban.toml` is refused with `409`, naming the
  cards (`kanban.toml would remove the column "Being measured" while cards are in it (#12, #15); move them to another
  column first`). The same holds for the Moebius agent's edits and for the merge of a proposal that changes the file.
  Move the cards first, then remove the column.
- A file that is not valid is refused with every mistake it has (`kanban.toml: column "Being measured": after "Review"
  names no column`). One that reaches the repository by a commit by hand is not refused: the project keeps the
  columns it had, its `columns_problem` (in `GET /v1/projects`, shown on the project's page) says what is wrong, and no
  card fails. Cards in a column the file lost run the script of the class that column had, with a notice as their
  waiting text, until they are moved.

## How to try it

`moebius check-script` runs a script once against a made-up card on the board that `--kanban` names:

```sh
moebius check-script testing.luau --kanban kanban.toml --column "Ready for testing"
moebius check-script measured.luau --kanban kanban.toml --column "Being measured"
```

The first prints `waited: waiting for a ## Measurement section in the issue`. Without `--kanban` the command knows the
six columns only, and `moebius.move("Being measured")` fails with `no column "Being measured"`. The made-up card has
`column_class` set to the class of the column you gave. For a card with a history, see [How to try a
script](column-scripts.md#how-to-try-a-script); `crates/moebius-core/tests/guide.rs` runs the scripts of this page on
the board of the file above.

## What can go wrong

| What happens | How you see it |
|---|---|
| The file is not valid. | The save is refused with the message that starts with `kanban.toml:` and names each mistake. A file committed by hand keeps the columns last known and says so in `columns_problem`. |
| A card sits in a column of the old file. | Its waiting text says that the project's `kanban.toml` no longer has the column, and it runs the script of its class. Move it, or put the column back. |
| A script file is missing. | The column runs the script of its class, and the card's waiting text says which file is missing. |
| A script moves a card to a name the board does not have. | `no column "<name>"` at the script's run, as for any script. Names are found in any case. |
| A card in a Done-like column is still open. | The class is read, not written: close the issue in the column's script. |

See [Column scripts](column-scripts.md) for writing the scripts, and [Kanban](kanban.md) for what the six built-in ones do.
