# The wiki works before it has a remote

[0022](0022-the-wiki-is-a-git-repository.md) made the wiki a git repository named by `[wiki] url`. A fresh server has
no `[wiki]` table, so every route of the wiki answered `409` and the Wiki page offered nothing: nobody could write a
first page, and nothing in the app could set the wiki up. Setting `wiki.url` through the Settings page was accepted,
said no restart was needed, and did nothing until one. This record changes where the wiki starts, not what it is.

## Considered Options

- **Keep `[wiki]` required and add a setup step** that asks for the repository before the Wiki page works.
- **A wiki that needs no remote**: without a `url` the pages are a repository of their own on the server, and a person
  connects a git repository to it when they want one.
- **A database of pages until a repository is named**, moved into one then.

## Decision

- **`[wiki]` and its `url` are optional.** Without a `url` the wiki is a plain git repository at `<data_dir>/wiki`,
  made when the server first starts, with no `origin`. It is the same kept clone as before (`repos/kept.rs`): an edit
  is a commit by its person, and nothing is pushed or followed, since there is nowhere to push. Every route works.
- **An owner connects a git repository while the server runs**, from a notice on the Wiki page and a row of the Settings
  page (`PUT /v1/wiki/repository`). The server checks that it can be reached; an empty repository gets the history
  pushed to it; a repository that has pages becomes the wiki when this one has none; when both have pages nothing is
  merged and the person is told. Only then is `[wiki] url` written, and the wiki uses it at once, with no restart.
- **Moebius never merges two histories on its own.** Two repositories that both have pages are a decision for a person
  with git, not something to guess at.
- **A `url` that cannot be used does not stop the server.** It starts without a wiki: the routes answer `409` with the
  reason, `GET /v1/wiki/status` says `unavailable`, Get started lists it as waiting, and an owner can connect a
  repository that works.
- **`wiki.url` is a key of the settings that needs a restart** when it is changed through `PUT /v1/settings`, as the
  answer says; the connect route is the way that applies at once.

## Consequences

- The wiki is usable from the first minute, and a team that wants it backed up or shared connects a repository later
  without losing a page.
- Pages kept only on the server are as safe as its disk until a repository is connected; the notice says so.
- A server whose file names a `url` that its local wiki has not been connected to (a hand edit) connects it at the next
  start by the same rules, and starts without the wiki when both have pages.
