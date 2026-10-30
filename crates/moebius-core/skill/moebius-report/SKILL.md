---
name: moebius-report
description: Send a fix or a problem in Moebius itself to Moebius's official repository as a pull request or an issue. Use it when the person has fixed something in Moebius's own code or files, or when you meet a problem in Moebius (not in the person's project) that the person may want reported. Always ask first.
---

# Report to Moebius's own repository

Goal: what is learned about Moebius itself reaches https://github.com/Code000-211/Moebius, but only when the person says
yes. A fix goes as a pull request, a problem as an issue. A problem in the person's own project is not for this skill.

## Is it Moebius?

It is Moebius when the code, a built-in skill, a document or the behaviour that is wrong belongs to Moebius: the server
(its files are in "Where Moebius runs" of your instructions), the desktop app, the `moebius` command or the documents of
`docs/`. It is not Moebius when it is in the person's repository, their column scripts, rules, agents or settings.

## Steps

1. Collect the facts the report needs: the version and system of the server and the app from
   `"$MOEBIUS" api GET /v1/server` and `"$MOEBIUS" api GET /v1/client` (the second is a 404 until the app has reported),
   what the person did or asked for, what happened, and what they expected. Check that the problem is not already an
   issue: `gh issue list --repo Code000-211/Moebius --search "<words>"`, when `gh` is there.
2. Ask the person, in one short question, whether to send it, and show them the title and the text you would send. Send
   nothing before a clear yes. A no ends the matter; do not ask again for the same thing.
3. Leave out every secret: no token, key, password or secret value, no contents of `secrets.toml` or the token file,
   and no private path, host name or repository name beyond what a reader needs to understand the problem. Read the
   text through once for them before it goes.
4. For a problem, an issue: a title in one line, then steps to reproduce, what happened, what was expected, the version
   and the system. `gh issue create --repo Code000-211/Moebius --title "<title>" --body-file <file>`.
5. For a fix, a pull request from a fork: `gh repo fork Code000-211/Moebius --clone=false`, push a branch of the fix
   to the fork, then `gh pr create --repo Code000-211/Moebius --head <you>:<branch> --title "<title>" --body-file <file>`.
   The body says what changed and why, and how it was tried. The repository's `CLAUDE.md` says how its code is checked
   (`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`): run them
   first when the fix is code, and say so if you could not.
6. When `gh` is missing or not logged in, do not try to log in for the person: give them the title and the text to paste
   at https://github.com/Code000-211/Moebius/issues/new and say so.
7. Tell the person what was sent and give the link.

## Details

In the `moebius` skill: `reference/desktop-api.md` (`GET /v1/server`, `GET /v1/client`).
