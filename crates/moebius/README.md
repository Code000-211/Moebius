# moebius

The `moebius` binary.

| Command | What it does |
|---|---|
| `moebius init` | Writes a commented `moebius.toml` and a harness configuration repository. |
| `moebius serve` | Runs the server. |
| `moebius check-script` | Runs a column script once against a made-up card. |
| `moebius export` | Writes the training set. |

Every other command operates a running server through the desktop API, for example `moebius board`,
`moebius issue create` or `moebius agent save`. `moebius api` reaches any route. The Moebius agent uses these
commands as `"$MOEBIUS"`.

## Layout

| Path | What it holds |
|---|---|
| `src/main.rs` | `init`, `serve`, `check-script`, `export`, and the start. |
| `src/cli/mod.rs` | The commands that operate a server: the arguments, the dispatch and the shared printing. |
| `src/cli/client.rs` | The connection to the server. |
| Other modules of `src/cli/` | One group of commands each. |

The commands are documented in [`docs/reference/cli.md`](../../docs/reference/cli.md) and
[the guide](../../docs/guide/command-line.md).
