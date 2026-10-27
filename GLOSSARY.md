# Moebius

Moebius is an orchestration layer: it takes work from GitHub, hands it to coding-agent harnesses and improves how they work
from what each day of work teaches it.

## Language

**Moebius**:
The orchestration layer: one lean, headless program that runs permanently on a server, holds the board, talks to GitHub,
schedules the work and drives the harnesses.
_Avoid_: meta-harness, harness, daemon, platform

**Harness**:
A coding-agent program that Moebius drives to do the work, such as Pi-Bolt.
_Avoid_: substrate, worker, bot

**Harness configuration**:
Everything that shapes how a harness works apart from its own code (instructions such as `AGENTS.md`, skills, extensions,
settings), kept in a git repository of its own.
_Avoid_: harness config, setup, profile

**Board**:
Moebius's own Kanban board for one GitHub repository, filled from its issues and pull requests; each connected repository
has its own.
_Avoid_: project, GitHub Project

**Card**:
One GitHub issue on the board, or a pull request that belongs to no issue; a pull request that closes an issue belongs to
that issue's card.
_Avoid_: task, ticket, item

**Run**:
One use of a harness for one purpose, such as working on one card in its own worktree; a card has as many runs as its
workflow needs.
_Avoid_: job, agent, session, execution

**Column**:
A stage on the board with its own behaviour: what happens to a card that enters it, how many cards it may hold at once and
where a card goes next.
_Avoid_: status, lane, state

**Script**:
A piece of behaviour behind a column, written in the script language built into Moebius.
_Avoid_: hook, plugin, automation

**Workflow**:
The columns a card goes through on its own and the behaviour behind them, by default coding and then testing and review, and
back to coding while anything is still wrong, until its pull request is ready to merge; a person steps in only before the
merge.
_Avoid_: pipeline, process, flow

**Autoresearch**:
A bounded loop in which runs change something, measure it against a benchmark and keep only the changes that improve the
metric.
_Avoid_: optimisation run, experiment loop, auto-tuning

**Brief**:
What an autoresearch is to achieve and within which bounds (the metric and its direction, the benchmark, the files in scope,
where it runs and how many iterations it may take), confirmed by a person before it starts.
_Avoid_: spec, plan, config

**Grilling**:
A conversation in which a person and a harness work an idea out, the harness researching and questioning its feasibility,
until it is understood; it ends in a set of issues the person confirms before Moebius creates them, or in a research report.
_Avoid_: shaping, session, interview

**Trusted author**:
A GitHub user whose issues start the workflow without anyone's approval; issues from anyone else wait until a person
approves them.
_Avoid_: collaborator, maintainer, allowlisted user

**Inbox**:
The one place, across all boards, where everything that needs a person arrives: proposals, cards ready to merge and cards
that got stuck.
_Avoid_: notifications, feed, queue

**Plugin**:
A unit of extension that can add to or replace any part of Moebius; it runs either as its own process or compiled into
Moebius.
_Avoid_: extension (that word belongs to Pi-Bolt), addon, module

**Desktop app**:
The native, GPU-rendered client through which a person works with a Moebius that runs on a server.
_Avoid_: UI, frontend, dashboard

## Improvement

**Retro**:
The scheduled daily review of a day's work, in which the errors that happened are analysed and concrete changes are proposed
to avoid them.
_Avoid_: retrospective run, recursion, harnessing

**Proposal**:
A concrete change a Retro suggests to avoid an error, to anything from a harness's configuration to Moebius itself; it is
never applied without the user's approval.
_Avoid_: suggestion, fix, patch, edit

**Trial**:
A repeat of the task an error happened in, with one Proposal applied on its own branch; it is positive when the error no
longer occurs and the result is no worse.
_Avoid_: test, probe, experiment

**Trial window**:
The daily stretch of time in which Moebius may run trials without asking.
_Avoid_: test window, maintenance window
