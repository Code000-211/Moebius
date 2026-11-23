# The wiki is a git repository

> Amended by [0026](0026-the-wiki-works-before-it-has-a-remote.md): `[wiki] url` is optional, and a wiki without one is a
> repository on the server that a git repository can be connected to later.

Teams keep what they know beside their work: how a service is built, a runbook, who is on call. A project
([0019](0019-a-project-is-a-board-over-any-repositories.md)) is to hold that as a wiki, the organisation's pages and a
folder per project, which people read and edit on the Wiki page and agents read when they work on a card. This record
says where the pages are kept; the work is cut into issues in
[`plan/issues-people-wiki-planning.md`](../plan/issues-people-wiki-planning.md) (P3).

## Considered Options

- **Records in the store**: a table of pages with their text, and a table of versions beside it.
- **A git repository of Markdown files**, cloned and kept by the server as it keeps the harness configuration
  repository, every edit a commit pushed to `origin`.
- **The wiki of a forge** (GitHub's, GitLab's), read and written through its API.

## Decision

- **The wiki is a git repository**, cloned from `[wiki] url` to `<data_dir>/wiki` and kept like the harness
  configuration repository: one kind of kept clone (`repos/kept.rs`) serves both. Pages are Markdown files in folders,
  the organisation's at the top and a project's in `projects/<key>/`; a page is named by its path without `.md`.
- **An edit is a commit with its person as the author**, made after the clone has followed `origin` and then pushed to
  it. A push that fails undoes the commit, so the clone never holds what `origin` does not; there is nothing to merge
  later. Moebius is the committer.
- **The history is git's.** A page's versions are the commits that changed it, followed through renames and moves; an
  older version is read from its commit, a deleted page keeps its history.
- **What is pushed from elsewhere is read after a refresh**: the server fetches every 30 seconds, before every edit and
  when asked.
- **What git does not give is kept beside the clone, in the store, and made again from the files**: the links between
  pages and cards and the search index, in the issues after this one.

## Consequences

- Pages outlive Moebius and move with any git tool: a team can clone, read, grep and review them, write them in an
  editor and push, or keep them in a repository that already holds its documentation. Agents get the same files they
  get in every other repository.
- The forge's own review works on the wiki: an agent's edit can be a branch that waits in the Inbox, as the Retro's
  proposals do, and is merged like any change.
- Every edit needs `origin` reachable, since it is pushed at once; a wiki whose `origin` is down cannot be edited, only
  read.
- Two people editing the same page at once both succeed, the later write replacing the earlier, as in most wikis; the
  history keeps both. Concurrent edits of different pages never conflict, since each edit follows `origin` first.
- A private project's pages ([0018](0018-more-than-one-person.md)) are kept from non-members by the server, not by git:
  whoever can read the repository itself reads every page.
- A forge's own wiki was not taken: not every forge has one, its API differs on each, and Moebius would not own the
  clone its agents work in.
