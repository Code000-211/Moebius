# Router

The router is the only model provider the harnesses see (ADR 0006). This page is for anyone who points a harness at
Moebius, configures upstreams or budgets, or reads the usage the router records. The code is in
`crates/moebius-core/src/router.rs`.

The router listens on `router.listen` (default `127.0.0.1:7172`, loopback only). Provider keys stay in the router: a
harness only ever holds its run's token. The same listener also serves the [bridge protocol](bridge-protocol.md)'s
`/moebius/*` endpoints.

## Endpoints

| Route | Protocol | Notes |
|---|---|---|
| `POST /v1/chat/completions` | `openai` | OpenAI chat completions, streamed (`"stream": true`) or not. |
| `POST /v1/messages` | `anthropic` | Anthropic messages, streamed or not. |

The body is passed on to the upstream of the same protocol, unchanged except that for a streamed OpenAI request
`stream_options.include_usage` is set to `true`, so that the upstream reports the tokens it used. The request is sent
to `<base_url><route>`, with the headers `anthropic-version` and `anthropic-beta` copied from the client, and with the
upstream's key added when `api_key_env` is set (`Authorization: Bearer` for OpenAI, `x-api-key` for Anthropic). The
upstream's status, content type and body (or stream) are passed back as they are.

An upstream is chosen by protocol: the first `[[router.upstreams]]` table with that `protocol`.

## Run tokens

Every run gets a token of the form `moebius-run-<48 hex digits>` when it starts, and the token is revoked when the run
ends. A request carries it as `Authorization: Bearer <token>` or, for Anthropic clients, as `x-api-key: <token>`. The
token identifies the run, its card and the card's token budget, so every request is attributed to a run.

The teacher model (see [rules](rules.md)) is asked through the same endpoints, with a token issued for the run being
checked and revoked after the call.

## Errors

Errors are JSON in the shape of the endpoint's protocol. For OpenAI endpoints:

```json
{"error": {"type": "budget_exceeded", "message": "the token budget of 2000000 is spent (2000413 used)"}}
```

For Anthropic endpoints the same object is wrapped as `{"type": "error", "error": {"type": ..., "message": ...}}`.

The checks run in this order:

| Status | `type` | When |
|---|---|---|
| `401` | `authentication_error` | The token is missing or unknown. |
| `429` | `budget_exceeded` | The run's budget is spent (below). |
| `500` | `api_error` | The usage cannot be read from the store. |
| `502` | `api_error` | There is no upstream for the protocol, or the upstream cannot be reached or its body cannot be read. |
| `400` | `invalid_request_error` | The body is not JSON. |

Any other status is the upstream's own, passed on.

## Budgets

An agent's `token_budget` (see [configuration](configuration.md)) is carried by every token issued for a run of that
agent. Before a request is forwarded, the router adds up the input and output tokens recorded for all runs of the same
card, the teacher's included. Cached tokens are not added. When the sum is at or above the budget, the request is
refused with `429` and `budget_exceeded`, and the message says the budget and what was used. A run without a card is
measured by its own usage. Without a `token_budget` there is no limit. A request already forwarded is never cut off, so a
card can go over its budget by what its last request used.

## Usage

After an answer, the router records one usage row per request in the store: the run, the model, input tokens, output
tokens and cached tokens. The numbers are read from the answer:

- OpenAI: `usage.prompt_tokens`, `usage.completion_tokens` and `usage.prompt_tokens_details.cached_tokens`. In a stream
  the last chunk that carries `usage` counts.
- Anthropic: `usage.input_tokens`, `usage.output_tokens` and `usage.cache_read_input_tokens`. In a stream, `message_start`
  carries `message.usage` and `message_delta` carries `usage`; later numbers replace earlier ones of the same kind.

The model is the one the upstream names in its answer, or else the one in the request. A streamed request is recorded
when its stream ends, however it ends, including when the client drops it. Nothing is recorded for an answer that is
not a success or that reports no usage.
