---
name: moebius-operate
description: Read the state of a Moebius server and act on it with the moebius command line. Use it when the person asks what is going on (board, cards, inbox, chats, runs, usage, costs, repositories, research, grillings, Retros, automations) or wants something done to it (approve, merge, reject, retry, dismiss, add a repository, create an issue, start a research, make an automation that runs on a schedule).
---

# Operate Moebius

Goal: answer from the server, never from memory, and do to it exactly what the person asked for. Every command is one
request to the server's desktop API as the Moebius agent; what it decides is recorded as `agent:Moebius` with this chat.

## Read the state

```sh
"$MOEBIUS" board                 # every repository, column by column, with why each card waits
"$MOEBIUS" board demo            # one repository, by name or id
"$MOEBIUS" card 12               # one card: column, rounds, tokens, latest run, change, note
"$MOEBIUS" card 12 --history     # its timeline
"$MOEBIUS" card 12 --runs        # its runs
"$MOEBIUS" inbox                 # what waits for a person
"$MOEBIUS" chats
"$MOEBIUS" usage --days 7        # tokens, calls and model time by agent and model
"$MOEBIUS" repos
"$MOEBIUS" agents
"$MOEBIUS" doctor                # what is still missing for a first card
"$MOEBIUS" research              # then: "$MOEBIUS" research 3
"$MOEBIUS" grillings             # then: "$MOEBIUS" grilling show 2
"$MOEBIUS" retro                 # then: "$MOEBIUS" proposals
"$MOEBIUS" automation list       # then: "$MOEBIUS" api GET /v1/automations/3/firings
```

Add `--json` to any command to read fields (`"$MOEBIUS" board --json`). For anything without a command use
`"$MOEBIUS" api GET <route>`, for example `"$MOEBIUS" api GET /v1/runs/41/events` or
`"$MOEBIUS" api GET /v1/cards/12/history`.

## Act

| The person wants | Command |
|---|---|
| An inbox item answered as the app's button does | `"$MOEBIUS" inbox act <item> <approve\|reject\|merge\|retry\|confirm\|dismiss> --reason "..."` |
| A repository added | `"$MOEBIUS" repo add <name> <url>` (`--github owner/repo` for GitHub, `--gitlab group/project` for GitLab, `--forgejo owner/repo` for Codeberg or another Forgejo) |
| An issue (a card in the backlog) | `"$MOEBIUS" issue create <repository id> "<title>" --body "..."` |
| A card's title or body changed (on GitHub the issue too; "blocked by #n" in the body sets its blockers) | `"$MOEBIUS" api PUT /v1/cards/<id> --body '{"title": "...", "body": "...", "reason": "..."}'` |
| The logins a repository on a forge trusts | `"$MOEBIUS" api PUT /v1/repositories/<id> --body '{"trusted": ["leon"], "reason": "..."}'` (the whole list) |
| A draft's brief changed before it is confirmed | `"$MOEBIUS" api PUT /v1/research/<id> --body '{"goal": "...", "measure": "...", "better": "lower", "token_budget": <n>}'` |
| A research brief | `"$MOEBIUS" research new --repository <id> --goal "..." --measure "<command>" --better lower --token-budget <n>`, then `"$MOEBIUS" research confirm <id>` once the person agrees |
| A grilling's issues made into cards | `"$MOEBIUS" grilling confirm <id>`; or `"$MOEBIUS" grilling dismiss <id>` |
| A Retro proposal decided | `"$MOEBIUS" proposal merge <id>` or `"$MOEBIUS" proposal dismiss <id>` |
| Recurring work on a schedule (an automation) | `"$MOEBIUS" automation save --name "..." --schedule "0 8 * * 1-5" --prompt "..." --agent <name> --repository <id>` (cron in the server's local time; `--harness` for the harness configuration, `--id <id>` to replace one, `--off` to keep it off), then `"$MOEBIUS" automation run <id>` to try it once |
| An automation removed | `"$MOEBIUS" automation delete <id>` |
| A chat with another agent | `"$MOEBIUS" chat start "<message>" --agent <name> --repository <id>` |
| Another chat's work offered as a change, the chat going on | `"$MOEBIUS" chat offer <id> --reason "..."` (once its agent has answered) |
| A run steered or stopped | `"$MOEBIUS" api POST /v1/runs/<id>/steer --body '{"message": "..."}'`, `"$MOEBIUS" api POST /v1/runs/<id>/stop --body '{"reason": "..."}'` |

The inbox item's kind decides which actions it takes; `inbox act` refuses any other and lists those it takes.

## Try it

Read the state again after every action (`"$MOEBIUS" board`, `"$MOEBIUS" inbox`) and say what changed. When you report a
figure, say which command it came from.

## What can go wrong

- `moebius: the server answered 4xx: ...` names the problem; read the message before you try anything else.
- Merging, rejecting, dismissing, stopping and restarting are hard to undo: do them only when the person asked for
  exactly that.
- `secrets set` and the terminals answer 403 for you: storing a key is for the person.
- A card that does not move: its `waiting` reason in `"$MOEBIUS" card <id>` says why; a script error starts with
  `script error:`.

## Details

The `moebius` skill holds the exact pages: `reference/cli.md` (every command and the route it calls),
`reference/desktop-api.md` (every route and body), `guide/workflows.md` (what each column does and what a person can
do), `guide/autoresearch.md`, `guide/grilling.md` and `guide/automations.md`.
