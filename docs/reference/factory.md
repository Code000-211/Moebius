# The factory

The exact contracts of the parts that keep the owner's work small ([ADR
0032](../adr/0032-the-factory-keeps-the-owners-work-small.md)): the daily digest, the stale handling of issues and
pull requests, and the backlog sync; and of the parts that let measurements and reviews flow without the owner's clicks
([ADR 0033](../adr/0033-measurements-and-reviews-flow-without-the-owners-clicks.md)): measurements read from comments,
review on green checks, and pull requests that name no issue. How to set them up is in [the
guide](../guide/software-factory.md) and [Helping on someone else's
repository](../guide/working-with-others.md#measurements-without-a-click); the routes are in the [desktop
API](desktop-api.md#the-software-factory) and the commands in the [command line](cli.md). The code is `app/digest.rs`,
`app/upkeep.rs`, `app/backlog.rs`, `app/flow/` and `store/factory.rs`, `store/flow.rs` of `moebius-core`.

## The log

`factory_log` is what the digest reads. A line has a `kind`, the repository and the `number` of the issue or pull
request it is about, `who` (a login or `moebius`), a `detail` (redacted like everything stored), and `data`.

| Kind | Written when | `number` | `who` | `data` |
|---|---|---|---|---|
| `merged` | a change is set to merged (`Store::set_change_merged`), by whatever merged it | the pull request, else the issue | the outside author of the change, else `moebius` | `change`, `card`, `issue`, `title` |
| `merge_refused` | [the merge gate](desktop-api.md#the-merge-gate) refused a merge (once for a reason): `detail` is the gate's reason | the issue, else the pull request | | `change`, `card`, `title` |
| `merge_failed` | a card's merge stopped (the `merge_stopped` inbox item) | the issue | | `change`, `card`, `title`; `detail` is the reason |
| `take`, `release` | the assignees of an issue changed, for a login that came or went, whoever did it | the issue | the login | `card` |
| `auto_release` | the stale handling released an issue | the issue | the login released | `card`, `title` |
| `stale_reminder`, `stale_closed` | the stale handling reminded or closed a followed pull request | the pull request | its author | `card`, `title`, `change`, `head` |
| `stale_failed` | the forge refused a reminder or a close; it is told once for the head and not tried again for it | the pull request | its author | `card`, `title`, `head`, `action` (`remind` or `close`); `detail` is why |
| `backlog_clash` | a sync found an issue somebody else opened under an item's ID | the issue | its author | `card`, `title`, `item` |
| `backlog_sync` | a sync that wrote | | | `project`, `path` |

A part that builds on the factory writes a line with `Store::note_activity(kind, repository, number, who, detail,
data)`; the digest also shows these kinds. What the merge gate keeps is read from where it keeps it: a merge is
`measured` when a measurement is accepted for the head that merged (`POST /v1/changes/{id}/measurement`, `measurements`),
reverts are the rows of `reverts` since the last digest, and a frozen repository is told with its causes' reasons and
source, whatever froze it:

| Kind | In the digest as | Meaning |
|---|---|---|
| `measurement` | `measured: true` on the `merged` line whose `data.change` it names | A measurement came with the change, for a part that keeps its own. |
| `revert` | a revert | A change was reverted; `detail` is why. |
| `frozen` | something frozen | What was frozen, and why. |

A line is told in the first digest made after it was written, and in no other.

## The digest

The clock makes it: once a minute `on_digest_schedule` looks, and when the server's local time is `digest_at` or later
and the day has no digest, the day is claimed in `digests` and the digest is made. The stale handling goes first, so
that what it did is in the digest. A claim that is made is never made again, so a day has one digest whatever happens
to the server; `POST /v1/digest` with `force` is the one way to a second.

**The first day has none, and an empty period is not written.** A server's first start is kept (`digest_epoch`, when its
store was made): no digest tells of a time before it, so a server that starts after `digest_at` makes none that day, and
the first digest starts at the first start. A period with nothing in it (no repository with anything to say, nothing
that needs a person, no tokens) is claimed for its day but writes no item, so the owner is not told that nothing
happened.

The inbox item has the kind `daily_digest` and no card. Its `title` is `Daily digest <day>`, with no figures, and its
`data`:

| Key | Type | Meaning |
|---|---|---|
| `day` | string | The server's local date, `YYYY-MM-DD`. |
| `since`, `until` | number | The window in seconds since the epoch that the time-based figures (tokens, review rounds) are read for. `since` is when the last digest ended, or a day before the first. |
| `repositories` | array | One section for each repository that had something to say, in the order of their ids. |
| `tokens` | object | `spent` (tokens used in the window), `day_budget` (`budgets.day_tokens`, or null), `today` (tokens used today). |
| `needs_a_person` | array | The open inbox items but digests, grouped: `{kind, reason, items: [{id, title, card_id, number, repository}]}`. `reason` is the words for the kind, such as `A merge stopped`; `card_id`, `number` and `repository` are null for an item with no card. |
| `text` | string | The same in a few lines of Markdown. |

**Who reads what.** One digest is made for the organisation, and each person is told only what they see: the snapshot
and `POST /v1/digest` leave out the sections of repositories whose projects are all private to people the reader is not
in, the lines about cards of a private project they are not a member of, and the items of `needs_a_person` they do not
see, and tell the `text` again from what is left. An owner who is no member of a private project reads none of it. A
line is about a card by `card`, which every list's line has when it has one.

A section is `{id, name, merged, refused, reverts, review_runs, rounds, taken, released, upkeep, frozen, more}`. Each
list is of lines `{number, title, who, detail, measured}`; a list longer than 8 is cut and `more` says how many were left out, by list
name. `rounds` are the Reviewer's rounds on followed pull requests (`detail` is `round <n>: approved`, `changes
requested` or `no verdict`), `review_runs` counts the runs of the Reviewer on a card's own change, `upkeep` the stale
handling's actions (`detail` starts with `released:`, `reminded:` or `closed:`), and `frozen` the cards that are held
now (`who` is who held it, `detail` why) and the lines of kind `frozen`. A release the stale handling made is told in `upkeep` only, not
also in `released`.

## The stale handling

Once a day, before the digest, `App::run_upkeep` looks at every repository that is on a forge, with its `Upkeep`
settings (`GET` and `PUT /v1/repositories/{id}/upkeep`):

| Setting | Default | Meaning |
|---|---|---|
| `release_after_days` | 14 | An issue a login took that is none of this server's people, by `/take` or an assignment on the forge, with no followed pull request, is released after this many days. 0 turns it off. |
| `stale_after_days` | 14 | A followed pull request whose last review asked for changes and that saw no push for this many days gets one reminder comment for its head. 0 turns the reminder off. |
| `close_after_days` | 30 | The same pull request is closed after this many days without a push. 0 turns the closing off. When both are on this must be more than `stale_after_days`. |

- **Taken.** The time runs from when the poll first saw an assignee on the issue (`assigned_since`), so an issue that was
  taken before the upgrade starts its time at the first look.
- **Last review asked for changes.** The pull request's latest approving, changing or dismissing review on the forge is
  `changes_requested` **and was written for the head the pull request has now** (GitHub says the commit of a review); a
  push after it means the author has answered and the reviewer is waited for, so nothing is done. Where the forge does not
  say (GitLab, Forgejo), the Reviewer's latest round with a verdict counts instead, when it is `changes` and of the head.
- **No push.** The days run from the later of that review and `published_at` of the change, when its head last moved: a
  pull request pushed 40 days ago and reviewed yesterday is the author's for a day.
- **Release.** The login is taken off the issue on the forge and the card, and the issue gets a comment that says who
  took it, how long ago, and that it is free again. It can be taken again.
- **Close.** The pull request is closed on the forge (`GitHub` and `Forgejo` through the issue, `GitLab` as a merge
  request) and then gets a polite comment; the card goes back where it was taken from, the author is released and the line
  is `stale_closed`. A close or a reminder the forge refuses is a `stale_failed` line, told once in the digest and not
  tried again for that head, so nothing is commented on every day.
- **Never on frozen or held cards.** A card with a hold (`held`) is left alone, so is everything in a repository that is
  frozen (a person, the label `factory:freeze` or a breaker froze it), and so is everything while **Pause all** is on. Both are checked at the moment of each action.
- A forge that refuses is a `forge_error` event; the item is tried again the next day.

## The backlog

A project's backlog is set with `PUT /v1/projects/{id}/backlog`:

| Key | Default | Meaning |
|---|---|---|
| `repository` | | The name of a repository of the project: where the file is and where the issues are made. Needed the first time. |
| `path` | | The file's path in the default branch, such as `notes/backlog/optimisation-backlog.md`. Needed the first time. |
| `pattern` | `^#{1,6}\s+(?P<id>[A-Z][A-Z0-9]*-\d+)\s*[:.–—-]?\s*(?P<title>.*?)\s*$` | A regular expression ([Rust syntax](https://docs.rs/regex)) matched against each line. The group `id` is required and is the item's ID, the group `title` is its title (the ID when empty). |
| `closed_marker` | `[closed]` | Text, in any case, that marks an item closed wherever it stands in the item. Empty has no marker. |
| `cap` | 15 | The most issues one sync makes, 1 to 100. |
| `difficulty_label` | `difficulty:` | The start of the label for a stated difficulty: `difficulty:easy`, `difficulty:medium`, `difficulty:hard`. Empty gives none. |
| `lane_label` | `lane-required` | The label for an item that says a lane is required. Empty gives none. |

**The file.** A line that matches the pattern, outside a fenced code block, starts an item. The item is that line and
the lines after it up to the next line that starts an item, or the next heading of the same level or a higher one
(`#` marks), or the end of the file. A deeper heading belongs to the item. An item whose ID stands twice is made once,
from the first, and the sync warns.

**What an item can say.** `Difficulty: easy|medium|hard` (any case, `**` and `:` or `=` between are fine) gives the
difficulty. `Lane required` gives the lane label, except `Lane required: no` (or `false`, `none`). An item is closed
when it holds the closed marker, or when the file `closed/<ID>.md` exists next to the backlog file (in the example,
`notes/backlog/closed/K-47.md`).

**The issue.** Its title is `<ID>: <title>`. Its body holds, between the markers `<!-- backlog:<ID> -->` and `<!-- /backlog
-->`, the item quoted in full, each line after `> `, then a line that says the file and the item it is kept in step
with by the sync. Only that part is the sync's: a later sync replaces it and nothing else, so a `Blocked by #n` or
`Part of #n` line and whatever a person wrote outside the markers stay.

**Which issues are the sync's.** Only those it made, which the store records (`backlog_cards`); an issue is not found by
its title. One that was made before the sync kept track is adopted once when a person of this server or the Moebius agent
opened it. An issue somebody else opened under an item's ID is never edited, labelled or closed: the sync makes its own
issue for the item, lists the other in `clashes` of the report, and the digest tells it once (`backlog_clash`).

**One sync at a time.** Syncs of a project are serialised: a second, from the route, the nightly workflow or the agent,
waits for the first. The sync writes down that it is about to make an issue (`backlog_intents`) before it asks the forge,
so an issue the forge made that the server died before recording is found by its exact title at the next sync and
adopted, not made twice.

**A sync** (`POST /v1/projects/{id}/backlog/sync`, `{"dry_run": bool}`; the answer is the report below):

1. Brings the clone up to date and reads the file from the default branch.
2. For each item, finds its issues by the ID: the cards of the repository whose title starts with `<ID>:`, closed ones
   included.
3. **No issue and the item is open:** makes one with the labels, unless `cap` issues were made by this sync already; the
   item then waits for the next sync (`deferred`). No issue and the item is closed: nothing.
4. **An open issue:** when its title or body is not what the item asks, they are replaced (`updated`, recorded as an
   `edit_card` decision); labels the item asks for and the issue lacks are added, the others stay (`labelled`). An item that
   is now closed closes its issue as **not planned** (`closed`), on GitHub with `state_reason: not_planned`, elsewhere
   as any close.
5. **An issue a person closed** while its item is open stays closed and is left as it is.
6. A step that fails for one item is a warning and the others go on.

A second sync with nothing changed makes, changes and closes nothing. The report:

```json
{ "project": "Moebius", "repository": "moebius", "path": "notes/backlog/optimisation-backlog.md", "dry_run": false,
  "items": 3, "created": [{"id": "K-47", "number": 12, "title": "K-47: Cache the plan"}], "updated": [],
  "labelled": [], "clashes": [], "closed": [], "deferred": ["M-01"], "unchanged": 1, "warnings": [] }
```

With `dry_run` nothing is written and `created` entries have `number` 0.

## Measurements read from comments

On an issue whose followed pull request waits for a measurement, a comment of a registered measurer is read at the poll
([ADR 0033](../adr/0033-measurements-and-reviews-flow-without-the-owners-clicks.md)).

**Which comments.** All of these: the repository lists the comment's author in `measurers` (any case); the comment's
first non-blank line is the heading `## Measurement` (any case, more words after it are fine, `### Measurement` and
`## Measurements` are not); and the latest round of the Reviewer on the followed pull request, a person's review apart,
ended `approve_code` (state `awaiting_measurement`). Anything else is not read and not answered. Each comment is read once,
as a command is: the newest comment read of an issue is remembered (`command_seen`) before it is acted on, so a comment is
answered once and a refused one is not read again. Measurements are read in a repository that has measurers even where
`commands` is off.

**The form.**

```
## Measurement
before <sha of the merge base>
after <sha of the head>
GPUs: 2x MI300X
Power cap: 750 W
ROCm: 6.2.1
Container: rocm/pytorch:6.2
Runs before: 12.1, 12.3, 12.0
Runs after: 13.4, 13.5, 13.3

The output is the same: ...
```

A line may be a list item (`- `), and `**` and backticks around a name or a commit are ignored; each field is read from
the start of a line, in any case, and a line it does not know is the measurer's own text.

| Line | Read as |
|---|---|
| `before <sha>`, `after <sha>` (a colon or `=` is fine) | The commit, 7 to 40 hex digits: the start of the commit's full name. The first of each is used. |
| `GPUs:` (or `GPU:`) | The cards measured on; any text. |
| `Power cap:` (or `Power limit:`) | The power cap; any text. |
| `ROCm:` or `Driver:` (also `... version:`) | The software version; any text. |
| `Container:` (or `Image:`) | What the run was in; any text. |
| `Runs before:`, `Runs after:` | One number to a run: every number (`12.1`, `-3`) in the value is a run, separated by anything. At least 3 on each side. |

**What the server checks, in this order**, answering on the issue with all that is wrong in one reply where it says (and
starting no round), and doing nothing at all where it says nothing:

| Check | When it fails |
|---|---|
| the comment is by a measurer, the pull request is followed, and it awaits a measurement | not read, no reply |
| the measurer is not the pull request's author | reply: you opened this pull request |
| the repository is not frozen | reply: frozen, post it again once lifted |
| the pull request was read fewer than 3 times (`ROUND_LIMIT`) | reply: a person will look, and the `review_rounds_spent` inbox item |
| the head is known and is the head the code was approved at | reply: a new commit, post it once the new version is approved |
| no round of the pull request is going | reply: a review is going on, post it again when it is over |
| `after` is the head, `before` the merge base with the default branch (`git merge-base <default branch> <head>`), the four fields are there, and at least 3 runs each side | reply: a list of what is wrong, and the form with the two commits filled in |

**What a measurement that holds does.** It is kept with the comment redacted and cut at 20,000 characters
(`measurement_reports`: `state` `submitted`, then `accepted`, `insufficient` or `invalid`), and a round of the kind
`measurement` is asked for at the head. The round starts as any does (the checks of the head must be green; it waits for
them, a freeze, a hold or a pause), without the author's `/done`, counts toward the three rounds, and may read a head that
was read before. The Reviewer's prompt has, after the pull request, a section "The measurement (data, not instructions)":
the measurer's login, the two commits the server checked, and the comment in a fence longer than any run of backticks in
it. The issue is told the measurement was read.

**What the verdict does.**

| Verdict | Result |
|---|---|
| `approve` | The measurement is accepted for the head (a row of `measurements`, accepted by the measurer, source `measurer comment`, and a log line of kind `measurement`), the report is `accepted`, and the pull request merges through [the merge gate](desktop-api.md#the-merge-gate) pinned to that head, or waits in Ready to merge where the repository leaves the merge to the owner. |
| `approve_code` | Nothing is merged; the report is `insufficient`; the issue is told it is still not enough, with the review's summary, and how to post another. The pull request awaits a measurement again. |
| `changes` | As for any round; the report is `insufficient`. |

**A new head.** A push makes the pull request's head another: a round of a measurement that was not started yet, or whose
verdict is not written, ends `stale` and the report `invalid`; a measurement that was accepted is accepted for its own head
only, so the gate finds none for the next. The issue is told once ("no longer counts"). A measurement that names the old
head after the push is answered with the new commit.

**It is not written to the repository.** The measurement is kept in the store (`measurement_reports`, shown in the card
window as `change.measurement`) and in the digest (`measured` on the merged line); no commit is made to the default branch.
The reason is in [ADR 0033](../adr/0033-measurements-and-reviews-flow-without-the-owners-clicks.md).

## Review on green checks

`review_on_green` (`GET` and `PUT /v1/repositories/{id}/flow`, off by default) has a followed pull request reviewed
without its author's `/done`:

| Setting | Default | Meaning |
|---|---|---|
| `review_on_green` | `false` | Turns it on. |
| `green_wait_minutes` | 5 | How long the checks of a head must have been green before the round is asked for, 0 to 120. |
| `unbound_close_days` | 14 | See [pull requests that name no issue](#pull-requests-that-name-no-issue); 0 to 365. |
| `status_labels` | `false` | Turns on [status labels](#status-labels-on-the-forge). |
| `status_prefix` | `status:` | What a status label starts with. |

The wait starts at the first poll that finds the checks of the head `success` for that head (`green_heads.since`); pending,
red or no checks do not count (a repository with none is left to `/done`), and a push is another head with its own wait.
When the wait is over, the head has no round of the pull request counted or going, and no round was asked for for this head
by green checks before (`green_heads.asked`, so a round that failed is not made again), a round is asked for as `/done`
asks for one, and the issue says so. The round limit holds (the owner is asked instead), and the round does not start while
the card is held, the repository is frozen or all agents are paused. `/done` works as before: whichever comes first adds the
round, and the other finds it asked for or read. The settings are a person's: an agent's `PUT` is `403`.

## Pull requests that name no issue

A pull request that has no closing keyword (`Fixes #N`, `Closes #N`, `Resolves #N`) for an issue of the repository is looked
at once at each poll, after the pull requests that name one were followed.

1. **Bound.** When its title or branch holds the ID of a backlog item the sync made ([the
   backlog](#the-backlog)) as a word of its own, case aside (`K-47` is in `perf/k-47-cache`, not in `K-471`), and that is
   exactly one open issue of the repository, or when its title or description holds `#N` of exactly one issue its
   author is assigned to, it is followed as if it closed that issue, under the same conditions as any pull request: the
   author may write to the repository or is assigned to that issue, it is against the default branch, and the issue is open,
   is not taken by someone else and has no open change. More than one issue that fits is no binding.
2. **Asked.** Else, for a pull request of anybody who is not one of this server's people, against the default branch: one
   comment that asks for `Fixes #<issue number>` and names how it can be bound by itself, and the label `unbound` where the
   forge adds labels to a pull request (GitHub). Each is written once (`unbound_requests`), and the label is taken off
   when the pull request is bound later.
3. **Closed.** `unbound_close_days` after it was first seen so (0 never closes one): the pull request is closed, then gets
   a polite comment, once; a pull request that was closed so and is reopened is left alone. The log line is
   `unbound_closed`, told in the digest with the stale handling's.

Nothing is done in a frozen repository, while agents are paused, for a pull request of one of this server's people (the
owner's own) or of Moebius's own, for a pull request against another branch, or where `commands` or `answer_on_forge` is off.

## What the flow writes to the log

| Kind | Written when | `number` | `who` | `data` |
|---|---|---|---|---|
| `measurement_read` | a measurement passed the checks and a round was asked for | the issue | the measurer | `change`, `head`, `card` |
| `measurement` | the Reviewer approved a measurement, so it is accepted for the head | the issue | the measurer | `change`, `head`, `card` |
| `unbound_closed` | a pull request that named no issue was closed | the pull request | its author | `title`; `detail` is why |

## Status labels on the forge

So that people who do not use Moebius see where an issue stands on the forge, a repository with `status_labels` on has
exactly one label that starts with `status_prefix` on every open issue that has a card (`app/flow/status.rs`). The server
that has the setting on writes them; nothing else does. The label is the prefix and the state's name; the first state that
holds is the label:

| Order | State (label after the prefix) | Holds while |
|---|---|---|
| 1 | `frozen` | the repository is frozen |
| 2 | `awaiting-measurement` | the Reviewer's latest round of the pull request's current head ended `approve_code` |
| 3 | `changes-requested` | the Reviewer's latest round of the current head asked for changes and is written |
| 4 | `taken` | somebody took the issue and no pull request is followed yet |
| 5 | the card's column as a slug | otherwise: `backlog`, `not-started`, `in-progress`, `in-review`, `ready-to-merge`, and a custom column of the project ([ADR 0030](../adr/0030-custom-board-columns-per-project.md)) such as `ready-for-testing` or `being-measured` |

A slug is the column's name in lower case with every run of other characters a hyphen.

- **When.** After each poll of the repository, so a move, a review or a freeze shows within a poll. A person who removes
  the label by hand, or adds another with the prefix, has it put right at the next poll.
- **How.** One write of the issue's labels: the new status label in, the old ones with the prefix out, every other label as
  it was. Labels without the prefix are never touched. A label the forge does not have is made first, with a colour fixed
  for its state (`frozen` 5319e7, `awaiting-measurement` fbca04, `changes-requested` d93f0b, `taken` 0e8a16, a column
  ededed); on a forge that cannot make labels (not GitHub) it is made as the forge makes new ones.
- **A closed issue** keeps the label it had; nothing is written for it.
- **Not done:** the followed pull request's labels are not touched (the issue's label is the one people see), and a
  repository on a forge other than GitHub gets the labels through the forge's own label write where it has one.
- A forge that refuses is a `forge_error` event, and the write is tried again at the next poll.
