# A project is a board over any number of repositories

Until now a board belonged to one repository, and a card was an issue of that repository. Teams plan work that spans
repositories (an API and the web app that uses it) and work that needs none (a pricing page's text, a contract to
check). A tool that is to hold a team's planning has to hold both. This record says what a project is; the work is cut
into issues in [`plan/issues-people-wiki-planning.md`](../plan/issues-people-wiki-planning.md), after people
([0018](0018-more-than-one-person.md)).

## Considered Options

- **Boards stay per repository**, and work without code goes to another tool or to a repository made only for it.
- **A project with at most one repository**: no card ever spans repositories; work across them is several cards.
- **A project with any number of repositories, or none.**
- For a card in such a project: **at most one repository per card**, **all of the project's repositories for every
  run**, the agent **asking for one during a run**, or **the repositories named before the run**.
- For merging a card with changes in several repositories: **each on its own**, **only all at once when all are
  ready**, or **together in a fixed order**.

## Decision

- **A project is one board with any number of repositories, or none.** The web app's Projects already are this, with
  one repository each. A repository can belong to several projects; its new issues land in its **home project**,
  unless a `project:<name>` label sends one to another project it belongs to. A card is on exactly one board.
- **Every project has a store of files of its own**, a git repository Moebius keeps for it without a forge. Work that
  touches no other repository is done there, so a run always has a worktree, the tool-call check and the review work
  the same everywhere, and what an agent writes is versioned.
- **A card names its repositories before its run starts**: an issue from a forge brings its repository, otherwise a
  person or the Moebius agent picks them, and none means the project's files. A run gets a worktree of each. A coder
  that finds it needs another says so in its result, and the card goes back to coding with that repository added; a
  run never gets a repository it was not given.
- **One change per repository, reviewed together and merged together.** The Reviewer sees every diff of the card at
  once: the standards axis per repository with that repository's settings, the spec axis over the whole card. Findings
  send the whole card back to coding. Merging merges every change of the card in a fixed order (the order the card
  names its repositories in). A merge that fails stops there and shows what is merged and what is not; there is no
  atomic merge across forges. The card is done when every change is merged.
- **An issue made in Moebius lives in Moebius**, even in a project whose repositories are on a forge; each change's pull
  request links the card. Issues made on a forge still come in by the poll and keep their forge.
- **Settings split by what they are about.** The workflow (column scripts, limits, board rules) is the project's, in
  `projects/<name>/` of the harness configuration repository. What is about code (checks, test first, review notes,
  code rules) stays with each repository in `repos/<name>/`.
- **A project can be private** ([0018](0018-more-than-one-person.md)): its board, wiki pages, cycles and what the
  Retro learns from it are seen only by its members.
- **Existing installations** get one project per repository, with its name, as its home project.

## Consequences

- The engine, the column scripts, the poll and the store stop assuming one repository per board: `card.repositories`
  replaces the single repository a script sees, `card.change` becomes `card.changes`, and the references for column
  scripts, the desktop API and the harness configuration change with it.
- A merge across forges can end half done. The card then stays in its column with the merged changes marked, and a
  person decides how to go on.
- Cards that span repositories make the regression set and trials harder: a past card is replayed with all its
  repositories at the commits it started from.
- Planning that spans projects goes through milestones, which belong to the organisation.
