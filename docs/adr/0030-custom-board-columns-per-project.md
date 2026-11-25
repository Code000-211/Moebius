# Custom board columns per project

[0004](0004-the-board-is-the-workflow.md) made the board the workflow, with six columns and a Luau script behind each,
and [0019](0019-a-project-is-a-board-over-any-repositories.md) made the scripts a project's. The six columns stayed
fixed. A project whose flow does not end in "In review" had two ways to make it say more: a label, or the card's waiting
text, and neither is a place on the board that a person can see, filter and measure. Issue #753 asks for columns of a
project's own, each with a script of its own: the worked case is a project that splits "In review" into "Ready for
testing", whose script waits for a person's measurement, and "Being measured".

The difficulty is that much of Moebius asks which column a card is in: the closed flag, blockers and dependents, the
cycle's burn-up, the analytics, the round limits, merging from "Ready to merge", reviews of followed pull requests,
loops, the open-card count of a project. A column the code has never heard of would count as nothing in all of them.

## Considered Options

- **Views, labels or the waiting text** over the six columns. Nothing to build, but the card's place on the board is
  the one thing a person reads first, and none of these is a place; the Kanban could not show the flow.
- **Columns that replace the six.** The most freedom, and every rule that reads a column breaks for every project at
  once, since none of them can know what a column means.
- **Custom columns among the six, each counting as one of them.** The six stay, so each rule keeps its meaning, and a
  custom column says which of them it is for the rules.
- **An editor for the columns in the web app.** Wanted, but it can wait: the file below is the contract, and an editor
  is a client of it.

## Decision

- **The file is `projects/<project name>/kanban.toml`** of the harness configuration repository, next to the project's
  other settings, read as they are. Each custom column is a `[[column]]` with `name`, and optionally `script`, `after`
  and `counts_as`; the scripts are `.luau` files beside it, named by `script`. There is no settings editor yet: the
  Columns tab of the project shows the custom columns read-only and lets a person edit their scripts. The file is
  edited in the repository, in a harness chat or by the Moebius agent.
- **Placement.** A custom column sits anywhere after Backlog and before Done: Backlog is first and Done is last on every
  board. `after` names a built-in or another custom column, and columns that come after the same one keep the order of
  the file.
- **Validation refuses a file as a whole, with every mistake it has.** Names are unique without regard to case, and
  do not clash with a built-in column or its alias `Ready`; `after` names a column that exists, with no cycle among the
  custom ones; `counts_as` is one of the six; a script name is a plain `.luau` file name of the project's folder.
- **A column counts as a class.** `counts_as` is one of the six built-in columns and defaults to the class of the column
  before it. A card in a custom column counts exactly as its class wherever built-in logic decides something: it is
  closed when its class is Done, it unblocks what it blocks when its class is Done, it burns up a cycle, it may be merged
  from "Ready to merge" when it counts as that. Scripts see it as `card.column_class`.
- **The class is for reads only.** What the engine writes goes through scripts: a card that enters a custom Done-like
  column is not closed unless that column's script closes the issue. A close the engine applies moves the card to the
  built-in Done, and the moves the code makes to a built-in column (a merge to Done, a reopened issue to Backlog) stay
  literal.
- **The six built-in columns cannot be renamed or removed**, and so none of the code that moves a card to one of them
  can ever fail.
- **A column with cards cannot be removed.** A save of `kanban.toml` that leaves cards in a column it no longer has is
  refused, with an error naming the cards. The same check holds for the Moebius agent's edits and for the merge of a
  proposal, which are the other ways the file changes.
- **A file that is wrong at load never fails every tick.** A file that is not valid leaves the columns last known in
  force. A column the file lost while cards are in it stays known as an orphan: its cards run the script of the class it
  had, with a notice in the card's waiting text and `columns_problem` on the project, so a bad merge by hand is seen
  rather than silent.
- **The columns of a project are resolved once per file and kept**, as the scripts are: the engine reads each file once
  per tick, and the parsed board is reused for as long as the text is the same.
- **SQL asks for the class from a table.** `column_classes` (project, name, class, present) is kept in step with every
  project's `kanban.toml` whenever the app reads it, and `store::class_sql` gives the expression for the class of a
  column, so a query reads `class = 'Done'` where it read `column_name = 'Done'`. A built-in column has no row and counts
  as itself.

## Consequences

- A project can show its own flow on the board, with a script for each step, and every metric and rule that already
  existed keeps its meaning for those columns without a change of its own.
- Every decision in the code that tested a column against a literal name has to ask for the class instead; a new one
  that forgets treats a custom column as no column. Tests put a card in a custom column and expect what its class gets.
- A custom column that counts as Done does not close issues by itself, so a project that wants that writes it in the
  script. This is deliberate: the engine's writes stay where a person reads them.
- The table is a copy of the file. It is rewritten whenever the file is read, so it is never older than the last read,
  and it keeps the class of an orphan for as long as cards are in it.
- The columns cannot be edited in the web app yet. Reading them, and editing their scripts, can.
- A card that changes project (a `project:<name>` label) keeps its column when the new board has one of that name, and
  otherwise goes to the built-in column of its class, with the move recorded. A followed pull request that is closed
  sends the card back to the column it was taken from when the board still has it, and to Not started when it does not
  (a column with no card in it can be removed, so `taken_from` is not a reason to refuse the save).
- A project whose file lost a column that cards are in is looked at again after a few seconds, not at every write, and
  the notice is logged once for each change of what is wrong.
- The Retro may propose changes to the scripts of a project's custom columns and its proposals are checked on that
  project's board.
