# Helping on someone else's repository with your own server

## When you want this

You run your own Moebius server and want its agents to help on a repository whose owner runs another server (or runs
none). Both of you poll the same issues, and nothing connects the two servers, so they keep out of each other's way
through the forge: taking an issue assigns you on it, an issue someone else took is left alone, and a pull request
you open for an issue shows up on the owner's board
([ADR 0027](../adr/0027-working-with-people-on-other-servers-through-the-forge.md)). It works on GitHub, GitLab and
Forgejo.

## The steps

For the helper, on their own server:

1. **Say who you are on the forge.** Open Settings, You, Profile and fill **Your login on GitHub** (or GitLab, or
   Forgejo) with your login, with or without the `@`. Without it, taking an issue assigns no one, other servers may
   take it too, and Get started lists "Tell Moebius your login on GitHub" as pending.
2. **Add the owner's repository** to your server, as any repository on a forge (see [Kanban](kanban.md) and
   [Running Moebius](running-moebius.md)). Your token on the forge must be allowed to assign issues, and on GitLab and
   Forgejo your login must be a member or collaborator of the project.
3. **Take only what is ready.** Open Project settings, Repositories, the repository, **Working with others** and set
   **Agents take** to "Only issues labelled Ready for agent and not assigned". Your agents then start only an issue
   that carries the label, or that someone of your server took, or that already has a run. Any other card waits with
   `waiting for the label "Ready for agent" (this repository's agents take only labelled issues nobody took)`. The
   same settings are the keys `agents_take`, `mark_ready` and `ready_label` of
   [`PUT /v1/repositories/{id}`](../reference/desktop-api.md#put-v1repositoriesid).

For the owner, on their server, once:

4. **Keep the label.** In the same place turn on **Mark ready issues on GitHub** (`mark_ready`). At each poll the
   server puts the label (named in the same settings, "Ready for agent" by default) on every open issue that is
   approved and has no open blocker and no unfinished sub-issue, and takes it off as soon as one appears. An issue
   whose blockers the forge has not yet reported is left as it is. Turn it on on one server only, the one of the
   repository's owner; helpers leave it off.

## What happens

- **You take an issue.** When you claim a card, or the board starts a run for you, your login becomes an assignee of the
  issue on the forge, and the other assignees stay. The board starts a run for the first person the card is given to
  who has a login there, or for the one person of the server who has one; if that is not clear, nobody is assigned and
  the card says so. If the forge refuses (the token cannot assign, or your login is no collaborator), the claim or run
  still goes ahead and the card shows the reason (`assign_problem`).
- **Someone else took an issue.** When an issue is assigned to a login that is none of your server's people's, the
  card is **taken by** that login (`card.taken_by` in a [column script](../reference/column-scripts.md)). It stays in
  its column, drawn greyed with "Taken by @alice". Your column scripts' `moebius.start_run` and `moebius.start_runs`
  start nothing and the card waits with `taken by @alice`, and claiming it is refused with `409`
  (`#12 is taken by @alice on GitHub`). It goes on when the assignee is removed on the forge.
- **You open a pull request for an issue you took.** Write a closing keyword for the issue in its title or description
  (`Fixes #12`, or `Fixes owner/repo#12`; a plain mention is not enough). On the owner's server, at the next poll, the
  card moves to **In review**, the head of the pull request is kept in the local branch `moebius/pr-<number>`, and an
  Inbox item "outside change" names you. The owner's server follows a request only when it is against the default branch and you may write to the repository or are assigned to the issue; a stranger's fork that says "Fixes #12" is ignored. The owner's column scripts do not run for the card while the request is open.
- **The owner reviews it.** **Review with agent** (`POST /v1/changes/{id}/review`) has the Reviewer read the pull
  request. When its run ends, what it found is written on the pull request as a review, and the answer on the issue
  names the verdict, as for the author's `/done`; nothing is merged and the card does not move, so the owner merges the
  request themselves. A review whose run cannot start is noted in the Inbox (`review_not_possible`), with what the run
  said. **Merge** is refused, `the checks of pull request #n have not passed`, while its checks are pending, unread for its current head or
  failing; a request with no checks may be merged.
- **It ends.** When the pull request is merged on the forge, or by the owner, the card goes to Done. When it is closed
  without a merge, the change is closed, the Inbox item is resolved, and the card goes back to the column it was in
  (Not started if that was In review or Done). It is still taken until its assignee is removed.

## Helpers without a role take issues with comments

On GitHub only people with the Triage role or higher may assign an issue, and the built-in roles cannot be edited without
Enterprise. A helper who only comments cannot take an issue with their own server's token. Anyone who commented on an
issue may *be* assigned, though, so the owner's server can assign them
([ADR 0028](../adr/0028-helpers-take-issues-with-commands-in-comments.md)).

For the owner, on their server, once:

1. Open Project settings, Repositories, the repository, **Working with others**, and in **Commands in issue comments**
   turn on **Take issues with /take, /release and /done** (`commands`). Only the server of the repository's owner should do
   this: two servers reading the same comments would both answer.
2. Leave **Answer on GitHub** (`answer_on_forge`) on, so that the server says what it did on the issue, and set **Issues
   one login may hold** (`command_limit`, default 3). Save.
3. Optionally have GitHub send issue comment events to the server ([ADR 0012](../adr/0012-forge-webhooks-wake-the-poll.md)),
   so that a command does not wait for the next poll.

For a helper, in a comment on the issue, as the first word of a line (not in a quote or a code block):

- `/take` assigns you. The server answers "Assigned to @alice." and the owner's card is greyed with "Taken by @alice with
  /take", so the owner's agents leave the issue alone.
- `/release` takes you off the issue. "Released."
- `/done`, with what you want to tell the owner on the same line or below it, asks the owner to close the issue: a helper
  can never close anything. If your pull request closes the issue and the owner's board follows it, your `/done` has the
  pull request reviewed instead (see [What helpers see after `/done`](#what-helpers-see-after-done)); anybody else's only
  gets thanks.

Who may use them: anyone who can comment on the issue, on a public repository everyone. What keeps that safe:

- a login holds at most **Issues one login may hold** open issues it took by command ("You already have 3 issues; release one first.");
- the issue must be open and nobody may be assigned to it ("Already taken by @bob."), and while **Mark ready issues** is on
  only an issue that carries the ready label can be taken ("Only issues labelled Ready for agent can be taken.");
- only the assignee may `/release` or `/done`;
- the owner can always release from the card (**Release from @alice**), and answers a `/done` there: the card window shows
  "Moebius needs you" with the helper's words and **Close the issue**, **Give it back to @alice** (keeps them as assignee) and
  **Release it**. The server writes each answer on the issue as well.

Commands are read from new comments only: each comment once, and none from before the setting was turned on. GitLab and
Forgejo work the same through their own APIs, but they assign only project members and collaborators, so for anyone else
the answer is "Could not assign @alice: ...".

## What helpers see after `/done`

When your pull request closes the issue and the owner's board follows it, write `/done` in a comment on the issue
(commands must be on; the pull request's own comments are not read). It is yours to write: nobody else's `/done` has your
pull request reviewed ([ADR 0029](../adr/0029-the-reviewer-reads-a-pull-request-when-its-author-says-done.md)).
Before it answers a `/done`, the server reads your pull request as it is now: a push right before it is the version
reviewed, and a pull request you just opened is followed first.

- **The server answers on the issue** "Thanks, the pull request will be reviewed once its checks have passed." The review
  starts when the checks of your pull request's current head are green. While they are pending it waits. If they fail,
  the pull request gets a comment that the checks fail and nothing is reviewed: fix them, push, write `/done` again.
- **The Reviewer reads it** and the server writes **a review on your pull request**. Changes requested are a review with a
  summary and comments on the exact lines; a comment about a line the diff does not show is in the summary instead.
  Push your changes, then write `/done` again for the next round. On the owner's board the card moves to **In progress**
  with the review's summary as its note, as it is your turn; when the next round starts (your `/done`, or with [**Review
  when the checks are green**](#review-when-the-checks-are-green) a new head whose checks are green) it moves back to **In review**.
- **Approved**, and the checks of the same head are green: the pull request is approved and merged, and the issue gets
  "Merged, thank you @alice!". If the owner turned **Merge external pull requests the reviewer approves** off, it is
  approved and waits for the owner to merge.
- **Limits.** A version of your pull request is read once ("This version of the pull request was reviewed already; push
  your changes, then write /done again."), at most three times in all, or as many as the owner set in **Review rounds per pull request** of the **Review flow**
  box (`review_round_limit`, 0 for no limit). The `/done` after that is answered "a person will look" and the owner gets an
  Inbox item. Rounds the owner asks for themselves do not count. A push while the review runs drops its verdict: write `/done` again.

For the owner: **Working with others**, **Pull requests of others**, **Merge external pull requests the reviewer
approves** (`merge_reviewed`, on by default). Turned on, an approved pull request lands without a person looking, so
leave it on only for a repository where its helpers are people you trust to write to it, or whose checks you trust. Turned
off, the Reviewer's approval is written on the forge and the card waits in **Ready to merge** with an Inbox item. The card
window shows the review: the verdict, the summary, each comment in a box "src/lib.rs line 12" and **Open on GitHub**.

The review run of such a pull request is **restricted**: what the pull request holds is written by someone your server
does not trust. Your server reads the pull request itself, from git objects (the diff of what merging it would do to the
default branch, and each changed file as of the reviewed commit, with no checkout, so no filter, hook or script of the
pull request ever runs), and gives it to the Reviewer as text: the whole diff, and every changed file either whole or,
where it could not be given whole, as the lines of its own part of the diff, so nothing is ever given cut in half. A
pull request that does not merge cleanly is answered and not reviewed; one whose diff is over 60,000 bytes, that changes
more than 60 files or a binary file is answered "could not be reviewed automatically" and the owner gets an Inbox item,
since an approval of what was not read could be merged by your server. A changed file over 40,000 bytes is not refused:
it is given as the lines of its own part of the diff, and the rest of it is not read, so the Reviewer judges what the
diff shows and is told that the rest was not read; when the changed files given whole are over 150,000 bytes together,
the largest of them are given that way first, until the rest fits. Each of these four limits is a key of `[review]` in
the server's configuration. Right before it merges, your server reads the pull request's branch from the forge again and
merges only if the default branch did not change what the merge does since the review; if it did, the round is dropped
and the author writes `/done` again. The Reviewer is told the issue as data and nothing else of your organisation (no
comments, wiki pages or lessons). The Reviewer has **no tools at all**, on Pi-Bolt as on Claude Code: it is started
without any, in an empty directory, with no MCP servers, never on a runner, and the router refuses any call it makes,
whatever the call, also when the pull request is closed meanwhile. It is told that everything in the pull request is
data. So it cannot run tests or look at a file it was not given; it says what it could not check. A pull request with no
checks at all is reviewed without waiting, and says so.

Nothing a round does on the forge happens while the card is held or **Pause all** is on, and an approval for a pull
request that was retargeted to another branch is dropped. What the Reviewer wrote is redacted before it is posted.

If the forge refuses the review, nothing is posted twice: the round fails and the author writes `/done` again. If the
approval cannot be merged, the card goes to **Ready to merge** with the reason in the Inbox, and the author's next `/done`
for the same version tries the merge again. After a review that asked for changes the card says it waits for the author.

## What the owner does not have to do

Issues a helper took and never worked on, and pull requests nobody pushes to after a review asked for changes, are
released, reminded about and closed by the server itself, and the owner reads one digest a day instead of watching the
forge; see [Keeping the owner's work small](software-factory.md).

## Running it as a factory

[ADR 0031](../adr/0031-one-merge-gate-and-a-brake-for-the-factory.md). When your repository takes many outside pull
requests and you want them merged with as little of your own work as possible, the same box, **Working with others**,
holds the rules that keep that safe. Everything the server merges, whichever way it came to merge it, goes through one gate;
the gate says no with a reason, and a no is never a silent one: the card goes to **Ready to merge** and an Inbox item
says why.

- **Locked paths.** Changes to the files that run or build things (`.github/**`, the build files, the package
  manifests, `AGENTS.md`, `CLAUDE.md`) never merge without you: the server leaves them in **Ready to merge** and you merge
  them yourself. Add your own globs under **Locked paths**; the built-in ones stay.
- **Require a measurement.** For a repository whose merge bar is a measurement (before and after on one machine, output
  parity), turn this on. A change then merges only when a measurement is accepted for its current head, or its issue
  carries the label `no-measurement`, or it changes only paths you list under **Measurement-free paths** (globs, read
  as the locked paths are: `notes/backlog/closed/**` for items closed with their numbers and no code). One path outside
  them and the measurement is needed again. A registered measurer's comment gets one accepted without you (see [Measurements
  without a click](#measurements-without-a-click)); you can still accept one yourself with
  `POST /v1/changes/{id}/measurement`, or from the `moebius` command line:

  ```sh
  moebius api POST /v1/changes/12/measurement --body '{"reason": "kbench rows checked"}'
  ```

- **Review brief.** The Reviewer is told your rules for a change from files of your default branch, not from the pull
  request: list them under **Review brief** (default `REVIEW.md`; for a GPU engine, `notes/backlog/CARDS.md` and
  `AGENTS.md`). The server reads them from the default branch at the commit the review is of, so a pull request that edits
  one is judged by yours. They are at most 40 KB together; more, and the review does not start and you get an Inbox item.
  When the rules ask for a measurement the pull request does not carry, and the code is right, the Reviewer answers
  `approve_code`: the server approves the code on the forge, merges nothing, labels the issue (the **Ready for testing label**,
  default `ready-for-testing`), moves the card to the project's **Ready for testing** column when it has one, and tells
  the author a measurement is needed, how a measurer posts one and the form it must have. The round counts toward the
  three.
- **Measurers.** The logins whose `## Measurement` comments the server reads. One per line, forge logins; the author of a
  pull request is never its measurer.
- **Review flow.** The box below it, **Review flow**, has the settings that save you the rest of the clicks; see
  [Measurements without a click](#measurements-without-a-click), [Review when the checks are
  green](#review-when-the-checks-are-green) and [Pull requests that name no issue](#pull-requests-that-name-no-issue).
- **Freeze.** The brake. A frozen repository starts no review round, merges nothing and starts no agent run of its cards.
  **Freeze** in the box does it, or `POST /v1/repositories/{id}/freeze`, or the label `factory:freeze` on any open issue.
  It also freezes by itself when two merged changes are reverted within seven days, when more than 80% of the day's token
  budget is used, or when a nightly check fails (`App::freeze_for_failed_nightly`). You get an Inbox item, and the card
  window says "Frozen: <reason>". Only you lift it: **Unfreeze**, or taking the label off.

Put rules in prose in `rules.md` too: the server's merge is checked against them the way an agent's tool call is (the
decision `merge_check`), so a rule such as "No merge without a measurement in the pull request's description" refuses the
server as it refuses an agent. A guard with no teacher refuses everything while prose rules exist, as it does for tool
calls.

## Measurements without a click

[ADR 0033](../adr/0033-measurements-and-reviews-flow-without-the-owners-clicks.md). When the Reviewer answers
`approve_code`, the issue is told how a measurement is posted. A **measurer** (a login you listed under **Measurers**, who
did not open the pull request; list `*` to let anyone but the author measure, so that a new helper's measurement counts
without you adding their login) writes a comment on the issue that starts with the heading `## Measurement`:

```text
## Measurement
before 0ba658192b0e14d5cce2b063ff8dfbca365c8619
after f868a450eeb84cc5251b15ab9155a5c542156106
GPUs: 2x MI300X
Power cap: 750 W
ROCm: 6.2.1
Container: rocm/pytorch:6.2
Runs before: 12.1, 12.3, 12.0
Runs after: 13.4, 13.5, 13.3

The output is the same, byte for byte, on all six runs.
```

- `before` is the merge base of the pull request with the default branch, and `after` its head, as the server's reply on
  `approve_code` fills them in for the version that was approved (seven characters of each are enough). A push makes both
  wrong.
- The fields are **GPUs**, **Power cap**, **ROCm** (or **Driver**) and **Container**, and **Runs before** and **Runs after**
  each list at least three numbers, one for each run. Write whatever else the review brief asks for under them; the
  Reviewer reads it.
- The server checks all of that and answers on the issue with what is wrong, and starts nothing; the measurer posts
  another comment. A comment that holds starts the next round of the Reviewer by itself, with the measurement in what it
  reads (as data, with the measurer's login), once the checks are green. It counts toward the round limit. Nobody writes
  `/done`.
- The Reviewer judges the measurement against your review brief. `approve` accepts it for that commit and merges through
  the gate (or leaves the pull request in **Ready to merge** if you merge by hand); `approve_code` says it is still not
  enough and what is missing; `changes` is as ever.
A machine of your own can make the measurement instead of a person: a [test lane](test-lane.md) builds, tests and
measures the pull request at its pinned head and hands the numbers to the Reviewer by the same path, with no comment
posted.
- A new commit on the pull request makes a measurement count for nothing; the issue says so.
- The measurement is kept in the card window (who measured which commit, and what came of it) and in the daily digest. It is
  **not** committed to your repository: the server writes no file to your default branch. A column script or your own
  workflow can copy the comment where you want it.

## Review when the checks are green

By default the author writes `/done` to have a pull request reviewed. Turn on **Review when the checks are green** in the
**Review flow** box (or `"$MOEBIUS" api PUT /v1/repositories/1/flow --body '{"review_on_green": true}'`) and the round
starts by itself once the checks of the head have been green for **Green for (minutes)** (default 5) with no push; a push
is a new head with its own wait. One round for each head, the round limit holds, and a freeze, a hold or Pause all
stops it. `/done` still works, and whichever comes first starts the round.

## Pull requests that name no issue

A pull request without `Fixes #N` is looked at: the server binds it, asks, or in the end closes it.

- **Bound.** When its title or branch holds the ID of a backlog item ([Keeping the owner's work
  small](software-factory.md)) that is exactly one issue the sync made (`perf/k-47-cache` for `K-47`), or its title or
  description names `#N` of an issue its author is assigned to, it is followed as if it had written `Fixes #N`. The author
  must still be someone who may write to the repository or be assigned to that issue.
- **Asked.** Otherwise it gets one comment that asks for `Fixes #N` and, on GitHub, the label `unbound`.
- **Closed.** After **Close a pull request that names no issue after (days)** (default 14, 0 never) it is closed with a
  polite comment, and not again if its author reopens it.
- Only in a repository where commands and the server's answers are on; never in a frozen one or for your own pull requests.

## Status labels on the forge

People who do not use Moebius cannot see your board. Turn on **Status labels on the forge** in the **Review flow** box and
every open issue that has a card carries one label that says where it stands: `status:in-progress`, `status:in-review`,
`status:ready-to-merge`, the name of a custom column (`status:ready-for-testing`), or one of four finer states that
win over the column while they hold, in this order: `status:frozen`, `status:awaiting-measurement`,
`status:changes-requested` and `status:taken`. The prefix is yours to change; labels without it are never touched. The label
is swapped within a poll of the change, a label removed by hand is put back, and a closed issue keeps its last one. It is
off by default. The followed pull request itself is not labelled.

## Templates for pull requests and comments

So that a card and the Reviewer read what people did instead of searching for words, a pull request and every comment
Moebius reads has a form with fixed headings ([ADR 0034](../adr/0034-templates-and-agents-that-work-on-the-forge.md);
the exact contracts are in [the reference](../reference/templates.md)). Helpers and Moebius's own agents write the same
ones.

**A pull request** starts with `Fixes #N` and has four sections, which the forge shows in the box of a new pull request
once the repository has the template file:

```
Fixes #12

### Item
K-4: cache the plan

### What the change does
Plans are cached for the life of a request, so a second query does not plan again.

### Tests I ran
- [x] `cargo test`: 412 passed, 0 failed
- [x] `cargo bench plan`: 4.2% faster

### Measurement
measured: 4.2% faster on 2x MI300X, see the measurement comment
```

`Measurement` starts with `measured`, `needs testing` or `not needed`, then the reason. When a section is missing or
empty, the server of the repository's owner writes one reply on the pull request that names it (`## Missing`, with the
headings it lacks), where commands and answers are on; it does not guess. Edit the body, and the next poll reads it again.
A column script can read the result as `card.change.fields` and `card.change.missing`.

**Comments** start with a fixed first line that names them:

| First line | What it is | Fields |
|---|---|---|
| `/take`, `/release` | take or give back an issue | none |
| `/done` or `## Done` | the work is done: asks the owner to look, or has the Reviewer read your pull request | `### What was done`, `### Tests run`, `### Open points`; `/done` with plain words still works |
| `## Review answer` | what you changed for each finding of the last review | one list line for each finding |
| `## Measurement` | a measurer's numbers | [as above](#measurement) |

A comment that lacks a field is answered with `## Missing` and the fields it lacks, and does nothing until it is posted
again complete.

**Where the templates are.** In the repository, the pull request template is `.github/PULL_REQUEST_TEMPLATE.md` and each
comment template is `.github/moebius/<name>.md` (`done.md`, `review-answer.md`, `measurement.md`, `take.md`,
`release.md`): that is where a helper looks. Moebius ships defaults. A project changes them in the harness configuration
repository, `projects/<project>/templates/pull-request.md` and `projects/<project>/templates/<name>.md`; a pull request
template may rename the headings, add sections of its own and mark one `(optional)`. Then keep the repository's copies
current:

```sh
"$MOEBIUS" templates 3            # which templates are in effect, from where
"$MOEBIUS" templates 3 --sync     # open the pull request that updates .github/
```

`.github/` is a locked path of [the merge gate](software-factory.md), so that pull request waits for the owner's click
in the Inbox, which says so; Moebius never merges it by itself.

**Issues** have a template too, kept as a GitHub issue form, `.github/ISSUE_TEMPLATE/moebius.yml`, with the sections `Item
(optional)`, `What is wanted` and `How we will know it is done`; a column script reads an issue as `card.issue.fields`.
Issues the backlog sync makes keep their own format. For a repository with a measured merge bar, copy
[`examples/project-templates/`](../../examples/project-templates/README.md) to `projects/<project>/templates/`: an item ID
such as `K-47`, a `[measured] K-47: <subject>` or `[needs testing] K-47: <subject>` title, the merge bar, the GPU-less checks of a C++/CMake project, and a `Measurement` section that names the committed file the numbers are in.

**A field is a claim.** What a pull request says (`Measurement: not needed`, a ticked test) is data for the Reviewer. It never
satisfies the merge gate, lifts `require_measurement` or replaces the `no-measurement` label, which only a person sets.

**What is posted.** Plans, commits, tests with numbers, findings and waiting reasons: what happened. Never a prompt,
the output of a tool, a token, a cost or a link into Moebius. Everything is redacted first.

## Own agents work on the forge

Where a repository has `work_on_forge` on (the default, for GitHub, GitLab and Forgejo), Moebius's own agents work the
way a developer does ([ADR 0034](../adr/0034-templates-and-agents-that-work-on-the-forge.md)):

1. After the first round of the Coder the pull request opens at once, as a **draft**, with `Fixes #N` and a body
   written from the template out of the Coder's report. One pull request for all the rounds; its body is written again
   after each round.
2. The Coder commits its own work, with messages that say what it did.
3. The Reviewer's verdict is a **review of the pull request**: changes requested with a comment on each line a finding
   names, or an approval. GitHub refuses a verdict on a pull request by the user who opened it, so for Moebius's own
   pull requests the review is written as a review that only comments, whose first line is the verdict.
4. The next round **answers each finding** in a `## Review answer` comment, and the Reviewer's next prompt gets it.
5. On approval the pull request leaves draft and merges through the merge gate as before.
6. The issue holds only the status label and the link to the pull request: the history is in the pull request.
7. The branch is `moebius/card-12`, or `moebius/K-47-card-12` when the issue came from a backlog item, and the merge commit
   says the item and what the change does, from the pull request's fields, with `Fixes #12`.

A card that is already under way when you turn the setting on gets its draft pull request at the next tick of In review, and a
verdict it already has is written once; a card waiting in Ready to merge is left as it is.

Turn it off for a repository with `PUT /v1/repositories/{id}/flow` `{"work_on_forge": false}` or the repository's flow
settings in the web app; the pull request then opens when the change is offered, as before.

## How to try it

Run two servers against one test repository, each with a different person and login. On the first, take an issue; the
issue shows the first person as assignee on the forge, and on the second server its card is greyed within one poll.
Then open a pull request from a branch with `Fixes #<n>` in its description, and watch the card of the first server move
to In review.

## What can go wrong

- **The card says a login could not be assigned.** The reason is on the card and in `assign_problem` of
  [`GET /v1/cards/{id}`](../reference/desktop-api.md). Check that the token may write issues and that the login is a
  collaborator (GitHub), a project member (GitLab) or a collaborator (Forgejo).
- **Both servers start the same issue.** Neither person had said their login, or the label was not used. Set the logins
  and `agents_take` as above.
- **A merge is refused.** Open its card: the Inbox item `merge_refused` names the reason (a locked path, a missing measurement, a freeze, checks, a hold or the rules). Fix what it names, or merge it yourself where a person may (locked paths).
- **The repository is frozen.** The card window says why. Take the label `factory:freeze` off, or unfreeze it in the box; a breaker's freeze stays until you do.
- **A pull request is not followed.** Its author is one of your server's people, or it is already a change of the
  repository, or it names no closing keyword for an issue that has a card that is not done, or the issue's card already
  has an open change. A failed fetch of its head is a `forge_error` event; the card is still followed.
- **`/done` has nothing reviewed.** It is not the author's, the pull request is not followed, the checks are pending or
  failing (the pull request says so when they fail), its head was reviewed already, or the round limit is spent.
- **A command is not answered.** `commands` is off, the comment is older than the moment it was turned on, the command is
  not the first word of a line or is in a quote or code block, or **Answer on GitHub** is off (the command still worked; see
  the issue's assignees). A comment that could not be written is a `forge_error` event.
- **No label appears.** `mark_ready` is off, the issue is not approved, something blocks it, or the forge refused the
  label (the server tries again later and reports it with the other forge errors).
