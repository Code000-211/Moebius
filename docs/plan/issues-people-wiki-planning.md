# Issues: people, projects, the wiki and planning

What a team needs to plan its work in Moebius instead of a separate project-management tool: more than one person,
projects over any number of repositories, the organisation's wiki, planning and intake. The decisions are
[ADR 0018](../adr/0018-more-than-one-person.md) (people) and
[ADR 0019](../adr/0019-a-project-is-a-board-over-any-repositories.md) (projects); the words are in the
[glossary](../../GLOSSARY.md#people-and-planning). Each part builds on the ones before it.

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

### 2. Access tokens and sessions
An owner adds a person and gets their first access token; a person makes and revokes more of their own; an owner sees
each token's last use and revokes any. Tokens are stored only as hashes and last until revoked. Signing in with a token
gives a session cookie; sessions move into the store as hashes with an expiry, and end with the token or the person.
`moebius login` saves a person's token in `cli.toml`. The server's token stays the first owner's.
**Done when** a token acts as its person with their role, a revoked token and its sessions are refused at once, a
session survives a restart of the server, and no token or session id appears in the store or a log.

### 3. Roles on every route
Each route of the desktop API needs a role, named in `desktop-api.md`; `person_only` becomes a check of it. A viewer's
issue waits in the inbox like an outside author's.
**Done when** a test calls every route as each role and gets `403` exactly where the reference says, and a route
without a role in the reference fails a test.

### 4. Assignees, mentions and notifications
Cards and inbox items can be assigned to people; scripts see `card.assignees`, and the default workflow starts no
agent on a card that has one. `@handle` in a comment notifies that person. A person follows the cards they wrote or
were assigned and can follow any other; notifications are events on the stream, filtered per person, with a read mark.
**Done when** assigning, mentioning and a change to a followed card each give exactly the right person one
notification, and the default workflow leaves an assigned card in Ready.

### 5. Chats per person
A chat belongs to the person who started it; another person sees it only once it is shared. The Retro still reads
every chat.
**Done when** a second member cannot read or write a chat that is not shared, can read it once it is, and the Retro's
input includes both.

### 6. People in the web app
A sign-in page for a token, a People page for owners (add a person, change a role, disable, tokens and their last
use), a page for a person's own tokens, avatars and assignee pickers, `@` completion in comments, and a notification
list.
**Done when** the end-to-end tests sign in as an owner, add a member, sign in with their token, assign them a card and
see the notification, and the design audit passes.

### 7. Review of P1

## P2 · Projects

### 8. Projects in the store
A project has a name, a board, any number of repositories and a store of files of its own (a git repository Moebius
keeps without a forge). A repository belongs to one or more projects and has a home project. The migration makes one
project per repository, with its name, as its home project.
**Done when** a store from before migrates with every card on the board of its repository's project, a project with
no repository runs a card in its own files, and a repository can be added to a second project.

### 9. Where a forge's issue lands
A new issue of a repository lands in its home project; a `project:<name>` label sends it to another project the
repository belongs to, and moving the label moves the card. A label naming a project the repository is not in leaves
the card where it is and says so in the inbox.
**Done when** a fake forge's issue lands in the home project, one with a label in the named project, and one with a
wrong label stays with a note in the inbox.

### 10. Settings per project and per repository
The workflow (column scripts, limits, board rules) moves to `projects/<name>/` of the harness configuration
repository; checks, test first, review notes and code rules stay in `repos/<name>/`. The migration moves each
repository's workflow to its project.
**Done when** two projects sharing a repository run different workflows, a card is checked with its repository's
checks, and a migrated configuration behaves as before.

### 11. Cards over several repositories
A card names its repositories (`card.repositories`): an issue from a forge brings its own, otherwise a person or the
Moebius agent picks them; none means the project's files. A run gets a worktree of each and nothing else. A coder that
needs another says so in its result, and the card goes back to coding with it added. Each repository gets its own
change; `card.change` becomes `card.changes`.
**Done when** a run of a card with two repositories has both worktrees and two changes, a write outside them is refused
by the tool-call check, and a result asking for a third sends the card back with it added.

### 12. Reviewing and merging a card's changes together
The Reviewer gets every diff of a card: the standards axis per repository with its settings, the spec axis over the
whole card; findings send the whole card back. Merging merges every change in the order the card names its
repositories; a merge that fails stops, the card stays with what is merged marked, and the card is done when all are.
A pull request links its card.
**Done when** a review sees both diffs, merging two local bare remotes merges them in order, a failing second merge
leaves the first merged and the card marked, and a later merge of the second finishes the card.

### 13. Private projects
A project can be private and names its members; everyone else, owners included unless they add themselves, sees
neither its board, its cards nor its files. What the Retro learns from it (proposals and lessons) is shown only to its
members; a proposal to the shared configuration is shown to all without quoting the project. Judgements in the training
export carry their project, and the export leaves private projects out unless asked to include them.
**Done when** a member who is not in a private project gets `404` for its board, cards and runs, a lesson from it is
found only by its members, and an export without the switch has none of its judgements.

### 14. Projects in the web app
The Projects page lists projects instead of repositories; a project's page shows its board, its repositories and its
members; a card shows and edits its repositories and shows each change; merging shows the order and what is merged.
**Done when** the end-to-end tests make a project with two repositories, run a card over both, merge it, make a
private project and see it disappear for a member who is not in it, and the design audit passes.

### 15. Review of P2

## P3 · The wiki

### 16. The wiki repository
The wiki is a git repository the server clones to `<data_dir>/wiki`, kept like the harness configuration repository:
Markdown pages in folders, an edit is a commit with the person as author pushed to `origin`, a push that fails undoes
the commit. Pages of the organisation are at the top; `projects/<name>/` holds a project's pages, which are private
with a private project. An ADR records why the wiki is a git repository and not records in the store.
**Done when** an edit is a commit by its person on a local bare `origin`, a failing push leaves the clone as it was, a
change pushed to `origin` by someone else is read after a refresh, and a private project's pages are refused to a
non-member.

### 17. Pages through the API
Routes to list, read, write, rename, move and delete pages and to read a page's history and an older version. A link
`[[Page name]]` leads to a page, `#12` and `project#12` to a card; each page knows the pages and cards that link to it.
Members write, viewers read and comment.
**Done when** every route is in `desktop-api.md`, a rename keeps the links to the page working, and a page lists its
backlinks.

### 18. Searching the wiki
Pages are indexed with SQLite FTS5, like lessons, and found by words in their title and text, per project or across the
organisation, within what the person may see.
**Done when** a search finds a page by a word of its text, ranks a title match first, finds an edited page by its new
words, and never returns a private project's page to a non-member.

### 19. Agents read and write the wiki
The pages a card's issue links to are in its code run's prompt. The Moebius agent and the other agents can search,
read and propose pages through `"$MOEBIUS"` commands; an agent's edit is a branch that waits in the inbox, as the
Retro's proposals do. Every call goes through the tool-call check.
**Done when** a run's prompt holds the linked page, an agent's edit arrives in the inbox as a branch and changes the
wiki only once a member merges it, and the skill's commands are checked by its test.

### 20. The wiki in the web app
A Wiki page: the tree of pages, a page with its backlinks and history, and an editor for Markdown with a preview
(Tiptap, without editing together in real time), `[[` and `#` completion.
**Done when** the end-to-end tests make a page, link it from a card, find it by search, read its history, and the
design audit passes.

### 21. Review of P3

## P4 · Planning

### 22. Priorities and due dates
A card has a priority (`urgent`, `high`, `medium`, `low`, none) and an optional due date, kept in Moebius and, where
the forge has them, written as labels or the forge's own field. Scripts see `card.priority` and `card.due`; the
default `ready.luau` starts urgent cards first.
**Done when** a script sees both fields, the default workflow starts an urgent card before an older low one, and a
forge's label sets the priority after a poll.

### 23. Estimates
A card has an estimate in points, set by a person or proposed by the Moebius agent from similar cards. Scripts see
`card.estimate`; the Retro compares estimates with the tokens and time cards took and says where they were wrong.
**Done when** a script sees the estimate, the agent's proposal names the cards it compared with, and the Retro's report
lists the cards whose estimate was furthest off.

### 24. Cycles
A cycle belongs to one project and has a name, a start and an end. Cards of the project are added to it; its progress is
how many of its cards and points are done; when it is closed, unfinished cards move to the next cycle or out of it, as
the person who closes it chooses. Scripts see `card.cycle`. A project need not use cycles.
**Done when** a cycle's progress follows its cards, closing it carries unfinished cards over, a card is in at most one
open cycle, and a card of another project cannot be added.

### 25. Milestones
A milestone belongs to the organisation and has a name, a target date and a description, holding cards of any
projects, with its progress like a cycle's. A person sees only the cards of projects they may see, and the progress
counts only those.
**Done when** a milestone shows cards of two projects with its progress, a card's page shows its milestones, and a
non-member of a private project sees neither its cards nor their count.

### 26. Saved views
A view is a filter (projects, repositories, labels, assignees, priority, cycle, milestone, column, author, text) with
a layout (list, board, calendar by due date, timeline by cycle) and a sort, saved for its person or shared with
everyone.
**Done when** a shared view shows every member the same cards within what each may see, a personal one only its person,
and every filter and layout is covered by a test.

### 27. Analytics
Throughput, the time from start to done, how long cards spend in each column, burn-up per cycle and per milestone, and
the tokens per point, per project and across the organisation, read from the moves and runs the store already keeps.
**Done when** the numbers of a made-up history in a test are right, and the page loads for a store with ten thousand
cards in under a time the test states generously.

### 28. Planning in the web app
Pages for cycles, milestones, views and analytics, and the fields of P4 on a card.
**Done when** the end-to-end tests plan a cycle, follow it to its end, save and share a view, and the design audit
passes.

### 29. Review of P4

## P5 · Intake

### 30. Intake
A project can turn on intake: a form anyone who may see the project fills in, a viewer too, with a request in their own
words. The Moebius agent grills it into an issue (the grilling Moebius already has), a member of the project accepts,
edits or declines it, and an accepted request becomes a card of the project, with the requester following it.
**Done when** a viewer's request becomes a draft issue from a grilling, a member's acceptance makes a card the viewer
follows, a declined request tells the viewer why, and a project without intake has no form.

### 31. Review of P5

## Later

Not cut into issues yet, because each needs a decision first:

- **Editing together in real time**, pages and issue bodies, with `yrs` on the server and Yjs in the web app.
- **Public pages**: a board, a view or wiki pages readable without an account.
- **Importing** from other tools through their APIs (issues, comments, cycles), never by reading their databases.
- **Templates** for issues and pages, and **kinds of issues** with fields of their own.
