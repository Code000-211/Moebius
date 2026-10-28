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
   with `card.confirmed` to Ready, where `ready.luau` holds the second one until the first is done. On a GitHub
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
  `.moebius/backlog.luau` of its own that ignores `card.confirmed`.
