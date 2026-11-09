# `schedule`

A trigger that starts a run when a cron comes due, in the server's local time. The workflow file names it in
`[[trigger]]` (see [the workflow file](../workflows.md#trigger)); the code is in
`crates/moebius-core/src/workflows/trigger/schedule.rs` and the waking in `server/schedule.rs`.

```toml
name = "nightly-triage"
description = "Reads the board each night."

[[trigger]]
kind = "schedule"
params = { cron = "30 3 * * *", repository = "moebius", inputs = { goal = "Read the board." } }

[[nodes]]
id = "sweep"
kind = "test"
params = { prompt = "At {{ trigger.fired }}: {{ trigger.inputs.goal }}" }

[[edges]]
from = "trigger"
to = "sweep"
port = "out"
```

The node kind is a placeholder, as in [the workflow file](../workflows.md#the-file); the trigger names what starts the
run, and [an expression](expressions.md) reads the value it started the run with as `trigger`.

## Parameters

| Key | Type | Meaning |
| --- | --- | --- |
| `cron` | string | Required. Five fields — minute, hour, day of the month, month, day of the week — in the server's local time, spelled as an automation's schedule is (see [`automation.schedule`](../desktop-api.md)). A field is `*`, a number, a range `a-b`, either with a step (`*/15`, `8-18/2`), or a list of these separated by commas. When both the day of the month and the day of the week are restricted (neither starts with `*`), a day that matches either comes due, as in cron. |
| `repository` | string | Required to start a run. The name of the repository the runs belong to, as the board knows it. A run belongs to a repository, so a schedule that names none starts nothing. |
| `inputs` | table | What a run starts its inputs with: names and their defaults, such as `{ goal = "Read the board." }`. Optional; defaults to an empty table. |

A cron that cannot be read is refused when the file loads, with a message that names which field is wrong:
`workflows/nightly-triage.toml: trigger: "cron": the minute 70 is not between 0 and 59`. So is a `cron` that is not a
string of five fields, an `inputs` that is not a table, and a `repository` that is not a name. A parameter that is not
one of the three above is refused too: `trigger: unknown trigger parameters ["count"]`.

## The value a run starts with

An object with the minute the run fired at and the defaults of the inputs:

```json
{ "fired": "2026-11-09 03:30", "inputs": { "goal": "Read the board." } }
```

`fired` is the server's local minute the cron came due at, as `YYYY-MM-DD HH:MM` with two digits each — the minute the
firing is recorded under, and the same text the clock automations keep. The trigger has no input and one output, `out`,
which carries this value.

## The clock

The server looks once a minute (`server/schedule.rs`, the one clock everything that happens by itself goes off; see
[the clock](../configuration.md#the-clock)). On each minute it reads every workflow the harness configuration
repository holds, and for each one whose trigger is a `schedule`:

- The cron is due when it names the minute just reached, or a minute the five-minute reach still covers, and the
  workflow has not fired for that minute yet. The minute a workflow last fired at is in the store
  (`workflow_schedules`), so one occurrence starts one run, and a server that restarts reads the same minute and
  starts no second one.
- The firing is recorded before the run starts. A run that cannot start because the repository the trigger names is
  not one the server has is a line on the server's output, and the workflow waits for its next minute rather than
  start runs every minute.
- A workflow file that cannot be read wakes no schedule of its own, and the schedules of the other workflows go on:
  what is wrong with the file is reported on the bus as `retro_error`, and it stays reported each minute the file
  remains as it is.
- A workflow that fires for the first time is read from five minutes back: the latest firing that came due up to five
  minutes ago is started, and an older one is not made up. A schedule finer than that reach therefore starts on the
  minute just reached rather than trailing the firings before it. A server that was off over the night starts no run
  for the hours it missed.
- The minutes are read as the clock reads them, so the season change fires a schedule once and not twice: the minute
  a fall-back day reads twice is the minute still to come, never one that has passed again, and an hour a
  spring-forward day skips is waited over.

Two workflows on the same cron both fire: each keeps the minute it last fired at under its own name.

## Removing a workflow's schedule

Deleting the workflow's file (`DELETE /v1/workflows/{name}`) leaves its row of last firings in the store; it does
nothing, since no workflow of that name is read again. Adding a workflow under a name that has fired before starts
where that name left off, so a workflow written again under an old name waits for its next minute.
