# More than one person works on one server

The first version had one person (`docs/spec.md`, "Not in the first version"): one token, one `Actor::Person`, sessions
in memory, and every issue a person wrote has the author `person`. A team that plans its work, writes its wiki and
reviews what the agents did together needs to be several people on the same server, each with a name, a role and their
own inbox items, chats and notifications. This record says how; the work is cut into issues in
[`plan/issues-people-wiki-planning.md`](../plan/issues-people-wiki-planning.md).

## Considered Options

- **Workspaces on one server**, several organisations side by side, each with its members, repositories and settings.
  Every record, the harness configuration repository, the router's budgets and the Retro would need a workspace, and
  isolating one workspace's runs from another's is a promise the core would have to keep everywhere. A second
  organisation can run a second server.
- **One organisation per server**, its people as members with a role. Nothing that is per server today has to be cut
  apart.
- **Passwords** stored as hashes in the store: Moebius would then hold credentials it must protect, reset and rate
  limit, which none of its other parts needs.
- **Passkeys** (WebAuthn): no secret on the server, but a browser offers them only on HTTPS or `localhost`, which a
  server on a private network often is not.
- **Signing in with a forge** (OAuth with GitHub, GitLab or Forgejo, which a team using Moebius already has), and
  **one-time codes** that an owner hands out, which Moebius already has for opening the web app.

## Decision

- **One server is one organisation.** Its people are records of their own in the store (`people`): an id, a handle
  (`@lea`), a display name, a role, and whether they are disabled. A person is never deleted, only disabled, so that
  what they did keeps its author.
- **Three roles.**
  - `owner`: everything, including the configuration, plugins, rules, agents, secrets, repositories and people.
  - `member`: issues and cards (writing, moving, holding, blocking, deleting), comments, the wiki, chats, and merging a
    change.
  - `viewer`: reads everything a member reads, comments, and writes issues, which wait in the inbox as an outside
    author's do.
  The desktop API reference names the role each route needs, and a test fails for a route without one. `person_only`
  becomes a check of the role.
- **The actor has an identity.** `Actor::Person` carries the person's id; a judgement records the decider
  `person:<handle>`, an issue a person wrote has the author `person:<handle>`. Records from before have the author and
  decider `person`, which the migration gives to the first owner. The training export keeps writing `person`, without
  who it was: who decided is not what a decision model should learn from.
- **Signing in.** The server's token stays and is the first owner's: the command line and scripts keep working. An owner
  invites a person with a one-time code that opens the web app as them; after that they sign in with a forge the server
  has an OAuth application for, or with a new code from an owner. Moebius stores no passwords. Each person can make
  personal tokens for the command line (`moebius login`), stored only as hashes and revocable.
- **Sessions survive a restart.** They move from memory into the store, stored only as hashes of their ids, with an
  expiry; signing out or disabling a person ends theirs.
- **What belongs to whom.**
  - The board, the inbox, the wiki and the runs are shared. An inbox item, a card and an issue can be assigned to a
    person; mentioning `@handle` in a comment or a page notifies them.
  - A chat belongs to the person who started it, and others see it only when it is shared. The Retro still reads every
    chat, as it reads every run.
  - Notifications are per person: what is assigned to them, what mentions them, and changes to cards they wrote or
    follow. They are events on the same stream, filtered for the person.
- **Nothing in a run changes.** A run still never waits for a person ([0008](0008-runs-never-wait-for-a-person.md)).
  Column scripts see which person: `card.author`, `card.claimed_by` and `card.held.by` say `person:<handle>` where
  they said `person`, and a new `card.assignees` lists the handles. A workflow can then treat people differently, but
  the decision is the script's, not a person's.
- **The network stays private.** The server still listens where `listen` says. A team reaches it over a private network
  or `moebius connect`; a server reachable from further away is put behind a reverse proxy with HTTPS, which the guide
  shows, and the session cookie is `Secure` when the request came over HTTPS.

## Consequences

- Every route checks a role, and every place that compares an author with `person` (about a hundred) learns about
  people. The desktop API, configuration, training export and column script references change with it.
- The web app gets a sign-in page, a People page for owners, assignees and mentions on cards and comments, and
  notifications.
- The glossary gets the words Person, Owner, Member and Viewer; "the person" in the documentation becomes "a person"
  where more than one is meant.
- The rest of the plan (the wiki, cycles, milestones, estimates, views and intake) builds on people: an assignee, an
  author of a page and a notification all need one.
