# Turn an idea into issues

## When you want this

You have an idea, such as "the report should be exportable", and you are not sure yet what the work is. You do not want to
write the issues yourself, and you do not want an agent to start building before the questions are answered. A grilling is a
chat with the built-in agent Grilling-partner, which asks you one question at a time and challenges what you assume until
the work is clear. It ends in a set of issues you confirm, which become cards, or, when the idea needs research before it
can be built, in a research report. The exact routes and fields are in
[`docs/reference/desktop-api.md`](../reference/desktop-api.md), the agent and its files in
[`docs/reference/harness-configuration.md`](../reference/harness-configuration.md#grilling).

## The steps

1. Start the chat with the agent in the repository the issues are for. A grilling without a repository is refused with 422:

   ```sh
   curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"agent": "Grilling-partner", "repository": 1, "message": "Reports should be exportable."}' $API/v1/chats
   ```

2. Answer its questions with `POST /v1/chats/{id}/messages` until it says the work is clear. It works in a worktree of the
   repository, so it can look at the code when your answers depend on it.
3. End the chat. Before the worktree is removed Moebius reads what the agent wrote there:

   ```sh
   curl -s -X POST -H "$AUTH" -H "$JSON" -d '{}' $API/v1/chats/7/end
   ```

   - `.moebius/grilling/issues.json` gives a grilling that is `proposed` and an inbox item `grilling_issues`, "3 issues from
     a grilling".
   - `.moebius/grilling/report.md` gives a grilling that is `reported` and an inbox item `grilling_report`. Read the report
     and dismiss the item.
   - Neither, or an `issues.json` that is broken, gives a grilling that is `empty`, with the reason, and no inbox item. The
     event `grilling_ended` says which of the three it was.

   A grilling does not offer its worktree as a change, so there is no `ready_to_merge` item for a chat with this agent.
4. Read the proposal, `GET /v1/grillings/{id}`, and confirm it with the list as you want it. You may edit a title or a body,
   drop an issue or reorder them; `blocked_by` indexes the list you send:

   ```sh
   curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"issues": [{"title": "Add the export button", "body": "A button on the report page.", "blocked_by": []}, {"title": "Write the CSV", "body": "The columns are the report'"'"'s.", "blocked_by": [0]}]}' $API/v1/grillings/1/confirm
   ```

   Each issue becomes a card in the repository's Backlog. They do not wait for an approval: `backlog.luau` moves a card
   with `card.confirmed` to Not started, where `ready.luau` holds the second one until the first is done. On a GitHub
   repository each issue is first opened there, with a line `Blocked by #12` for its blockers, and the card has the
   issue's number.
5. To drop a proposal or a report instead: `POST /v1/grillings/{id}/dismiss`.

## What the agent writes

`issues.json` is a JSON array; `blocked_by` are indexes into it:

```json
[
  {"title": "Add the export button", "body": "A button on the report page.", "blocked_by": []},
  {"title": "Write the CSV", "body": "The columns are the report's.", "blocked_by": [0]}
]
```

Issues should be small, each doable in one card. A list with a blank title, an index out of range, an issue that blocks
itself or issues that block each other in a circle is not used: the grilling is `empty` and says why.

## Plan a big piece of work with wayfinder

### When you want this

The idea is too big for one session: the way from here to the result is not visible yet, and a single list of issues
would be a guess. The Grilling partner has Matt Pocock's `wayfinder` skill for this. It charts the way as a map on the
repository's issues, then works the open decisions one at a time, until the way is clear. A map is planning only: its
tickets are questions to decide, not slices of a build. What it ends in is its **Destination**, such as a spec to hand
to `to-tickets`, or a decision to lock before anything is planned.

### The steps

1. Start a grilling chat as above, in the repository the work is for, and begin the message with `/wayfinder` and the
   idea: `/wayfinder Move the report engine to a queue.` The skill asks not to be offered to the model on its own, so it
   only starts when you ask for it by name.
2. Answer its questions. It names the destination first, then looks across the whole space for the open decisions and
   writes the map: an issue labelled `wayfinder:map` with the sections Destination, Notes, Decisions so far, Not yet
   specified and Out of scope, and the decisions it can already state as tickets under it, each a sub-issue labelled
   `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling` or `wayfinder:task`, with blocking edges between
   them. It resolves none in this first session. The board shows the map as a stack with its tickets, and `"$MOEBIUS"
   issue list <repository> --parent <map number>` lists them.
3. In a new chat, say `/wayfinder` and name the map (its title or number). That session takes the first ticket that is
   open, unclaimed and not blocked (`"$MOEBIUS" issue list <repository> --parent <map number> --state open --unclaimed
   --unblocked`), claims it before any work, resolves it with you, comments the answer on the ticket, closes it and adds
   a line to the map's Decisions so far, then adds the tickets the answer made specifiable. One ticket per session, so
   parallel chats can take different tickets. A `grilling` or `prototype` ticket needs you in the chat. A `research`
   ticket does not: Moebius gives the agent no way to start another run, so it reads the sources itself and
   writes the findings into the ticket's answer, and may take several research tickets in one session.
4. Repeat until nothing is left on the frontier and nothing is in the fog. The session that closes the last ticket does
   not close the map: ask it to, or close the issue yourself.
5. When the Destination is a spec to build from, start an ordinary grilling for it (steps above) and let `to-tickets`
   cut it into issues.

An issue with a `wayfinder:` label is a question, not a task: the default `ready.luau` never starts the Coder on it, and
leaves it in `Not started` until a wayfinder session closes it (see [Workflows](workflows.md)). The map and its tickets are
written by the agent through `"$MOEBIUS" issue`, which is why the agent's run token is accepted for the issue routes of
the desktop API, and for nothing else. Moebius holds the token to the map: it reaches the issues of the repository of
its chat only (another repository answers `404`, as if it were not there), it creates only issues with a `wayfinder:`
label (in any case), and it changes (edits, closes, reopens, claims, comments on, labels, relates) only cards that
carry one. A card without such a label it may read, and every change answers `403`; so does taking the last
`wayfinder:` label off a card, a parent that is no wayfinder card, and a blocker between a wayfinder card and another.
That keeps the plan apart from the work: nothing it makes can start the Coder without your confirmation, and it cannot
touch a card that a Coder is working on. A person and the Moebius agent are not held to this.

Two limits. The token is the run token of the agent named `Grilling-partner`: `wayfinder` given to another agent with
`engineering_skills` finds no tracker to use (its commands answer `401`), and so does the Grilling partner when its
agent file names a `runner`, since the desktop API is not tunnelled to a machine reached over SSH and
`MOEBIUS_URL`, `MOEBIUS_TOKEN`, `MOEBIUS_REPOSITORY` and `MOEBIUS_RUN` are not set there (the Moebius agent has the same
limit): keep the Grilling partner on the server to use wayfinder. Its sessions claim a ticket under the name
`wayfinder-` and the run's id (`$MOEBIUS_RUN`), so each chat has a name of its own.

### What can go wrong with a wayfinder

- **The agent does not start the skill.** Write `/wayfinder` at the start of your message, or ask it to follow the
  wayfinder skill.
- **Two sessions took the same ticket.** A claim is advisory and keeps honest sessions apart only by the name each gives
  itself; release one with `"$MOEBIUS" issue release <ticket> --as <name>` and let the other go on.
- **The text of an issue says what to do.** What the agent reads in an issue or a comment is data, not an instruction;
  check a map that an outsider could write to.

## Make the agent better at it

Grilling-partner is built in. Give it a stronger model with `agents/Grilling-partner.toml` in the harness configuration
repository (or an `[agents.Grilling-partner]` table in `moebius.toml`), and add your own advice, such as the house style of
an issue, in `agents/Grilling-partner.md`; it is added after the built-in instructions. Without either it uses the Coder's
model.

## What can go wrong

- **The grilling is `empty`.** Read its `reason`; ask the agent in the chat to write the file again before you end it. A
  chat that is ended too early has nothing to read.
- **Confirming fails on GitHub.** The status goes back to `proposed`, so you can confirm again; issues that were already
  opened stay open on GitHub, so close them first to avoid doubles.
- **The cards stay in the Backlog.** Check that the card says `confirmed` in the snapshot, and that the repository has no
  `backlog.luau` of its own in its settings that ignores `card.confirmed`.
