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

**Agent**:
A named setup that a run uses on one harness: its instructions, skills, MCP servers, extensions of that harness, model and
thinking level, such as a coder, a reviewer or the Retro's analyst; everything but the extensions carries over to another
harness. Agents are kept, versioned, in the harness configuration, so that the Retro can propose changes to them.
_Avoid_: profile, persona, role, bot

**Harness configuration**:
Everything that shapes how a harness works apart from its own code (the agents, and the instructions, skills, extensions
and settings they draw on), kept in a git repository of its own.
_Avoid_: harness config, setup, profile

**Bridge**:
The part of Moebius that lives inside a harness, as an extension of that harness, and through which Moebius checks every tool
call before it runs.
_Avoid_: Moebius extension, hook, plugin, adapter

**Lens**:
The extension loaded next to the bridge that shows an agent the lint, syntax and type problems its own edits made, at once
after an edit and once more when it is done.
_Avoid_: linter, LSP, diagnostics

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

**Model class**:
How Moebius groups the models the router reaches: Frontier for the strongest models, Flash for the fast and cheaper ones, and
Local for models running on one's own hardware.
_Avoid_: tier, model size, Sonnet class, Opus class

**Decision point**:
A place where Moebius or a person chooses between options, such as whether a tool call may run, whether a review passed,
whether an outside issue may start or whether a proposal is accepted.
_Avoid_: hook, checkpoint, gate

**Decision model**:
The one model that decides at every decision point that is not left to a person; until one has been trained on Moebius's own
judgements, a teacher model takes its place.
_Avoid_: judge, classifier, guard model

**Teacher model**:
A strong general model, reached through the router, that stands in for the decision model and whose judgements train it.
_Avoid_: base model, oracle

**Judgement**:
The record of one choice at a decision point, by whoever made it (a pattern, a script, the teacher model, the decision model
or a person), with its full context, the options, the choice and why, later marked right or wrong from how the work turned
out and from the Retro.
_Avoid_: verdict, label, decision log

**Shadowing**:
A trained decision model deciding alongside whoever decides now, without effect, so that it takes over a kind of decision only
once it has shown it decides that kind at least as well.
_Avoid_: dry run, canary, A/B test

**Replay**:
Running a refused tool call that can be undone in a throwaway copy of its worktree during the trial window, to learn what it
would have done.
_Avoid_: counterfactual, sandbox run, re-run

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
One use of a harness, as one agent, for one purpose, such as working on one card in its own worktree; a card has as many
runs as its workflow needs.
_Avoid_: job, session, execution

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

**Attempt**:
One of several runs a hard card gets in one go, each in a worktree and on a branch of its own and perhaps with another
model; each is judged by the repository's checks and the reproduction test, and one that passed is picked, by a selection
run when several did. The one picked becomes the card's `code` run.
_Avoid_: sample, candidate, try

**Review axes**:
The two reviews of a round, Standards (does the change follow the repository's rules) and Spec (does it do what the
issue asked); each finding is blocking or a judgement call and cites its evidence, and only blocking findings send the
card back.
_Avoid_: second opinion

**Reproduction test**:
A test the Coder writes from the issue before the fix, which fails without the change and passes with it; in a
repository that works test first, a change goes on to review only when its reproduction test does both.
_Avoid_: regression test, acceptance test

**Experiment**:
Changing one thing on a branch of its own, measuring the result and keeping the change only if it is better; a trial and each
step of an autoresearch are experiments, which differ only in how they measure.
_Avoid_: probe, A/B test

**Autoresearch**:
A loop, bounded by its brief, that first measures a baseline for anything measurable and then has a harness keep finding new
levers on its own (in the code and profiles, in its own history of experiments, in papers and on the web), running an
experiment for each and keeping only those that improve the metric; each lever kept becomes a change of its own.
_Avoid_: optimisation run, experiment loop, auto-tuning

**Lever**:
One idea for a change that might improve an autoresearch's metric, found by the harness that runs it.
_Avoid_: hypothesis, knob, candidate

**Brief**:
What an autoresearch is to achieve and within which bounds (the metric and its direction, the benchmark, the files in scope,
where it runs and the budget it may spend), confirmed by a person before it starts; the autoresearch ends when the budget is
spent or a person stops it.
_Avoid_: spec, plan, config

**Chat**:
An interactive run without a card: a person writes to an agent and it answers; it can be continued, steered and stopped.
_Avoid_: conversation, session, thread

**Moebius agent**:
The bundled agent that knows Moebius from its guide and builds what a person asks for (column scripts, rules, agents,
extensions), offering it as a change; a new chat's default agent.
_Avoid_: assistant, helper, copilot

**Grilling**:
A chat in which a person and a harness work an idea out, the harness researching and questioning its feasibility,
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

**Bundled plugin**:
A plugin that ships with every installation of Moebius and is on by default, such as the GitHub forge or self-improvement,
but that can still be removed.
_Avoid_: built-in, default plugin, core plugin

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
The experiment that tests one proposal: the task the error happened in, repeated with the proposal applied; it is positive
when the error no longer occurs and the result is no worse.
_Avoid_: test, probe

**Regression set**:
A fixed sample of a repository's cards merged in the past, on which every proposal is also tried; a proposal that makes one
of them fail where it passed before is not offered.
_Avoid_: test suite, benchmark

**Trial window**:
The daily stretch of time in which Moebius may run trials without asking.
_Avoid_: test window, maintenance window
