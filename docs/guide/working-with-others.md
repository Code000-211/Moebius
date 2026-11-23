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
- **No label appears.** `mark_ready` is off, the issue is not approved, something blocks it, or the forge refused the
  label (the server tries again later and reports it with the other forge errors).
