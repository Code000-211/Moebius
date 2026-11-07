# moebius-core

The Moebius server: the boards and their column scripts, the agents' runs, the checks of every tool call, the model
router, the nightly Retro and everything the desktop API serves. The `moebius` binary runs it with `moebius serve`.

## Layout

The modules at the top of `src/` tie everything together:

| Module | What it is |
|---|---|
| `app` | The store, the bus, the repositories, the forges and the decisions put together. |
| `config` | `moebius.toml`. |
| `store` | The SQLite records. |
| `blobs` | The blob store, and the redaction of secrets. |
| `bus` | The events behind the event stream. |
| `router` | The model provider. |
| `platform` | What differs between Linux and Windows. |

The other parts have one folder each:

| Folder | What it holds |
|---|---|
| `server/` | The wiring, the loops, the desktop API (`server/api/`), the settings, the package manager (`server/packages/`). |
| `repos/` | Git, the forges, GitHub, GitLab, Forgejo and their poll, the files of a repository. |
| `board/` | The board engine and its Luau sandbox, and editing a column's script. |
| `decide/` | Decision points, the tool-call check, the guard, the rules, the files a code run is hinted, the decision model. |
| `harness/` | One run lifecycle shared by every harness (`harness/run/`), Pi-Bolt and Claude Code runs and chats (`harness/pibolt/`, `harness/claude/`) with the harness picked per agent (`harness/pick.rs`), terminals, the harness configuration repository, MCP servers and the MCP gateway, skills, the Pi packages vendored into the harness configuration repository (`harness/packages/`), grilling, tool outputs kept small, runners (machines that run a harness over SSH). |
| `improve/` | The Retro, trials, the Retro's proposals to Moebius's own configuration, replays, autoresearch, the training-set export. |

Outside `src/`:

| Path | What it holds |
|---|---|
| `workflow/` | The built-in column scripts. |
| `bridge/` | The Pi extensions: the bridge, loaded into every run, and the question tool of the agents that operate Moebius. |
| `skill/` | The built-in skills, and in `vendor/` the engineering skills copied from Matt Pocock's skills with their license. |
| `testdata/` | The fake Pi, the fake `claude`, the fake `ssh`, an MCP server that echoes, a GitHub App key and the manifest of a package (`packages/`). |
| `tests/` | The integration tests, one file per feature. |

Every module is described in the [module map](../../docs/architecture.md#modules). The contracts the server keeps are
in [`docs/reference/`](../../docs/reference/).
