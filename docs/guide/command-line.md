# Command line

## When you want this

You want to do from a terminal or a script what the desktop app does: read a board, merge a change, add a repository,
change an agent, a rule or a setting. Or you want to know what the Moebius agent does when it operates Moebius for you,
since it uses the same commands. Every command is one request to the [desktop API](desktop-api.md); the exact list, with
the route each one calls and what it writes, is in the [command line reference](../reference/cli.md).

## The steps

1. Have a server running (`moebius serve`) and know its address and token. On the machine where you use the desktop
   app, the command line finds both in the app's saved settings. In the directory of `moebius.toml` it reads them from
   the file. Elsewhere, set them:

   ```sh
   export MOEBIUS_URL=http://127.0.0.1:7171
   export MOEBIUS_TOKEN="$(cat .moebius/token)"
   ```

2. Read before you change. `board` shows every repository's columns and cards, `inbox` what waits for a person,
   `chats`, `usage`, `agents`, `rules` and `settings` the rest:

   ```sh
   moebius board
   moebius inbox
   moebius settings
   ```

3. Change what you want with the command for it. A command that changes something records the decision as yours,
   the same as the button in the app:

   ```sh
   moebius inbox act 12 merge --reason "reviewed"
   moebius settings set board.round_limit=7 budgets.day_tokens=5000000
   moebius issue create 1 "Cache the model downloads" --body "Downloads are slow."
   ```

4. For anything without a command of its own, call the route with `api`; the body is JSON, a file, or `-` for
   standard input:

   ```sh
   moebius api GET /v1/judgements
   moebius api PUT /v1/skills/house-style --body '{"description": "Write plainly.", "text": "Short sentences."}'
   ```

5. In a script, add `--json` and read the fields you need; a failed request exits with 1 and says why on standard
   error:

   ```sh
   moebius --json board | jq '.boards[].repository.name'
   ```

## How the Moebius agent uses it

In a chat with the Moebius agent, its runs have `MOEBIUS_URL` and `MOEBIUS_TOKEN` set by the server, the token being the
chat's own run token. It calls the same commands as `"$MOEBIUS" board` through its `bash` tool, so the guard checks each
call like any other. The desktop API accepts that token only for the Moebius agent and records every decision it makes
as `agent:Moebius` with the chat's run, so you can tell its decisions from yours in the judgements. It cannot store a
key (`secrets set`) or open a shell in a worktree or on the server.

Ask it for anything the app does, in your words: "what is stuck on the radiance board?", "give the Coder a token budget
of two million", "add a rule that no run edits the migrations". It reads the state, does what you asked, reads it back,
and says what it changed.

## How to try it

`moebius board` against your server: it lists your repositories. `moebius settings` shows every key with its value and
which are defaults. After a change, read the same thing again; the command that changed it also writes the answer.

## What can go wrong and how you see it

- `moebius: no server to connect to`: none of `--server`, `MOEBIUS_URL`, the app's saved settings or a `moebius.toml` in
  the current directory names one. Set `MOEBIUS_URL` and `MOEBIUS_TOKEN`.
- `moebius: the server answered 401`: the token is not the server's (it changes when the token file is replaced).
- `moebius: the server answered 422: <message>`: the request was refused, and the message says why, as in the app.
- `--server` without `--token`: refused, so that a token never goes to a server it does not belong to.
