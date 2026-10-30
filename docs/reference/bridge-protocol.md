# Bridge protocol

The bridge is the part of Moebius that lives inside a harness and through which every tool call is checked before it
runs. This page is for anyone who writes a bridge for another harness or tests the endpoints: it describes the HTTP
endpoints the server offers (the check, the result and the lessons), their JSON, the rules a client must follow so that it fails closed, and how the harness is
told where the server is. It also describes the [MCP gateway](#the-mcp-gateway), the one MCP server every harness is
given, which checks the calls of MCP tools on the server's side. The server side is `crates/moebius-core/src/decide/check/`; the reference client is the Pi
extension `crates/moebius-core/bridge/moebius-bridge.ts`.

## Endpoints

The endpoints below are served on the router's listener (`router.listen`, default `127.0.0.1:7172`), next to the model endpoints. They
take and give JSON.

### Authentication

Every request carries the run's token as `Authorization: Bearer <token>` (the token is also accepted as `x-api-key`). It is
the same token the harness uses as its API key for the model, and it is valid only while the run is going. An unknown,
missing or expired token is answered with `401` and `{"error": "not a run's token"}`.

### `POST /moebius/check`

Asks whether a tool call may run. Send it before every tool call.

Request:

| Field | Type | Meaning |
|---|---|---|
| `tool` | string | The tool's name, such as `bash`. |
| `input` | any JSON | The call's arguments. |
| `tool_call_id` | string | The harness's id for the call. |

All three are required.

Response, status `200`:

| Field | Type | Meaning |
|---|---|---|
| `decision` | string | `allow` or `refuse`. |
| `reason` | string | Why. For a refusal it starts with the rule's name, as in `no-force-push: git push --force rewrites history on the remote`. |

```sh
curl -s -X POST "$MOEBIUS_CHECK_URL/moebius/check" \
  -H "authorization: Bearer $MOEBIUS_RUN_TOKEN" -H 'content-type: application/json' \
  -d '{"tool":"bash","input":{"command":"git push --force"},"tool_call_id":"call-1"}'
```

```json
{"decision":"refuse","reason":"no-force-push: git push --force rewrites history on the remote"}
```

What the server does with the call: it asks the decision point `tool_call` (see the [plugin API](plugin-api.md) for what a
decider receives and [rules](rules.md) for what the guard refuses), records one judgement, stores a run event
`tool_check` with `{tool, tool_call_id, decision, reason}` and publishes it as a `run_event` on the desktop API's event
stream. Anything that fails while deciding (a decider that times out, panics or answers nonsense, a missing teacher) is
answered as `refuse` with status `200` and a reason that explains it.

Status codes:

| Status | Meaning |
|---|---|
| `200` | An answer, `allow` or `refuse`. |
| `401` | Not a run's token. |
| `400`, `422` | With a valid token: the body is not JSON (`400`), or lacks a field or has the wrong type (`422`). The token is checked first, so without one the answer is always `401`. |
| `500` | `{"error": "cannot decide"}`: the server could not even record the decision. The client must treat it as a refusal. |

### `POST /moebius/result`

Reports a tool's result before the model sees it, so that secrets can be removed. Send it for every tool result.

Request:

| Field | Type | Meaning |
|---|---|---|
| `content` | array of JSON objects | The result's content blocks, as the harness has them (for Pi: `{"type": "text", "text": "..."}` and other block types). Required. |

| `tool_call_id` | string | The harness's id for the call. Optional; named in the `tool_output` event. |
| `tool` | string | The tool's name. Optional; with `bash`, the command's output can be compressed by what the command is. |
| `input` | any JSON | The call's arguments. Optional; for `bash`, `input.command` is the command line. |
| `is_error` | boolean | Whether the call failed. Optional; not read. |
| `tool_output` | object | The agent's budget: `{"hide_after_turns", "max_lines", "max_bytes", "compress"}`, as `agents.<name>.tool_output` in [configuration](configuration.md) (a key left out has its default; an unknown key is `422`). Optional; without it the result is only cleaned of secrets. |

Response, status `200`: `{"content": [...]}` with the same blocks. In every block of `"type": "text"` each secret is
replaced by `[redacted:<kind>]`; other blocks are unchanged. The kinds are `private-key`, `anthropic-key`, `openai-key`,
`github-token`, `gitlab-token` and `aws-access-key`. The patterns are the ones listed under `no-secrets-in-commands` in [rules](rules.md). With `tool_output`, each text is then made smaller where it is
made (the server side is `crates/moebius-core/src/harness/output/`):

1. With `compress`: terminal escape codes go, a line keeps what follows its last carriage return, and a line that is only
   a progress bar goes. For a `bash` command line that runs `cargo test`, `pytest`, `go test`, `jest` or `vitest` (also
   as `npm test`, `pnpm test` and the like) or a cargo build (`build`, `check`, `clippy`, `doc`, `run`, `install`), the
   lines known to be noise go: passing tests, test binaries in which every test passed, build progress, and a cargo
   diagnostic that repeats is kept once with `(×<n>)`. A line that is not known is kept, so error messages and failing
   tests' output stay. A note at the end says how many lines were left out and what they were. Last, each run of the
   same line becomes one line followed by `(×<n>)`.
2. Above `max_lines` or `max_bytes`, the head and the tail are kept, half the budget each, around
   `[1,840 lines left out; the full output is in .moebius/out/3.txt]`.
3. When lines were left out by 1 or 2, the whole text (without secrets) is written to the next free
   `.moebius/out/<n>.txt` of the run's worktree, which the agent can read in ranges; the directory holds a `.gitignore`
   of `*`, so it is never committed.
4. Each text made smaller is stored as a `tool_output` run event: `{"tool", "tool_call_id", "steps", "bytes_before",
   "bytes_after", "lines_before", "lines_after", "saved"}`, where `steps` lists what was done (`clean`, the compressor's
   name such as `cargo-test`, `fold`, `budget`) and `saved` is the path of step 3 or `null`. `GET /v1/usage` adds them
   up.

```json
{"content": [{"type": "text", "text": "token ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}]}
```

```json
{"content": [{"type": "text", "text": "token [redacted:github-token]"}]}
```

Status codes: `200`, `401` and the `400` and `422` of the check endpoint. Nothing but the `tool_output` events is
recorded.

### `POST /moebius/lessons`

Looks up the lessons the Retro learned from earlier cards (see [the Retro](harness-configuration.md#the-retro)). The Pi
bridge offers it as the tool `lessons(query)`; a bridge for another harness may offer the same. The repository searched
is the one of the run's card, or, for a chat, the one it works in; a run with neither gets no lessons. Only active
lessons are found, best match first by BM25 over the situation, the lesson and the advice, at most 8.

Request: `{"query": string}`, the words of the task or the problem. Response: `{"lessons": [...], "text": string}`:
`lessons` are the stored lessons as `GET /v1/lessons` has them, and `text` is what the agent is shown, under the heading
"Lessons from earlier cards", or `No lessons match.` Each lesson found counts as retrieved, which keeps it from being retired
for lack of use. Errors: `401` for a token that is not a run's; `500` when the lessons cannot be searched, which the tool
reports to the agent as text.

## Rules for a client

A client must fail closed. Do this:

1. Before a tool call, `POST /moebius/check`. Run the call only if the answer is status `200` with `decision` equal to
   `allow`. Block it for a `refuse`, for any other status, for a `decision` that is neither value, for an unreachable
   server, for an unparsable body and when the two environment variables below are not set.
2. Give the model the refusal's `reason` as the tool's result, so that it can find another way. A refusal is an answer:
   do not stop the run and never wait for a person.
3. Before the model sees a tool result, `POST /moebius/result` and use the `content` of the answer. If that fails for any
   reason, withhold the result: replace its text blocks with a message saying that the result could not be checked for
   secrets. Also drop fields that may carry the same text again, such as `structuredContent` in Pi.

The reference client does exactly this: a blocked call carries `Moebius refused this call: <reason>`, or `Moebius refused
this call: the check could not be reached (<error>)`; a withheld result says `Moebius withheld this result because it could
not be checked for secrets.`

## The environment of the harness

The harness starts every Pi-Bolt run with these variables set:

| Variable | Value |
|---|---|
| `MOEBIUS_CHECK_URL` | The router's base URL, such as `http://127.0.0.1:7172`, without a trailing slash. The client appends `/moebius/check` and `/moebius/result`. |
| `MOEBIUS_RUN_TOKEN` | The run's token, of the form `moebius-run-<48 hex digits>`. |
| `MOEBIUS_TOOL_OUTPUT` | The agent's `tool_output` as JSON, such as `{"hide_after_turns":10,"max_lines":200,"max_bytes":16384,"compress":true}`. The bridge hides older outputs by it and sends it with each result. |
| `MOEBIUS_URL` | Only for a run of the Moebius agent: the desktop API's base URL, such as `http://127.0.0.1:7171` (a server that listens on all interfaces is given as `127.0.0.1`). The `moebius` binary the agent runs as `$MOEBIUS` connects there (see [the command line](cli.md)). |
| `MOEBIUS_TOKEN` | Only for a run of the Moebius agent: the run's own token, the same as `MOEBIUS_RUN_TOKEN`, which the desktop API accepts while the run lasts (see [the desktop API](desktop-api.md#authentication)). |
| `MOEBIUS_LENS` | The lens's plan as JSON (see [the lens](#the-lens)). Only for Pi-Bolt; absent or `enabled: false` makes the lens do nothing. |
| `PI_CODING_AGENT_DIR` | The run's own agent directory, `<data_dir>/runs/<id>/agent`, which holds `models.json` that makes the router the `moebius` provider. |

For each run the server writes the bridge to `<data_dir>/runs/<id>/moebius-bridge.ts` and the lens (below) to
`<data_dir>/runs/<id>/moebius-lens.ts`, and starts Pi-Bolt with `--extension <lens>` and then `--extension <bridge>`
after the agent's own `extensions`, so that the bridge is the last extension to run, and with `--no-extensions`
and `--no-skills`, so that Pi loads no extension or skill it finds on its own, only those the agent is configured with and
the bridge. The token is revoked when the run ends.

The bridge being last is what lets it have the last word. Pi runs the handlers of an event in load order, a later
`tool_call` handler sees and may change the `input` an earlier one left, and `tool_result` handlers chain. Last, the bridge
checks the input the call will run with and redacts the content the result will carry. A harness that cannot guarantee
this order must not be used.

## The bridge as the reference client

`crates/moebius-core/bridge/moebius-bridge.ts` is a Pi extension of about a hundred and fifty lines. It registers the tool `lessons` with `pi.registerTool` (its
only parameter is `query`; it asks `/moebius/lessons` and shows `text`, or says that the lessons could not be looked up)
and three handlers with `pi.on`:

- `tool_call` (event fields `toolName`, `input`, `toolCallId`): asks `/moebius/check` and returns `undefined` to let the
  call run or `{ block: true, reason }` to block it.
- `tool_result` (event fields `toolCallId`, `toolName`, `input`, `content`, `isError`): asks `/moebius/result` with the
  agent's `tool_output` and returns `{ content }` with the cleaned blocks.
- `context` (event field `messages`, the conversation of one model request): with `hide_after_turns` above 0, replaces
  the content of each `toolResult` message whose call is older than the last `hide_after_turns` assistant messages by
  one text block, `[<tool> <input as JSON, cut at 200 characters>: output hidden: <n> lines]`. A result with `isError`
  is kept, and so is one shorter than its placeholder. Only the request changes: Pi's session, and the
  `tool_execution_end` run events, keep every output. When anything fails here, the messages are left as they are.

A bridge for another harness does the same with that harness's hooks. See
[`examples/pi-extension/log-tool-calls.ts`](../../examples/pi-extension/log-tool-calls.ts) for the smallest extension.

## The lens

The lens gives an agent fast feedback on its own edits: the Pi extension `crates/moebius-core/bridge/moebius-lens.ts`,
loaded just before the bridge. The checks run in the extension, in the run's worktree; the server decides what the agent
is told (through `decide`) and stores the time each check took. The server side is `crates/moebius-core/src/harness/lens/`.

### What the extension does

- **After an edit** (the `edit` and `write` tools, when the call did not fail): within the budget (`budget_ms`, default
  1500, for all the checks of the one edit together, run in parallel), a syntax check of the file and the repository's
  fast linter on that file alone. The built-in syntax checks are a JSON parser for `.json`, `python3` for `.py` and
  `bash -n` for `.sh` and `.bash`; `syntax = false` in `.moebius/lint.toml` turns them off. A command that is not
  finished by the end of the budget is killed and skipped, and the result says so once per tool and run. A program that is
  not installed is skipped for the rest of the run, silently apart from the `lens_check` event.
- **Only new problems.** Before the first edit of a file in the run, the extension runs the same checks on the file as it
  is, the baseline; later edits take the previous edit's diagnostics as theirs. A problem is *new* when the file now has
  more problems with the same tool, rule and message than the baseline had (the line does not count, since an edit moves
  lines; of equal problems the ones off the touched lines are the old ones). A problem is on a *touched line* when its
  line is between the first and the last line of the file that the edit changed. Without a baseline (the tool timed out
  then, or the file is new) only the problems on touched lines are new.
- **Appended to the result.** What the server lets through is added to the tool's result as one more text block,
  `Moebius lens: <n> new problems after this edit of <file>:` and a line `<file>:<line>:<column> <severity> <rule>
  <message> (<tool>)` for each, at most `max_lines` (default 10), errors first. The bridge, which comes next, redacts and
  shrinks it like the rest of the result.
- **Before the run settles** (Pi's `agent_before_settle`): the files the run changed (the edit tools' and whatever
  `git diff --name-only HEAD` and untracked files add, so the shell's changes count) are put to the server, and if it
  says to run, the `deep` command of each check whose glob matches a changed file runs once with a timeout of
  `deep_timeout_seconds` (default 120) each: `tsc --noEmit` incrementally, `pyright` on the files, `cargo check` once
  for each package a changed file belongs to (`--manifest-path {manifest}`, so a workspace is not checked whole for a
  change in one of its parts, and its `target` stays warm through the run), `go vet` for the packages. The findings that
  are in a changed file and were not told before come as one follow-up message, `Moebius lens: the type check found <n>
  new problems in files you changed:`, and the agent goes on. It asks at most three times per run, so a check that
  cannot be satisfied does not loop.
- It never starts a language server.
- It never stops a call or the run: when anything in the lens fails, the result is left as it is. When the server cannot
  be asked, it takes the safe side: it shows every new finding and runs the end-of-turn check.

### The plan

The plan in `MOEBIUS_LENS` is read by the server from the worktree when it starts Pi, from `.moebius/lint.toml` or, without
`[[check]]` entries in it, detected from the repository's files (see [harness configuration](harness-configuration.md#moebiuslinttoml)).
It is JSON: `{"enabled", "budget_ms", "max_lines", "deep_timeout_ms", "syntax", "checks": [{"name", "files", "edit",
"deep", "syntax", "format", "pattern", "severity"}], "error"}`, where `files` is a regular expression over the path
from the worktree, and `error` says why `.moebius/lint.toml` could not be read (the lens then does nothing but report
it as a `lens_check` event).

### Endpoints

All three are on the router's listener, authenticated like the check endpoints (the run's token; `401` without one, and
the token is checked before the body: `400` for a body that is not JSON, `422` for the wrong shape). They answer `500`
when the server cannot decide; the extension then takes the safe side.

#### `POST /moebius/lens/report`

After an edit. Request: `file` (from the worktree), `edit` (`{"tool", "first_line", "last_line", "excerpt"}`: the lines
the edit changed and their new text), `max_lines`, `findings` (the new findings, each `{"tool", "severity": "error" |
"warning", "line", "column"?, "rule"?, "message", "touched", "key"}`), `present` (the `key` of every finding the file has
now, new or old) and `checks` (`[{"phase": "syntax" | "lint" | "deep", "tool", "ms", "skipped"?, "findings"}]`), and
optionally `tool_call_id`.

Each finding is put to the decision point `lint_report` (see [the decision model](decision-model.md#the-lens-decisions)),
errors first and then by line, at most 40 of them; the answer is `{"shown": [positions in findings], "hidden": n}`. A
finding that was judged before in this run and that `present` no longer holds gets the outcome `fixed`. Each entry of
`checks` is stored as a `lens_check` run event, `{"file", "tool_call_id", "phase", "tool", "ms", "skipped", "findings",
"shown"}`.

#### `POST /moebius/lens/deep`

Before the run settles. Request: `files` (`[{"path", "comment_only"}]`: the files the run changed, and whether every
edit to the file only changed comments), `round` (from 1) and `tools` (the names of the commands that would run). The
question is `deep_check`; the answer is `{"run": boolean, "judgement_id": number or null}`. A failure to decide answers
`run: true`.

#### `POST /moebius/lens/deep-result`

After the end-of-turn check ran. Request: `judgement_id` (from the answer above), `new_findings` (how many findings
the agent was told about), `checks` (as above, with `shown`). Stores a `lens_check` event for each check, and the
outcome of the judgement: `found something new`, `nothing new`, or `not run` when every check was skipped. The
judgement must be one of this run's `deep_check` judgements. Answer: `{}`.

## The MCP gateway

The MCP gateway is Moebius's one MCP server for every harness: it bundles the MCP servers of the run's agent (see
[harness configuration](harness-configuration.md#mcptoml)), so that every call of their tools passes through Moebius.
The server side is `crates/moebius-core/src/harness/gateway/`.

### `POST /mcp`

Served on the router's listener next to the check, as MCP's streamable HTTP transport. Authentication is the same as
above: the run's token as `Authorization: Bearer <token>` (or `x-api-key`), checked before the body; an unknown,
missing or expired token is answered `401` with `{"error": "not a run's token"}`. Pi-Bolt is given it in its
`mcp.json` as the server `moebius`:

```json
{"mcpServers": {"moebius": {"url": "http://127.0.0.1:7172/mcp", "headers": {"Authorization": "Bearer moebius-run-..."}, "exposure": "direct"}}}
```

The body is one JSON-RPC 2.0 message or an array of them. A request is answered `200` with its JSON-RPC response as
`application/json` (an array for an array); notifications and responses are taken with `202` and no body; a body that
is not JSON is `400` with a JSON-RPC parse error (`-32700`). The gateway gives no `Mcp-Session-Id`: the run's token is
the session. `GET /mcp` and `DELETE /mcp` are answered `405` with a valid token (there is no stream from the server and
no session to end), `401` without one.

| Method | Answer |
|---|---|
| `initialize` | Starts the run's servers (below), then `{"protocolVersion"` (the client's), `"capabilities": {"tools": {"listChanged": false}}`, `"serverInfo": {"name": "moebius", "version"}}`. |
| `ping` | `{}`. |
| `tools/list` | `{"tools": [...]}`: every tool of every server of the run that could be reached, as the server describes it, named `<server>__<tool>`. All in one page. |
| `tools/call` | `{"name": "<server>__<tool>", "arguments"}`: the call checked, then made (below). A name the run does not have is the error `-32602`. |
| anything else | The error `-32601`. |

At the run's first request the gateway starts each stdio server of the run as a child process of the Moebius server,
in the run's worktree, and connects to each remote one, sending its `headers`; it initializes each and lists its tools.
A server that cannot be started, reached or initialized within 60 seconds leaves out its tools and is stored as an
`mcp_error` run event, `{"server", "message"}`. When the run ends, its token is withdrawn and its servers are stopped.

Every `tools/call` is a tool call like any other: it goes through `decide("tool_call")` with the tool named
`mcp__<server>__<tool>`, the `arguments` as its input and `mcp-<JSON-RPC id>` as its `tool_call_id`, and leaves one
judgement (see [above](#post-moebiuscheck)). A refused call never reaches the server; its result is
`{"content": [{"type": "text", "text": "Moebius refused this call: <reason>"}], "isError": true}`, and the reason names
the rule. Anything that fails while deciding refuses the call. An allowed call goes to the server, which may take ten
minutes; what it answers comes back with every secret in its strings replaced by `[redacted:<kind>]`, as from
`/moebius/result`, and a server that fails gives `isError: true` with why. Every call is stored and published as an
`mcp_call` run event, `{"server", "tool", "tool_call_id", "input", "decision", "reason", "result"}`, with secrets
removed.

A harness with a bridge checks the call under its own name too (Pi-Bolt names the tools
`mcp__moebius__<server>__<tool>`), so with Pi-Bolt each call of an MCP tool leaves two judgements; a harness without a
bridge is still checked by the gateway.
