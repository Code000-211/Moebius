# Issues: M2 to M9

The milestones after M1 of [`docs/spec.md`](../spec.md), cut so that each is one card. Every milestone ends with a review
issue: all tests run many times, the code read for bugs, overengineering and readability, and what is found fixed.

## M2 · GitHub

### 13. GitHub App authentication
Moebius acts as its own GitHub App with webhooks off: from the app id and private key (`[github]` in the configuration,
the key from a file) it signs a JWT (RS256) and exchanges it for an installation token per repository owner, cached
until shortly before it expires.
**Done when** a test against a fake GitHub API signs a JWT the fake verifies and gets, caches and renews a token.

### 14. Issues by polling
A repository with the GitHub forge is polled about every 30 seconds with conditional requests (ETag). Open issues become
cards; issues from trusted authors (per repository) move on by themselves, others wait in the inbox for approval; a
closed issue moves its card to done; "blocked by #n" in an issue's body becomes a dependency.
**Done when** a fake GitHub serves issues, a second poll sends `If-None-Match` and changes nothing on 304, and trusted
and outside issues land where they should.

### 15. Changes as pull requests
A run's branch is pushed to GitHub with the installation token and offered as a pull request that closes its issue; the
pull request's state, checks and reviews are polled; a pull request merged on GitHub moves its card to done, and a
person can merge it from the app through the GitHub API.
**Done when** a test pushes to a local bare remote, opens a pull request on the fake GitHub, and a merge there moves the
card to done.

### 16. Dependencies
A card waits in Ready until every card it is blocked by is done; `ready.luau` sees the card's dependencies.
**Done when** a card blocked by another starts only after the other is done.

### 17. Review of M2

## M3 · Rules

### 18. The tool-call check
An endpoint on localhost for the bridge: every tool call of a run is asked before it runs (`allow` or `refuse` with the
rule), every tool result is reported and comes back with secrets removed. Each call goes through `decide("tool_call")`
and leaves a judgement; the run's token authenticates the bridge.
**Done when** a check and a result round-trip in a test, a refusal names its rule, and an unknown token is refused.

### 19. Rules and the pattern decider
Rules come from the harness configuration's `rules.md` and a repository's `.moebius/rules.md` on its default branch.
Fixed patterns (destructive commands, secrets in commands, paths a rule forbids) are checked first by a compiled-in
decider.
**Done when** `rm -rf /`, `git push --force` and writing outside the worktree are refused with their rule, and ordinary
calls are allowed.

### 20. The teacher model
Rules in prose are checked by the teacher model through the router: it gets the rules, the call and its context, and
answers allow or refuse with the rule. A teacher that fails, times out or answers nonsense refuses (fail closed).
**Done when** a recorded teacher refuses a call that breaks a prose rule and its judgement names the teacher.

### 21. The bridge for Pi-Bolt
A Pi extension shipped with Moebius and loaded into every run: on `tool_call` it asks the check and blocks with the rule
on a refusal; on `tool_result` it replaces the result with the redacted one. A run never waits for a person: a refusal
is an answer, and the run goes on.
**Done when** a real `pi-bolt` against a recorded upstream tries `rm -rf /`, is refused with the rule, and finishes the
issue anyway.

### 23. Reference documentation and an agent-friendly repository
Everything one can build on or plug into Moebius is documented exactly, so that a person or an agent new to the
repository can write scripts, rules, plugins and extensions without reading the code: `README.md` and `AGENTS.md`;
`docs/architecture.md` (how a card moves and which module does what); `docs/reference/` (the column-script API, `rules.md`,
the configuration, the plugin API, the bridge protocol, the desktop API); `examples/` that run in CI; and tests that fail
when a route, a script function or a configuration key is missing from the reference.
**Done when** each of these exists, the examples run in CI, and the coverage tests pass.

### 22. Review of M3

## M4 · The loop

### 24. Review by the Reviewer
In review, a Reviewer run reads the change and ends with a verdict (approve or changes with findings). Findings send the
card back to In progress, and the next Coder round gets them in its prompt.
**Done when** a card goes code → review → code → review → ready to merge in a test with a fake harness.

### 25. CI and conflicts
For GitHub, the card waits in review for the pull request's checks; failures go back to coding with the failing checks.
Without a forge, a repository may name its checks in `.moebius/checks` (a command run in the worktree). A change that
no longer merges cleanly goes back to coding with the conflict.
**Done when** failing checks and a conflict each send a card back with what failed in the next prompt.

### 26. Limits and the inbox
A card that reaches its round limit or its token budget stops and goes to the inbox as stuck; a daily token budget stops
new runs. A person can retry a stuck card, reject a change, steer a run with a message, and stop a run, through the
desktop API; each is a recorded decision.
**Done when** each limit stops a card in a test and each action works through the API.

### 27. Run events through the API
The events of a run (`GET /v1/runs/{id}/events`), the runs of a card, and a card's history, for the live view.
**Done when** a test client reads a finished run's events and a running run's new ones arrive on the stream.

### 28. Review of M4

## M5 · Chat and the Moebius agent

### 29. Chat
A chat is an interactive run without a card: the person picks an agent, writes, and the agent answers in a stream; it
can be continued, steered and stopped, and it keeps its history. Through the desktop API: start a chat, send a message,
read and follow its events, end it.
**Done when** a chat with a recorded upstream answers two messages in a row through the API, and a stopped chat ends.

### 30. The Moebius guide
Documentation of everything one can build on Moebius, for people and for the Moebius agent: column scripts and their
API, workflows and repository overrides, rules, agents and the harness configuration, skills, MCP servers, plugins and
the plugin API, the Pi-Bolt bridge and Pi extensions, and the desktop API. Every example in it is run by a test.
**Done when** `docs/guide/` covers each of these and a test runs every script example in it.

### 31. The Moebius agent
A bundled agent whose skill is the guide: it knows Moebius and builds what a person asks for (a column script, a rule,
an agent, an extension) in the harness configuration repository or a repository's `.moebius/`, tests it, and offers it as
a change the person merges. It is the default agent of a new chat.
**Done when** a chat with the Moebius agent and a recorded upstream writes a column script, and it is offered as a change.

### 32. Review of M5

## M6 · Desktop app

### 33. The desktop app's shell
An iced app built to `docs/design.md`: connection settings (address and token), the API client and event stream, the
collapsible grouped sidebar, the fonts (Manrope, JetBrains Mono for code) and colours.
**Done when** the app connects to a server, shows its repositories in the sidebar, and a headless render test draws it.

### 34. Board
One board per repository: columns with their cards (number, title, round, tokens, state), red for blocked and broken
rules, amber for what needs the person; a card opens its live view.
**Done when** the board renders a snapshot and moves cards on events.

### 35. Live view
A run's stream of text and tool calls with each check's decision, code with highlighting, steering and stopping, and a
shell in the worktree (a terminal on the server, shown with alacritty_terminal).
**Done when** a finished run's events render, a steer message reaches the run, and the shell echoes a command.

### 36. Inbox
Everything that needs the person in one column, filterable, with its actions (merge, approve, retry, reject).
**Done when** merging and approving from the inbox work against a test server.

### 37. Chat screen
The chat with an agent (the Moebius agent by default): the conversation with code highlighted, tool calls with their
checks, and the changes the agent offers, to merge from there.
**Done when** a chat renders against a test server and a message sent from it arrives.

### 38. Review of M6

## M7 · Agents and usage

### 39. The harness configuration repository
The server clones a configured git repository holding `agents/<name>.toml`, `agents/<name>.md`, `skills/`, `rules.md`
and `mcp.toml`; agents come from it, and edits through the API are commits to it.
**Done when** an agent defined in the repository runs, and an edit through the API is a commit.

### 40. Skills and MCP servers
An agent's skills are passed to Pi; its MCP servers (from `mcp.toml`) are given to the run, behind the router's token.
**Done when** a run gets exactly its agent's skills and MCP servers.

### 41. Agents screen
The agent list and editor (model, thinking, token budget, instructions, skills, MCP servers, extensions) and the open
proposals for an agent.
**Done when** editing an agent in the app is a commit in the configuration repository.

### 42. Usage
The usage API (tokens per day by leading model, tokens by model, by agent, where the time goes) and the usage screen with
the daily heatmap.
**Done when** the API's numbers match the stored usage and the screen renders them.

### 43. Review of M7

## M8 · Self-improvement

### 44. The Retro
At `retro_at`, the day's errors are collected without a model (failed tool calls, refusals, aborted and failed runs,
steering, CI failures, review findings) into a report; the Retro analyst agent turns it into proposals: changes to the
harness configuration, each a branch.
**Done when** a day with errors produces a report and a recorded Retro analyst produces a proposal branch.

### 45. Trials
In the trial window, each proposal is tried on the task its error happened in, on its own branch; it is positive when
the error is gone and nothing got worse (tests, rounds, tokens). Positive proposals go to the inbox as changes to merge.
**Done when** a positive and a negative trial are told apart in a test.

### 46. Replays
In the trial window, a sample of refused, reversible tool calls is run in a throwaway copy of the worktree; what happens
becomes the judgement's outcome.
**Done when** a replayed refusal gets an outcome and the original worktree is untouched.

### 47. Autoresearch
From a brief a person confirms, a baseline is measured, then the Autoresearcher proposes levers and an experiment runs
for each; every lever kept becomes its own change. It ends when the brief's budget is spent or a person stops it.
**Done when** a brief with a measurable command runs a baseline and two experiments and keeps the better one.

### 48. Grilling
A chat with the Grilling partner agent; it ends in a set of issues the person confirms, or
in a research report.
**Done when** a grilling session through the API ends in confirmed issues that become cards.

### 49. Review of M8

## M9 · Decision model

### 50. Training-set export
Judgements with their full contexts and outcomes are exported as JSON Lines, per kind, for fine-tuning.
**Done when** an export contains every judgement with its context and outcome.

### 51. Shadowing and takeover proposals
A decision model (an OpenAI-compatible endpoint) decides alongside the teacher for a kind; agreement is measured; when it
agrees often enough, a takeover proposal for that kind goes to the inbox, and merging it routes the kind to the model.
**Done when** a shadowing model's agreement is measured and a takeover proposal routes the kind.

### 52. Final review
