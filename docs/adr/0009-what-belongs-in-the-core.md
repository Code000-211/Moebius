# The core is small; forges, harnesses, the router and the Retro are plugins

The core holds only what the plugins need in common and the guarantees none of them may break: repositories (git, branches,
worktrees, merges), the store of cards, changes, runs, events and judgements, the plugin host and its API, runs and their
scheduling (limits, and one card waiting on another), the boards with their Luau runtime, the inbox, the check every tool call
passes and the enforcement of budgets, and the API the desktop app uses. Everything else is a plugin, including what Moebius
ships: the forges (GitHub, Codeberg, GitLab), the harnesses with their bridges, the router, subscription logins, the MCP
gateway, whatever decides a tool-call check (patterns, the teacher model, the decision model), the Retro with its trials,
autoresearch, grilling, runners (where a run executes) and training. A repository without a forge is fully supported: Moebius
then keeps its issues and changes itself, and a person merges in the desktop app.

## Consequences

- The core enforces, for every plugin call, a time limit and a failure that blocks rather than lets through, so that no plugin
  can make a run wait (ADR 0008) or let an unchecked tool call pass (ADR 0007).
- Budgets are enforced in the core from the usage the router reports, so that a budget does not depend on a plugin behaving.
- Subscription logins are a plugin of their own, installed separately, so that their risk under the providers' terms stays out
  of the core and out of every installation that does not want it.
- In the first version the plugins Moebius ships are compiled in, against the same API that process plugins will use over
  JSON Lines; plugins in their own process are opened up once that API has stopped changing.
- Plugins need a way to show their own views (lists, tables, charts, actions) in the desktop app, described as data, or each
  of them ends up built into the app.
