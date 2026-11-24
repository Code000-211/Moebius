# The Reviewer reads a pull request when its author says it is done

Amended 2026-11-24: the four limits are the keys of `[review]` in the configuration (`diff_limit`, `file_limit`,
`files_limit`, `files_at_most`), and a changed file over the file limit no longer refuses the round: it is given as the
lines of its own part of the diff, with a notice on its entry and its blob never loaded, so a large file that the pull
request barely touches no longer costs the review. When the files given whole are over the total, the largest of them
are given that way first, until the rest fits. Nothing is given halfway either way: a diff over its limit and a binary
file are refused as below.

[0027](0027-working-with-people-on-other-servers-through-the-forge.md) follows a pull request someone else opened as a
change in review, and the owner has the Reviewer read it and merges it by hand. [0028](0028-helpers-take-issues-with-commands-in-comments.md)
gives helpers `/done`, which with a followed pull request only said thanks. This record has `/done` of the pull request's
author start the review, writes what the Reviewer found on the forge where the author reads it, and lets the owner's
server merge what the Reviewer approves. A pull request is somebody else's code, so the review run is restricted.

## Considered Options

- **Review every followed pull request at once.** Costs a run for each push of every stranger's draft, and the author has
  not said it is ready.
- **Only the owner starts it** (as `POST /v1/changes/{id}/review` does). Nothing changes for the helper, and the owner
  reads the Reviewer's findings in the card, which the author never sees.
- **The author starts it with `/done`**, and what the Reviewer found is a review on the pull request.
- **A summary as one comment.** Works everywhere, but the author must find the lines by hand.
- **A review with comments on lines**, where the forge has one.
- **Merge by hand after the Reviewer approved**, or **merge for the owner** by a setting.

## Decision

- **The author's `/done` asks for a round.** Only the login that opened the pull request has it reviewed, whoever the
  issue is assigned to; anybody else's `/done` on that card is only thanked. The round starts when the checks of the head
  the poll last read are green. Pending checks, or checks not yet read after a push, wait; red checks are answered on the pull
  request and nothing is reviewed. Comments on the pull request itself are not read: the poll reads the comments of
  issues, and a second place to read would double what [0028](0028-helpers-take-issues-with-commands-in-comments.md) built.
- **A round never holds its pull request for ever.** One round of a pull request goes at a time, a person's review ("Review with agent") takes the same claim, and a sweep, a closing pull request and a person's "End this review round" end the rounds that cannot end by themselves; a review left half written is never written again by itself.
- **At most three rounds per forge pull request, one per head, one at a time.** Counted across the change rows that followed it, so closing and reopening it does not begin again; the claim of a round also writes its two commits, and the run reads them from the round. A round counts once its run started, whatever came of it. A fourth `/done` is
  answered that a person will look, and the owner gets an Inbox item (`review_rounds_spent`). A head with a round is not
  read again. A head that ended with a verdict is not read again; one whose round ended without a verdict, or went stale, may be read again, and each such round has counted toward the three, so the author cannot spend runs for ever.
- **The Reviewer answers in a structure.** Its answer ends with one JSON object: `verdict`, `summary` and `comments` of
  `path`, `line` and `body`. Anything else is no verdict, and no verdict never approves. The card's own review reads the
  same object. The server checks each comment against the diff of the request: a comment on a line the diff shows is a
  comment on that line, the others go into the summary.
- **A real review on the forge.** GitHub and Forgejo take a review with `REQUEST_CHANGES` or `APPROVE` and comments on
  lines. GitLab has no review that asks for changes: a note and a discussion on the diff per comment, and an approval for
  an approving review. A forge without any of these gets the whole review as one comment (`Forge::review_request`). A
  forge that refuses the lines gets the review again with the comments in the body. The review is tied to the commit that
  was read.
- **A verdict counts for one head.** The review is used only while the head the poll knows is the one that was read; a push
  drops it. A merge is pinned to that head, so a push the poll has not read yet is refused by the forge.
- **Approved is merged, by a setting that is on.** `merge_reviewed` (default on; a setting of the repository, in "Working
  with others") has the server write an approving review and merge when the checks of the same head are green, and
  thank the author on the issue. Off, the review is approved and the card goes to Ready to merge with an Inbox item for
  the owner. The default is on because the owner asked for it; the hint in the settings says that foreign code then
  lands without a person looking.
- **The Reviewer of such a pull request has no tools and no checkout.** What the pull request holds is the work of a
  person the server does not trust, and every attempt to let an agent read it with tools failed in another way (a path
  the checks read differently from the harness, arguments the harness repairs after the check, a checkout that runs the
  git filters the pull request's attributes name, a symlink on a runner). So the server reads it itself, from git objects
  only, and gives it to the Reviewer as text (`app::review_context`): the diff of what merging it does to the default
  branch (the default branch's tip against the tree `git merge-tree --write-tree` makes, since `git merge-base` names one
  of several best ancestors of a criss-crossed history and hides what the forge's merge would revert) and each changed
  file whole as the pull request has it, with `git diff` and `git cat-file`, no textconv, no external diff, no attributes
  file, and no worktree, so no filter or hook of the pull request ever runs. The files are those of the merged tree
  (what lands), with a note on a file the default branch changed too, whose lines are then not the pull request's. The
  commit is recorded before anything is read and every read is of that commit, never of a branch name that may move, so the
  Reviewer reads the tree that is merged, whatever earlier work the card has.
- **The Reviewer reads all of it or nothing.** An approval of what was not read could be merged by the server, so nothing
  is cut: the whole diff (60,000 bytes) and every changed file whole (40,000 bytes each, 150,000 together, 60 files) fit,
  or the pull request is refused with the reason, and so is one that changes a binary file, which no text shows. A refused
  pull request is answered on the forge and the owner gets a `review_not_possible` inbox item; it is the owner's to read.
  The sizes are asked of the object headers (`git diff --raw`, `git ls-tree -l`) before any command loads a blob, every
  command runs with `core.bigFileThreshold=1m`, a limit on its address space (Unix) and a time limit, and a command that
  does not succeed is an error, never an empty change: a killed git cannot read as "it changes no file". Files that run on
  the forge (`.github/`, `.forgejo/`, `.gitea/`, `.gitlab-ci.yml`) come first.
- **The merge is of what was read.** Besides the pinned head, the server reads the pull request's base from the forge right
  before it merges, brings the clone up to date and merges only if the merge of the head into the default branch's tip
  now does what the merge into the tip that was read did (the same paths become the same files). Otherwise the round is
  `stale`, nothing is merged and the author is asked to write `/done` again.
- **The run is started restricted, once.** It is recorded as restricted on its row before anything else (`runs.restricted`),
  and nothing that happens later lifts it: closing the pull request, or the card moving, changes nothing. It has an empty
  directory as its working directory and no clone, a *checked token* on either harness (so the router decides every call
  of its answers; Pi-Bolt's bridge blocks none), no MCP servers, and it never runs on a runner. Pi-Bolt is started with
  `--no-tools` and Claude Code with `--tools ""`; neither flag could be tried against a real Pi here, so the router is the
  guarantee, not the flags. The router refuses *every* call of such a run, whatever it is (`restricted-review`, before the
  rules in prose), and refuses any call of any checked run whose arguments are not a JSON object, which a harness may
  repair and read where the check could not. Anything that cannot be decided is refused.
- **The narrower mode stays for other restricted uses** (`guard::restricted_refusal`): only the tools that read files, on
  paths that resolve inside a worktree with symlinks followed, no leading `@` (which Pi strips from a path), no absolute or
  `..` pattern, and arguments that are a JSON object. Nothing uses it for a review of a pull request now.
- **A round acts only while the owner lets it.** Starting a run, writing a review and merging are the server acting for the
  owner, so a held card and Pause all stop them on every path (the poll, the tick and the author's `/done`), right before
  they act. The merge is pinned to the head and to the branch: the poll reads the branch a request is against, and a
  request retargeted from the default branch is neither started nor merged, and its verdict is dropped. The default branch's
  tip is recorded with the round, so what was judged is known; the tip may move on, as it does for a person's review.
- **The prompt holds the issue only, as data.** The wiki pages an issue links, the lessons, and the comments are the
  organisation's and could be made to be quoted into a review by a stranger's text, so the prompt has none of them; the
  issue's title and body are in a fence. What the Reviewer wrote is redacted before it is posted.
- **The run starts in a directory of its own** under the system's temporary directory, with no `AGENTS.md` or `CLAUDE.md`
  above it, removed with the run.
- **Rounds are taken by one thread.** The poll and the engine both move rounds along, so a round changes state by a
  compare-and-set in the store (`starting`, `posting`), and only the thread that wins starts the run, writes the review or
  merges. A review the forge fails to take is not written again by itself (a forge such as GitLab may have written part of
  it): the round is `failed`, and the author may write `/done` for another. A merge that fails after the approval puts the
  card in Ready to merge with the reason, and the author's next `/done` for the same head tries the merge again.
- **The merge is of the head that was read.** The round is of the head its run reads (set when it starts), and the merge
  is refused unless the change's head is that one, besides the forge's own refusal of a pinned commit that moved.
- **A pull request with no checks at all is reviewed** and merged as one with green checks, and the pull request is told
  that no checks ran; waiting for nothing is what a person's merge does too. A repository whose checks start late can
  show none for a moment: leave `merge_reviewed` off where that matters.

## Consequences

- A review of a pull request cannot run the repository's tests, read a file it was not given or look anything up, so it
  judges by reading what it is given, and says what it could not check or what was cut. The checks of the forge are what
  runs the code, and they must be green before anything is merged.
- With `merge_reviewed` on, a pull request that the Reviewer approves wrongly is merged. The checks and the pinned head are
  what stand between a stranger's code and the default branch; a person who wants more turns the setting off.
- The restricted mode is for pull requests someone else opened; the Reviewer of the board's own changes still runs the
  repository's checks.
- A restricted run can still *ask* for a tool, if the flag that removes them does not work; the router removes the call
  from the answer, so the agent is told it was refused, and nothing runs. The restriction holds as long as the router does,
  which is the only place Pi-Bolt's calls are decided. If Pi did not know `--no-tools`, it would not start, and the review
  would fail rather than run unrestricted.
- A review is only as quick as a poll, or a tick of the card, after the run ends.
