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
   `[1,840 lines left out; the full output is in /srv/.moebius/runs/7/out/3.txt]`.
3. When lines were left out by 1 or 2, the whole text (without secrets) is written to the next free `<n>.txt` in the
   `out` folder of the run's own directory (`<data_dir>/runs/<run id>/out/`), outside the worktree, so it is never
   committed. The server writes it and the agent reads it in ranges, by the full path the text names: the check of a
   tool call refuses only writes outside the worktree, so a read of that folder is allowed, and a launcher is asked to
   grant it for reading (see the [plugin API](plugin-api.md#launchers)). The folder is removed when the run ends (and
   when the server finds the run unended at its next start). A run on a runner cannot read the server's files, so
   nothing is saved for it: its text says how many lines were left out and names no path, and `saved` is `null`.
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

The harness starts every Pi-Bolt run (and every Claude Code run) with a cleared environment: only `PATH`, `HOME`, `USER`, `SHELL`, `TERM`, the locale (`LANG`, `LC_ALL`, `LC_CTYPE`), the temporary directory (`TMPDIR`, `TEMP`, `TMP`), the proxy settings (`HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`, `ALL_PROXY`, each in upper and lower case), the certificate settings (`SSL_CERT_FILE`, `SSL_CERT_DIR`, `NODE_EXTRA_CA_CERTS`), the toolchain places (`CARGO_HOME`, `RUSTUP_HOME`) and, for Windows, `SYSTEMROOT`, `WINDIR`, `SystemDrive`, `ProgramData`, `ProgramFiles`, `USERNAME`, `USERPROFILE`, `HOMEDRIVE`, `HOMEPATH`, `APPDATA`, `LOCALAPPDATA`, `PATHEXT`, `COMSPEC` and `CLAUDE_CODE_GIT_BASH_PATH` are taken from the server's environment, so that a provider key named by `api_key_env` never reaches an agent. The same holds for the Pi-Bolt terminal interface of a chat. It sets these variables:

| Variable | Value |
|---|---|
| `MOEBIUS_CHECK_URL` | The router's base URL, such as `http://127.0.0.1:7172`, without a trailing slash. The client appends `/moebius/check` and `/moebius/result`. |
| `MOEBIUS_RUN_TOKEN` | The run's token, of the form `moebius-run-<48 hex digits>`. |
| `MOEBIUS_TOOL_OUTPUT` | The agent's `tool_output` as JSON, such as `{"hide_after_turns":10,"max_lines":200,"max_bytes":16384,"compress":true}`. The bridge hides older outputs by it and sends it with each result. |
| `MOEBIUS_REPOSITORY` | Only for a run of the Moebius agent or the Grilling partner in a repository: its id, which `"$MOEBIUS" issue create` and `issue list` take. |
| `MOEBIUS_RUN` | Only for a run of the Moebius agent or the Grilling partner: the run's id, which the agent names its claims by (`"$MOEBIUS" issue claim <id> --as "wayfinder-$MOEBIUS_RUN"`). |
| `MOEBIUS_URL` | Only for a run of the Moebius agent or the Grilling partner: the desktop API's base URL, such as `http://127.0.0.1:7171` (a server that listens on all interfaces is given as `127.0.0.1`). The `moebius` binary the agent runs as `$MOEBIUS` connects there (see [the command line](cli.md)). |
| `MOEBIUS_TOKEN` | Only for a run of the Moebius agent or the Grilling partner (the desktop API accepts the latter's for the issue tracker only): the run's own token, the same as `MOEBIUS_RUN_TOKEN`, which the desktop API accepts while the run lasts (see [the desktop API](desktop-api.md#authentication)). |
| `PI_CODING_AGENT_DIR` | The run's own agent directory, `<data_dir>/runs/<id>/agent`, which holds `models.json` that makes the router the `moebius` provider. |

For each run the server writes the bridge to `<data_dir>/runs/<id>/moebius-bridge.ts` and starts Pi-Bolt with
`--extension <bridge>` after the agent's own `extensions` (and, for the Moebius agent and the Grilling partner, after `<data_dir>/runs/<id>/moebius-ask.ts`, the question tool, [below](#the-question-tool)), so that the bridge is the last extension to run, and with `--no-extensions`
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

## The question tool

`crates/moebius-core/bridge/moebius-ask.ts` registers `ask_user_question` for the chats of the Moebius agent and the
Grilling partner (the agents that operate Moebius); no other agent has it, and Claude Code's own `AskUserQuestion` is
switched off (`--disallowedTools`), because it is a picker for a terminal. The model puts a few questions to the person
with options to choose from, instead of asking in prose.

- Arguments: `questions`, one to four, each with `question` (the whole question), `header` (a word or two, at most 16
  characters), `options` (two to four, each with a `label` of at most 60 characters, which ends in "(Recommended)" for the
  one the agent recommends, and a `description`) and `multiSelect` (false by default). The person can always write their
  own answer, so the agent adds no "Other". The web app cuts a header or label past its limit and keeps the whole in a
  tooltip.
- Returns at once with a text that tells the model to end its turn without answering for the person. The call waits for
  nobody, so a run never holds a tool open for a person and a chat that was paused meanwhile loses nothing. The
  `tool_execution_start` event of the call is the question: a chat that waits and whose last call is this tool, with no
  message of the person after it, has `asks` set in the list of chats.
- The answer is the person's next message (`POST /v1/chats/{id}/messages`, no route of its own). The web app writes it in
  one fixed shape, which is also the only shape it reads back to mark the chosen options of a question that was
  answered:

  ```
  My answers:
  1. Database: "SQLite"
  2. Extras: "Metrics", "Tracing", Other: "my own words"
  ```

  The first line is `My answers:`; then one line for each question, in the order asked: its place, a full stop, its
  header (its question when it has none, on one line), a colon, and a comma-separated list of JSON strings: the `label`
  of each chosen option, whole, and after `Other:` the person's own words. A label is matched whole against the options
  of the question at that place, so "Yes" does not mark "Yes, but later"; two questions with one header are told apart by
  their place; words that hold a label are the person's own words. A message of any other shape marks nothing. An agent
  reads the message as it is, and answers a person who wrote theirs by hand just as well.

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
