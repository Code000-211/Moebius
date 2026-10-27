# Moebius

Moebius is an orchestration layer: it takes work from a repository's issues, hands it to coding-agent harnesses and improves
how they work from what each day of work teaches it.

## Language

**Moebius**:
The orchestration layer: one lean, headless program that runs permanently on a server, holds the boards, schedules the work
and drives the harnesses.
_Avoid_: meta-harness, harness, daemon, platform

**Core**:
The part of Moebius that is not a plugin: repositories, runs and their scheduling, the boards, the inbox, the check every
tool call passes and the guarantees no plugin may break.
_Avoid_: kernel, engine, base

**Repository**:
A git repository Moebius works in, with its branches, worktrees and merges; how its issues and changes are hosted is up to its
forge, if it has one.
_Avoid_: repo, project, codebase

**Forge**:
A plugin that connects a repository to the place where its issues and changes are hosted, such as GitHub, Codeberg or GitLab;
a repository without one keeps its issues and changes in Moebius.
_Avoid_: host, source, provider

**Change**:
A branch proposed for merging into a repository, which a forge shows as a pull request or merge request.
_Avoid_: pull request, merge request, PR

**Harness**:
A coding-agent program that Moebius drives to do the work, such as Pi-Bolt.
_Avoid_: substrate, worker, bot

**Harness configuration**:
Everything that shapes how a harness works apart from its own code (instructions such as `AGENTS.md`, skills, extensions,
settings), kept in a git repository of its own.
_Avoid_: harness config, setup, profile

**Bridge**:
The part of Moebius that lives inside a harness, as an extension of that harness, and through which Moebius checks every tool
call before it runs.
_Avoid_: Moebius extension, hook, plugin, adapter

**Router**:
The part of Moebius that every harness uses as its model provider and that passes each request on to a real provider.
_Avoid_: proxy, gateway, LLM provider

**MCP gateway**:
The part of Moebius that every harness uses as its one MCP server and that bundles the MCP servers a person manages in the
desktop app.
_Avoid_: MCP proxy, MCP hub

**Rule**:
Something the work must or must not do, which Moebius enforces on every tool call; a call that breaks one goes back to the
model to find another way, and the run does not stop.
_Avoid_: policy, guardrail, guideline

**Decision model**:
The model that judges whether a tool call breaks a rule and answers the questions a harness asks during a run; until one has
been trained on Moebius's own judgements, a teacher model takes its place.
_Avoid_: judge, classifier, guard model

**Teacher model**:
A strong general model, reached through the router, that stands in for the decision model and whose judgements train it.
_Avoid_: base model, oracle

**Judgement**:
One recorded decision of the decision model or the teacher model, with what it judged, what it decided and why, later marked
right or wrong from the outcome of the work and from the Retro.
_Avoid_: verdict, label, decision log

**Live view**:
The view of a run as it happens, in the desktop app, in which a person can steer, stop or message it and open a shell in its
worktree.
_Avoid_: terminal, console, log

**Board**:
Moebius's own Kanban board for one repository, filled from its issues and changes; each repository has its own.
_Avoid_: project, GitHub Project

**Card**:
One issue on the board, or a change that belongs to no issue; a change that closes an issue belongs to that issue's card.
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
back to coding while anything is still wrong, until its change is ready to merge; a person steps in only before the merge.
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
A person whose issues start the workflow without anyone's approval; issues from anyone else wait until a person approves
them.
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
