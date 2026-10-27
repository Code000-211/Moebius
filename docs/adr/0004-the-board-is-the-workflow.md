# The board is the workflow

There is no workflow definition apart from the board: a workflow is its columns and the behaviour behind each of them (what
happens to a card that enters, how many cards a column holds at once, where a card goes next), written as scripts in a script
language built into Moebius. Moebius ships a default workflow, which a repository can replace in its own `.moebius/`
directory. Moebius reads that directory only from the repository's default branch, so that a pull request cannot run scripts
of its own on the server.

## Consequences

- What a person sees on the board is the state of the work: a card that goes back from review to coding moves back on the
  board too.
- A column can carry a constraint the work needs, such as a testing column that holds one card at a time so that two cards
  never share a GPU.
