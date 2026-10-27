# Improvement runs as a daily Retro, not as a per-task recursion

Moebius improves how its harnesses work through a Retro that runs once a day over all of that day's runs, analyses the errors
that happened (failed tool calls, aborted runs, CI failures, review feedback) and proposes concrete changes. It does not run
Harness²'s Propose–Probe–Compose recursion on every task, which costs about 4k runs per task and makes every card several
times more expensive; the daily Retro spends that effort once, on the errors that actually occurred.

## Consequences

- A Retro may propose a change to anything (harness configuration, a harness's code, Moebius itself), but nothing is changed
  autonomously. Proposals are tried out on their own branches inside a set time window, and a proposal reaches the user only
  when it did better on the same task the error happened in. Adopting it is always the user's decision.
