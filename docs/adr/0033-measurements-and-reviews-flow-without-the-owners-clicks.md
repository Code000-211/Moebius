# Measurements and reviews flow without the owner's clicks

[0031](0031-one-merge-gate-and-a-brake-for-the-factory.md) made the merge gate hold a change for a measurement, and gave
the Reviewer a verdict, `approve_code`, for a pull request whose code is right and whose measurement is missing. What
was left to the owner was the rest of the loop: reading the measurement a person posted and pressing "accept"
(`POST /v1/changes/{id}/measurement`), writing `/done` on behalf of a helper whose checks just went green, and finding
the pull requests that name no issue and so never reach the Reviewer at all. This is the part that removes those clicks
without removing a rule.

## Considered Options

- **Let the Reviewer's `approve` accept the measurement of whatever the pull request says.** The author writes the
  numbers, the Reviewer reads the author's numbers, and the owner's gate is satisfied by the stranger. Rejected: the
  measurement has to come from someone the owner named.
- **Accept a measurer's comment at once, with no review of it.** The measurer is trusted, but the bar is the repository's
  review brief, which only the Reviewer reads; a typo in `after` would accept a measurement of other code.
- **A measurer's comment, checked by the server for what it can check, then judged by the Reviewer against the brief.**
  The server checks what is mechanical (who, which commits, which fields); the Reviewer judges what is not (do the
  numbers meet the bar). The measurement is accepted for the head only when the Reviewer approves.
- **Review a pull request when its checks turn green, as soon as they do.** A push that fixes a flaky check would
  start a round at once and another when the author pushes again; the three rounds are spent on a draft.
- **Review once the checks have stayed green for a few minutes with no push, once for each head.**
- **Close every pull request that names no issue.** A quick fix by a stranger is closed as soon as it is seen.
- **Bind it where it can be done without guessing, ask once otherwise, and close after a long wait.**

## Decision

- **A measurer's comment is read at the poll, once, as a command is.** On an issue whose followed pull request's
  latest round ended `approve_code` (`awaiting_measurement`), a comment that starts with a `## Measurement` heading and
  is by a login in the repository's `measurers` is read by `App::read_commands`, which remembers the newest comment it
  looked at for each issue, so a comment is read once whatever came of it. A comment of anybody else is not read and not
  answered. Measurers are read even where commands are off.
- **The server checks before the Reviewer sees anything.** In this order, each answered on the issue with what is
  wrong, in one reply, and no round starts:
  1. the measurer is not the author of the pull request;
  2. the repository is not frozen, and the pull request has not been read three times;
  3. the code of the head the pull request has now was the one approved (a push since means the measurement waits for
     the code to be approved again);
  4. no round of the pull request is going;
  5. the comment has `before <sha>` and `after <sha>` (seven characters or more of the commit, or all of it): `after`
     is the pull request's head, `before` is its merge base with the default branch (`git merge-base`);
  6. it has the fields **GPUs**, **Power cap**, **ROCm** (or **Driver**) and **Container**, and **Runs before** and
     **Runs after** each list at least three numbers.
  The format is the one in [the guide](../guide/working-with-others.md#measurements-without-a-click) and
  [the reference](../reference/factory.md#measurements-read-from-comments), and the server's reply on `approve_code`
  shows it with this version's two commits filled in. The parser is a few line patterns, not a grammar: a line is a
  field, a list item or bold name is fine, anything it does not recognise is the measurer's own text.
- **A round of its own kind, `measurement`.** A measurement that holds is kept (`measurement_reports`, with the
  comment redacted and cut at 20 KB) and adds a round that starts as any does, once the checks of the head are green,
  without the author's `/done`. It counts toward the three-round limit like an author's round, and may read a head that
  was read before, which an author's round may not. The Reviewer's prompt gets a section after the pull request: the
  comment, fenced as data, with the measurer's login and the two commits the server checked, and the instruction to judge
  it against the review brief. `approve` merges through the gate exactly as an author's approved round does;
  `approve_code` again means it is still not enough, and a reply says what is missing (the review's summary);
  `changes` is as today.
- **The measurement is accepted for the head when the Reviewer approves it.** `approve` writes a row of
  `measurements` for that head, with the measurer as the one who accepted it and the source `measurer comment`, before
  the merge is asked for; so the gate's one condition, "a measurement is accepted for this head", holds for a merge by
  the Reviewer and for the owner's own. A repository that merges by hand records it and waits in Ready to merge.
- **A new head makes it count for nothing.** The measurement is of one commit. A push before the Reviewer ends makes the
  round stale and the report `invalid`; the issue is told. A measurement that was accepted is accepted for its head in
  `measurements`, which the gate reads by head, so the next head has none.
- **It is recorded in the store and the digest, and not committed to the repository.** The brief asked for
  `notes/backlog/measurements/<item>.md` as a separate docs commit on the default branch, optional and only if it could be
  done safely through the forge's API. It cannot be, today: no forge call writes a file (the forge trait publishes, pushes
  a branch and merges), a commit pushed straight to the default branch bypasses the branch protection the merge gate sits
  beside and the owner's locked paths, and a push that races a merge needs a retry that has no place in a poll. So the
  measurement is in `measurement_reports` (the card window shows who measured which commit and what came of it) and in the
  digest (`measured` on the merged line, the log kind `measurement`). A repository setting for the path was not added, since nothing
  would read it.
- **`review_on_green`, off by default, per repository.** When on, a followed pull request's round starts once the
  checks of its head have been green for `green_wait_minutes` (default 5) with no push. The first poll that sees the head's
  checks green starts the wait (`green_heads.since`); a push is a new head and starts its own. The head is asked for once
  (`green_heads.asked`), whatever became of the round, so a round that failed is not made again every tick. Only checks
  that are `success` count: a repository with no checks is left to `/done`. The round limit, a freeze, a hold and Pause all
  stop it as they stop every round. `/done` still works: whichever comes first adds the round, and the other finds it
  asked for.
- **A pull request with no closing keyword is bound, or asked, or closed.**
  - **Bound** when its title or branch holds the ID of a backlog item as a word of its own (`K-4` is not in `K-47`) and
    that is exactly one open issue the backlog sync made ([0032](0032-the-factory-keeps-the-owners-work-small.md)), or when its title
    or description names `#N` of exactly one issue its author is assigned to. It is then followed as if it had written
    `Fixes #N`, under the same conditions (its author may write or is assigned to that issue). Two issues that fit is
    no binding.
  - **Asked once** otherwise, for a pull request of anybody who is no one of this server's people: a comment that asks for
    `Fixes #N`, and the label `unbound` where the forge can (GitHub). The comment and the label are written once each; a
    pull request bound later has the label taken off.
  - **Closed** after `unbound_close_days` (default 14, 0 never) from the first time it was seen without an issue: closed,
    then a polite comment. Never again for the same pull request, so a reopened one is left alone.
  - **Never** in a frozen repository, while agents are paused, for a pull request of one of this server's people (the
    owner's own) or one of Moebius's own, for one against another branch, or in a repository where commands or the
    server's answers on the forge are off, since there the owner has not asked for helpers. The closes are in the digest
    (`unbound_closed`, under what the stale handling did).
- **The settings are the owner's.** `GET` and `PUT /v1/repositories/{id}/flow`
  (`review_on_green`, `green_wait_minutes`, `unbound_close_days`), by a person only, recorded as `edit_flow`, shown in
  the repository's page as "Review flow". The measurers stay with the repository's other factory settings.

- **Status labels, off by default.** `status_labels` and `status_prefix` (default `status:`) keep one label with the prefix
  on every open issue that has a card, so that people outside Moebius see where it stands: the column's slug, or while
  they hold, in this order, `frozen`, `awaiting-measurement`, `changes-requested`, `taken`. It is looked at after each poll
  and swapped in one write of the issue's labels, so a person's removal is put back at the next poll; labels without the
  prefix are never touched; missing labels are made with a colour fixed for the state (GitHub). A closed issue keeps its
  last label, since removing it would be a write for nothing and the last state is the true one. The followed pull request
  is not labelled: the issue is where people look, and a second set of labels would need the same swap on a second
  object for the same information.

## Consequences

- A measurer is trusted to say which commits they measured, and not to have measured them: the server checks the commits
  exist as named, not that the numbers are true. What the owner trusts them with is the list of logins; the Reviewer
  reads the numbers against the brief, and the owner may still reject the change.
- The measurement round is a third read of a pull request at most, and the round limit is shared: a pull request that
  needed two author rounds has one measurement round left. The owner is asked after that, as before.
- A measurement comment that arrives while a round is going, or before the code is approved, is answered and not
  remembered; the measurer posts it again. The alternative, remembering it, would need a queue and a rule for which of
  several wins.
- `review_on_green` waits for the first poll that sees green, so its wait runs from a poll and not from the check's own
  time; the wait is as long as asked and at most one poll longer.
- The unbound comment goes to every pull request without an issue in a repository that has helpers turned on, strangers'
  included, once. The owner who does not want it leaves commands off or sets `unbound_close_days` to 0 to never close.
- Only GitHub gets the label. A forge that cannot label is asked through the comment alone.
