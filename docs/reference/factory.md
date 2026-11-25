# The factory

The exact contracts of the parts that keep the owner's work small ([ADR
0031](../adr/0031-the-factory-keeps-the-owners-work-small.md)): the daily digest, the stale handling of issues and
pull requests, and the backlog sync. How to set them up is in [the guide](../guide/software-factory.md); the routes are
in the [desktop API](desktop-api.md#the-software-factory) and the commands in the [command line](cli.md). The code is
`app/digest.rs`, `app/upkeep.rs`, `app/backlog.rs` and `store/factory.rs` of `moebius-core`.

## The log

`factory_log` is what the digest reads. A line has a `kind`, the repository and the `number` of the issue or pull
request it is about, `who` (a login or `moebius`), a `detail` (redacted like everything stored), and `data`.

| Kind | Written when | `number` | `who` | `data` |
|---|---|---|---|---|
| `merged` | a change is set to `merged`, by whatever merged it | the pull request, else the issue | the outside author of the change, else `moebius` | `change`, `card`, `issue`, `title` |
| `merge_failed` | a card's merge stopped (the `merge_stopped` inbox item) | the issue | | `change`, `card`, `title`; `detail` is the reason |
| `take`, `release` | the assignees of an issue changed, for a login that came or went, whoever did it | the issue | the login | `card` |
| `auto_release` | the stale handling released an issue | the issue | the login released | `card`, `title` |
| `stale_reminder`, `stale_closed` | the stale handling reminded or closed a followed pull request | the pull request | its author | `card`, `title`, `change`, `head` |
| `backlog_sync` | a sync that wrote | | | `project`, `path` |

A part that builds on the factory writes a line with `Store::note_activity(kind, repository, number, who, detail,
data)`; the digest also shows these kinds:

| Kind | In the digest as | Meaning |
|---|---|---|
| `measurement` | `measured: true` on the `merged` line whose `data.change` it names | A measurement came with the change. |
| `merge_refused` | a refused merge | A merge was refused (as opposed to failed); `detail` is why. |
| `revert` | a revert | A change was reverted; `detail` is why. |
| `frozen` | something frozen | What was frozen, and why. |

A line is told in the first digest made after it was written, and in no other.

## The digest

The clock makes it: once a minute `on_digest_schedule` looks, and when the server's local time is `digest_at` or later
and the day has no digest, the day is claimed in `digests` and the digest is made. The stale handling goes first, so
that what it did is in the digest. A claim that is made is never made again, so a day has one digest whatever happens
to the server; `POST /v1/digest` with `force` is the one way to a second.

The inbox item has the kind `daily_digest` and no card. Its `title` is `Daily digest <day>: <n> merged, <m> need you`,
and its `data`:

| Key | Type | Meaning |
|---|---|---|
| `day` | string | The server's local date, `YYYY-MM-DD`. |
| `since`, `until` | number | The window in seconds since the epoch that the time-based figures (tokens, review rounds) are read for. `since` is when the last digest ended, or a day before the first. |
| `repositories` | array | One section for each repository that had something to say, in the order of their ids. |
| `tokens` | object | `spent` (tokens used in the window), `day_budget` (`budgets.day_tokens`, or null), `today` (tokens used today). |
| `needs_a_person` | array | The open inbox items but digests, grouped: `{kind, reason, items: [{id, title, card_id, number, repository}]}`. `reason` is the words for the kind, such as `A merge stopped`; `card_id`, `number` and `repository` are null for an item with no card. |
| `text` | string | The same in a few lines of Markdown. |

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
- **Last review asked for changes.** The pull request's latest review on the forge is `changes_requested`, or the
  Reviewer's latest round with a verdict is `changes`.
- **No push.** `published_at` of the change: when its head last moved.
- **Release.** The login is taken off the issue on the forge and the card, and the issue gets a comment that says who
  took it, how long ago, and that it is free again. It can be taken again.
- **Close.** The pull request gets a polite comment and is closed on the forge (`GitHub` and `Forgejo` through the issue,
  `GitLab` as a merge request), the card goes back where it was taken from, the author is released and the line is
  `stale_closed`.
- **Never on frozen or held cards.** A card with a hold (`held`) is left alone, and so is everything while **Pause all**
  is on. Both are checked at the moment of each action.
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

**The issue.** Its title is `<ID>: <title>`. Its body quotes the item in full, each line after `> `, then a line that
says the file and the item it is kept in step with by the sync.

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
  "labelled": [], "closed": [], "deferred": ["M-01"], "unchanged": 1, "warnings": [] }
```

With `dry_run` nothing is written and `created` entries have `number` 0.
