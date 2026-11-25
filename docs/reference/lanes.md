# The test lane

The exact contract of the test lane ([ADR 0035](../adr/0035-a-test-lane-measures-on-the-owners-own-gpus.md)): a named
machine on which a change is built, tested and measured at its pinned head once the Reviewer approved its code, and
what comes of the job. How to set one up is in [the guide](../guide/test-lane.md); the keys of `[[lanes]]` are in the
[configuration](configuration.md#lanes), the routes in the [desktop API](desktop-api.md#the-test-lane) and the
`moebius.lane_job()` call in the [column scripts](column-scripts.md). The code is `app/lane/`, `store/lane.rs` and
`config/lanes.rs` of `moebius-core`.

## What a lane is

A lane is an entry of `[[lanes]]`: the server itself, or a `[[runners]]` machine reached over SSH. It has

- **a lock**: one job runs at a time, the oldest queued job of the lane that is not stopped (see
  [Triggering](#triggering)) first; the lock is a statement of the store that moves a job to `running` only while no job
  of the lane is, so two threads cannot both start one;
- **a queue**: the jobs that wait, oldest first; a job that is blocked for the moment is passed over and stays queued;
- **what a report says of the machine**: `gpus`, `power_cap`, `rocm` or `driver`, and the container `image`;
- **an isolation**: `always`, `external_only` or `off`.

Which lane a repository uses is its setting (`PUT /v1/repositories/{id}/lane`), made by a person. A lane has a pause (see
[A lane that stopped itself](#a-lane-that-stopped-itself)). The jobs, the pause and the setting are kept in the store
(`lane_jobs`, `lane_state`, `lane_settings`).

## The job definition

`projects/<project name>/lane.toml` of the [harness configuration
repository](harness-configuration.md#project-settings): the steps a lane runs for every change of the project's
repositories. It is read from the settings when the job starts, never from the pull request, and a save through the API is
refused with `422` when it is not valid. A file with any mistake is refused as a whole, and the message starts with
`lane.toml:` and has every mistake in it, separated by `; `.

```toml
[[step]]
name = "build"
command = "./build.sh"
timeout_seconds = 1800
on = "both"

[[step]]
name = "measure"
command = "./bench.sh"
timeout_seconds = 900
[step.measure]
runs = 3
prefix = "MEASURE "
json = "out/bench.json"
parity_file = "out/result.bin"
```

Every `[[step]]` has these keys, and no other is accepted. There are at least one and at most 30 steps.

| Key | Required | Meaning |
|---|---|---|
| `name` | yes | 1 to 40 letters, digits, spaces, dots, hyphens and underscores; unique among the steps without regard to case. |
| `command` | yes | What `sh -c` runs in the checkout, once for each side the step runs on. Not empty. |
| `timeout_seconds` | no | How long one run of the command may take, 1 to 14400; default 900. The command is stopped past it and the step fails (`it ran past its limit of <n> seconds`). It is also cut to the time the job has left. |
| `on` | no | `head` (the default) runs the step in the checkout of the head only; `both` runs it in the checkout of the merge base first and then in the head's, such as a build the measurement needs on both sides. A measuring step runs on both whatever it says. |
| `measure` | no | A table that makes the step a measurement. |

A `[step.measure]` table has these keys, and no other:

| Key | Meaning |
|---|---|
| `runs` | How many runs of each side, 3 to 20; default 3. A measurement is at least three runs of each side. |
| `prefix` | The start of a line of the command's output that is a row, such as `MEASURE `. |
| `json` | A file the command writes, a path inside the checkout without `..`. It is removed before each run. |
| `parity_file` | A file whose bytes must be the same in every run, before and after; its `sha256` is compared. Optional. |

At least one of `prefix` and `json` is needed. A row of output is `<prefix><name> <value> [unit]` or
`<prefix><name>=<value> [unit]`. A JSON file is `{"rows": [{"name": "decode_tps", "value": 41.5, "unit": "tok/s"}]}` or a
flat object of names and numbers. A name is 1 to 80 characters of letters, digits and `_ . / : + - % @ [ ] ( )`, a unit
is up to 20 of the same, a value is a finite number, and a run has at most 500 rows; anything else is left out, since
what a step printed is data written by the code under review. A run with no row fails the step, and so does a row that is
not given in every run of both sides.

## The job

A job is one change of one card at one pinned head: the row `lane_jobs` has the repository, card and change, the lane,
`head` (the commit measured, `after`), `base` (its merge base with the default branch, `before`, read when the job is
queued) and `external` (whether the change is a pull request someone else opened).

What a job does, in order, on the lane's machine, each command with a time limit:

1. **Preflight.** `git`, `timeout` and `sha256sum` are there; for a job that has to be contained the container program
   is there and `--version` works, else the job ends `errored` with `the container runtime <name> is missing on the
   machine` (or `does not run`) and `the job was not run, since this lane never runs a job that must be isolated without
   the container runtime`. Nothing runs directly instead.
2. **The commits.** The head and the base are pushed from the server's clone into a bare clone on the machine
   (`<work_dir>/repos/repo-<repository id>.git`, made when missing) as `refs/lane/<job>/after` and `refs/lane/<job>/before`,
   over SSH for a runner and by path for the server. Each is fetched into `<work_dir>/jobs/<job>/after` and `.../before`
   and checked out detached, and the job is refused (`errored`) unless `git rev-parse HEAD` there is exactly the commit.
   The refs are removed again. Submodules are not fetched.
3. **The steps**, in order. A step runs in `after` (the head), and a step with `on = "both"` first in `before` (the merge
   base). A measuring step runs `runs` times: `before`, `after`, `before`, `after`, ..., so that what the machine does
   over time falls on both. Every command has the variables `LANE_SIDE` (`before` or `after`), `LANE_RUN` (the number of
   the run of a measurement, else empty), `LANE_JOB`, `LANE_HEAD`, `LANE_BASE` and `CI=1`, and `HOME` is
   `<job directory>/home` (`/tmp` in a container).
4. **The report** and the end of the job. The directory of the job is removed from the machine, the container of each
   command is removed whatever came of it.

The job never pushes to a forge and has no secret of the server. The server runs the machine's commands with only `PATH`
and `HOME` (and `SSH_AUTH_SOCK` for `ssh`) of its own environment, and a container has only the variables above.

### Isolation

| `isolation` | A job for a pull request someone else opened | A job for a change of the server's own |
|---|---|---|
| `always` | in a container | in a container |
| `external_only` | in a container | directly on the machine |
| `off` | directly on the machine | directly on the machine |

A **container** is made for every command: `run --rm --name moebius-lane-<job>-<n> --network none --read-only --cap-drop
ALL --security-opt no-new-privileges --user <user> --pids-limit 4096 --tmpfs /tmp:rw,exec,nosuid,size=8g -e HOME=/tmp -e
<LANE_...> --device <each device> --group-add <each group> -v <job directory>:/work:rw -w /work/<before or after> <image>
sh -c <command>`, started by `timeout -k 10 <timeout_seconds>` and followed by `rm -f` of the container. It has no
network, no credentials and no SSH key, an unprivileged user, a read-only image and the job's directory as the only place
to write, besides `/tmp`. An exit code of 125 (the runtime could not start the container: the image is missing, a
device is not there) ends the job `errored`.

A job **directly on the machine** runs `cd <job directory>/<side> && timeout -k 10 <timeout_seconds> sh -c <command>` as the
account the lane logs in as, which can reach what that account can. This is why the settings of a repository whose lane is
not `always` show a warning, and the daily digest tells it each day: the warning is the text of `GET
/v1/repositories/{id}/lane` `current.warning`.

## Triggering

A job is asked for by a card entering a column marked `lane = true` in `kanban.toml` (a column that counts as In review;
see [custom columns](column-scripts.md#custom-columns)), by `moebius.lane_job()`, by a person (`POST /v1/cards/{id}/lane`)
and, every few seconds, by the lane's pass for each card that sits in such a column and has no job for its head yet. A
card that follows a pull request someone else opened has no column script run, so for it the column and the pass are the
triggers.

A job is queued only when all of these hold, and the same checks are made again when it is about to start, so that
something that came while it waited stops it; a job that is stopped stays queued:

1. The repository uses a lane that the configuration has, and the project has a valid `lane.toml`.
2. The repository is not frozen, agents are not paused and the card is not held.
3. The change is open and its head is the head asked for.
4. **The review has come first**: a followed pull request has a round of the Reviewer, not a person's, for this head that
   ended `approve` or `approve_code` (and was not dropped as stale or failed); a change of Moebius's own has a latest
   review that approves.
5. The change touches no locked path (the built-in ones, the owner's `locked_paths` and the review brief; the paths the
   merge into the default branch changes). Paths that cannot be read refuse it.

A head has one job that counts: a job that is queued, running, `measured`, `passed` or `failed` for it stops another from
being queued, and so does one that `errored` in the last ten minutes. A job for another head of the same change is
another job.

## States

| State | Meaning |
|---|---|
| `queued` | Waits for the lock, or for what stops it to be lifted. |
| `running` | The lane is at `step`. |
| `measured` | Every step passed and a measuring step gave rows. |
| `passed` | Every step passed, and no step measures. |
| `failed` | A step failed in the checkout of the head: the change is to blame. |
| `errored` | The lane is to blame: the machine could not be reached or is not ready, the container runtime is missing or cannot start a container, the commits could not be sent or checked out, the merge base itself fails a step, the definition is missing or wrong, or the job took longer than the lane's `timeout_seconds`. |
| `cancelled` | A new push, a closed change or a person ended it first; a running one is stopped. |

## What comes of a job

A result is handed on once, and only for the head the change has now. A result that waits for a hold, a pause or a freeze to
be lifted is tried again by the pass; one for another head, or for a change that is closed, is dropped (it counts for
nothing).

- **`failed`**: one plain reply on the pull request (on the issue when the change is not offered yet), when the
  repository answers on the forge, `The test lane `<lane>` built and tested <commit> and stopped at the step **<name>**:
  <why>.`, then the last lines of the step's output as data, redacted and cut (40 lines, 300 characters of each, 3500 in
  all), in a fence longer than any run of backticks in it, then `Push a fix and the new version is read again.` The card
  goes back to the author's turn: a pull request someone else opened to `In review`, a change of Moebius's own to `In
  progress`, with the failure and the output as its note, which the Coder is told.
- **`measured`** for a followed pull request whose latest round ended `approve_code` for this head
  (`awaiting_measurement`): the report is kept as a measurement of the change at its head, `measurement_reports` with the
  measurer `lane:<lane>` and the job as its comment (a negative number, so that no comment of a forge has it), and a
  round of the kind `measurement` is added, which starts once the checks of the head are green and counts toward the round
  limit, as a measurer's comment does ([ADR 0033](../adr/0033-measurements-and-reviews-flow-without-the-owners-clicks.md)).
  The issue is told in one reply. If a round is going the result waits; if the round limit is spent the owner is asked to
  look. The Reviewer's prompt has the report as data, fenced, with the lane and the two commits; the Reviewer's `approve`
  accepts the measurement for the head (source `test lane`) and merges through the [merge
  gate](desktop-api.md#the-merge-gate). `approve_code` again, `changes` and the rounds are as for a measurer's comment.
- **`measured` for a change of Moebius's own**: no round reads it; the report is on the card and `moebius.lane_job()`
  gives a script the job. A person accepts a measurement as before.
- **`passed`**: nothing is handed on; the report is on the card.
- **`errored`**: nothing is written on the pull request, the card stays, and the pass of a lane counts it (see below).
- **A new push** (the poll sees a new head) cancels the change's queued and running jobs of the old head and makes the
  results of the old head count for nothing; the new head needs its own review verdict before it has a job.

## A lane that stopped itself

When the last three jobs of a lane that ended (not counting the cancelled) all `errored`, the lane pauses: its queued jobs
wait, no job starts, the Inbox gets an item `lane_paused` (`data.lane`, `data.reason`), and the log has a line
`lane_paused` for each repository that uses it. A person resumes it (`POST /v1/lanes/{name}/resume`), which resolves the
item and starts the count again. An agent does not.

## The report

The job's `report` (JSON) and the text the Reviewer reads, which is the report's `text`:

```json
{
  "machine": {"lane": "gpu1", "where": "gpu-box", "gpus": "2x MI300X", "power_cap": "750 W", "rocm": "6.2.1",
              "driver": null, "image": "rocm/dev-ubuntu-24.04:6.2", "isolation": "always", "contained": true},
  "head": "<sha>", "base": "<sha>",
  "steps": [{"name": "build", "state": "passed", "seconds": 212}],
  "measurements": [{"step": "measure", "runs": 3, "parity": "identical (out/result.bin, every run)",
                    "rows": [{"name": "decode_tps", "unit": "tok/s", "before": [40, 41, 42], "after": [44, 45, 46],
                              "median_before": 41, "median_after": 45, "change_percent": 9.76}]}],
  "text": "## Measurement of test lane gpu1\nbefore <sha>\nafter <sha>\nGPUs: 2x MI300X\n...",
  "failure": {"step": "build", "why": "it exited with code 2", "output": "<the last lines, redacted>"},
  "error": "why a job errored"
}
```

`failure` is there for a `failed` job, `error` for an `errored` one, `measurements` and `text` for a `measured` one. A step
is `passed`, `failed` or `skipped`. `text` is cut at 20000 characters, and every number in it and the rows is the raw
value a run gave.

## Limits

| What | Limit |
|---|---|
| A command's output kept | the last 256 KiB |
| Rows of one run | 500 |
| A measurement text | 20000 characters |
| A reply's output | 40 lines, 300 characters a line, 3500 characters |
| A job altogether | `timeout_seconds` of the lane |
| A step's command | its `timeout_seconds` (at most 14400) |
| A definition | 30 steps |

## The log and the digest

A job's end writes the log line `lane` (`number` the card's issue, `who` the lane, `detail` `<state>: <summary>`, `data`
`job`, `state`, `lane`, `head`, `card`, `title`), a lane's pause writes `lane_paused`, and a measurement handed to the Reviewer
writes `measurement_read` (as the measurement of a measurer's comment does). The [digest](factory.md#the-digest) of a
repository has them in `lane` of its section, and a line `Test lane <name> runs with isolation <value>` with the warning
for as long as the repository's lane is not `always`.
