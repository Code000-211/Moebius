# Automations

You want recurring work to run on its own, as a cron job would: every weekday morning triage the issues opened since
yesterday, every Friday look for outdated dependencies, every night sum up what the agents did. An automation does
that: when its schedule comes due, the server starts a chat with an agent and a prompt, the agent works as in any chat,
and what it offers lands in the Inbox.

An automation is:

- a **name**;
- a **schedule**: a cron expression of five fields (minute, hour, day of the month, month, day of the week) in the
  server's local time, such as `0 8 * * 1-5` for weekdays at 8:00;
- an **agent**, the Moebius agent or any agent of the configuration;
- a **prompt**, the first message of each chat;
- **where it works**: a repository (each chat gets its own branch, so what it builds is offered as a change), the
  harness configuration repository, or an empty directory, as a chat does;
- whether it is **on**.

The exact contract is in the [desktop API reference](../reference/desktop-api.md#post-v1automations).

## Make one in the app

Open **Automations** in the sidebar and press **New automation**. Name it, pick a schedule from **Repeats** (or type the
cron expression next to it), choose the agent and where it works, write the prompt and press **Save**. The list then
says when it fires next. **Run now** fires it at once, which is the quickest way to try the prompt; the firing shows
under **Firings**, and a click on it opens its chat.

## Make one from the command line

The same, as a person in a terminal or the Moebius agent in a chat (see [Command line](command-line.md)):

```sh
"$MOEBIUS" automation save --name "Morning triage" --schedule "0 8 * * 1-5" --repository 1 \
  --prompt "Triage the issues opened since yesterday: say which are ready to start and why."
"$MOEBIUS" automation list
"$MOEBIUS" automation run 1
```

`--agent` names another agent than the Moebius agent, `--harness` makes the chats work in the harness configuration
repository, `--off` saves it switched off, and `--id <id>` replaces an automation instead of adding one.
`"$MOEBIUS" automation delete <id>` deletes it; the chats it started stay.

Or with `curl` (see [Desktop API](desktop-api.md) for `$AUTH`, `$JSON` and `$API`):

```sh
curl -s -X POST -H "$AUTH" -H "$JSON" \
  -d '{"name": "Outdated dependencies", "schedule": "0 9 * * fri", "agent": "Coder", "repository": 1,
       "prompt": "Look for outdated dependencies and upgrade what is safe to upgrade."}' $API/v1/automations
curl -s -H "$AUTH" $API/v1/automations/1/firings
```

## What happens when it fires

1. Once a minute the server reads its local time. An automation that is on and came due since the minute before starts
   a chat with its agent and prompt; the chat is listed on the Chat page with the origin `automation`.
2. The agent answers as in any chat. A run never waits for a person: once the agent has answered and the chat waits for
   a message, the server ends the chat within the minute. With a repository, what the agent changed is then offered as
   a change, which waits in the Inbox to be merged.
3. Each firing is recorded with its chat, and published on the event stream as `automation_fired`.

## Try it

Make the automation with a schedule that comes due soon, or press **Run now**. Its firing appears with the status
`started` and its chat; open the chat to read what the agent did. If it changed files in a repository, the Inbox has a
`ready_to_merge` item for it once the chat has ended.

## What can go wrong

- **`skipped`**: the chat of the firing before still runs (its agent has not answered yet). A schedule that fires more
  often than the agent takes to answer skips every other firing; make it less frequent.
- **`failed`**: the chat could not start, for example because its agent can no longer run or the repository is gone.
  The reason is on the firing and on the automation's card in the list.
- **Nothing fires**: the automation is off, or the schedule is in the server's local time, which may not be yours. The
  list says when it fires next. A firing that came due while the server was not running is not made up.
- **The schedule is refused**: the message names the field that is wrong, such as `the hour 24 is not between 0 and
  23`.
