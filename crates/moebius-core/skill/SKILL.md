---
name: moebius
description: Build on Moebius, the orchestration layer that drives coding agents from a repository's issues to changes ready to merge. Use it when asked for a column script, a rule, an agent, a plugin, a Pi extension or something that uses the desktop API, and whenever a question is about how Moebius behaves.
---

# Building on Moebius

You build things for Moebius that a person asks for: a column script, a rule, an agent, a plugin, a Pi extension, a
call of the desktop API. This skill holds the Moebius guide and its exact references.

1. Read the guide page for the task first. `guide/README.md` says which page that is: `guide/column-scripts.md`,
   `guide/workflows.md`, `guide/rules.md`, `guide/agents.md`, `guide/plugins.md`, `guide/extensions.md` or
   `guide/desktop-api.md`. Start from its complete example and change it.
2. Check every name and field you use in the reference (`reference/`), which is exact. Do not guess a function, a
   `card` field or a configuration key: if the reference does not list it, it does not exist.
3. Work in the repository the chat is in. What belongs to one repository goes in its `.moebius/` directory
   (`.moebius/<column script>.luau`, `.moebius/rules.md`). What is for the whole server (agents, server rules,
   configuration) is written as the file the person has to put on the server, and you say so.
4. Check a column script before you finish, with the command that came with Moebius, which runs it once against a
   made-up card and prints what it did or the error:

   ```sh
   "$MOEBIUS" check-script .moebius/ready.luau --column Ready
   ```

   `--column` is the column the script belongs to, `--title` and `--author` change the made-up card. Read the output
   and make sure the script does what was asked. Fix every error. Try the cases the person cares about by changing
   `--title` and `--author`.
5. Never weaken a rule: do not remove or loosen a rule, the checks or a refusal to make something pass. If a rule is
   in the way, say so and ask the person.
6. Keep what you build small, and finish with a short answer saying what you made and how you tried it. Do not commit
   or push: when the chat ends, what you left in the repository is offered to the person as a change, which they
   merge.
7. Say what you could not check. Moebius reads a script on its default branch, so it takes effect only once the person
   has merged the change.

Links in the guide to files outside this skill (the README, the glossary, the examples) are not here; the pages say
what they hold.
