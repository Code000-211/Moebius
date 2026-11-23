# Working with people on other servers through the forge

[0018](0018-more-than-one-person.md) made one server serve several people. Two people may also each run a server of
their own on the same repository: the owner on theirs, a helper on theirs. Their servers share nothing but the forge.
Each polls the same issues, so each makes a card for every issue, and each may start a run, open a pull request and
merge for the same one. A claim (`cards.claimed_by`) lives in one store, so the other server never learns of it. This
record says how two servers keep out of each other's way without a link between them.

## Considered Options

- **A claim service or a link between servers**, so that a server asks another before it starts. Every server would
  need the address and a credential of every other, a team would have to run or trust a third party, and a server that
  is off would hold up the rest.
- **Claims kept only in Moebius**, in a shared store or in files of the repository. Nothing on the forge would show
  who works on an issue, so people who do not use Moebius could not tell either, and the files would be a second place
  to keep in step.
- **Webhooks from one server to another.** The same addresses and credentials as a link, and a delivery missed while a
  server is off is a claim lost.
- **The forge as the only channel.** The forge already holds who an issue is assigned to, its labels and its pull
  requests, every server already polls it, and people read it too.

## Decision

- **The forge is the only channel.** An assignee is the claim, a label is the release, a pull request is the result.
  No server calls another.
- **Taking an issue assigns the person on the forge.** A person tells their server their login on each forge (Settings,
  You, Profile; `GET` and `PUT /v1/logins`). When they claim a card, or the board starts a run for them, the server
  adds that login to the issue's assignees (`App::take_issue`). Nothing fails for it: with no login set, or when the
  forge refuses, the card says why (`assign_problem`) and Get started lists the missing login.
- **An issue someone else is assigned to is left alone.** The poll records an issue's assignees. The first that is none
  of this server's people's logins is the card's `taken_by`. The column scripts' run starters start nothing for such a
  card and make it wait, a claim of it is refused, and the board draws it greyed. It stays so until the assignee is
  removed on the forge.
- **The owner's server may keep a label.** A repository has `mark_ready`, `ready_label` and `agents_take`, kept in the
  store. While `mark_ready` is on, the poll puts the label on every open issue that is approved and has no open blocker
  and takes it off when one appears. A helper sets `agents_take` to `labelled`, so that their agents take only labelled
  issues that nobody took.
- **Another person's pull request is followed.** Each poll lists the open pull requests. One whose author is none of
  this server's people and that closes an issue that has a card (a closing keyword in its title or body) is a change of
  that card: the card is in review, the head is kept in a local branch, and the owner is told in the inbox. The
  owner may have the Reviewer read it and merge it once its checks have passed (a request with no checks may be merged). A request closed without a merge
  gives the card back to the column it came from, still taken.

## Consequences

- An assignment can be refused (a token that may not assign, a login that is no collaborator or project member). The
  card still moves; the cost is that other servers may take the issue too, which the card says.
- A person who has not said their login is invisible to the other servers. The Get started page tells them.
- Keeping the label is the owner's job and only one server should do it. Two servers marking the same repository would
  fight over it, so the setting says it belongs to the server of the repository's owner.
- Closing keywords are read from the text of a pull request, not from the forge's own list of closed issues
  (`closingIssuesReferences` is GitHub's GraphQL only). A request that closes an issue another way is not followed.
- Each poll of a repository costs one more list, of its open pull requests.
- GitLab and Forgejo assign only a project member or collaborator. A login that is neither is refused there, as on
  GitHub.
- A card another server took is greyed, not hidden: the owner still sees the issue and who has it, and the card goes
  on when the assignee is removed.

## Safeguards

- A pull request is followed only when it is against the default branch and its author may write to the repository or is
  assigned to the issue, so that a stranger's fork saying "Fixes #n" cannot freeze a card. A card with a run going, or
  taken by someone other than the author, is never moved.
- A merge of a followed pull request is pinned to the head the poll read, and is refused until the checks are known for
  that head. A review counts for the commit it read.
- Two servers that take an issue at once are told apart by the forge's own order of assignees: the one listed first
  keeps it, the other takes itself off and starts nothing.
