# Keeping the owner's work small

## When you want this

You own a repository that agents and helpers work on, and you want to look at Moebius once a day: one inbox item that
says what happened, issues nobody works on given back, pull requests nobody pushes to answered, and a backlog file that
turns itself into issues a few at a time. The reasons are in [ADR
0032](../adr/0032-the-factory-keeps-the-owners-work-small.md), the exact contracts in [the
reference](../reference/factory.md).

## The daily digest

Nothing to set up. At `digest_at` (default `07:00`, the server's local time; see [the
clock](../reference/configuration.md#the-clock)) the Inbox gets one item, **Daily digest**, for each day. In the Inbox
it shows, repository by repository, what merged and who wrote it (with a mark when a measurement came with it), the
merges that were refused or failed and why, reverts, the review rounds and how they ended, who took or released which
issue, what the stale handling did, the tokens spent against the day's budget, what is held, and **Needs a person**:
everything open in the Inbox, grouped by reason, each with a link to its card. Dismiss it when you have read it.

- Change the time in Settings (`digest_at`), or `"$MOEBIUS" settings set digest_at 06:30`. It takes effect at once.
- A server that is down at that time makes the digest when it is up again the same day; a day it missed is not made up.
- The first day of a server has no digest (nothing before its first start is told), and a period with nothing to tell
  writes none.
- `"$MOEBIUS" digest` makes today's digest now if it has none; `--force` makes it again.

## Stale issues and pull requests

Open the repository's **Project settings, Repositories, Working with others, Stale handling** (or `"$MOEBIUS" upkeep
<repository>`). The defaults:

| Setting | Default | What it does |
|---|---|---|
| Release an issue taken for | 14 days | A helper took it (with `/take`, or by being assigned) and no pull request of theirs is followed: they are taken off, with a reply on the issue, and it can be taken again. |
| Remind after | 14 days | A followed pull request whose last review asked for changes saw no push: its author gets one reminder comment. |
| Close after | 30 days | The same pull request is closed with a polite comment and its issue released. |

`0` turns one off. A card that is **held**, and every card while **Pause all** is on, is left alone. The stale handling
runs once a day just before the digest, and every action it takes is in the digest. It needs the repository to be on a
forge, and it only ever touches issues taken by people who are not of this server and pull requests Moebius follows (see
[working with others](working-with-others.md)); your own agents' work is never released or closed by it.

## A backlog file that becomes issues

Say your project keeps `notes/backlog/optimisation-backlog.md`, with an item for each piece of work:

```markdown
## K-47: Cache the plan
Difficulty: hard

Cache the compiled plan between runs.

## P-16: Shorter prompts
Lane required

Cut the prompt of the Reviewer by a third.
```

1. **Point the project at it.** Open **Project settings, Backlog**, choose the repository, give the path, and save; or
   `"$MOEBIUS" backlog set Moebius --repository moebius --path notes/backlog/optimisation-backlog.md`. The pattern, the
   closed marker, the cap (15 issues a sync) and the label names have defaults and can be changed in the same place.
2. **Look first.** `"$MOEBIUS" backlog sync Moebius --dry-run` says what a sync would do and writes nothing.
3. **Sync.** `"$MOEBIUS" backlog sync Moebius` makes `K-47: Cache the plan` and `P-16: Shorter prompts` with the item
   quoted in full, and the labels `difficulty:hard` and `lane-required`. Run it again and nothing happens; edit an item
   in the file and the issue's body follows; mark an item `[closed]`, or add `notes/backlog/closed/K-47.md`, and its
   issue is closed as *not planned*. The Moebius agent does the same when you ask it to sync the backlog.
4. **Make it a habit.** [`examples/workflows/nightly-backlog-sync.toml`](../../examples/workflows/nightly-backlog-sync.toml)
   is a workflow of the project that syncs every night at half past two. It calls the sync through the `http` node, so
   the server must allow its own address (`workflows.http_hosts = ["127.0.0.1"]`) and have a secret that holds a token
   of a member (`"$MOEBIUS" secrets set moebius-token ...`). Save it with `moebius workflow save --project Moebius
   nightly-backlog-sync examples/workflows/nightly-backlog-sync.toml`.

The cap is what keeps the set of issues agents may take small: with 200 items and a cap of 15, the first sync makes 15
issues and the next night makes 15 more. Lower it, or close items in the file, to make fewer.

## What can go wrong

- **The sync says the file is not found.** The path is read from the repository's default branch as the forge has it,
  not from a branch or an unpushed commit.
- **An item is skipped.** Its line does not match the pattern: the ID must look like `K-47` (letters, a dash, digits),
  right after the `#` marks. Change the pattern to your own IDs, keeping the group `id`.
- **An issue was left alone.** The sync changes only the issues it made (and adopts, once, those a person of this server
  opened by hand). One somebody else opened with `<ID>:` in its title is a clash: the sync reports it, the digest tells it
  once, and the sync makes its own issue for the item.
- **My text in an issue was kept.** Only the part between `<!-- backlog:<ID> -->` and `<!-- /backlog -->` is replaced; a
  blocker, the parent line and anything you wrote outside the markers stay.
- **Nothing is closed.** The marker is looked for in the item's text only, in any case, and `closed/<ID>.md` is looked for
  next to the backlog file. A closed issue is never reopened by a sync.
- **A helper was released while working.** The stale handling counts a followed pull request as work. A helper who
  works locally and has not opened one has 14 days; raise `release_after_days` for the repository.
- **The digest has the issues already taken as taken.** The first poll after an upgrade reports assignees that were
  there before; the next digests do not.
