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

## Choosing the upstream

A request goes to the `[[router.upstreams]]` tables with the endpoint's `protocol`, in the order of the file: to the
first, and to the next when one fails before it answers. An upstream fails before it answers when it cannot be reached,
when it logs in with OAuth and is not logged in (or its token cannot be refreshed), or when it answers `5xx`, `429` or
`404` (it does not serve the model); its answer is then dropped unread. Once an upstream has answered with any other
status, its answer is passed on and nothing falls back any more: no request is sent twice after a byte of an answer
reached the harness. When the last upstream fails too, the harness gets its answer, or the `502` of its failure.

## Model classes

A request whose `model` is the name of a class of `[router.classes]` (see
[configuration](configuration.md#routerclasses)) is served as that model name: the router tries the class's models in
order, each on every upstream of the protocol as above, setting the body's `model` to the model tried. A model whose
`day_tokens` are spent (its input and output tokens since the start of the server's local day, counted over all its
usage, at or above `day_tokens`) is passed over; when every model of the class is, the request is refused with `429`
and `budget_exceeded`. Usage that cannot be read counts as spent. A class takes the place of a model of the same name;
a request naming any other model goes to that model alone.

`GET /v1/models/available` lists the classes beside the models (see the [desktop
API](desktop-api.md#get-v1modelsavailable)), with the protocols the upstreams list any of their models under.

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

A `code` login can also end by the person pasting what the browser showed: the code, `code#state` (as the code page of
a provider shows it), or the whole address the browser ended on, which does not have to load (`POST
/v1/models/upstreams/{place}/login/code`, see the [desktop API](desktop-api.md)). The `code` and `state` are read from
it; a `state` that is not the login's is refused, one that is missing is taken to be it. This is for a provider whose
page shows a code, a browser on another machine than the server (over SSH, say), and a loopback port that is taken (a
login without a listener waits for a pasted code only). The listener stays for when it works. The redirect host is
`localhost` for a preset (as the provider registered it) and `127.0.0.1` otherwise; the listener is always on
`127.0.0.1`.

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

### OAuth requests per provider

Codes and refresh tokens are posted as a form, except for the `anthropic` preset, which takes a JSON body (and `state`
with the code), as Pi does.

- `anthropic`: every request with the token (and the model list) is sent with `Authorization: Bearer`, no `x-api-key`,
  and the headers `user-agent: claude-cli/2.1.280`, `x-app: cli`, `anthropic-dangerous-direct-browser-access: true` and
  `accept: application/json`. The `anthropic-beta` header is the harness's features followed by `claude-code-20250219`
  and `oauth-2025-04-20`, none twice. The system prompt of the request starts with the block `You are Claude Code,
  Anthropic's official CLI for Claude.` (a string or blocks already there follow it). Why: the subscription is granted
  to Claude Code, and the API refuses its token for a request that does not identify itself as Claude Code; Pi does the
  same. Pi also writes the names of the tools in Claude Code's casing; the router does not.
- `openai`: a ChatGPT Plus/Pro subscription, whose backend takes requests only in its own Codex Responses protocol. The
  router translates, following the `openai-codex` provider of Pi (commit `f09d01b85e0384647fcab6779d35ab9dabe04be3` of
  `badlogic/pi-mono`, `packages/ai/src/api/openai-codex-responses.ts`). The request to `POST /v1/chat/completions` is
  sent to `{base_url}/codex/responses` (a `base_url` that already ends in `/codex` or `/codex/responses` is not
  extended twice) as a Responses request: `store: false`, `stream: true` (also when the harness did not stream),
  `instructions` the system and developer messages before the first other message joined by a blank line (`You are a
  helpful assistant.` without any), `input` the other messages (a user message's `text` and `image_url` parts as
  `input_text` and `input_image`; an assistant message's text as a `message` item and its `tool_calls` as
  `function_call` items; a `tool` message as a `function_call_output`; a later system message as a `developer`
  message), `tools` as flat `function` tools with `strict: null`, `tool_choice` (`auto` when absent),
  `parallel_tool_calls: true`, `text.verbosity: low`, `temperature` when set, `reasoning` (`effort` from
  `reasoning_effort`, `summary: auto`) when `reasoning_effort` is set, and a `prompt_cache_key` that is a hash of the
  run's token. Other fields (`max_tokens`, `stream_options`, ..) are not sent. The headers are `Authorization: Bearer`
  with the access token, `chatgpt-account-id` read from the token's claims
  (`https://api.openai.com/auth`, `chatgpt_account_id`), `originator: moebius`, `OpenAI-Beta: responses=experimental`,
  `accept: text/event-stream`, and `session-id` and `x-client-request-id` with the cache key. A token that names no
  account answers `502` with `authentication_error`, and the next upstream is tried.

  The answer is read as Responses events and given back as chat completions, streamed (`chat.completion.chunk`
  events, ending with a chunk of `usage` and `data: [DONE]`) or, when the harness did not stream, as one
  `chat.completion`: text (`response.output_text.delta`, a refusal as text), tool calls (`function_call` items, with
  their arguments from the argument events or whole from the finished item), the finish reason (`tool_calls` when the
  answer asked for a tool, `length` for an incomplete answer cut by `max_output_tokens`, `content_filter`, otherwise
  `stop`) and `usage` (`prompt_tokens`, `completion_tokens`, `total_tokens` and `cached_tokens`). The model's
  reasoning is not given back. An `error` or `response.failed` event ends a stream with a chunk holding an `error`
  object, and answers a request that is not streamed with `502` and `api_error`; so does an answer that ends before its
  last event. An error status of the backend (`400`, `401`, `429` for the usage limit, ..) is passed on with its status
  and, when its body has no `error` object, its `detail` or `message` in one.

  Everything after the translation is the path of every other upstream: a checked token's tool calls are checked, a
  refused one replaced by a text; the tokens are counted against the run, and the run's token, the card's budget and the
  day's budget apply. The upstream lists the models its subscription serves, which are Pi's list for a ChatGPT account:
  `gpt-6.1-sol`, `gpt-6.1-terra`, `gpt-6-astra`, `gpt-6-sol`, `gpt-6-luna`, `gpt-5.6-luna`, `gpt-5.6-sol` and
  `gpt-5.6-terra`; the list needs a login but asks the backend nothing. Pi's WebSocket transport, reasoning replay and
  tool search are not used.

## Run tokens

Every run gets a token of the form `moebius-run-<48 hex digits>` when it starts, and the token is revoked when the run
ends. A request carries it as `Authorization: Bearer <token>` or, for Anthropic clients, as `x-api-key: <token>`. The
token identifies the run, its card and the card's token budget, so every request is attributed to a run.

The teacher model (see [rules](rules.md)) is asked through the same endpoints, with a token issued for the run being
checked and revoked after the call.

## The check of a harness without a bridge

Pi-Bolt asks for every tool call through its bridge (see the [bridge protocol](bridge-protocol.md)). Claude Code has no
bridge, so the token of a run on Claude Code is a checked one, and the router checks the answers to it instead:

- Every tool call in the answer, OpenAI `tool_calls` of a choice's message and Anthropic `tool_use` blocks, streamed or
  not, goes through `decide("tool_call")` before the answer reaches the harness, with the same question the bridge's
  `/moebius/check` asks (`tool` the tool's name, `input` its arguments as JSON, `tool_call_id` its id, and the rules that
  apply). It leaves the same judgement and `tool_check` run event. OpenAI arguments are parsed as JSON; arguments that
  are not JSON are given as text.
- An allowed call is passed on. A refused call is replaced by the text `Moebius refused the tool call <tool> (<id>):
  <reason>`, whose reason names the rule: an Anthropic `tool_use` block becomes a text block, an OpenAI call leaves
  `tool_calls` and its text is added to the message's `content`. When no call is left, `stop_reason` `tool_use` becomes
  `end_turn` and `finish_reason` `tool_calls` becomes `stop`, so the harness ends its turn with the refusal as its answer.
- Anything that fails while deciding refuses (fail closed).
- A stream is passed on event by event as it comes, except a tool call's events, which are held back until the call is
  complete: an Anthropic `tool_use` block from its `content_block_start` to its `content_block_stop`, then passed on as
  it came or replaced by a text block of the same index; OpenAI tool-call deltas until the choice's `finish_reason` (or
  `[DONE]`), then each allowed call in one chunk (renumbered from 0) and each refused one as a `content` delta. A call
  the stream did not finish is never passed on. Tokens are counted from the stream as it came.
- The tool results in such a run's requests (OpenAI messages with the role `tool`, Anthropic `tool_result` blocks, as
  text or text blocks) have their secrets removed before the request goes upstream, as `/moebius/result` removes them
  for the bridge.

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
| `429` | `budget_exceeded` | The request names a model class whose every model has spent its `day_tokens` ([model classes](#model-classes)). |
| `500` | `api_error` | The usage cannot be read from the store. |
| `502` | `api_error` | There is no upstream for the protocol, or the last upstream tried cannot be reached, or the body of the upstream that answered cannot be read. |
| `502` | `authentication_error` | The last upstream tried logs in with OAuth and is not logged in, or its token could not be refreshed. |
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

The model is the one the upstream names in its answer, or else the one in the request. For a request that names a
model class, the model is the class's model that answered, as the class names it, and the class is recorded beside it;
the desktop API's `GET /v1/usage` adds those calls up by class and model. A streamed request is recorded
when its stream ends, however it ends, including when the client drops it. Nothing is recorded for an answer that is
not a success or that reports no usage. Calls recorded before the router read tools have no tools, which the usage
screen counts as unknown.
