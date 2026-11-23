# Helpers take issues with commands in comments

[0027](0027-working-with-people-on-other-servers-through-the-forge.md) lets servers keep out of each other's way
through the forge: taking an issue assigns the person's login, and an issue someone else is assigned to is left alone.
That needs a helper's server to assign, and on GitHub only people with the Triage role or higher may assign. The
built-in roles cannot be edited without Enterprise. Anyone who has commented on an issue, however, may be assigned. So
the owner's server can assign on behalf of a helper who has no role and only comments.

## Considered Options

- **Give helpers a role.** Triage lets them assign anything, labels and closing included, and a public repository
  would have to hand it out to strangers one by one.
- **A web page or bot of the owner's that helpers click.** A second service with its own login, and nothing on the issue
  to read for people who do not use Moebius.
- **Commands in comments, read by the owner's server.** The helper writes `/take`; the owner's server, which has the
  token, assigns them and says so on the issue. The forge stays the only channel, and the comment is the record.

## Decision

- **`/take`, `/release` and `/done` as the first word of a line in an issue comment.** Case does not matter. Lines
  quoted with `>`, indented as code and inside fenced blocks are not commands; the first command of a comment is the
  one that counts. The server reads new comments at each poll, and from a webhook when the forge is set to send issue
  comment events ([0012](0012-forge-webhooks-wake-the-poll.md); any delivery wakes the poll).
- **Each comment is read once.** The newest comment id read for an issue is kept in the store (`command_seen`) and is
  written before the command is acted on, so a failed command is not retried by the next poll. Comments from before the
  commands were turned on (`repositories.commands_since`) are never commands. The server's own comments, which the
  store knows as written through Moebius, are never read.
- **Off by default, and for the owner's server only.** `commands` is a setting of the repository, kept with the other
  settings for working with others. Two servers reading the same comments would both answer.
- **Anyone who can comment may use them**, so on a public repository everyone. There is no setting for who: no second
  value is planned, and a setting nobody can choose is noise. The safeguards are what limits it:
  - a login holds at most `command_limit` open issues it took by command (default 3, 1 to 50);
  - `/take` needs an issue that is open and that nobody is assigned to (a login of this server's people counts), and,
    while the repository marks ready issues, one that carries the ready label;
  - `/release` and `/done` are for the issue's assignee; for anybody else the server says who it is;
  - helpers can never close anything. `/done` asks the owner (a `done_asked` inbox item and the "Moebius needs you"
    block of the card), who closes the issue, gives it back to the assignee or releases it; when a followed pull request
    is already the change of the card, the server only says thanks;
  - the owner can always release from the card (`POST /v1/cards/{id}/release-taken`).
- **Always answer.** `answer_on_forge` (default on) has the server write the result of every command as a short comment,
  and the same for the owner's answers on the card. It never answers a comment of its own, and its answers contain no
  command, so two servers cannot loop. Turned off, the commands still work and nothing is written.
- **The card shows it.** A card taken by command is `taken_by` the login as before, with `taken_by_command` set; it is
  greyed and left alone like any issue someone else took, and says "Taken by @alice with /take".
- **Through `repos/forge.rs`.** `assign_issue`, `unassign_issue`, `comment_issue` and `set_issue_open` are the forge's
  own, so GitLab and Forgejo work as far as their rules do: GitLab assigns only project members and Forgejo only
  collaborators, so for a helper who is neither the answer says the login could not be assigned. The roles and the
  idea that a commenter may be assigned are GitHub's.

## Consequences

- A command is only as quick as a poll, unless a webhook wakes it.
- The server's token is what assigns, so a helper is assigned though their own account could not. The limit and the
  ready label are what keep a stranger from taking every issue; the owner may release from the card.
- A comment that is edited after it was read is not read again, and a comment deleted before the poll is never read.
- Issues are matched by comment id, so ids must grow with time, as they do on the three forges.
- Turning the setting on is not retroactive: comments from the same second as the change or earlier are not commands.
