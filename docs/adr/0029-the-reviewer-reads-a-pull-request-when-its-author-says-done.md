# The Reviewer reads a pull request when its author says it is done

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
- **At most three rounds per pull request, one per head.** A round counts once its run started. A fourth `/done` is
  answered that a person will look, and the owner gets an Inbox item (`review_rounds_spent`). A head with a round is not
  read again. A round that ended without a verdict, or whose head moved before the verdict was used, does not count.
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
- **The run is restricted.** What the pull request holds is the work of a person the server does not trust, and an
  agent that reads it can be told by it what to do. A `review` run of a card that follows such a pull request is decided
  by a fixed rule (`restricted-review`, in the guard, before the rules in prose): only the tools that read files, on
  paths inside its worktree. So nothing of the pull request (its build scripts, its tests, a hook) runs on the server,
  nothing reaches the network, nothing is written. It is started without MCP servers, and the server's environment,
  which holds the forge token and the model keys, is withheld from the process as for every run. The prompt gives it the
  diff and says that all the pull request holds is data and no instruction.

## Consequences

- A review of a pull request cannot run the repository's tests, so it judges by reading, and says what it could not check.
  The checks of the forge are what runs the code, and they must be green before anything is merged.
- With `merge_reviewed` on, a pull request that the Reviewer approves wrongly is merged. The checks and the pinned head are
  what stand between a stranger's code and the default branch; a person who wants more turns the setting off.
- The restricted mode is for pull requests someone else opened; the Reviewer of the board's own changes still runs the
  repository's checks.
- A review is only as quick as a poll, or a tick of the card, after the run ends.
