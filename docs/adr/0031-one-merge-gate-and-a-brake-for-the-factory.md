# One merge gate and a brake for the factory

[0027](0027-working-with-people-on-other-servers-through-the-forge.md) to [0029](0029-the-reviewer-reads-a-pull-request-when-its-author-says-done.md)
let a server follow a pull request someone else opened, have the Reviewer read it when its author says `/done`, and
merge what the Reviewer approves. The owner of a repository that takes many outside pull requests (a GPU inference
engine whose rule is that a lossless change merges only when its merge bar is measured) wants them processed and merged
with as little of their own work as possible, which asks more of the server than a green check and an approval:

- **Merges came by several paths with several rules.** The Reviewer's approval, a person's Merge, a column script's
  `moebius.merge_change`, the merge of a card from Ready to merge and the desktop API's route all ended in
  `App::merge_one`, which asked only whether the checks of an outsider's pull request had passed. Anything the owner
  wanted held (a path that needs a person, a measurement, a daily limit) would have had to be added to each path, and
  would have been missing from the next.
- **The server's merges did not answer to the rules.** An agent's tool call goes through the decision `tool_call`, and
  the repository's `rules.md` apply to it. The server's own merge went through no decision at all, so a rule such as "no
  merge without a measurement" held for the agents and not for the server.
- **The owner had no brake.** Once it merges by itself, the owner needs a way to stop everything at once, and the
  server needs to stop itself when something is plainly wrong.
- **The Reviewer was told the rules by the pull request.** What a repository requires of a change is in its files
  (here `notes/backlog/CARDS.md` and `AGENTS.md`), and a pull request can change those files.

## Considered Options

- **A check in each merge path.** Small for the first rule, and wrong for the second: a path added later has none.
- **The gate in `merge_one`, the one function every path ends in.** Every path, existing and future, is held to it.
- **The rules for the server's merge as a new engine.** Two readings of `rules.md` that can disagree.
- **The rules as a decision point of their own kind, decided by the guard like a tool call.** One reading, one set of
  judgements, a decider the owner can replace with a plugin, as for every other decision.
- **The review brief read from the pull request's checkout.** Simple, and the stranger writes the rules.
- **The brief read from the default branch's commit the round is recorded for.** The pull request cannot change what the
  Reviewer is told to apply.

## Decision

- **`merge_one` asks the merge gate before anything else that changes the forge** (`app/factory/gate.rs`). The gate
  refuses with a reason, and what it cannot find out is a refusal. In this order: the repository is not frozen; for a
  script or agent, the agents are not paused and the card is not held; the checks of the head have passed; the daily
  merge limit is not reached; a measurement is accepted for the head, or the issue carries `no-measurement`, when the
  repository requires one; for a script or agent, no path of the change is locked; the rules allow it. A person's own
  Merge is their decision, so hold and pause do not stop it and locked paths are theirs to merge; a freeze, the
  checks of an outsider's pull request, the limit and the measurement hold for everyone.
- **Locked paths** are globs: the built-in ones (forge workflows, build files, package manifests, `AGENTS.md` and
  `CLAUDE.md`) and the owner's, which are added and never replace. The paths are those the merge into the default
  branch changes, read from git objects, so a rename or a deletion of a locked file counts.
- **A measurement** is accepted for one head of one change. Today a person accepts it
  (`POST /v1/changes/{id}/measurement`); the measurers a repository lists are kept for the comments that will do it
  (the next release). A push makes a new head that needs its own.
- **The pin of head and base of an approval stays** in `merge_one` (`expected_head`) and in `merge_reviewed`
  (`effect_unchanged`), as before.
- **A refusal is shown, not swallowed.** The card goes to Ready to merge, or stays there, with one open Inbox item
  `merge_refused` that names the reason, and the card's `ready_to_merge` item that offers a person the merge; a script that asks again every tick does not make a new item each time. The
  merge fails with `AppError::Refused` (409 over the API).
- **The rules are the decision `merge_check`**, asked only when rules apply, answered by the guard exactly as `tool_call`
  is (the same rules text, the same teacher or decision model, the same fail-closed behaviour: rules and no decider,
  or a decider that fails, refuse). The question gives the tool name `merge` and the facts of the merge as its input,
  so the rules a repository wrote for tool calls read on it too.
- **Freeze** (`app/factory/freeze.rs`). A frozen repository starts no review round, merges nothing and starts no agent
  run of a card. A person freezes it (`POST /v1/repositories/{id}/freeze`), the label `factory:freeze` on an open issue
  freezes it for as long as the label is there, and a breaker freezes it: two merged changes reverted within seven days
  (a revert commit of a change's merge commit, or a merged request that says `Reverts #<n>`), more than 80% of the day's
  token budget, or a failed nightly check (`App::freeze_for_failed_nightly`, the hook for it). Only a person lifts what a
  person or a breaker froze (`POST /v1/repositories/{id}/unfreeze`), and a breaker trips once for what tripped it, so
  that unfreezing is not undone at the next poll while the cause stands. The owner gets an Inbox item `factory_frozen`,
  and the card window says "Frozen: <reason>".
- **The review brief** is a list of paths of the default branch (`review_brief`, default `REVIEW.md`). The server reads
  each with `git show <base>:<path>` through the bounded `run_git` of the review, at the base commit the round is
  recorded for, never from the pull request, and gives them to the Reviewer in a section marked as the repository's
  rules (trusted), above the pull request, which is marked as data. All of them together are at most 40 KB; a bigger
  brief refuses the review with the Inbox item `review_not_possible`, never a cut brief. A pull request that edits a
  brief file is read with the base's version.
- **A third verdict, `approve_code`:** the code is right and the measurement the brief asks for is missing. Only a run
  that was started restricted can give it, as `approve` and `changes` already are. The server writes an approving review
  and merges nothing; the round ends `awaiting_measurement` and counts. The issue gets the repository's
  `testing_label` (default `ready-for-testing`), the card moves to the project's column "Ready for testing" when the
  project has one (custom columns, [0030](0030-custom-board-columns-per-project.md)) and otherwise stays in In review,
  and the issue is told a measurement is needed and how to post one. Starting the next round from a measurer's comment
  is the next release's.
- **The settings** live with the repository's others, in the "Working with others" box, and a person changes them: an
  agent does not (an agent that could turn off the measurement would hold nothing back).

## Consequences

- A new way to merge needs nothing but `merge_one`. A new condition is one more line of the gate, with a test that
  every path refuses it.
- The gate asks git for the paths of every non-person merge. A repository whose clone cannot be read refuses
  those merges: failing closed has a price, a visible one, since the card says why.
- A repository with rules and no decider that answers `merge_check` merges nothing by script. That is as true of its
  tool calls; the guard answers both, and turning the guard off (knowingly) leaves both open with a note.
- The daily limit counts the changes of the repository merged in the last 24 hours, whoever merged them, a person on the
  forge included, since the owner's budget for risk is spent either way.
- The label freeze is read from the issues the poll knows, so an issue the poll does not list (a closed one) does not
  freeze. Reading the repository's label list instead is possible later.
- A breaker that freezes every repository when the day's tokens pass 80% also stops merges, which cost no tokens. That
  is intended: the owner looks first.
