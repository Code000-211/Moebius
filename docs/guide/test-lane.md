# Build, test and measure changes on your own GPUs

## When you want this

Your repository has a merge bar that only a machine with GPUs can check: a lossless change must be measured before and
after on one machine, at least three runs each, with its output unchanged. The Reviewer can say that a change's code is
right and that the measurement is missing (the verdict `approve_code`, see [Working with others](working-with-others.md)),
and a registered person can post the numbers. A **test lane** makes the measuring itself automatic: the change is built,
tested and measured on a machine of yours, the numbers go to the Reviewer, and an approval merges it through the merge
gate, with no click of yours.

The code that runs is a stranger's pull request. The lane runs it in a container with no network and no credentials
unless you say otherwise, and never reads a secret of the server. Read [Isolation](#isolation) before you choose.

The exact contracts are [the lane](../reference/lanes.md), the [`[[lanes]]` keys](../reference/configuration.md#lanes) and
the [routes](../reference/desktop-api.md#the-test-lane). The decision is [ADR
0035](../adr/0035-a-test-lane-measures-on-the-owners-own-gpus.md).

## The steps

### 1. Prepare the machine

It needs `sshd` with key login for the account the lane uses, `git`, `sh`, `timeout` and `sha256sum`, and `podman` or
`docker`. Pull the image you want the jobs to run in (the container has no network, so the image holds the toolchain). Use
an account and a machine of their own for the lane when its jobs are strangers' code.

### 2. Describe the lane in `moebius.toml`

A lane is a machine of your `[[runners]]`, or the server itself. Restart the server: lanes are read at its start.

```toml
[[runners]]
name = "gpu-box"
host = "gpu.example.com"
user = "lane"
key_file = "gpu-box.key"
working_dir = "/srv/moebius"

[[lanes]]
name = "gpu1"
runner = "gpu-box"
isolation = "always"
runtime = "podman"
image = "rocm/dev-ubuntu-24.04:6.2"
devices = ["/dev/kfd", "/dev/dri"]
gpus = "2x MI300X"
power_cap = "750 W"
rocm = "6.2.1"
timeout_seconds = 7200
```

### 3. Add a column that queues the job

A column that counts as In review and says `lane = true` queues a job for the card's change at its head whenever a card
enters it, once the Reviewer has approved the code. "Ready for testing" is the column the Reviewer moves a card to on
`approve_code`, so it is the one to mark. In the project's `kanban.toml` (see [Custom columns](custom-columns.md)):

<!-- kanban -->
```toml
[[column]]
name = "Ready for testing"
script = "testing.luau"
after = "In review"
lane = true
```

### 4. Write the job

`projects/<project name>/lane.toml` of the harness configuration repository lists the steps. This is the one for a GPU
inference engine whose rule is "build, run the tests that need no card, run the selftests, then measure before and after,
interleaved, at least three runs each"; the commands are the repository's own, so change them:

<!-- lane -->
```toml
[[step]]
name = "build"
command = "./build.sh"
timeout_seconds = 3600
on = "both"

[[step]]
name = "card-free tests"
command = "tests/r4d_emu && ./sched_test && ./hyv4flash_arch_test"
timeout_seconds = 900

[[step]]
name = "gpu selftests"
command = "./gpu_selftest.sh"
timeout_seconds = 1800

[[step]]
name = "measure"
command = "./kbench.sh"
timeout_seconds = 1200
[step.measure]
runs = 3
prefix = "MEASURE "
json = "out/kbench.json"
parity_file = "out/logits.bin"
```

The build runs on both sides (`on = "both"`): the merge base is built too, since the measurement needs it. A measuring
step runs `runs` times on each side, the merge base and the head by turns, and the lines of its output that start with
`MEASURE ` (`MEASURE decode_tps 41.5 tok/s`) and the rows of the JSON file it writes are the rows. The bytes of
`out/logits.bin` must be the same in every run, before and after: the report says whether they are. A save of the file is
refused when it is not a valid definition.

### 5. Give the repository the lane

In the repository's settings, the "Test lane" box (a person only), or:

```sh
curl -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"lane": "gpu1", "reason": "the GPU box"}' http://127.0.0.1:7171/v1/repositories/1/lane
```

### 6. Make the Reviewer ask for the measurement

Put the merge bar in the repository's review brief (`REVIEW.md` or `notes/backlog/CARDS.md`) and turn on
`require_measurement` in the "Working with others" box, as in [Working with
others](working-with-others.md#measurements-without-a-click). Leave `measurers` empty if only the lane measures.

## What happens

The Reviewer reads a pull request and answers `approve_code`: the code is right, the measurement is missing. The card goes
to "Ready for testing", which queues a job at the pull request's head. The job waits for the lane's lock, then:

- **A step fails.** The author gets one reply on the pull request that names the step and gives the last lines of its
  output (redacted and cut), and the card goes back to In review. They push a fix, the Reviewer reads it again, and a new
  job runs for the new head. A push while a job is queued or running cancels it.
- **Every step passes.** The numbers, the machine and every run become the measurement of that head and the next
  round of the Reviewer starts with them, as it does when a measurer posts one. The Reviewer judges them against the
  review brief. `approve` accepts the measurement for the head and merges through the gate; `approve_code` says what is
  still missing.

Nothing runs before the Reviewer has approved the code, or while the repository is frozen, agents are paused, the card is
held or the change touches a locked path; a job that waits for one of them stays queued. The card window shows the lane's
report, and the daily digest tells what the lane did.

## Isolation

`isolation` is a setting of each lane:

| Value | What it does |
|---|---|
| `always` (the default) | Every command of every job runs in a fresh container with no network, no capability, no new privilege, an unprivileged user, a read-only root and only the job's directory to write in, and the GPU devices passed through. A missing or broken container runtime refuses the job: it is never run directly instead. |
| `external_only` | A job for a pull request someone else opened runs in the container; a job for your own server's change runs directly on the machine. |
| `off` | Every job runs directly on the machine, as the account the lane logs in as. |

A lane that is not `always` shows a warning in the "Test lane" box, and the daily digest says so every day. Choose
`external_only` or `off` only for a machine whose account has nothing to lose, because the code of a pull request can do
what that account can. The container has no network, so a build that has to download something fails there: put the
toolchain in the image.

## A column script for your own changes

For a change of the server's own (its coder's), a script of the column asks the lane itself and acts on the answer. A
followed pull request has no column script run, so for it the column alone queues the job. This is the script of a
column that waits for the lane and then offers the change:

<!-- script: testing.luau -->
```luau
function on_card(card)
  local job, why = moebius.lane_job()
  if job == nil then
    moebius.wait("no test lane job: " .. why)
  elseif job.state == "passed" or job.state == "measured" then
    moebius.offer_change("Built, tested and measured on " .. job.lane .. ".")
  elseif job.state == "errored" then
    moebius.ask("lane_error", job.summary)
  else
    moebius.wait("the test lane: " .. job.state)
  end
end
```

It reads `state` at every tick, and `moebius.lane_job()` asks for a job only the first time. A job that fails sends the
change back to its coder with the failure as the note; the script does not have to.

## Trying it and what goes wrong

- `GET /v1/repositories/{id}/lane` shows the lane, its isolation and warning, whether the project's `lane.toml` is valid
  and the latest jobs; `GET /v1/cards/{id}/lane` says why a card has no job (`waiting`).
- A job that **errored** is the lane's, not the change's: the machine could not be reached, the image could not start, the
  merge base itself does not build, the definition is wrong or the job took longer than `timeout_seconds`. Three in a row
  pause the lane and put an item `lane_paused` in your Inbox; fix the cause and resume it with `POST /v1/lanes/{name}/resume`.
- A job that **failed** names its step. The same step on the merge base is shown as `errored` ("the merge base itself
  fails"), since that is not the change's doing.
- A pull request that touches a locked path (`.github/**`, build files, your `locked_paths`, the review brief) gets no
  job: a person merges those.
- Submodules are not fetched, and a command that needs the network fails in a container.
