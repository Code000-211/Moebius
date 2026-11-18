# Issues: people, the wiki and planning

What a team needs around the board to plan its work in Moebius instead of a separate project-management tool: more than
one person, an organisation's wiki, cycles, milestones, estimates, views and intake. The decision behind the first part
is [ADR 0018](../adr/0018-more-than-one-person.md); every later part builds on people.

Each issue is one card and one commit, and changes its references, examples and the glossary with it, as every issue
does ([`CLAUDE.md`](../../CLAUDE.md)). Every milestone ends with a review issue: all tests run many times, the code read
for bugs, overengineering and readability, and what is found fixed.

**Built from descriptions, not from other code.** Project-management tools such as Plane are open source under the
AGPL. Their features are ideas anyone may build; their code, texts, icons and names are not. Whoever builds these
issues, person or agent, works from this plan and the public description of a feature, does not read the source of
such a tool, and copies none of its text or assets. The editor in the web app may use Tiptap and Yjs (MIT), not a
tool's own editor built on them.

## P1 · People

### 1. People in the store
A `people` module of the store: id, handle, display name, role (`owner`, `member`, `viewer`), disabled. `Actor::Person`
carries the person's id; authors, deciders, `claimed_by` and `held.by` become `person:<handle>`. The migration makes
the first owner and gives every earlier `person` to them; the training export still writes `person`.
**Done when** a store from before migrates with its issues, judgements and holds belonging to the first owner, a second
person's issue has their author, and an export of either has the decider `person`.

### 2. Roles on every route
Each route of the desktop API needs a role, named in `desktop-api.md`; `person_only` becomes a check of it. A viewer's
issue waits in the inbox like an outside author's.
**Done when** a test calls every route as each role and gets `403` exactly where the reference says, and a route
without a role in the reference fails a test.

### 3. Invitations and sessions in the store
An owner invites a person (handle, name, role) and gets a one-time code that opens the web app as them, or makes a new
code for an existing person. Sessions move into the store as hashes of their ids with an expiry; signing out or
disabling a person ends theirs.
**Done when** an invited person signs in with their code, the code works once, a session survives a restart of the
server, and disabling a person signs them out at once.

### 4. Signing in with a forge
An OAuth application per forge in the configuration (GitHub, GitLab, Forgejo); a person links their forge login once
after an invitation and then signs in with it. A forge login not linked to a person is refused.
**Done when** a fake OAuth server on port 0 signs a linked person in, refuses an unlinked login, and refuses a callback
whose state does not match.

### 5. Personal tokens
A person makes, lists and revokes tokens for the command line, stored only as hashes; `moebius login` makes one through
the browser and saves it in `cli.toml`. The server's own token stays the first owner's.
**Done when** a token acts as its person with their role, a revoked token is refused, and no token appears in the store
or a log.

### 6. Assignees, mentions and notifications
Cards, issues and inbox items can be assigned to people (`card.assignees` in the scripts). `@handle` in a comment
notifies that person. A person follows the cards they wrote or were assigned and can follow any other; notifications
are events on the stream, filtered per person, with a read mark.
**Done when** assigning, mentioning and a change to a followed card each give exactly the right person one
notification, and a script sees `card.assignees`.

### 7. Chats per person
A chat belongs to the person who started it; another person sees it only once it is shared. The Retro still reads
every chat.
**Done when** a second member cannot read or write a chat that is not shared, can read it once it is, and the Retro's
input includes both.

### 8. People in the web app
A sign-in page (code or forge), a People page for owners (invite, change a role, disable, new code), avatars and
assignee pickers on cards and inbox items, `@` completion in comments, and a notification list.
**Done when** the end-to-end tests sign in as an owner, invite a member, sign in as them, assign them a card and see
the notification, and the design audit passes.

### 9. Review of P1

## P2 · The wiki

### 10. The wiki repository
The wiki is a git repository the server clones to `<data_dir>/wiki`, kept like the harness configuration repository:
Markdown pages in folders, an edit is a commit with the person as author pushed to `origin`, a push that fails undoes
the commit. Pages of the whole organisation are at the top; `repos/<name>/` holds a repository's own pages. History,
review of a change and backups then come from git. An ADR records why the wiki is a git repository and not records
in the store.
**Done when** an edit is a commit by its person on a local bare `origin`, a failing push leaves
the clone as it was, and a change pushed to `origin` by someone else is read after a refresh.

### 11. Pages through the API
Routes to list, read, write, rename, move and delete pages and to read a page's history and an older version. A link
`[[Page name]]` leads to a page, `#12` and `repo#12` to a card; each page knows the pages and cards that link to it.
Members write, viewers read and comment.
**Done when** every route is in `desktop-api.md`, a rename keeps the links to the page working, and a page lists its
backlinks.

### 12. Searching the wiki
Pages are indexed with SQLite FTS5, like lessons, and found by words in their title and text, per repository or across
the organisation.
**Done when** a search finds a page by a word of its text, ranks a title match first, and finds an edited page by its
new words after the edit.

### 13. Agents read and write the wiki
The pages a card's issue links to are in its code run's prompt. The Moebius agent and the other agents can search,
read and propose pages through `"$MOEBIUS"` commands; an agent's edit is a branch that waits in the inbox, as the
Retro's proposals do. Every call goes through the tool-call check.
**Done when** a run's prompt holds the linked page, an agent's edit arrives in the inbox as a branch and changes the
wiki only once a member merges it, and the skill's commands are checked by its test.

### 14. The wiki in the web app
A Wiki page: the tree of pages, a page with its backlinks and history, and an editor for Markdown with a preview
(Tiptap, without editing together in real time), `[[` and `#` completion.
**Done when** the end-to-end tests make a page, link it from a card, find it by search, read its history, and the
design audit passes.

### 15. Review of P2

## P3 · Planning

### 16. Priorities and due dates
A card has a priority (`urgent`, `high`, `medium`, `low`, none) and an optional due date, kept in Moebius and, where
the forge has them, written as labels or the forge's own field. Scripts see `card.priority` and `card.due`; the
default `ready.luau` starts urgent cards first.
**Done when** a script sees both fields, the default workflow starts an urgent card before an older low one, and a
forge's label sets the priority after a poll.

### 17. Estimates
A card has an estimate in points, set by a person or proposed by the Moebius agent from similar cards. Scripts see
`card.estimate`; the Retro compares estimates with the tokens and time cards took and says where they were wrong.
**Done when** a script sees the estimate, the agent's proposal names the cards it compared with, and the Retro's report
lists the cards whose estimate was furthest off.

### 18. Cycles
A cycle is a time box with a name, a start and an end, for the organisation. Cards are added to it; its progress is how
many of its cards and points are done; at its end, unfinished cards move to the next cycle or out of it, as the person
who closes it chooses. Scripts see `card.cycle`.
**Done when** a cycle's progress follows its cards, closing it carries unfinished cards over, and a card is in at most
one open cycle.

### 19. Milestones
A milestone is a named goal with a target date and a description, holding cards of any repository, with its progress
like a cycle's. Parent issues stay as they are; a milestone groups across repositories.
**Done when** a milestone shows cards of two repositories with its progress, and a card's page shows its milestones.

### 20. Saved views
A view is a filter (repositories, labels, assignees, priority, cycle, milestone, column, author, text) with a layout
(list, board, calendar by due date, timeline by cycle) and a sort, saved for its person or shared with everyone.
**Done when** a shared view shows every member the same cards, a personal one only its person, and every filter and
layout is covered by a test.

### 21. Analytics
Throughput, the time from start to done, how long cards spend in each column, burn-up per cycle and per milestone, and
the tokens per point, all read from the moves and runs the store already keeps.
**Done when** the numbers of a made-up history in a test are right, and the page loads for a store with ten thousand
cards in under a time the test states generously.

### 22. Planning in the web app
Pages for cycles, milestones, views and analytics, and the fields of P3 on a card.
**Done when** the end-to-end tests plan a cycle, follow it to its end, save and share a view, and the design audit
passes.

### 23. Review of P3

## P4 · Intake

### 24. Intake
A form anyone with an account can fill in, a viewer too: a request in their own words. The Moebius agent grills it into
an issue (the grilling Moebius already has), a member accepts, edits or declines it, and an accepted request becomes a
card in the repository the member picks, with the requester following it.
**Done when** a viewer's request becomes a draft issue from a grilling, a member's acceptance makes a card the viewer
follows, and a declined request tells the viewer why.

### 25. Review of P4

## Later

Not cut into issues yet, because each needs a decision first:

- **Editing together in real time**, pages and issue bodies, with `yrs` on the server and Yjs in the web app.
- **Public pages**: a board, a view or wiki pages readable without an account.
- **Importing** from other tools through their APIs (issues, comments, cycles), never by reading their databases.
- **Templates** for issues and pages, and **kinds of issues** with fields of their own.
