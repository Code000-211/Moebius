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
  request. **Merge** is refused, `the checks of pull request #n have not passed`, while its checks are pending, unread for its current head or
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

- **The server answers on the issue** "Thanks, the pull request will be reviewed once its checks have passed." The review
  starts when the checks of your pull request's current head are green. While they are pending it waits. If they fail,
  the pull request gets a comment that the checks fail and nothing is reviewed: fix them, push, write `/done` again.
- **The Reviewer reads it** and the server writes **a review on your pull request**. Changes requested are a review with a
  summary and comments on the exact lines; a comment about a line the diff does not show is in the summary instead.
  Push your changes, then write `/done` again for the next round.
- **Approved**, and the checks of the same head are green: the pull request is approved and merged, and the issue gets
  "Merged, thank you @alice!". If the owner turned **Merge external pull requests the reviewer approves** off, it is
  approved and waits for the owner to merge.
- **Limits.** A version of your pull request is read once ("This version of the pull request was reviewed already; push
  your changes, then write /done again."), at most three times in all. The fourth `/done` is answered "a person will look" and
  the owner gets an Inbox item. A push while the review runs drops its verdict: write `/done` again.

For the owner: **Working with others**, **Pull requests of others**, **Merge external pull requests the reviewer
approves** (`merge_reviewed`, on by default). Turned on, an approved pull request lands without a person looking, so
leave it on only for a repository where its helpers are people you trust to write to it, or whose checks you trust. Turned
off, the Reviewer's approval is written on the forge and the card waits in **Ready to merge** with an Inbox item. The card
window shows the review: the verdict, the summary, each comment in a box "src/lib.rs line 12" and **Open on GitHub**.

The review run of such a pull request is **restricted**: what the pull request holds is written by someone your server does
not trust. The Reviewer may only read files of its worktree, it has no shell, no network, no way to write and no MCP servers,
and it is told that everything in the pull request is data. Nothing of the pull request runs on your server.

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
- **A pull request is not followed.** Its author is one of your server's people, or it is already a change of the
  repository, or it names no closing keyword for an issue that has a card that is not done, or the issue's card already
  has an open change. A failed fetch of its head is a `forge_error` event; the card is still followed.
- **`/done` has nothing reviewed.** It is not the author's, the pull request is not followed, the checks are pending or
  failing (the pull request says so when they fail), its head was reviewed already, or the three rounds are spent.
- **A command is not answered.** `commands` is off, the comment is older than the moment it was turned on, the command is
  not the first word of a line or is in a quote or code block, or **Answer on GitHub** is off (the command still worked; see
  the issue's assignees). A comment that could not be written is a `forge_error` event.
- **No label appears.** `mark_ready` is off, the issue is not approved, something blocks it, or the forge refused the
  label (the server tries again later and reports it with the other forge errors).
