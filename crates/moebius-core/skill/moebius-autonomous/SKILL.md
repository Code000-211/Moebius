---
name: moebius-autonomous
description: How to follow the engineering skills (tdd, diagnosing-bugs, code-review, grilling, to-tickets) inside Moebius, where a card run has no person to ask and the card's issue is the spec. Use it whenever one of those skills tells you to ask the user, to use an issue tracker, or to run a setup skill.
---

# Engineering skills in Moebius

The skills `tdd`, `diagnosing-bugs`, `code-review`, `grilling` and `to-tickets` come from Matt Pocock's skills. They are
written for a person working with an agent. In Moebius you are an agent in a run: read the skill, follow it, and use this
page wherever the skill assumes something Moebius does not have. This page wins over the skill where they differ.

## No person in a card run

A run of the Coder or the Reviewer never waits for anyone: nobody answers a question. So where a skill says to ask,
confirm, show or check with the user, do not ask. Decide yourself, as the skill would have the person decide, and write
the decision and the reason in one line in your last answer, the run's summary (and in the commit message, if you commit
yourself). Examples:

- `tdd` asks you to confirm the seams with the user: choose the seams yourself, the public interfaces where the issue's
  behaviour can be seen, and name them with what each catches and misses.
- `diagnosing-bugs` asks you to show the ranked hypotheses before testing them: write the ranked list down in your
  answer and go on with your ranking. It asks you to stop and ask for access, a captured artifact or permission when you
  cannot build a feedback loop: stop, and end your answer with what you tried and what is missing, rather than guessing a
  fix. Do not use the human-in-the-loop script, which needs a person to click.
- `code-review` asks for the fixed point and the spec: see below.
- Anything else that asks the user: take the choice you would recommend, and say so.

Never invent an answer the issue or the repository does not give you when a wrong guess would be costly or hard to undo;
say in the summary what you could not know and what you assumed.

## The issue is the spec

Moebius has no issue tracker configuration and no triage labels. The issue the prompt starts with (its number, title and
body, and the notes from the last round) is the spec, the originating issue and the tracker. Do not fetch issues, do not
publish tickets to a tracker and do not apply labels. `GLOSSARY.md`, `docs/` and the architecture decision records of the
repository, where it has them, are its domain language and its standards.

Never run `/setup-matt-pocock-skills`, and never tell anyone to: it does not exist here. A skill that says it needs what
that skill sets up has what it needs in the issue and the repository.

Skills that are not given to you (`codebase-design`, `code-review` for the Coder, the others) are not available: do what
the sentence needs without them. The Coder does not refactor beyond the issue; the Reviewer's findings cover it.

## The Reviewer and `code-review`

- The fixed point is the merge-base of the branch and the repository's default branch:
  `git merge-base HEAD origin/HEAD`, or against `main` or `master` when `origin/HEAD` is not set. Check it resolves and
  that the diff is not empty. Never ask for one.
- The spec is the issue in the prompt. It is always there, so the Spec axis always runs.
- You may not have sub-agents. Then do the Standards review and the Spec review one after the other yourself, keeping
  them apart as the skill says, and report them under `## Standards` and `## Spec`.
- In a workflow that reviews along two axes (the default), each axis is a run of its own, and your prompt says which
  one: a `review-standards` run does the Standards review only and a `review-spec` run the Spec review only, one
  yourself, with no sub-agents. Report as the prompt says, each finding as `FINDING blocking:` or `FINDING judgement:`
  with an `EVIDENCE:` line quoting the hunk or the line of the issue (a finding without evidence is dropped), instead of
  the skill's headings. The skill's smell baseline stays: its smells are judgement calls.
- Your instructions for the run still hold over the skill's report format: do not edit any files, and end as they say
  (the last line `VERDICT: approve` or `VERDICT: changes`).

## The Coder

- Moebius commits what you leave in the worktree and runs the checks; do not push. Run the repository's own checks and
  tests yourself before you stop.
- A test the skill asks for belongs in the repository's own test setup, like the tests around it.
- Debug output and throwaway files are removed before you stop, as `diagnosing-bugs` says.

## The Grilling partner

A person is in the chat, so `grilling` and the questions of `to-tickets` go to them, as the skills say. Where the
Grilling partner's own instructions differ from a skill, for example in how many questions to ask at a time, the
instructions win. Look up in the worktree what you can look up yourself.

`to-tickets` does not publish anywhere. Its tickets are the entries of `.moebius/grilling/issues.json`, as your
instructions describe: `title`, `body` (what to build and the acceptance criteria, without file paths) and `blocked_by`,
the indexes in the list of the tickets that block it, in dependency order. Write the file when the person has approved
the breakdown, and write no tracker files, no `.scratch/` files and no labels. Moebius shows the person the issues to
confirm before a card is made.

## Secrets

Redact secrets as `diagnosing-bugs` says in everything you write: the summary, a commit message, a ticket. Moebius also
removes secrets it recognises, but do not rely on that.
