# Moebius — specification of the first version

This is what the first version of Moebius does and how it is built. The words are the ones in [`GLOSSARY.md`](../GLOSSARY.md);
the decisions behind the shape are in [`docs/adr/`](adr/); the look of the desktop app is in [`docs/design.md`](design.md).
Where this document and an ADR disagree, the ADR wins and this document is wrong.

## 1. What the first version does

A person connects a repository, puts issues on its board, and comes back to changes that are ready to merge. In between,
Moebius runs the workflow on its own: a harness codes in a worktree, the change is tested and reviewed, it goes back to
coding while anything is wrong, and every tool call is checked against the rules on the way. Every night the Retro looks at
what went wrong and tries out fixes, and the ones that worked wait in the inbox.

### In the first version

- One server process, `moebius`, headless, on a machine in a private network; one desktop app talking to it.
- Repositories with the **GitHub forge**, and repositories **without a forge** (plain git).
- One harness, **Pi-Bolt**, driven in its RPC mode, with the Moebius **bridge** compiled into it.
- The **router** as every harness's only model provider, with provider API keys and local OpenAI- or Anthropic-compatible
  servers (such as Radiance) upstream; budgets in tokens.
- The **MCP gateway**, skills and agents managed in the desktop app and kept in the harness configuration repository.
- The **default workflow** (section 6), with Luau scripts behind the columns and repository overrides in `.moebius/`.
- **Rules** checked on every tool call: fixed patterns and the teacher model; nothing waits for a person.
- **Judgements** recorded at every decision point, in full, with export of a training set.
- **Self-improvement** as a bundled plugin: the Retro, proposals, trials, autoresearch and shadowing.
- **Chat** with any agent, and the **Moebius agent**, which knows Moebius from the guide (`docs/guide/`) and builds
  column scripts, rules, agents and extensions on request, offered as changes.
- **Grilling** (a chat with the Grilling partner) that ends in issues a person confirms, or in a research report.
- The desktop app's seven screens: board, live view, inbox, chat, fleet, agents, usage.

### Not in the first version

- Harnesses other than Pi-Bolt; the router's fallback check for harnesses without a bridge.
- Forges other than GitHub (Codeberg, GitLab come as plugins later).
- Subscription logins (OAuth) in the router; they come as a separately installed plugin with its warning.
- Runners other than local worktrees on the server (GPU machines come as runner plugins).
- Plugins in their own process: the API is designed for them, but they are opened up only once it has stopped changing
  (ADR 0009). All plugins of the first version are compiled in.
- Training the decision model; the first version records judgements and exports them.
- Webhooks; forges are polled (ADR 0003).
- More than one person.

## 2. Processes and how they talk

```
desktop app ──(private network, token, WebSocket + HTTP)──▶ moebius (server)
                                                              │
                         ┌────────────────────────────────────┼─────────────────────────┐
                         ▼                                    ▼                         ▼
                 forges (poll APIs)               runs: pi-bolt --mode rpc      model providers
                                                   in a worktree, with the        (via the router)
                                                   bridge compiled in
```

- **`moebius`** is one binary. It holds the core and the bundled plugins (section 4), the store, the API for the desktop app,
  the router's HTTP endpoints and the MCP gateway's endpoint.
- **A run** is a `pi-bolt` process started by the Pi-Bolt plugin in the run's worktree, controlled over Pi's RPC protocol
  (JSON Lines on stdin and stdout, records split on LF only). The bridge inside it talks back to `moebius` over a local
  socket: every tool call before it runs, every tool result, every question the harness asks.
- **The router** listens on localhost; each run gets the router as its only provider and a per-run token, so that every model
  request is attributed to a run, a card and an agent.
- **The desktop app** connects only over the private network and authenticates with a token issued by the server.

## 3. The core

The core is everything that is not a plugin (ADR 0009). Its parts:

| Part | Holds | Notes |
|---|---|---|
| Repositories | git clones, branches, worktrees, merges | One clone per repository; a worktree per run; merge for repositories without a forge. |
| Store | cards, changes, runs, events, judgements, agents' bindings, budgets | SQLite for records; a content-addressed blob store on disk for large and repeating content (contexts, diffs, logs). |
| Plugin host | the plugin API and the compiled-in plugins | Every call into a plugin has a time limit and fails closed. |
| Runs and scheduling | starting, stopping and limiting runs | Global and per-column limits; a card waits for the cards it depends on; token budgets per card and per day. |
| Boards | columns, cards, transitions | Each column's behaviour is a Luau script; scripts decide through `moebius.decide`. |
| Inbox | what needs a person | Ready to merge, stuck, proposals, outside issues awaiting approval, takeover proposals from shadowing. |
| Tool-call check | the one point every tool call passes | Asks the deciders (plugins), records the judgement, answers allow or refuse with the rule. |
| Invariants | what no plugin may break | A run never waits for a person; a plugin call that times out or fails blocks; budgets are enforced from the router's usage. |
| Desktop API | the app's view of everything | Snapshots plus an event stream; plugins describe their own views as data. |

### 3.1 The store

- Records: `repository`, `card` (issue or change without an issue), `change`, `run`, `event` (everything a run emitted),
  `judgement`, `proposal`, `experiment`, `brief`, `inbox_item`, `usage` (tokens per model call).
- Blobs: content-addressed (SHA-256), deduplicated, with secrets removed before they are written.
- Judgements keep their full context as blob references; what the decision model sees is cut from it at training time
  (ADR 0010). Nothing is deleted on a schedule.

### 3.2 Decision points

Every decision point goes through one call, whoever decides:

```
decide(kind, context, options) -> { choice, reason, decider }
```

The core records a judgement for every call: kind, context blobs, options, choice, reason, decider (pattern, script, teacher
model, decision model, person), and later the outcome. A person's decisions in the desktop app (merge, reject, approve,
steer, stop) go through the same record.

## 4. Bundled plugins in the first version

| Plugin | Does |
|---|---|
| GitHub forge | Acts as its own GitHub App with webhooks off; polls issues, pull requests, checks and reviews about every 30 seconds with conditional requests; opens and updates changes; maps trusted authors and "blocked by". |
| Git forge (none) | For a repository without a forge: issues and changes live in Moebius; merging happens in the desktop app. |
| Pi-Bolt harness | Starts `pi-bolt --mode rpc` per run with the agent's model, thinking level, instructions (`--append-system-prompt`), skills (`--skill`) and extensions; maps Pi's events to run events. |
| Bridge (Pi-Bolt) | A Pi extension compiled into Pi-Bolt: on `tool_call` asks the core and blocks with the rule on refusal; on `tool_result` reports the result and removes secrets; answers extension UI questions through `decide`. |
| Router | OpenAI- and Anthropic-compatible endpoints for the harnesses; upstreams with API keys and local servers; reports usage per call; refuses calls over budget. |
| MCP gateway | One MCP server for every harness, bundling the MCP servers enabled for the run's agent. |
| Guard deciders | Fixed patterns (destructive commands, secrets, paths a rule forbids), then the teacher model for rules in prose. |
| Self-improvement | The Retro at 23:30, proposals, experiments (trials and autoresearch) in the trial window 00:00–06:00, replays of refused calls, shadowing and takeover proposals. |
| Grilling | An interactive run with the grilling partner agent; ends in a confirmed set of issues or a research report. |
| Local runner | Runs harnesses on the server in worktrees under the server's data directory. |

## 5. Agents, skills, MCP servers and rules

- The **harness configuration** is a git repository the server clones. It holds `agents/<name>.toml` (harness, model, thinking
  level, token budget, skills, MCP servers, extensions), `agents/<name>.md` (instructions), `skills/`, `rules.md` and
  `mcp.toml`. Changes made in the desktop app are commits to it; proposals are branches on it.
- A repository can add rules in `.moebius/rules.md` and override columns in `.moebius/`; both are read from its default branch
  only.
- Default agents: Coder, Reviewer, Moebius, Grilling partner, Autoresearcher, Retro analyst.
- A **chat** is an interactive run without a card: the person writes, the agent answers, and the chat can be continued,
  steered and stopped. The Moebius agent is a new chat's default; what it builds is offered as a change to merge.

## 6. The default workflow

| Column | Script | Behaviour |
|---|---|---|
| Backlog | `backlog.luau` | Outside issues wait here until a person approves them; trusted authors' issues and confirmed grilling issues move on. |
| Ready | `ready.luau` | Starts a Coder run when the card's dependencies are merged and the run limit allows. |
| In progress | `code.luau` | One Coder run per round; when it ends, the change is pushed and the card moves to review. |
| In review | `review.luau` | Waits for CI and runs the Reviewer; on failures, findings or a merge conflict, back to In progress for the next round. |
| Ready to merge | `merge.luau` | Puts the change in the inbox; a person merges it in the app or on the forge. |
| Done | `done.luau` | Records the outcome on every judgement made for the card. |

A card that reaches its round limit (default 5) or its token budget stops and goes to the inbox as stuck.

## 7. Self-improvement

- **Retro** (23:30): Moebius collects the day's errors without a model (failed tool calls, refused calls, aborted runs,
  steering, CI failures, review findings) into a report; the Retro analyst agent writes proposals from it.
- **Trials** (00:00–06:00): each proposal is tried on the task its error happened in, on its own branch; positive when the
  error is gone and nothing got worse (tests, rounds, tokens). Positive proposals go to the inbox as changes to merge.
- **Autoresearch**: starts from a brief a person confirms; measures a baseline, then the Autoresearcher finds levers (code and
  profiles, its own history, papers and the web) and runs an experiment for each; every lever kept becomes its own change.
  Ends when the brief's token budget is spent or a person stops it.
- **Replays** in the trial window: a sample of refused, reversible tool calls is run in a throwaway copy of the worktree;
  the result becomes the judgement's outcome.
- **Shadowing**: a trained decision model (outside the first version) decides alongside the teacher model; a kind of decision
  is taken over only through a proposal a person merges.

## 8. The desktop app

Native, GPU-rendered, written in Rust with iced (ADR 0003, ADR 0011), built to [`docs/design.md`](design.md). Screens:

- **Board** per repository: columns with their scripts, cards with round, tokens and state; red for blocked and broken rules.
- **Live view** of a run: the stream of text and tool calls with each check's decision, code with highlighting, a shell in
  the worktree, steering and stopping.
- **Terminal** (Setup): a shell on the server, in the directory of `moebius.toml`.
- **Inbox**: everything that needs the person, one column, filterable.
- **Chat**: a conversation with an agent, the Moebius agent by default, with code highlighted, tool calls with their
  checks, and the changes the agent offers.
- **Fleet**: every card run and chat that is working, one tile each with its last steps; a click opens the live view or the chat.
- **Agents**: the agent list and editor (model, thinking, token budget, instructions, skills, MCP servers, extensions), and
  open proposals for the agent.
- **Usage**: tokens per day by leading model, tokens by model, where the tokens go, where to save, where the model time goes,
  by agent.

## 9. Security

- Nothing on the server is reachable from the internet; the app authenticates with a token.
- Provider keys live in the router only; harnesses never see them.
- Scripts, rules and the repository's own checks (`.moebius/checks`) come only from a repository's default branch and from the harness configuration repository, never from the change being checked.
- Outside issues never start work without a person's approval; trusted authors are configured per repository.
- Secrets are removed from tool output before the model sees it and before anything is stored.
- Shell commands from scripts run only where the configuration allows them.

## 10. Build order

| Milestone | Delivers | Done when |
|---|---|---|
| M0 Skeleton | `moebius` binary, store, plugin API, config, desktop API stub | The server starts, opens its store, and answers the app's health and snapshot calls. |
| M1 One card, end to end | Git forge (none), local runner, Pi-Bolt harness, router pass-through, default workflow without review | An issue in a plain git repository becomes a merged change through one Coder run, with its events stored. |
| M2 GitHub | GitHub forge with polling, changes as pull requests, trusted authors, dependencies | The same flow works on a GitHub repository, from issue to pull request. |
| M3 Rules | Bridge in Pi-Bolt, tool-call check, patterns, teacher model, judgements | A call that breaks a rule is refused with the rule and the run goes on; every decision has a judgement. |
| M4 The loop | Review, CI, conflicts, round limit, token budgets, inbox | Cards go back and forth between coding and review on their own and stop at their limits. |
| M5 Chat and the Moebius agent | Chats, the Moebius guide, the Moebius agent | A person asks the Moebius agent for a column script in a chat and gets it as a change. |
| M6 Desktop app | Board, live view, inbox, chat | A person can follow and steer a run, chat, and merge from the app. |
| M7 Agents and usage | Harness configuration repository, agents, skills, MCP gateway, usage page | Agents are edited in the app and the usage page shows real numbers. |
| M8 Self-improvement | Retro, proposals, trials, replays, autoresearch, grilling | A proposal tried overnight is waiting in the inbox in the morning. |
| M9 Decision model | Training-set export, shadowing, takeover proposals | A trained model can shadow the teacher model and be proposed to take over. |

## 11. Open questions

- Exact formats of the plugin API messages and the desktop API; both are designed in M0.
- How the router's model catalogue is configured (which provider models map to which model class).
- How a person approves a proposal that changes Moebius itself, as opposed to a harness configuration.
