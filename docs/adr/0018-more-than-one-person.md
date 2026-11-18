# More than one person works on one server

The first version had one person (`docs/spec.md`, "Not in the first version"): one token, one `Actor::Person`, sessions
in memory, and every issue a person wrote has the author `person`. Moebius is to be a tool other teams plan and review
their work in, so a server needs several people, each with a name, a role and their own assignments, chats and
notifications. This record says how; projects, which people are members of, are
[0019](0019-a-project-is-a-board-over-any-repositories.md). The work is cut into issues in
[`plan/issues-people-wiki-planning.md`](../plan/issues-people-wiki-planning.md).

## Considered Options

- **Workspaces on one server**, several organisations side by side. Every record, the harness configuration
  repository, the router's budgets and the Retro would need a workspace, and isolating one workspace's runs from
  another's is a promise the core would have to keep everywhere. A second organisation can run a second server.
- **One organisation per server**, its people with a role. Nothing that is per server today has to be cut apart.
- **Passwords**: Moebius would hold credentials it must protect, reset and rate limit.
- **Signing in with a forge or an OpenID provider**: an OAuth application per provider to set up, and a team without
  one has nothing.
- **Access tokens an owner makes**, which is what the server's token already is for its one person.
- **A role per person and project**, as some trackers have, against **a role in the organisation and private projects**
  that name their members.

## Decision

- **One server is one organisation.** Its people are records of their own in the store (`people`): an id, a handle
  (`@lea`), a display name, a role, and whether they are disabled. A person is never deleted, only disabled, so that
  what they did keeps its author.
- **Three roles in the organisation.**
  - `owner`: everything, including the configuration, plugins, rules, agents, secrets, repositories, projects and
    people.
  - `member`: issues and cards (writing, moving, holding, blocking, assigning, deleting), comments, the wiki, cycles,
    milestones, views, chats, and merging a change.
  - `viewer`: reads, comments, and writes issues and requests, which wait in the inbox as an outside author's do.
  A **private project** names its members; a person who is not one, owners included unless they add themselves, does
  not see it. The desktop API reference names the role each route needs, and a test fails for a route without one.
  `person_only` becomes a check of the role and the project.
- **The actor has an identity.** `Actor::Person` carries the person's id; a judgement records the decider
  `person:<handle>`, an issue a person wrote has the author `person:<handle>`, and `claimed_by` and `held.by` say the
  same. Records from before say `person`, which the migration gives to the first owner. The training export keeps
  writing `person`, without who it was: who decided is not what a decision model should learn from.
- **Signing in is an access token.** An owner makes a person's first token when adding them and hands it over; a person
  makes more of their own (for the command line on a second machine) and revokes any of theirs; an owner sees when each
  was last used and revokes any. A token lasts until it is revoked and is stored only as a hash. The web app asks for a
  token once and keeps a session cookie; the command line keeps the token in `cli.toml`. The server's own token stays
  and is the first owner's, so that scripts keep working. Moebius stores no passwords and needs no identity provider.
- **Sessions survive a restart.** They move from memory into the store, stored only as hashes of their ids, with an
  expiry; signing out, revoking the token they came from or disabling the person ends them.
- **What belongs to whom.**
  - Boards, the inbox, the wiki and runs are shared within what a person may see. A card and an inbox item can be
    assigned to people; mentioning `@handle` in a comment or a page notifies them.
  - A chat belongs to the person who started it, and others see it only when it is shared. The Retro still reads every
    chat, as it reads every run.
  - What the Retro learns from a private project (its proposals and lessons) is seen only by the project's members; a
    proposal to the shared configuration is shown to everyone without quoting the project.
  - Notifications are per person: what is assigned to them, what mentions them, and changes to cards they wrote or
    follow. They are events on the same stream, filtered for the person.
- **Nothing in a run changes.** A run still never waits for a person ([0008](0008-runs-never-wait-for-a-person.md)).
  Column scripts see `card.assignees` and decide whether an agent works on a card a person was given; the default
  workflow leaves such a card to its people.
- **The network stays private.** The server still listens where `listen` says. A team reaches it over a private network
  or `moebius connect`; a server reachable from further away is put behind a reverse proxy with HTTPS, which the guide
  shows, and the session cookie is `Secure` when the request came over HTTPS.

## Consequences

- Every route checks a role, and every place that compares an author with `person` (about a hundred) learns about
  people. The desktop API, configuration, training export and column script references change with it.
- A token handed over in a chat or a mail is as good as the person; revoking one is the remedy, so the People page
  shows every token's last use.
- The web app gets a sign-in page, a People page for owners, tokens for each person, assignees and mentions, and
  notifications.
- "The person" in the documentation becomes "a person" where more than one is meant.
