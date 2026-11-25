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

Besides the steps the file has one top-level key:

| Key | Meaning |
|---|---|
| `harness` | A list of paths of the checkout (files or directories, such as `["bench/", "tools/rad-kbench"]`; at most 50, none with `..`) that **measure**: the scripts and tools a measuring step runs. For the head they are taken from the merge base: before the steps run, the head's copy of each path is removed and the merge base's put in its place. A change that touches a harness path is **not measured by the lane**: no job is queued, the reason says `the change touches <file>, which is in the measuring harness of the lane (<path>): it is not measured by itself, a person looks first`, and the owner gets one Inbox item `lane_needs_a_person` (`data.reason`) for the card. The paths also count as locked paths for the [merge gate](desktop-api.md#the-merge-gate), so only a person merges such a change. |

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
is up to 20 of the same, and a value is a finite number; a line that is not that is left out, since what a step printed is
data written by the code under review. The step **fails closed** where the code could crowd the harness's rows out: a
run that prints more than 20000 lines that start with the prefix (`it printed more lines that start with the measurement
prefix than a run may`), a run with more than 500 rows (`it gave more than 500 rows in one run`) and a run in which a name is
given twice (`the row <name> was given twice in one run`, whether in the output or the JSON file) are not cut but failed.
A run with no row fails the step, and so does a row that is not given in every run of both sides.

## The job

A job is one change of one card at one pinned head: the row `lane_jobs` has the repository, card and change, the lane,
`head` (the commit measured, `after`), `base` (its merge base with the default branch, `before`, read when the job is
queued) and `external` (whether the change is a pull request someone else opened).

What a job does, in order, on the lane's machine, each command with a time limit:

1. **Preflight.** `git`, `timeout`, `sha256sum`, `readlink`, `tar`, `find`, `xargs`, `cp`, `df`, `awk` and `ps` are there; the lane's
   directory (`work_dir`) has at least `disk` free (`df`), else the job ends `errored` with `the directory <work_dir> of the
   lane has only <n> MiB free, and a job needs <disk> bytes; the job was not run`; for a job that has to be contained the container program
   is there and `--version` works, else the job ends `errored` with `the container runtime <name> is missing on the
   machine` (or `does not run`) and `the job was not run, since this lane never runs a job that must be isolated without
   the container runtime`. Nothing runs directly instead.
2. **The commits.** Every git command runs with `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` set to `/dev/null`, `core.hooksPath` and `core.attributesFile` at `/dev/null`, `git-lfs` and its filter switched off, no template directory and no prompt, so the configuration, hooks and filters of the account cannot run on the tree of a pull request and its `.gitattributes` and `.lfsconfig` name nothing that exists. The head and the base are pushed from the server's clone into a bare clone on the machine
   (`<work_dir>/repos/repo-<repository id>.git`, made when missing) as `refs/lane/<job>/after` and `refs/lane/<job>/before`,
   over SSH for a runner and by path for the server. Each is fetched into `<work_dir>/jobs/<job>/after` and `.../before`
   and checked out detached, and the job is refused (`errored`) unless `git rev-parse HEAD` there is exactly the commit.
   The refs are removed again. Submodules are not fetched. Each side is made writable (`chmod -R a+rwX`) on its own, the job's directory is not.
   The **harness** is taken out of the merge base with `git archive` into `<job directory>/harness`, which belongs to the
   machine's account, is never made writable and is never mounted writable; the sha256 of every file is kept in
   `<job directory>/harness.sha256`, and a harness that holds a link refuses the job. The same files are copied into the
   head's checkout (what the head has at those paths is removed first), for the steps that use them.
3. **The steps**, in order. A step runs in `after` (the head), and a step with `on = "both"` first in `before` (the merge
   base). A measuring step runs `runs` times: `before`, `after`, `before`, `after`, ..., so that what the machine does
   over time falls on both. Every command has the variables `LANE_SIDE` (`before` or `after`), `LANE_RUN` (the number of
   the run of a measurement, else empty), `LANE_JOB`, `LANE_HEAD`, `LANE_BASE` and `CI=1`, and `HOME` is
   `<job directory>/home` (`/tmp` in a container).
4. **The report** and the end of the job. What the job's commands left is ended (its containers, found by the label
   `moebius-lane-job=<job>`, and the process group of each `pid-<n>`), and the directory of the job is removed from the
   machine. A container's user may have made what the machine's account cannot remove (a directory of mode 000, a name that
   starts with two dots): the lane then runs one more container, with no network and no capability, that does
   `chmod -R u+rwX /work; find /work -mindepth 1 -delete` on each side. If the directory is still there afterwards that is
   an error of the lane: it **pauses** (`lane_paused`, naming the job) and no next job starts until a person has looked.

If a server stops while a job runs, the next start ends what that job left in the same way (containers by their label,
process groups from the `pid-*` files, the directory) before the job is queued again; when that cannot be done the lane
pauses. A container of the same name is also removed before a command starts.

The job never pushes to a forge and has no secret of the server. The server runs the machine's commands with only `PATH`
and `HOME` (and `SSH_AUTH_SOCK` for `ssh`) of its own environment, and a container has only the variables above.

### Stopping a command

A command that is cancelled (a new push), runs past the time the server allows, or whose script was lost is stopped **on the machine**, over SSH for a runner, and the job is not over until that is confirmed: a container is removed by name and asked for until it is gone, a direct command's process group is killed until it is. A lane that cannot confirm it pauses itself (`lane_paused`), so that no next job starts on the same GPUs. The `timeout` that starts a command, `--timeout` of `podman` and the kill of the process group all bound it on the machine, not only the client on the server. Whatever a command left running in its process group ends with it, and its pipes are read for five seconds after it ended and then let go, so that a background process cannot hold the lane. A job that runs out of its total time on the **merge base** is `errored` (the lane's), says `the job ran out of its total time of <n> seconds at the step <name>`, and gets no reply on the pull request. On the **head** it is `failed`, naming the limit (`it ran out of the lane's total time of <n> seconds for a job`): the head was given the whole time, so it does not count towards pausing the lane.

### Isolation

| `isolation` | A job for a pull request someone else opened | A job for a change of the server's own |
|---|---|---|
| `always` | in a container | in a container |
| `external_only` | in a container | directly on the machine |
| `off` | directly on the machine | directly on the machine |

A **container** is made for every command: `run --name moebius-lane-<job>-<n> --label moebius-lane=moebius-lane-<job>-<n> --network none --read-only --cap-drop
ALL --security-opt no-new-privileges --user <user> --pids-limit 4096 --memory <memory> --memory-swap <memory> --oom-score-adj 1000 --ulimit fsize=<disk>:<disk> [--cpus <cpus>] --tmpfs /tmp:rw,exec,nosuid,size=<the smaller of 8 GiB and memory> -e HOME=/tmp -e
<LANE_...> --device <each device> --group-add <each group> -v <job directory>/<before or after>:/work:rw
[-v <job directory>/harness/<path>:/work/<path>:ro for each harness path the merge base has] -w /work <image>
sh -c <command>` (and `--timeout <timeout_seconds>` for `podman`), labelled `moebius-lane=<name>` and `moebius-lane-job=<job>`, started by `timeout -k 10 <timeout_seconds>`. **The container sees the checkout of its own side and nothing else of the job** (and the harness, read-only, over it): the other side is not mounted, so the head's code cannot rewrite the merge base's scripts or copy its output over its own, and a step that overwrites a harness file writes nowhere the measurement reads. The files a measuring step is read from (rows, the parity file) are read and hashed by the host after the container has ended, never by the container, and only if each is a regular file inside the checkout, reached through no link (`readlink -f` and `! -L`); anything else fails the step with `it wrote no regular file <path> inside its checkout`, and a file is removed before a run only by the same rule. After the command the container is asked how it ended (`inspect`), then removed (`rm -f`), whatever came of it. It has no
network, no credentials and no SSH key, an unprivileged user, a read-only image and the job's directory as the only place
to write, besides `/tmp`. A container that is not there to be asked (the runtime could not start it: the image is missing, a device is not there) ends the job `errored`. **The command's exit code is its own result**, whatever the number is: its last line, which says how it ended, starts with a random word only the server knows, so an exit code of 124, 125 or 255 of the change's own command is a `failed` step, not an error of the lane, and a pull request cannot make a lane pause by exiting with one.

A job **directly on the machine** has no mount to lay over the checkout, so before **every run of a measuring step** the harness is put back from `<job directory>/harness` (each path through the guard that stops at a link leading out of the checkout) and the hash of every file is checked, in both places, against `harness.sha256`; a harness that cannot be put back or does not match fails the step of the head (`the measuring harness of the merge base could not be put back ...`). It runs `cd <job directory>/<side> && timeout -k 10 <timeout_seconds> sh -c <command>`, in a process group of its own whose id is kept in `<job directory>/pid-<n>` and which is killed when the command ends, as the
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
item and starts the count again. An agent does not. A lane also pauses at once when a command or what a job left cannot be ended or removed (see above). The pause remembers the card whose job made it: its reason is shown to those who see that card, and a generic one to everyone else.

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
| Rows of one run | 500 (more fails the step) |
| Lines with the prefix of one run | 20000 (more fails the step) |
| What a container may use | `memory` (swap included), `cpus`, and a file of `disk` bytes |
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
