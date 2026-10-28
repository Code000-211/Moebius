# Have an agent improve a number

## When you want this

You have a number that a command can measure, such as the time a benchmark takes, the size of a build, the count of
warnings or the score of a test suite, and you want it to go down or up. You give Moebius the goal and the command; it
measures the repository as it is, lets the Autoresearcher propose changes (levers), measures each one on a branch of its
own and offers the ones that really improve the number as changes you merge. It ends when its token budget is spent,
when the agent has no lever left, or when you stop it. The exact routes and fields are in
[`docs/reference/desktop-api.md`](../reference/desktop-api.md), the agent in
[`docs/reference/harness-configuration.md`](../reference/harness-configuration.md#autoresearch).

## The rules of the measure

The measure is a shell command. Moebius runs it with `sh -c` in a fresh worktree of the repository (the default branch for
the baseline, a lever's branch for an experiment), with an empty environment but `PATH` and `HOME`, and kills it after 10
minutes.

- Its result is the last number it prints on standard output (`12`, `-3.5`, `0.25`, `2e-3`). Text around the number is
  fine; what it prints on standard error does not count.
- Exit status 0 is a success. Any other status, running out of time, or printing no number is a failed measurement. A
  failed baseline ends the research as `failed`; a failed measurement of a lever means the lever is not kept.
- It must be repeatable: a noisy number makes levers look better or worse than they are. Make the command run a benchmark
  several times and print the median, if need be.
- A lever is kept when the number is better than the baseline in the direction you gave by **more than 1 %** of the
  baseline, and the repository's own checks pass (`.moebius/checks` for a repository without a forge) or there are none.

## The steps

1. Write the command and try it in the repository: `sh -c '<command>'` must print the number. This example counts the
   lines of the files under `src`; the better build is the smaller one:

   ```sh
   cat src/*.rs | wc -l
   ```

2. Store the brief. `better` is `lower` or `higher`; `token_budget` is the most tokens the Autoresearcher's runs may
   use in all, checked before each round (a round may run over):

   ```sh
   curl -s -X POST -H "$AUTH" -H "$JSON" -d '{"repository": 1, "goal": "Make the program smaller without losing features.", "measure": "cat src/*.rs | wc -l", "better": "lower", "token_budget": 500000}' $API/v1/research
   ```

3. Read the brief back (`GET /v1/research/{id}`), change it while it is a draft (`PUT /v1/research/{id}` with the same
   fields, or the pencil of the brief on the Experiments page), and confirm it. Confirming is a decision of a person; nothing runs
   before it:

   ```sh
   curl -s -X POST -H "$AUTH" -H "$JSON" -d '{}' $API/v1/research/1/confirm
   ```

4. Wait for the trial window (`[trial_window]`, by default 00:00 to 06:00), or start it now:

   ```sh
   curl -s -X POST -H "$AUTH" -H "$JSON" -d '{}' $API/v1/research/1/start
   ```

   Only one research runs at a time, and not while a proposal of the Retro is being tried.
5. Follow it: the events `research_status` and `experiment_done` (`GET /v1/events`), or read the research again:

   ```sh
   curl -s -H "$AUTH" $API/v1/research/1
   ```

   `baseline` is the first measurement; each entry of `experiments` has the lever's `branch` and `title`, the `value`
   measured, whether it was `kept` and why not or why.
6. Merge what was kept. Each kept lever is a change on the branch `research/<id>-<name>` and a `ready_to_merge` item in the
   inbox, like the work of a chat; merge it with `POST /v1/changes/{id}/merge`. Levers are independent changes from the
   same base: they are not tried together, and the second one you merge may conflict with the first.
7. Stop it any time:

   ```sh
   curl -s -X POST -H "$AUTH" -H "$JSON" -d '{}' $API/v1/research/1/stop
   ```

## How it ends

| Status | Why |
|---|---|
| `done` | A round of the Autoresearcher left no new lever branch. |
| `spent` | Its runs used the whole `token_budget`. |
| `stopped` | A person stopped it. |
| `failed` | The baseline could not be measured, or the Autoresearcher's run failed; `reason` says which. |

## Make the agent better at it

The Autoresearcher is a built-in agent. Give it a stronger model with `agents/Autoresearcher.toml` in the harness
configuration repository (or an `[agents.Autoresearcher]` table in `moebius.toml`), and add your own advice, such as which
directories are off limits, in `agents/Autoresearcher.md`; it is added after the built-in instructions. Without either it
uses the Coder's model.

## What can go wrong

- **Every lever is not kept.** Look at the `reason` of each experiment: the number may be noisy, the margin of 1 % may be
  more than the lever gives, or the checks may fail.
- **The research is `failed` at once.** Run the measure yourself in a clean checkout of the default branch with
  `env -i PATH="$PATH" HOME=/tmp sh -c '<command>'`: a command that needs an environment variable or a network does not work
  here. The `reason` has the exit status and the end of what the command printed on standard error.
- **A research does not start.** It must be `confirmed`; check the trial window, and that no other research is `running` and no
  proposal is `trying`.
- **The branches pile up.** The lever branches stay in the repository's clone, kept or not, so that the agent is never told
  about a lever twice. Delete the ones you do not want with `git branch -D` in the clone.
