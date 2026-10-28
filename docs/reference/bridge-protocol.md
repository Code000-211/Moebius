# Bridge protocol

The bridge is the part of Moebius that lives inside a harness and through which every tool call is checked before it
runs. This page is for anyone who writes a bridge for another harness or tests the endpoints: it describes the two HTTP
endpoints the server offers, their JSON, the rules a client must follow so that it fails closed, and how the harness is
told where the server is. The server side is `crates/moebius-core/src/check.rs`; the reference client is the Pi
extension `crates/moebius-core/bridge/moebius-bridge.ts`.

## Endpoints

Both are served on the router's listener (`router.listen`, default `127.0.0.1:7172`), next to the model endpoints. They
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

The bridge may add `tool_call_id`, `tool` and `is_error`; the server ignores them.

Response, status `200`: `{"content": [...]}` with the same blocks. In every block of `"type": "text"` each secret is
replaced by `[redacted:<kind>]`; other blocks are unchanged. The kinds are `private-key`, `anthropic-key`, `openai-key`,
`github-token` and `aws-access-key`. The patterns are the ones listed under `no-secrets-in-commands` in [rules](rules.md).

```json
{"content": [{"type": "text", "text": "token ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}]}
```

```json
{"content": [{"type": "text", "text": "token [redacted:github-token]"}]}
```

Status codes: `200`, `401` and the `400` and `422` of the check endpoint. Nothing is recorded.

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
| `MOEBIUS_URL` | Only for a run of the Moebius agent: the desktop API's base URL, such as `http://127.0.0.1:7171` (a server that listens on all interfaces is given as `127.0.0.1`). The `moebius` binary the agent runs as `$MOEBIUS` connects there (see [the command line](cli.md)). |
| `MOEBIUS_TOKEN` | Only for a run of the Moebius agent: the run's own token, the same as `MOEBIUS_RUN_TOKEN`, which the desktop API accepts while the run lasts (see [the desktop API](desktop-api.md#authentication)). |
| `PI_CODING_AGENT_DIR` | The run's own agent directory, `<data_dir>/runs/<id>/agent`, which holds `models.json` that makes the router the `moebius` provider. |

For each run the server writes the bridge to `<data_dir>/runs/<id>/moebius-bridge.ts` and starts Pi-Bolt with
`--extension <that file>` after the agent's own `extensions`, so that it is the last extension to run, and with `--no-extensions`
and `--no-skills`, so that Pi loads no extension or skill it finds on its own, only those the agent is configured with and
the bridge. The token is revoked when the run ends.

The bridge being last is what lets it have the last word. Pi runs the handlers of an event in load order, a later
`tool_call` handler sees and may change the `input` an earlier one left, and `tool_result` handlers chain. Last, the bridge
checks the input the call will run with and redacts the content the result will carry. A harness that cannot guarantee
this order must not be used.

## The bridge as the reference client

`crates/moebius-core/bridge/moebius-bridge.ts` is a Pi extension of about sixty lines. It registers two handlers with
`pi.on`:

- `tool_call` (event fields `toolName`, `input`, `toolCallId`): asks `/moebius/check` and returns `undefined` to let the
  call run or `{ block: true, reason }` to block it.
- `tool_result` (event fields `toolCallId`, `toolName`, `content`, `isError`): asks `/moebius/result` and returns
  `{ content }` with the cleaned blocks.

A bridge for another harness does the same with that harness's hooks. See
[`examples/pi-extension/log-tool-calls.ts`](../../examples/pi-extension/log-tool-calls.ts) for the smallest extension.
