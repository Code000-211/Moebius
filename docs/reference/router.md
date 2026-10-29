# Router

The router is the only model provider the harnesses see (ADR 0006). This page is for anyone who points a harness at
Moebius, configures upstreams or budgets, or reads the usage the router records. The code is in
`crates/moebius-core/src/router/`.

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
upstream's key added when `api_key_env` or `api_key_secret` is set (`Authorization: Bearer` for OpenAI, `x-api-key` for Anthropic), or
the access token of its [OAuth login](#oauth-logins) as `Authorization: Bearer` for either protocol. The
upstream's status, content type and body (or stream) are passed back as they are.

An upstream is chosen by protocol: the first `[[router.upstreams]]` table with that `protocol`.

## Listing models

The desktop API's `GET /v1/models/available` (see the [desktop API](desktop-api.md#get-v1modelsavailable)) is answered by
the router. It asks each upstream, once even if it is configured twice, which models it serves: for `openai`,
`GET <base_url>/models`; for `anthropic`, `GET <base_url>/v1/models?limit=1000` with `anthropic-version: 2023-06-01`. Both
read `data[].id`, and send the upstream's key as for a request (`Authorization: Bearer` or `x-api-key`). An upstream has
10 seconds to answer. The answer is kept for 5 minutes, and is dropped when the upstreams are edited.

## OAuth logins

An upstream with a `[router.upstreams.oauth]` table (see [configuration](configuration.md#routerupstreamsoauth)) logs in
with OAuth 2.0 instead of sending a key. A person starts the login from the Models page, which first shows the warning
that using a subscription through a third-party tool may break the provider's terms and asks them to confirm it
(`POST /v1/models/upstreams/{place}/login` with `"accept_terms": true`, see the [desktop API](desktop-api.md)). The
login runs on the server in the background:

- `device` posts `client_id` and `scope` to `device_url` (form-encoded, as are all requests to the provider) and shows
  the `user_code` and `verification_uri` (or `verification_url`) it answers. It then polls `token_url` with
  `grant_type=urn:ietf:params:oauth:grant-type:device_code`, `device_code` and `client_id` every `interval` seconds (5
  when not given; 5 more after `slow_down`), while the answer is `authorization_pending`, until `expires_in` (900 when
  not given) has passed. `access_denied` or `expired_token` end it.
- `code` listens on `127.0.0.1:<redirect_port>` (any free port when not set) and answers `authorize_url` with
  `response_type=code`, `client_id`, `redirect_uri` (`http://127.0.0.1:<port><redirect_path>`), `scope`, a random
  `state` and an S256 `code_challenge`; the desktop app opens it in the browser. The first request to the redirect with
  that `state` ends the wait (others are ignored): its `code` is posted to `token_url` with
  `grant_type=authorization_code`, `redirect_uri`, `client_id` and the `code_verifier`; an `error` ends the login. The
  browser has 10 minutes.

The answer of `token_url` must carry `access_token`, and may carry `refresh_token` and `expires_in` (an hour when not
given). The refresh token is kept in `secrets.toml` under `oauth.secret`; the access token only in memory. When the
login has its tokens, the router asks the upstream for its models with the access token (as in [Listing
models](#listing-models)): the login holds when that succeeds.

Before each request the access token is refreshed when less than a minute of its life is left (or less than half of a
life shorter than two minutes): `grant_type=refresh_token`, `refresh_token` and `client_id` are posted to `token_url`,
and a new `refresh_token` in the answer replaces the stored one. One refresh serves every request waiting for it. An
upstream with no stored refresh token, or whose refresh is refused, answers `502` with `authentication_error` and a
message that says to log in again on the Models page; no token is ever in a message. Every token the router is given or
reads is redacted wherever it would appear (in the store, a blob or a log) as `[redacted:secret]`.

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
| `429` | `budget_exceeded` | The run's budget or the day's budget is spent (below). |
| `500` | `api_error` | The usage cannot be read from the store. |
| `502` | `api_error` | There is no upstream for the protocol, or the upstream cannot be reached or its body cannot be read. |
| `502` | `authentication_error` | The upstream logs in with OAuth and is not logged in, or its token could not be refreshed. |
| `400` | `invalid_request_error` | The body is not JSON. |

Any other status is the upstream's own, passed on.

## Budgets

An agent's `token_budget` (see [configuration](configuration.md)) is carried by every token issued for a run of that
agent. Before a request is forwarded, the router adds up the input and output tokens recorded for all runs of the same
card, the teacher's included. Cached tokens are not added. When the sum is at or above the budget, the request is
refused with `429` and `budget_exceeded`, and the message says the budget and what was used. A run without a card, such as a chat, is
measured by its own usage. Without a `token_budget` there is no limit. Tokens of runs from before the card's latest retry (a person's
`POST /v1/cards/{id}/retry`, see the [desktop API](desktop-api.md)) are not counted. A request already forwarded is never
cut off, so a card can go over its budget by what its last request used.

The daily budget, `budgets.day_tokens`, is checked first: when the input and output tokens of all runs since the start of
the server's local day are at or above it, the request is refused with `429` and `budget_exceeded`, and the message says
that the daily token budget is spent and how much was used today. It applies to every request, whatever the run.

## Usage

After an answer, the router records one usage row per request in the store: the run, the model, input tokens, output
tokens, cached tokens and how long the call took in milliseconds (from the request going upstream to the end of the
answer or stream: the model time of the usage screen) and the names of the tools the answer asks for. The numbers are
read from the answer:

- OpenAI: `usage.prompt_tokens`, `usage.completion_tokens` and `usage.prompt_tokens_details.cached_tokens`. In a stream
  the last chunk that carries `usage` counts.
- Anthropic: `usage.input_tokens`, `usage.output_tokens` and `usage.cache_read_input_tokens`. In a stream, `message_start`
  carries `message.usage` and `message_delta` carries `usage`; later numbers replace earlier ones of the same kind.

The tools are read from the same answer, in the order they are asked for, and stored as a list of names (`tools`; empty
for an answer that asks for none, which the usage screen calls answering):

- OpenAI: `function.name` of each of `choices[].message.tool_calls[]`. In a stream, of each of
  `choices[].delta.tool_calls[]` that has one (a call's name comes with its first chunk, its arguments after).
- Anthropic: `name` of each `content[]` block of type `tool_use`. In a stream, of each `content_block_start` event whose
  `content_block` is of type `tool_use`.

The model is the one the upstream names in its answer, or else the one in the request. A streamed request is recorded
when its stream ends, however it ends, including when the client drops it. Nothing is recorded for an answer that is
not a success or that reports no usage. Calls recorded before the router read tools have no tools, which the usage
screen counts as unknown.
