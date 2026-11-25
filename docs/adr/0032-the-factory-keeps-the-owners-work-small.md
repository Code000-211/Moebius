# The factory keeps the owner's work small

[0027](0027-working-with-people-on-other-servers-through-the-forge.md) and
[0028](0028-helpers-take-issues-with-commands-in-comments.md) let helpers take issues and open pull requests, and
Moebius drives the work from an issue to a merge. What is left for the owner is attention: finding out what happened,
chasing issues nobody works on and pull requests nobody pushes to, and keeping the set of issues agents may take
small and well described. This is the part of a software factory that does it without a person, and tells the person
once a day what it did.

## Considered Options

- **Notifications for every event.** A merge, a refusal, a take and a close each tell the owner, and the owner reads
  fifty of them. Nothing is summarised and the owner is the filter.
- **A report agent that reads the board each morning.** It costs tokens every day for what is a query, and what it
  writes is not the same two days running.
- **A digest the server writes from what it recorded, as one inbox item.** The server already knows what merged, who
  took what and what waits; what it lacked was a log of what happened, which it writes as it goes.

For the backlog, a person with an optimisation backlog in a Markdown file and a few hundred items has to choose between
typing the issues by hand and an agent that makes them all at once, so that the ready set is no longer small.

## Decision

- **One digest a day, one inbox item.** From `digest_at` (default 07:00, the server's local time) the clock makes the
  digest of the day if the day has none. The day is claimed in the store before anything is done (`digests`), so a day
  has one however often the clock looks and whether or not the server restarts. A server that was down at `digest_at`
  makes it when it is up the same day; a day it missed is not made up, because the next digest reports from the last
  one on. It is told by repository: what merged, with its author and whether a measurement came with it; the merges
  that were refused or failed, with their reasons; reverts; the review rounds that ran and how they ended; who took or
  released an issue; what the stale handling did; the tokens spent against the day's budget; what is held or frozen;
  and everything that waits for a person, grouped by what it waits for, each with its card so that the page links it.
  It is kept short: a list longer than eight lines says how many more there were. One digest is made for the
  organisation and **cut to what the reader sees** when it is read: a person who is no member of a private project reads
  none of its titles, logins or links.
- **A log, written where it happens.** `factory_log` takes a line when a change is set to `merged`, when a merge stops,
  when the assignees of an issue change (so a `/take`, an assignment on the forge and a release by the owner are all
  seen the same way), when the merge gate refuses a merge ([0031](0031-one-merge-gate-and-a-brake-for-the-factory.md)),
  and for what the stale handling and the backlog sync do. What the gate keeps is read where it keeps it: a merge is
  measured when a measurement was accepted for its head, reverts are the gate's `reverts`, and a frozen repository is
  told with the reasons of its causes. A part that builds on the factory writes its own line with
  `Store::note_activity`.
  The digest partitions the log by line number, which only grows, so a line is told once whatever the second it was
  written in.
- **Stale handling, once a day, before the digest.** Per repository on a forge, with its own settings
  (`release_after_days` 14, `stale_after_days` 14, `close_after_days` 30; 0 turns one off):
  an issue a helper took, by `/take` or an assignment, that has no followed pull request after `release_after_days` is
  released, with a reply on the issue; a followed pull request whose last review asked for changes and that saw no push
  for `stale_after_days` (counted from the later of the review and the push, and only when the review is of the head the
  pull request has now) gets one reminder comment for that head; after `close_after_days` it is closed with a polite
  comment and its issue released. The first time an assignee is seen starts their time (`assigned_since`), so
  installing this releases nobody at once. A person of this server who is assigned is not "taken": only logins that
  are no one of this server's people count. **Nothing is done for a card that is held, in a repository that is frozen ([0031](0031-one-merge-gate-and-a-brake-for-the-factory.md)), nor while Pause all is on**, and
  every action is a line of the log and so of the digest.
- **Backlog sync is a tool.** A project says where its backlog is (`backlogs`: repository, path, a pattern for an item,
  a closed marker, a cap and the label names) and a sync, run by a person, by the Moebius agent as `"$MOEBIUS" backlog
  sync` or by a scheduled workflow, makes one issue for each item without one, titled `<ID>: <title>` with the item
  quoted in full between markers. It only ever changes issues it made (recorded in the store, not found by title; one
  somebody else opened under an ID is a reported clash), so it is idempotent; syncs of a project are serialised and an
  intent is written before the forge is asked; it rewrites the title and the marked part of the body of an open issue
  whose item changed, keeping blockers, the parent and a person's text, and closes the issue of a closed item as *not
  planned*. A cap (default 15) limits the issues one sync makes, so that the ready set stays small; the
  rest waits for the next sync. A difficulty (`easy`, `medium`, `hard`) or "lane required" in an item becomes the labels
  `difficulty:<level>` and `lane-required`, whose names the project may change. Nothing is hard-coded to one
  repository or file layout: the pattern is a regular expression with the groups `id` and `title`.
- **It is exposed the ways Moebius is built on:** the routes `GET`, `PUT` and `DELETE /v1/projects/{id}/backlog` and
  `POST /v1/projects/{id}/backlog/sync`, the commands `"$MOEBIUS" backlog show|set|sync|remove`, a page of the
  `moebius` skill, and a scheduled workflow example that syncs every night through the `http` node.

## Consequences

- The digest is only as complete as the log: what happened before the log existed, or happens somewhere that writes no
  line, is not in it. The first poll after an upgrade reports the assignees already on issues as taken once.
- Closing a pull request is the one thing the stale handling does that a person might not want. It is off per
  repository with `close_after_days = 0`, comes after a reminder a person can answer by pushing, and never touches a held
  card.
- A sync never reopens an issue a person closed, and never closes one whose item is open: the file is the source for
  what exists and for the text of open issues, and a person keeps the last word on closing them by hand.
- A cap per sync, not per open total, means a nightly sync adds up to the cap every night while the file has items; a
  project that wants the set smaller closes items in the file or lowers the cap.
- Second-resolution time and the poll mean a digest is a few seconds behind what happened; a take written in the same
  second as a digest is told in it or in the next, never in both and never in neither.
