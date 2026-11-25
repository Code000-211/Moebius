# A test lane measures on the owner's own GPUs

[0031](0031-one-merge-gate-and-a-brake-for-the-factory.md) made the merge gate hold a change for a measurement, and gave
the Reviewer the verdict `approve_code` for a pull request whose code is right and whose measurement is missing.
[0033](0033-measurements-and-reviews-flow-without-the-owners-clicks.md) made a measurer's comment the way a measurement
reaches the Reviewer. What was left to people is the measuring itself. The owner's repository is a GPU inference engine
whose rule is that every lossless change meets a merge bar measured on GPUs, before and after on one machine, at least
three runs each, with the output unchanged; it has no CI, so nothing builds a stranger's pull request, tests it or
measures it, and the owner or a helper has to, on a machine with GPUs, by hand. The owner wants a column of the board,
"Ready for testing", to have the change built, tested and measured on one of their own machines, and the change to go
on from there with no click.

The code being measured is a stranger's. Running it on a machine with GPUs, the owner's credentials and the owner's
network is the opposite of what the merge gate is for, so most of this record is about what the job may touch.

## Considered Options

- **A GitHub Actions self-hosted runner on the GPU machine.** The repository has no CI, and a runner executes a
  workflow the pull request can rewrite.
- **Run the job inside the Reviewer's run.** The Reviewer is restricted to reading files so that a pull request cannot
  act through it; giving it a shell on a GPU machine undoes that.
- **A script that runs `ssh` itself, in a column script.** A script waits for nothing for long: it has a time limit, and
  its output has no place to go that the gate and the Reviewer already read.
- **A lane of the server: a named machine with a lock and a queue, a job definition kept with the project's settings, a
  job that runs the definition at two pinned commits, and a result handed to the paths that exist.** The failure goes to
  the author, the measurement goes to the Reviewer as a measurer's comment does, and the gate is not touched.
- **Isolation as one fixed behaviour.** Strongest, and it makes the owner's own agents' changes, which they trust, pay for
  a container, and the owner's machine may not support one. **Isolation as the owner's choice of three values** keeps the
  default strict and lets the owner loosen it knowingly, with the loosening shown.

## Decision

- **A lane is a machine** named in `[[lanes]]` of the configuration: one of the `[[runners]]` (reached over SSH like a
  runner's harness is, [0027](0027-working-with-people-on-other-servers-through-the-forge.md) aside), or the server itself.
  It has a lock, so one job runs at a time, and a queue. It has what a report says of the machine (GPUs, power cap, ROCm
  or driver version) and how it isolates. Which lane a repository uses is the repository's setting, assigned by a person in
  its "Test lane" box and by nobody else. A lane that ends three jobs in a row for a reason of its own (it cannot be
  reached, the container runtime is missing or cannot start a container, the merge base itself does not build, the
  definition is wrong, the job's time ran out) pauses itself and the owner gets an Inbox item; only a person resumes it.
- **The job is the project's own**, `projects/<project>/lane.toml` of the harness configuration repository: a list of steps
  with a command and a time limit, and which steps measure. A measuring step runs `runs` times (at least three) on the
  merge base and on the head, one after the other by turns, so that drift of the machine falls on both; its rows are the
  lines of its output that start with a prefix, or a JSON file it writes, and a file whose bytes must be the same
  before and after can be named. A step that must also run on the merge base (a build) says `on = "both"`. The
  definition is read from the settings, not from the pull request, so a pull request cannot change what is run on it.
- **A job is for one change at one pinned head.** The server sends the two commits into a bare clone on the machine and
  checks each out detached into a directory of the job, and refuses the job unless what is checked out is exactly the
  commit. The job never pushes, and no secret of the server is in its environment: the processes it starts get `PATH`
  and `HOME` and nothing else.
- **Isolation is a setting of each lane with three values.** `always` (the default): every command of the job runs in a
  fresh container with no network, no capability, `no-new-privileges`, an unprivileged user, a read-only root, a `/tmp`
  and the job's directory as the only places to write, a time limit, and the GPU devices passed through
  (`/dev/kfd` and `/dev/dri` unless configured). `external_only`: a job for a pull request someone else opened runs in
  the container, and a job for the server's own change runs directly on the machine. `off`: everything runs directly.
  The runtime is `podman` or `docker`, whichever the lane names; **if it is missing or does not run, a job that has to be
  contained is not run at all**, never run directly instead. While a lane is not `always`, the repository's settings show
  a warning and the daily digest tells it every day.
- **What the code under test may touch is narrow, on purpose.** The checkout runs git with none of the account's
  configuration, hooks or filters; a container sees only the checkout of the side it runs, and what a measurement reads
  is read by the host, from regular files inside the checkout, after the container has ended; a cancelled or timed-out
  command is stopped on the machine (the container removed by name, the process group killed) and confirmed before the
  lane is free, or the lane pauses; the SSH agent goes to the `ssh` client alone and nothing is forwarded; and the
  command's own exit code is its result, told from the runtime's and the shell's by a last line only the server can write.
- **The harness that measures is the owner's, and no step of the pull request can write it.** It is taken out of the merge
  base into a directory of the job's own that is never writable by what runs, and laid over the container's checkout
  read-only (or, on the machine, put back and its hashes checked before every measuring run). Rows fail closed: more
  lines, more rows or a name twice than a run may give fail the step, so that the code under review cannot crowd the
  harness's own rows out. A container has a memory, a processor and a file-size limit and the lane's directory a minimum of
  free space, on a disk that is not the server's own; what a container made that the machine cannot remove is removed
  by a container, and a directory that stays pauses the lane. The time of a job is blamed on the side that used it: a
  head that uses it up fails, the merge base doing so is the lane's (and, for a pull request of someone else's, does not
  count towards pausing it). Every directory above a harness path is mounted over itself so that it cannot be renamed
  away, and the harness is hashed after every run. A line with the prefix in its middle, or too long to read, fails the
  step, as a partial line of the code under test would hide the harness's row. The disk of a job is watched while it runs
  (its size, the blocks and the inodes left), the cgroup limits are read by a container before the first step, a
  lane that is the server itself needs a disk of its own, and a pause that a card's job made is told in the Inbox and the
  log without its reason.
- **The harness that measures is the owner's, not the pull request's.** `lane.toml` can name `harness` paths (the bench
  scripts and tools). For the head they come from the merge base, a change that touches one is not measured but looked at
  by a person, and the paths count as locked paths for the merge gate. `lane.toml` is changed by a person only, as the
  merge gate's settings are.
- **A job starts only when the owner's brakes are off and the review has come first.** A card entering a column marked
  `lane = true` in `kanban.toml` (a column that counts as In review, [0030](0030-custom-board-columns-per-project.md)), a
  column script's `moebius.lane_job()`, a person's request, and the lane's own pass for a card that waited all ask the
  same question: the repository is not frozen, agents are not paused, the card is not held, the change touches no locked
  path, and the Reviewer's verdict for this head is `approve` or `approve_code` (for a change of the server's own, the
  card's latest review approves). The same question is asked again when the job is about to start, since any of them
  may have come while it waited; a job that is blocked stays queued.
- **A failure goes to the author.** A step of the head that fails or runs past its limit ends the job `failed`; the card
  goes back to the author's turn (a pull request someone else opened to In review, the server's own change to In
  progress with the failure as the note) and one plain reply on the pull request names the step and gives the last lines
  of its output, redacted and cut. A failure on the merge base is not the change's: that job is `errored`, which is the
  lane's.
- **A measurement goes to the Reviewer, by the path a measurer's comment takes.** The job's report (the machine, the
  commits, the steps, every row of every run, the medians and the output parity) is kept as a measurement of the change
  at its head (`measurement_reports`, by `lane:<name>`), and starts the Reviewer's next round of the kind `measurement`,
  which counts toward the round limit. The Reviewer is given it as data, fenced, in a section that says what produced it,
  and judges it against the review brief. `approve` accepts the measurement for the head (the source is `test lane`) and
  merges through the gate; `approve_code` and `changes` are as they were. Nothing is posted or parsed as a comment. Only
  numbers reach the Reviewer: what else the job printed is never given to it.
- **A new push ends it.** The jobs of the old head that are queued or running are cancelled, a running one is stopped,
  and the result of one that had ended counts for nothing. A result is handed on only for the head the change has now.
- **Output is data and is capped.** A command's output is kept to its last 256 KiB; a measurement's rows are plain
  names and finite numbers, at most 500 a run; a reply quotes at most 40 lines and 3500 characters, redacted
  ([`blobs::redact`](../../crates/moebius-core/src/blobs.rs)) and fenced with a fence longer than any run of backticks in
  it. A job has a total time limit (`timeout_seconds` of the lane) as well as one for each step.
- **The merge gate is not touched** and the lane does not merge. The one thing the lane adds to what the gate reads is
  the measurement the Reviewer accepts.

## Consequences

- The owner trusts the lane's account with what the job runs. In `always` isolation that is a container the lane's user
  may start, with the GPUs in it; in the two looser values it is the lane's user itself, which is why the settings and the
  digest say so, and why a lane for outside pull requests should be a machine and an account of its own.
- A container with no network cannot fetch what a build needs. The image holds the toolchain, and the project's `build.sh`
  must not reach the net. This is the cost of the default, and `external_only` or `off` is the owner's to choose against
  it.
- The commits reach the machine through the server's clone, so a pull request whose head the server has not fetched has
  no job. The poll fetches the head of every followed pull request, so this does not happen to one the lane is asked for.
- The measurement of a change of the server's own is not handed to a round of the Reviewer, which only exists for a pull
  request someone else opened: the report is on the card and `moebius.lane_job()` gives a script the job, and a person
  accepts the measurement as before.
- A job that was running when the server stopped is queued again at the next start and runs from the beginning, after what
  it left is ended: its containers (found by the label `moebius-lane-job`), the process groups of its direct commands and
  its directory. A lane where that cannot be done pauses.
- A lane serves one repository's pull requests as well as another's; the lock is the lane's, so a long measurement holds
  the others' jobs. A second lane on the same machine is two locks on one set of GPUs, and the owner does not make one.
- A lane is a Unix thing: the machine runs `sh`, `git`, `timeout` and `sha256sum`. The server on Windows can run a lane
  that is a runner, and cannot run one that is itself.
