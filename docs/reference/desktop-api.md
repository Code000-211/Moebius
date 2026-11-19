# Desktop API

The desktop API is how the web app, or any client such as `curl`, reads the boards and acts on them. This page is for
anyone who writes a client or a script against a running server. The code is in `crates/moebius-core/src/server/api/`,
anyone who writes a client or a script against a running server. The code is in `crates/moebius-core/src/server/api/`,
one module per part; the events are published from `app/`, `harness/pibolt/`, `decide/check/`, `repos/poll/`,
`improve/retro/`, `improve/trial/`, `server/loops.rs` and `server/schedule.rs`.

The API is HTTP with JSON bodies plus one WebSocket, served on `listen` (default `127.0.0.1:7171`). Keep it on a private
network.

## Authentication

Every request to `/v1/`, the WebSocket upgrade included, carries `Authorization: Bearer <token>` (or, in a browser, the
cookie of a [session](#post-v1session)). The token is a person's [access token](#people-and-access-tokens), which acts
as that person with their role, or the server's token, the content of `token_file` (by default `<data_dir>/token`,
created when the server first starts), which acts as the first owner, `person:owner`. A missing or wrong token, a
revoked access token, one of a disabled person and the server's token while the first owner is disabled are answered
with `401` and the plain text `a valid token is needed`. The exceptions are `POST /v1/webhooks/{forge}`, which a forge
sends and which carries the forge's signature instead, and `/v1/session`, which signs a browser in. The paths outside
`/v1/` need no token: they are the [web app](#the-web-app) and the [hooks a workflow's `webhook` trigger answers
on](#post-apihookspath), which the secret that trigger names opens instead.

The run token of a run of the Moebius agent (`MOEBIUS_TOKEN` in its environment, see the [bridge
protocol](bridge-protocol.md#the-environment-of-the-harness)) is accepted as well, while the run lasts: it is how the
agent operates Moebius with the [command line](cli.md). A request made with it acts as the agent: every decision it
records has the decider `agent:Moebius` instead of `person:owner`, and the run is the judgement's `run` (for a decision about
another run, such as a steer, the judgement keeps that run, and the agent's run is in the context as `acting_run`).
The run token of a run of the Grilling partner is accepted for the routes of [the issue tracker](#the-issue-tracker) only,
as `agent:Grilling-partner`: `GET` and `POST /v1/repositories/{id}/issues`, `GET` and `PUT /v1/cards/{id}`, `PUT
/v1/cards/{id}/parent` and `/labels`, `POST /v1/cards/{id}/blockers` and `DELETE /v1/cards/{id}/blockers/{number}`,
`POST` and `DELETE /v1/cards/{id}/claim`, `POST /v1/cards/{id}/close` and `/reopen`, and `GET` and `POST
/v1/cards/{id}/comments`: what `"$MOEBIUS" issue` does but delete an issue, with which it keeps a wayfinder map. It is held to its chat's repository
and to wayfinder cards (those with a label that starts with `wayfinder:`, in any case): the routes of another
repository (`{id}` of the issues, a card of it) answer `404` as for something that is not there, and a run that works in no
repository gets `403` for all of them. `POST /v1/repositories/{id}/issues` answers `403` unless `labels` has a `wayfinder:`
label and `parent` and `blocked_by` name wayfinder issues; `PUT /v1/cards/{id}` and the other changes (`parent`,
`labels`, `blockers`, `claim`, `close`, `reopen`, `comments`) answer `403` for a card without a `wayfinder:` label, `PUT
/v1/cards/{id}/labels` for a list without one, and `parent` and `blockers` for an issue that is no wayfinder card; reads
(`GET` of an issue, its comments and the list) are open for the repository's cards. The message says what is refused. The
Moebius agent and a person are not held to any of this. Every
other route answers it `401`, as it does the run token of any other agent, and a revoked one. Which routes the Moebius
agent's run token may use is the column Run token of [Roles](#roles); the others answer it `403` with `only a person
can do this`. Where a route below says "a person" and "decider
`person`" (it is `person:<handle>` of the person whose access token or session it is, and `person:owner` for the
server's token), an agent that uses its token is the decider instead.
Records from before people had identities keep the author or decider `person`, which the migration gave to the first owner; no new record is made with it.

A browser signs in once and then carries the cookie `moebius_session` (`HttpOnly; SameSite=Strict; Path=/`, and `Secure` when
a proxy in front of the server says with `X-Forwarded-Proto: https` that the browser used TLS). Every route, the
WebSocket upgrades included, accepts a valid cookie as the token of the person who signed in: a person's route is open
to it and the decider of a decision is that person. A bearer token that is not valid is refused even when the request also has a
valid cookie. Because a browser sends a cookie by itself, a request that only a cookie vouches for has to prove that
it comes from the web app: a method other than `GET`, `HEAD` and `OPTIONS` needs the header `X-Moebius: 1`, and an
`Origin` header, which browsers send on changes and on WebSocket upgrades, must name the host of the `Host` header
(scheme left out). Otherwise the answer is `403` with `cross-site request refused`. A request with the bearer token
needs neither.

```sh
TOKEN=$(cat .moebius/token)
curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7171/v1/health
```

## Roles

Every person of the organisation has a role (ADR 0018), and every route needs one: `viewer` (reads, comments, and
writing issues, which wait in the inbox for approval as an outside author's do: the column script of the Backlog asks
`approve_issue` for them, since such a card is neither `card.trusted` nor `card.confirmed`), `member` (issues and cards,
chats, runs and merging a change, besides what a viewer does) or `owner` (everything: the configuration, plugins, rules,
agents, secrets, repositories, people and the Retro's work). The server's token acts as the first owner, a person's
access token and the cookie of their session as that person. A person whose role may not use a route gets `403` with
`only an owner can do this` or `only a member or an owner can do this`, before anything is done. Run token says whether
the run token of the Moebius agent may use the route (`no`: `403` with `only a person can do this`); the Grilling
partner's is held to the routes of [the issue tracker](#the-issue-tracker) besides (see Authentication). A route marked
`none` needs no token and checks its own credential. A route that is in no row is refused with `403`, and a test fails
for a route of the code that is missing here, for a row here that the code does not name the same, and for an answer of
any route to any role that is not what its row says.

| Route | Role | Run token |
|---|---|---|
| `GET /v1/health` | viewer | yes |
| `POST /v1/session` | none | — |
| `GET /v1/session` | none | — |
| `DELETE /v1/session` | none | — |
| `POST /v1/session/codes` | viewer | no |
| `GET /v1/people` | viewer | no |
| `POST /v1/people` | owner | no |
| `PATCH /v1/people/{id}` | owner | no |
| `GET /v1/people/{id}/tokens` | viewer | no |
| `GET /v1/tokens` | viewer | no |
| `POST /v1/tokens` | viewer | no |
| `DELETE /v1/tokens/{id}` | viewer | no |
| `GET /v1/places` | viewer | no |
| `POST /v1/favourites` | viewer | no |
| `PUT /v1/favourites/order` | viewer | no |
| `DELETE /v1/favourites/{id}` | viewer | no |
| `POST /v1/recent` | viewer | no |
| `DELETE /v1/recent/{id}` | viewer | no |
| `GET /v1/server` | viewer | yes |
| `GET /v1/client` | viewer | yes |
| `PUT /v1/client` | viewer | no |
| `GET /v1/snapshot` | viewer | yes |
| `GET /v1/events` | viewer | yes |
| `GET /v1/projects` | viewer | yes |
| `POST /v1/projects` | owner | yes |
| `GET /v1/projects/{id}` | viewer | yes |
| `PATCH /v1/projects/{id}` | owner | yes |
| `GET /v1/wiki/pages` | viewer | yes |
| `GET /v1/wiki/pages/{*path}` | viewer | yes |
| `PUT /v1/wiki/pages/{*path}` | member | no |
| `DELETE /v1/wiki/pages/{*path}` | member | no |
| `POST /v1/wiki/moves` | member | no |
| `GET /v1/wiki/history/{*path}` | viewer | yes |
| `GET /v1/wiki/search` | viewer | yes |
| `POST /v1/wiki/refresh` | member | yes |
| `POST /v1/wiki/proposals` | member | yes |
| `POST /v1/wiki/proposals/{id}/merge` | member | no |
| `POST /v1/wiki/proposals/{id}/dismiss` | member | no |
| `PUT /v1/projects/{id}/intake` | owner | no |
| `GET /v1/projects/{id}/requests` | viewer | no |
| `POST /v1/projects/{id}/requests` | viewer | no |
| `GET /v1/requests/{id}` | viewer | no |
| `PUT /v1/requests/{id}` | member | no |
| `POST /v1/requests/{id}/accept` | member | no |
| `POST /v1/requests/{id}/decline` | member | no |
| `POST /v1/wiki/from-template` | member | no |
| `GET /v1/templates` | viewer | yes |
| `PUT /v1/templates/issues/{kind}` | member | no |
| `DELETE /v1/templates/issues/{kind}` | member | no |
| `PUT /v1/templates/pages/{name}` | member | no |
| `DELETE /v1/templates/pages/{name}` | member | no |
| `GET /v1/milestones` | viewer | yes |
| `POST /v1/milestones` | member | no |
| `GET /v1/milestones/{id}` | viewer | yes |
| `PATCH /v1/milestones/{id}` | member | no |
| `DELETE /v1/milestones/{id}` | member | no |
| `GET /v1/cards/{id}/milestones` | viewer | yes |
| `PUT /v1/cards/{id}/milestones` | member | no |
| `PUT /v1/projects/{id}/kinds` | owner | yes |
| `GET /v1/kinds` | viewer | yes |
| `POST /v1/kinds` | owner | no |
| `PUT /v1/kinds/{id}` | owner | no |
| `DELETE /v1/kinds/{id}` | owner | no |
| `GET /v1/issues` | viewer | yes |
| `GET /v1/projects/{id}/scripts` | viewer | yes |
| `GET /v1/projects/{id}/scripts/{column}` | viewer | yes |
| `PUT /v1/projects/{id}/scripts/{column}` | owner | yes |
| `DELETE /v1/projects/{id}/scripts/{column}` | owner | yes |
| `PUT /v1/projects/{id}/access` | owner | no |
| `POST /v1/projects/{id}/repositories` | owner | yes |
| `DELETE /v1/projects/{id}/repositories/{repository}` | owner | yes |
| `POST /v1/repositories` | owner | yes |
| `PUT /v1/repositories/{id}` | owner | yes |
| `DELETE /v1/repositories/{id}` | owner | yes |
| `POST /v1/repositories/{id}/issues` | viewer | yes |
| `GET /v1/repositories/{id}/issues` | viewer | yes |
| `GET /v1/repositories/{id}/settings` | viewer | yes |
| `PUT /v1/repositories/{id}/settings/{file}` | owner | yes |
| `DELETE /v1/repositories/{id}/settings/{file}` | owner | yes |
| `GET /v1/repositories/{id}/plugin-settings` | viewer | yes |
| `PUT /v1/repositories/{id}/plugin-settings/{plugin}` | owner | yes |
| `GET /v1/repositories/{id}/tree` | viewer | yes |
| `GET /v1/repositories/{id}/files` | viewer | yes |
| `GET /v1/repositories/{id}/search` | viewer | yes |
| `GET /v1/repositories/{id}/files/{*path}` | viewer | yes |
| `PUT /v1/repositories/{id}/files/{*path}` | member | yes |
| `GET /v1/cards/by-key/{key}` | viewer | yes |
| `GET /v1/cards/{id}` | viewer | yes |
| `PUT /v1/cards/{id}` | member | yes |
| `DELETE /v1/cards/{id}` | member | no |
| `PUT /v1/cards/{id}/parent` | member | yes |
| `PUT /v1/cards/{id}/labels` | member | yes |
| `PUT /v1/cards/{id}/assignees` | member | no |
| `PUT /v1/inbox/{id}/assignees` | member | no |
| `PUT /v1/cards/{id}/priority` | member | no |
| `PUT /v1/cards/{id}/due` | member | no |
| `PUT /v1/cards/{id}/estimate` | member | no |
| `GET /v1/cards/{id}/estimate/proposal` | viewer | yes |
| `PUT /v1/cards/{id}/assignees/{handle}` | member | no |
| `DELETE /v1/cards/{id}/assignees/{handle}` | member | no |
| `PUT /v1/inbox/{id}/assignees/{handle}` | member | no |
| `DELETE /v1/inbox/{id}/assignees/{handle}` | member | no |
| `GET /v1/notices` | viewer | no |
| `POST /v1/notices/{id}/read` | viewer | no |
| `GET /v1/cards/{id}/followers` | viewer | yes |
| `PUT /v1/cards/{id}/follow` | viewer | no |
| `PUT /v1/cards/{id}/kind` | member | yes |
| `PUT /v1/cards/{id}/fields` | member | yes |
| `POST /v1/cards/{id}/blockers` | member | yes |
| `DELETE /v1/cards/{id}/blockers/{number}` | member | yes |
| `PUT /v1/cards/{id}/duplicate` | member | yes |
| `DELETE /v1/cards/{id}/duplicate` | member | yes |
| `POST /v1/cards/{id}/related` | member | yes |
| `DELETE /v1/cards/{id}/related/{number}` | member | yes |
| `GET /v1/cards/{id}/similar` | viewer | yes |
| `POST /v1/cards/{id}/claim` | member | yes |
| `DELETE /v1/cards/{id}/claim` | member | yes |
| `POST /v1/cards/{id}/close` | member | yes |
| `POST /v1/cards/{id}/reopen` | member | yes |
| `GET /v1/cards/{id}/comments` | viewer | yes |
| `POST /v1/cards/{id}/comments` | viewer | yes |
| `POST /v1/cards/{id}/approve` | member | yes |
| `POST /v1/cards/{id}/block` | member | yes |
| `POST /v1/cards/{id}/unblock` | member | yes |
| `POST /v1/cards/{id}/retry` | member | yes |
| `GET /v1/cards/{id}/runs` | viewer | yes |
| `GET /v1/cards/{id}/history` | viewer | yes |
| `POST /v1/changes/{id}/merge` | member | yes |
| `POST /v1/changes/{id}/reject` | member | yes |
| `GET /v1/changes/{id}/diff` | viewer | yes |
| `GET /v1/runs/{id}` | viewer | yes |
| `GET /v1/runs/{id}/events` | viewer | yes |
| `GET /v1/runs/{id}/diff` | viewer | yes |
| `POST /v1/runs/{id}/steer` | member | yes |
| `POST /v1/runs/{id}/stop` | member | yes |
| `GET /v1/runs/{id}/terminal` | owner | no |
| `GET /v1/chats` | viewer | yes |
| `POST /v1/chats` | member | yes |
| `DELETE /v1/chats/{id}` | member | no |
| `POST /v1/chats/{id}/messages` | member | yes |
| `POST /v1/chats/{id}/offer` | member | yes |
| `POST /v1/chats/{id}/publish` | member | yes |
| `POST /v1/chats/{id}/end` | member | yes |
| `POST /v1/chats/{id}/seen` | member | yes |
| `POST /v1/chats/{id}/settings` | member | yes |
| `POST /v1/chats/{id}/title` | member | yes |
| `POST /v1/chats/{id}/share` | member | no |
| `GET /v1/chats/{id}/tui` | member | no |
| `GET /v1/grillings` | viewer | yes |
| `GET /v1/grillings/{id}` | viewer | yes |
| `DELETE /v1/grillings/{id}` | member | no |
| `POST /v1/grillings/{id}/confirm` | member | yes |
| `POST /v1/grillings/{id}/dismiss` | member | yes |
| `GET /v1/terminal` | owner | no |
| `GET /v1/repositories/{id}/terminal` | owner | no |
| `GET /v1/ports` | viewer | yes |
| `GET /v1/ports/{port}/tunnel` | member | no |
| `POST /v1/uploads` | member | yes |
| `GET /v1/runners` | viewer | yes |
| `GET /v1/agents` | viewer | yes |
| `GET /v1/agents/{name}` | viewer | yes |
| `PUT /v1/agents/{name}` | owner | yes |
| `DELETE /v1/agents/{name}` | owner | yes |
| `POST /v1/harness/setup` | owner | no |
| `GET /v1/skills` | viewer | yes |
| `GET /v1/skills/{name}` | viewer | yes |
| `PUT /v1/skills/{name}` | owner | yes |
| `DELETE /v1/skills/{name}` | owner | yes |
| `GET /v1/skills/{name}/files/{*path}` | viewer | yes |
| `PUT /v1/skills/{name}/files/{*path}` | owner | yes |
| `DELETE /v1/skills/{name}/files/{*path}` | owner | yes |
| `GET /v1/mcp-servers` | viewer | yes |
| `GET /v1/mcp-servers/{name}` | viewer | yes |
| `PUT /v1/mcp-servers/{name}` | owner | yes |
| `DELETE /v1/mcp-servers/{name}` | owner | yes |
| `GET /v1/rules` | viewer | yes |
| `PUT /v1/rules` | owner | yes |
| `GET /v1/workflows` | viewer | yes |
| `GET /v1/workflows/{name}` | viewer | yes |
| `PUT /v1/workflows/{name}` | owner | yes |
| `DELETE /v1/workflows/{name}` | owner | yes |
| `GET /v1/workflows/{name}/run-inputs` | viewer | yes |
| `POST /v1/workflows/{name}/runs` | member | yes |
| `POST /v1/workflows/runs/{id}/cancel` | member | yes |
| `POST /v1/workflows/runs/{run_id}/resume` | member | yes |
| `GET /v1/automations` | viewer | yes |
| `POST /v1/automations` | owner | yes |
| `GET /v1/automations/{id}` | viewer | yes |
| `PUT /v1/automations/{id}` | owner | yes |
| `DELETE /v1/automations/{id}` | owner | yes |
| `POST /v1/automations/{id}/run` | owner | yes |
| `GET /v1/automations/{id}/firings` | viewer | yes |
| `GET /v1/extensions/search` | viewer | yes |
| `GET /v1/extensions/info` | viewer | yes |
| `GET /v1/extensions/packages` | viewer | yes |
| `POST /v1/extensions/packages` | owner | yes |
| `POST /v1/extensions/packages/{name}/update` | owner | yes |
| `DELETE /v1/extensions/packages/{name}` | owner | yes |
| `POST /v1/extensions/update` | owner | yes |
| `GET /v1/packages/search` | viewer | yes |
| `GET /v1/packages` | viewer | yes |
| `POST /v1/packages` | owner | yes |
| `GET /v1/packages/{name}` | viewer | yes |
| `DELETE /v1/packages/{name}` | owner | no |
| `POST /v1/packages/{name}/update` | owner | no |
| `GET /v1/usage` | viewer | yes |
| `GET /v1/judgements` | viewer | yes |
| `GET /v1/judgements/kinds` | viewer | yes |
| `POST /v1/judgements/{id}/replay` | owner | yes |
| `GET /v1/replays` | viewer | yes |
| `DELETE /v1/replays/{id}` | owner | no |
| `GET /v1/exports` | viewer | yes |
| `POST /v1/exports` | owner | yes |
| `DELETE /v1/exports/{id}` | owner | no |
| `GET /v1/exports/{id}/{file}` | viewer | yes |
| `GET /v1/retros` | viewer | yes |
| `POST /v1/retros` | owner | yes |
| `GET /v1/retros/{date}` | viewer | yes |
| `POST /v1/retros/{date}/retry` | owner | yes |
| `GET /v1/proposals` | viewer | yes |
| `POST /v1/proposals/{id}/trial` | owner | yes |
| `POST /v1/proposals/{id}/merge` | owner | yes |
| `POST /v1/proposals/{id}/dismiss` | owner | yes |
| `GET /v1/decision-model` | viewer | yes |
| `GET /v1/takeovers` | viewer | yes |
| `POST /v1/takeovers/{id}/merge` | owner | yes |
| `POST /v1/takeovers/{id}/dismiss` | owner | yes |
| `POST /v1/takeovers/{id}/release` | owner | yes |
| `GET /v1/research` | viewer | yes |
| `POST /v1/research` | owner | yes |
| `GET /v1/research/{id}` | viewer | yes |
| `PUT /v1/research/{id}` | owner | yes |
| `DELETE /v1/research/{id}` | owner | no |
| `POST /v1/research/{id}/confirm` | owner | yes |
| `POST /v1/research/{id}/start` | owner | yes |
| `POST /v1/research/{id}/stop` | owner | yes |
| `GET /v1/lessons` | viewer | yes |
| `GET /v1/lessons/{id}` | viewer | yes |
| `PUT /v1/lessons/{id}` | owner | yes |
| `DELETE /v1/lessons/{id}` | owner | yes |
| `GET /v1/models` | viewer | yes |
| `GET /v1/models/available` | viewer | yes |
| `GET /v1/models/providers` | viewer | yes |
| `GET /v1/models/upstreams/{place}/login` | viewer | yes |
| `POST /v1/models/upstreams/{place}/login` | owner | no |
| `POST /v1/models/upstreams/{place}/login/code` | owner | no |
| `GET /v1/start` | viewer | yes |
| `GET /v1/plugins` | viewer | yes |
| `GET /v1/plugins/settings` | viewer | yes |
| `PUT /v1/plugins/{plugin}/enabled` | owner | no |
| `PUT /v1/plugins/{plugin}/settings` | owner | yes |
| `GET /v1/plugins/{plugin}/settings-ui` | viewer | yes |
| `GET /v1/settings` | viewer | yes |
| `PUT /v1/settings` | owner | yes |
| `PUT /v1/secrets/{name}` | owner | no |
| `DELETE /v1/secrets/{name}` | owner | yes |
| `POST /v1/server/restart` | owner | yes |
| `GET /v1/outgoing-webhooks` | owner | no |
| `POST /v1/outgoing-webhooks` | owner | no |
| `DELETE /v1/outgoing-webhooks/{id}` | owner | no |
| `PUT /v1/repositories/{id}/private` | owner | no |
| `POST /v1/webhooks/{forge}` | none | — |
| `GET /v1/webhooks` | owner | no |
| `POST /v1/webhooks/{forge}/secret` | owner | no |
| `POST /api/hooks/{path}` | none | — |
| `POST /api/hooks/{repository}/{path}` | none | — |

## Errors

A failed request has a status code and a plain-text message as its body.

| Status | Meaning |
|---|---|
| `401` | No valid token. |
| `403` | The person's [role](#roles) may not use the route (`only an owner can do this`, `only a member or an owner can do this`), or a run token of the Moebius agent is on a route that only a person may use (`only a person can do this`). |
| `404` | The repository, project, card, change or run in the path does not exist. |
| `409` | A merge conflicts: the default branch is left as it was. |
| `422` | The request is understood but not allowed: invalid values, or a card or change in the wrong state. Also what axum answers for a JSON body with a missing or mistyped field. |
| `400`, `415` | The body is not valid JSON, or the content type is not `application/json` (`application/zip` for `POST /v1/uploads`). |
| `413` | The body of `POST /v1/uploads` is larger than 20 MiB. |
| `500` | Anything else: git, the store or a forge failed. |

A request with a body must have `Content-Type: application/json` and a JSON object, `{}` at least. The one exception is
`POST /v1/uploads`, whose body is a zip file.

## Types

These objects appear in the responses. All ids are numbers; times are Unix seconds.

`Repository`: `id`, `name`, `path` (the clone on the server), `forge` (`none`, `github`, `gitlab` or `forgejo`),
`default_branch`, `github` (`owner/repo` or null), `trusted` (array of logins on the forge), `project` (the path on
GitLab, such as `group/project`, or the repository on Forgejo as `owner/repo`, or null) and `home_project_id` (the
[project](#projects) its new issues land in, which was made with it) and `files_of` (the id of the project whose own
files the repository holds, see [a project's own files](#a-projects-own-files), or null for every repository a person
added).

`Card`: `id`, `repository_id`, `project_id` (the [project](#projects) whose board it is on), `number` (the number of its issue in its repository: the forge's number for an issue of a forge), `project_number` (its number in its project) and `project_key` (the key of its project), which together name it, `WEB-24`; `title`, `body`, `author`, `column`, `created_at`, `note` (why the card was sent
back to coding, or null), `retried_after` (the id of the card's latest run when a person last retried it, or null) and
`confirmed` (true for a card made from a confirmed grilling, which needs no approval) and `held` (null, or `{"reason":
string or null, "by": "person" or "agent:Moebius", "at": number}` while a person or the Moebius agent holds the card; see
`POST /v1/cards/{id}/block`). The card is also an issue of a tracker (see [the issue tracker](#the-issue-tracker)):
`parent` (the number of the issue it is a sub-issue of, or null), `closed` (true when the issue was closed or the card is
in `Done`), `labels` (array of text, in order), `claimed_by` (who claimed it, or null) and `assignees` (the handles of
the people it is given to, without the `@`, in the order the people were added; empty when it is no one's; see
[Assignees](#assignees)), `priority` (`urgent`, `high`, `medium`, `low` or `none`) and `due` (a day as
`YYYY-MM-DD`, or null) and `estimate` (points, or null); see [Priority and due date](#priority-and-due-date) and
[Estimates](#estimates).
[Assignees](#assignees)), `moved_at` (when the card last moved, was edited or was released from a hold, Unix seconds;
scripts read it as `card.moved_at`) and `archived_at` (null, or when a column script archived the card with
`moebius.archive`). An archived card is not on the board: the snapshot (`GET /v1/snapshot`) leaves it out, and a
move (a person's, or the forge reopening its issue) brings it back. It stays in the store, in the issue list and in the
analytics.
[Assignees](#assignees)), `kind` (the name of its [kind of issue](#kinds-of-issues), or null) and `fields` (the values set for the fields of its kind, an object by field name: a number for a `number` field, text for the others; `{}` when none is set).

`Comment`: `id`, `card_id`, `author` (a forge login, `person`, `person:<handle>` or `agent:<name>`), `body`, `at`.

`Issue`: a `Card` with `state` (`open` or `closed`), `blocked_by` (the numbers of the issues that block it and are not
done), `children` (its sub-issues, oldest first, each `{"id", "number", "title", "state", "column"}`), `duplicate_of` (the
original it was closed as a duplicate of, in the same shape, or null), `duplicates` (the issues closed as duplicates of it)
and `related` (the issues related to it, each shown with the other).
`Run`: `id`, `card_id` (null for a chat or an agent of a workflow run), `agent`, `harness`, `purpose` (`code`, `review`, other purposes of column scripts,
`chat`, `retro`, the Retro analyst's run, `trial`, the trial of a proposal on a card, `research`, a round of the Autoresearcher, or
`workflow`, an agent of a workflow run), `status` (`starting`, `running`, `waiting`, `succeeded`, `failed` or `stopped`; `waiting` is a chat waiting for
the person's next message), `worktree`, `started_at`, `ended_at`, `summary`, `repository_id` (the repository a chat
works in, or null) and `harness_repository` (true for a chat that works in the [harness configuration
repository](harness-configuration.md#changing-it-in-a-chat); `harness` is the name of the harness that runs the agent)
and `origin` (where a chat was started: `chat`, or `assistant` for the web app's Moebius agent of the right column; `chat` for every
other run) and `runner` (the name of the [runner](configuration.md#runners) the run's harness goes on, or null for a run on the server) and
`owner_id` (the person a chat belongs to, or null) and `shared` (see `Chat`), `title` (the name a person gave this chat, or null while it is untitled and its `Chat` title comes from its first
message; see `POST /v1/chats/{id}/title`. Only a chat is ever named, so it is null for every other run).

`Grilling`: `id`, `run_id` (the chat), `repository_id`, `issues` (array of `{"title", "body", "blocked_by"}`, where
`blocked_by` is an array of indexes into `issues`; empty for a report and for an empty grilling), `report` (text or null),
`status` (`proposed`, `reported`, `empty`, `confirmed` or `dismissed`), `reason` (why a grilling is `empty`; else empty) and
`at`.

`Chat`: `id` (the id of its run), `agent`, `status` (that of the run), `repository_id` (or null), `harness_repository`
(true for a chat in the harness configuration repository), `started_at`, `ended_at`, `title` (the name a person gave
the chat, see `POST /v1/chats/{id}/title`, or the first line of the first message, at most 80 characters; empty while a
chat started without a message has none), `answered` (the `seq` of the chat's last `agent_settled` or `moebius_error`
event, 0 without one), `seen` (how far a person has read it, see `POST /v1/chats/{id}/seen`) and `changed_files` (how
many files the chat's worktree changes against the default branch of its repository, committed or not, new files
included: what `POST /v1/chats/{id}/offer` would offer; 0 for an ended chat and one without a repository), `asks`
(true while the chat waits and either the agent's last answer ends in a question to the person, or the last thing it did
was call its question tool, `ask_user_question` or Claude Code's `AskUserQuestion`, with no message of the person after
it, so it needs an answer) and
`origin`
(`chat`, `assistant` for a chat of the web app's Moebius agent of the right column, see `POST /v1/chats`, or `automation` for a chat an
automation started, see `POST /v1/automations`), `model` and `thinking`
(the model and reasoning level the chat was given in place of its agent's, see `POST /v1/chats` and
`POST /v1/chats/{id}/settings`; null while it uses its agent's), `owner` (the handle of the person who started the chat, or null for a chat that belongs to everyone), `shared` (true when other people read it: its owner shared it, see `POST /v1/chats/{id}/share`, or it has no owner) and `mine` (true when the person who asked started it). A chat is unread while `answered` is greater than `seen`.

`Automation`: `id`, `name`, `schedule` (a cron expression of five fields in the server's local time), `agent`, `prompt`,
`repository_id` (the repository its chats work in, or null), `harness_repository` (true when they work in the harness
configuration repository), `enabled`, `created_at`, `next_firing` (when the schedule comes due next, as Unix seconds;
null while it is off or when the schedule never comes due) and `last_firing` (its latest `Firing`, or null).

`Firing`: `id`, `automation_id`, `status` (`started`, `skipped` or `failed`), `run_id` (the chat it started, or null),
`workflow_run_id` (the workflow run that carries it, or null when it starts a chat directly, with no repository: in an
empty directory or in the harness configuration repository), `chat_status` (that chat's status now, or null), `manual` (true when a person fired it with
`POST /v1/automations/{id}/run`), `reason` (why it was skipped or failed; else empty) and `at`.

`Change`: `id`, `repository_id` (null for a change of the harness configuration repository), `harness_repository` (true
for such a change), `card_id` (or null), `run_id` (the workflow run whose work it offers, or null), `branch`, `state` (`open`, `merged` or `closed`), `pull_request`
(the number of the pull request on GitHub or Forgejo or of the merge request on GitLab, or null), `checks` (`pending`,
`success`, `failure` or null; on GitLab the latest pipeline of the head commit: `failed` and `canceled` are `failure`,
`success` and `skipped` are `success`, anything else is `pending`; on Forgejo the latest status of each context of the
head commit: any `failure` or `error` is `failure`, else any `pending` is `pending`, else `success`), `review` (the
latest review's state in lower case, such as `approved`; on GitLab `approved` once anyone approved; on Forgejo the
latest review given that was not dismissed, with `REQUEST_CHANGES` as `changes_requested`; or null), `head` (the published commit), `published_at`, `checks_head` (the commit
`checks` are for), `failed_checks` (the failing checks of a pull request, the failed jobs of a pipeline, or the failed
statuses of a commit, one per line, or null) and `check_output` (the
end of the output of the repository's own checks, or null). A change's `branch` is `moebius/card-<number>`, `moebius/chat-<run id>` for the work of a chat, which has no card, `moebius/workflow-run-<run id>` for the work of a workflow run, which has none either and names the run in `run_id`, or `research/<research id>-<name>` for a lever of a research, which has none either.

`InboxItem`: `id`, `kind`, `card_id` (or null), `title`, `data` (an object that depends on the kind), `resolved_at` (null
in the snapshot, which holds only open items), `at` and `assignees` (the handles of the people it is given to, like a
`Card`'s). Kinds the core adds: `approve_issue` (`data.reason`), `stuck`
(`data.reason`), `stale` (`data.reason`; a column script told the people a card is given to that it is held for not having moved, and releasing the card resolves it), `package_update` (`data.name`, `data.installed`, `data.version`; no card), `ready_to_merge` (`data.change`, `data.branch`, `data.run` when the change is a workflow run's, and `data.judgements`, the review's judgement calls, when it had any), `change_closed` (`data.change`, `data.pull_request`) and
`project_label` (`data.label`, `data.repository`; its `title` says the label names no project of the repository, and the card it is about stays where it is: see [the project label](#the-project-label)) and
`proposal` (`data.proposal`, `data.branch`, `data.trials`: an array of `{"card", "outcome", "reason"}` for the cards it
addresses, and `data.regression`: the same for the cards of the regression set, empty when none was tried; its `title` is the
proposal's and it has no card; a proposal for Moebius's own configuration has `data.kind` (`settings` or
`repository`), `data.repository` (the repository's id, or null), `data.reason` (the Retro's reason) and `data.diff`
instead of `data.trials`), `wiki_proposal` (`data.branch`, `data.page`, `data.message`, `data.by`, `data.commit`, `data.diff`; an agent's edit of a wiki page, no card: see [`POST /v1/wiki/proposals`](#post-v1wikiproposals)), `grilling_issues` (`data.grilling`, `data.repository`, `data.issues`: an array of `{"title",
"body", "blocked_by"}`; its `title` is "N issues from a grilling" and it has no card) and `grilling_report` (`data.grilling`,
`data.repository`, `data.report`: the text; no card) and `intake_request` (`data.request`, `data.project`, `data.requester`; its `title` names the
project and the draft issue and it has no card; see [Intake](#intake)) and `takeover` (`data.takeover`, `data.kind`, `data.agreement`,
`data.count`; its `title` is "The decision model can take over <kind>" and it has no card) and `extension_update`
(`data.package`, `data.from`, `data.to`, `data.source`; its `title` is "<package> <from> -> <to>" and it has no card) and
`webhook_failed` (`data.webhook`, `data.url`, `data.event`, `data.error`, `data.attempts`; its `title` says which webhook was given up on and it has no card; see [Outgoing webhooks](configuration.md#outgoing-webhooks)) and
`workflow_approval` (`data.run`, `data.node`, `data.prompt`, `data.value` and `data.deadline` in whole seconds since the
epoch; its `title` is the prompt and it has no card; it carries the decision a workflow run waits for, see
[the approval node](workflows/approval.md)). The nightly schedule, from `retro_at` on, once a day, puts one
`extension_update` item in the inbox for each installed package with a newer version, and no second one for a version it
has told of; the item is resolved when the package is current or an update is proposed.
A column script can add other kinds with `moebius.ask`.

## Endpoints

### `GET /v1/health`

Response: `{"version": "0.1.0"}`, the server's version.

### The web app

The server serves the web app, built from `web/` into `web/dist` and embedded in the binary when it is built, at `/` on
the address of this API. Every `GET` of a path outside `/v1/` that is no file of the app answers `index.html`, since the
app routes in the browser; the files get their media type, `index.html` has `Cache-Control: no-cache` and the files
under `assets/` are cached for a year. A binary built without `web/dist` answers a short page that says to run `pnpm -C
web build` and build the server again. Every file is answered with `X-Content-Type-Options: nosniff` and a
`Content-Security-Policy` (the file `web/csp.json`): scripts, styles, fonts, images and connections from the server's own
address only, inline styles but no inline script or `eval`, frames of its own address only, and the app is not to be
framed by another site. `plugin-frame.html`, the frame a plugin's page for its settings runs in sandboxed (see [a page of
its own](plugin-api.md#a-page-of-its-own)), has a stricter policy that forbids it every connection; the files under
`fonts/` carry `Access-Control-Allow-Origin: *`, since that frame has no origin. Nothing under `/v1/` is a file: an unknown `/v1/` path is answered `401` without
a valid token and `404` with one.

At start, `moebius serve` prints the line `Open the web app: http://<listen address>/?login=<code>` on standard output
(not in the log). The page signs in with the code and takes it out of the address.

### `POST /v1/session`

Signs a browser in. No `Authorization` is needed. Request body: `{"token": string}` with a person's access token or the
server's token (which signs in the first owner), or `{"code": string}` with a one-time code of `POST
/v1/session/codes`. Response: `204` and the cookie `moebius_session=<id>; Max-Age=2592000` (see Authentication). The
session is kept in the store, as the SHA-256 of its id and never the id itself, for 30 days, so it survives a restart of
the server. It ends sooner when the browser signs out, when the access token it was opened with (or the token that
made its code) is revoked, and when its person is disabled; from then on the cookie is answered `401`. Errors: `401`
with `a valid token is needed` for a wrong or revoked token, a wrong, used or expired code, or a body with neither;
`403` with `only a person can do this` for a run token.

### `GET /v1/session`

`200` with `{"signed_in": true, "person": Person | null}` for a request with a valid cookie or bearer token, else
`401`. `person` is who the request acts as (see [people](#get-v1people)); it is `null` for an agent's run token.

### `DELETE /v1/session`

Signs out: deletes the session of the cookie from the store and clears the cookie. Response: `204`.

### `POST /v1/session/codes`

A new one-time code that signs a browser in, as the person who asked, without the token: it works once and for 10
minutes. Request body: `{}`.
Response: `{"url": string, "expires_at": number}`, where `url` is `http://<Host of the request>/?login=<code>` (`https` when
the request says with `X-Forwarded-Proto` that it came over TLS) and `expires_at` is Unix seconds. The code is only in
memory, never stored or logged. [`moebius web`](cli.md) prints the address. Errors: `403` to a run token.

### People and access tokens

A person signs in with an access token (ADR 0018): `moebius-access-` and 64 hexadecimal digits. It belongs to one
person, has a name, and lasts until it is revoked. The store keeps only its SHA-256 and when it was last used (written
at most once a minute); the token itself is in the answer that made it and nowhere else, and `blobs::redact` replaces
one wherever it would be logged. These routes are a person's: a run token gets `403` with `only a person can do this`.
A `Person` is `{"id": number, "handle": string, "name": string, "role": "owner" | "member" | "viewer", "disabled":
bool, "created_at": number}`; an `AccessToken` is `{"id": number, "person_id": number, "name": string, "created_at":
number, "last_used_at": number | null, "revoked_at": number | null}`, times in Unix seconds.

### `GET /v1/people`

Every person, disabled ones included, in the order they were added: `[Person]`.

### `POST /v1/people`

An owner adds a person and gets their first access token, named `first token`, to hand over. Request body: `{"handle":
string, "name": string, "role": "owner" | "member" | "viewer"}`; the handle may start with `@`, which is not kept, and
is 1 to 39 lowercase letters, digits, `-` and `_`, starting with a letter or a digit. Response: `{"person": Person,
"token": AccessToken & {"token": string}}`. Errors: `403` with `only an owner can do this`, `409` for a handle that is
taken in any case, `422` for a handle or an empty or longer than 100 characters name.

### `PATCH /v1/people/{id}`

An owner changes a person's role or disables or enables them. Request body: `{"role"?: "owner" | "member" | "viewer",
"disabled"?: bool}`; what is absent stays. Disabling signs the person out at once: their tokens and sessions are
refused. Response: the `Person`. Errors: `403` with `only an owner can do this`, `404` for no such person, `422` for
changing the last owner who is not disabled to another role or disabling them.

### `GET /v1/people/{id}/tokens`

The access tokens of a person, revoked ones included, with when each was last used: `[AccessToken]`. An owner may ask
for anyone's, another person only for their own (else `403`). Errors: `404` for no such person.

### `GET /v1/tokens`

The access tokens of the person asking: `[AccessToken]`.

### `POST /v1/tokens`

A new access token for the person asking, such as for the command line on a second machine. Request body: `{"name":
string}` (1 to 100 characters). Response: `AccessToken & {"token": string}`. Errors: `422` for an empty or long name.

### `DELETE /v1/tokens/{id}`

Revokes an access token, the asker's own or, for an owner, anyone's; the sessions it opened end with it, and every
request with it is refused at once. Revoking one that is revoked already changes nothing. Response: the `AccessToken`,
with `revoked_at`. Errors: `404` for no such token, and for another person's token when the asker is not an owner.

### Favourites and recent places

What a person pins to the sidebar and what they opened lately, kept for them on the server, so both follow them to
every browser they sign in on. These routes are a person's own: a run token gets `403`. A `Place` is `{"id": number,
"kind": "project" | "view" | "card" | "page", "repository_id": number, "key": string, "title": string}`: `key` is a
card's id, a view's or a page's slug, and empty for a project. Every answer but a refused one is `{"favourites":
[Place], "recent": [Place]}`: the pins in the order the person put them, and the places they opened, newest first,
at most 20. A place whose repository or card is gone, so that the person can no longer see it, is left out of both
lists and forgotten the first time they are read; a project's and a card's `title` is its name or title now.

### `GET /v1/places`

The two lists of the person asking.

### `POST /v1/favourites`

Pins a place at the end of the list; a place that is pinned already keeps its position. Request body: `{"kind":
string, "repository_id": number, "key": string, "title": string}`. A project needs its `repository_id`; a card its
`key`, which is its id, and the server fills in its repository and title; a view or a page needs `repository_id`, `key`
and `title` (at most 200 characters). Errors: `422` for another kind or a missing field, `404` for a repository or card
that does not exist.

### `PUT /v1/favourites/order`

Puts the pins in the order of `ids`, a list of pin ids. A pin the list leaves out follows the ones it names; an id that
is not the asker's pin is ignored.

### `DELETE /v1/favourites/{id}`

Removes a pin. Errors: `404` for no such pin, and for another person's.

### `POST /v1/recent`

Notes that the person opened a place: it goes to the top of the recent list, once. The body is that of
`POST /v1/favourites`.

### `DELETE /v1/recent/{id}`

Forgets a recent place. Errors: `404` for no such place, and for another person's.

### `GET /v1/server`

Where the server runs, found when it started: `{"program": string, "version": string, "config_file": string,
"data_dir": string, "source_dir": string, "api_address": string, "router_address": string, "host": string, "os": string,
"arch": string}`. `program` is the running executable, `config_file` the `moebius.toml` it was started with,
`source_dir` the source checkout the program lies in (`target/` of a cargo workspace; absent otherwise), `api_address`
the base URL of this API and `router_address` the address of the router. No token or key is in it. The Moebius agent is
given the same facts at the start of each of its runs (section "Where Moebius runs" of its instructions). Errors: `404`
a core that was not started as a server.

### `GET /v1/client`

What a client app last reported about itself (the web app does not report): the body of `PUT /v1/client` and `reported_at`, the time the server
heard it (Unix seconds). Errors: `404` no app has reported since the server started.

### `PUT /v1/client`

The app tells the server where it runs, when it connects and whenever the event stream connects again. Request body:
`{"program": string, "version": string, "os": string, "arch": string, "host": string, "server_address": string,
"ssh_host": string or null}`. `program` is the app's executable, `host` the computer it runs on, `server_address` the
address it reaches this server by (over SSH the local end of the tunnel) and `ssh_host` the host name it reaches it
through over SSH, absent when it connects directly. The server keeps the latest report only, in memory, cuts every
field to 500 characters and redacts it. Response: what `GET /v1/client` answers. The Moebius agent is told it at the
start of each run. Errors: `403` to a run token.

### `GET /v1/snapshot`

Everything the app shows. Response:

```json
{
  "boards": [
    {"repository": {"id": 1, "name": "demo", "path": "/srv/.moebius/repos/demo", "forge": "none",
                    "default_branch": "main", "github": null, "trusted": []},
     "cards": [{"id": 1, "repository_id": 1, "number": 1, "title": "Add a feature", "body": "", "author": "person",
                "column": "Backlog", "created_at": 1790000000, "note": null, "retried_after": null,
                "parent": null, "closed": false, "labels": [], "claimed_by": null,
                "counts_as_sub_issue": true,
                "rounds": 0, "tokens": 0, "run": null, "blocked_by": [], "refusals": 0, "change": null}],
     "round_limit": 5,
     "waiting": {},
     "changes": []}
  ],
  "harness_changes": [],
  "inbox": []
}
```

`harness_changes` are the `Change`s of the harness configuration repository, which belongs to no board.
`boards` has one entry per repository, with all its cards and changes. `round_limit` is the most `code` rounds a card
gets. Each entry of `cards` is a `Card` with what a board shows of it, so that a client needs no other read:

| Field | Meaning |
|---|---|
| `rounds`, `tokens` | The card's `code` runs (and rounds of attempts of which none passed) and the tokens of all its runs since it was last retried, as `card.rounds` and `card.tokens` in [column scripts](column-scripts.md). |
| `run` | The card's latest run that is not an attempt, `{"id", "agent", "purpose", "status"}`, or null when it has none: an attempt that was not picked is shown by `attempts`, not here. A board counts the runs working on it from the cards whose `run.status` is `starting` or `running`. |
| `blocked_by` | The numbers of the issues that block the card and are not done yet, as `card.blocked_by`. |
| `counts_as_sub_issue` | Whether the card counts as a sub-issue of its `parent` (see [only issues a person can have vouched for](#the-issue-tracker)). A client that builds stacks, counts "done of total" or decides what is next from `parent` uses only the cards where this is true; one that is false is an ordinary top-level card, whatever parent it names. |
| `refusals` | How many tool calls of that latest run were refused (its `tool_check` and `mcp_call` run events with the decision `refuse`). |
| `attempts` | The latest group of attempts of a card that was tried several times at once (see [column scripts](column-scripts.md#attempts)), oldest first, each `{"run", "model", "status", "result", "output", "picked", "reason"}`: `model` the model or class the attempt was started with, or null; `status` its run's status; `result` `passed`, `failed` or null before it was judged; `output` why it failed; `picked` whether it was the one kept, and `reason` why. Absent when the card has no attempts. |
| `change` | The card's open change, `{"id", "state", "checks", "review_verdict"}`, or null. `checks` is `pending`, `success`, `failure` or `none`, as `card.change.checks`; `review_verdict` is `approve` or `changes` once a review ended since the latest `code` run, else null. Whether the change conflicts is not in the snapshot. |

A card in `Done` has `rounds` and `tokens` 0, no `run`, no `change`, no blockers and no refusals: the snapshot is read on
every event, and a finished card shows none of it. `waiting` maps the id of each card whose column
script is waiting (`moebius.wait`, `ask`, `stuck`) to the reason, as text; a card that does not wait has no entry. The
reasons are kept in memory, so after a restart they are missing until the board engine's next tick; `card_waiting`
(below) keeps them current. `inbox` holds the items that are not
resolved yet, oldest first.

### `GET /v1/events`

A WebSocket. After the upgrade, every event the core publishes arrives as one JSON text message, see Events below. Events
are not replayed: a client sees only what happens after it connected, so read the snapshot after connecting. Messages the
client sends are ignored; closing the socket ends the stream.

### Projects

A project (ADR 0019) is one board and the work on it, with any number of repositories. Its `key` names its cards
(`WEB-24`): an uppercase letter and then 1 to 9 uppercase letters or digits, unique on the server, chosen when the
project is made and kept for as long as the project is; renaming changes only the `name`, which is unique regardless of case. Every
repository has a home project, where its new issues land: adding a repository makes one with the repository's name and a
key made from it (its letters and digits in capitals, after a `P` when it does not start with a letter, at most 10, with
a number at its end when that key is taken: `web` is `WEB`, a second `web-` `WEB2`, `2fa` `P2FA`), and removing a
repository removes its home project when no repository and no card is left on it, and its key can then be taken again. A
server from before projects has one project per repository after its first start, with the repository's name and id,
and every card on it; a repository whose name an earlier one has in another case gets the project name `<name> (<id>)`.

A `ProjectView` is `{"id": number, "name": string, "key": string, "created_at": number, "repositories": [{"id": number,
"name": string, "home": bool}], "members": [{"id": number, "handle": string, "name": string}], "open_cards": number, "files": number or null,
"kinds": [number], "intake": bool, "private": bool, "member": bool}`: `repositories` in the order the project names them, `home` true when this project is
the repository's home project; `members` the people a private project names, and everyone of the organisation in a
public one, without the disabled; `open_cards` the cards on its board that are neither closed nor in `Done` (0 when
`member` is false); `private` whether only its members see it; `member` whether the person asking sees its board (always
true for a public project); `files` the id of the repository that holds the project's own files, or null until they
are made; `kinds` the ids of the [kinds of issues](#kinds-of-issues) the project offers; `intake` whether the project takes requests (see [Intake](#intake)). A project made, renamed, given intake or made private or public sends `project_changed`.

#### A project's own files

Every project made with `POST /v1/projects` has a store of files of its own: a git repository the server keeps for it
without a forge, at `<data_dir>/projects/<key>`, started with one empty commit on `main`. It is a `Repository` like any
other without a forge, named by the project's key, with `files_of` and `home_project_id` the project, and it is not one
of the project's `repositories`. So a project without a repository is a full board: its cards are made with `POST
/v1/repositories/{files}/issues` and live in Moebius, their runs work in worktrees of the files under the same tool-call
check, their changes are reviewed and merged into them as for any repository without a forge, and the files are read
and written through `GET /v1/repositories/{files}/tree` and `/files/{path}`. Nothing refreshes them, since they came
from nowhere, and `DELETE /v1/repositories/{files}` is refused with `422`: they go only with their project. A project
made before projects had files gets them at the server's next start, if it has no repository. Because the key names the
files, a repository cannot be added with a name that is a project's key, and a project cannot take a key that is a
repository's name (`409`), in any case, since a repository's clone and worktrees are folders of its name. A project whose
files cannot be made is not made. Making the files announces `repository_added`, then `project_changed` with `files`.

#### The project label

An issue read from a forge (GitHub, GitLab or Forgejo) makes a card on the board of its repository's home project. A
label `project:<name>` (the prefix in any case, the name in any case) sends the card to the board of the project of that
name, when the repository is one of its repositories, with its column kept; the card is on exactly one board, and
`card_updated` is sent when it moves. Changing the label moves the card again, and taking it off sends the card back to
the home project. This is done when the issue's labels change on the forge, so a card is not moved back after it was put
elsewhere while its labels stay as they are. A label naming a project the repository is not one of the repositories of
(or none) leaves the card where it is and adds an inbox item of kind `project_label` for it, once for that set of labels.

#### Private projects

A project can be private (ADR 0018): it names its members, and everyone else, owners included unless they name
themselves, sees neither its board, its cards, its runs nor its files. For someone who is not a member:

- Every route that names one of its things by id answers `404`, as if it did not exist, whatever the method: a card
  (`/v1/cards/{id}/...` and `/v1/cards/by-key/{key}`), an inbox item, a change, a run or chat, a repository each of whose projects is hidden, the
  project itself (beyond its record, below), a lesson learned from one of its cards, a judgement on one of its cards or
  runs, a grilling, a Retro's proposal, an automation, a research or a workflow run (`/v1/workflows/runs/{id}/...`) of
  such a repository, a request made to the project (`/v1/requests/{id}/...`), and an export holding its judgements. On these routes an id that is not written as a plain number
  (such as `%31`) is answered `404` too, for anyone.
- Nothing is started in it: `POST /v1/chats` with such a `repository`, and `POST /v1/workflows/{name}/runs` naming such
  a repository (as `repository` or as the input `repository`), answer `404`, and a tunnel to a port a run of it opened
  (`GET /v1/ports/{port}/tunnel`) is `404`.
- The lists leave them out: `GET /v1/snapshot` (its boards, cards, changes and inbox items), `GET /v1/events` (no event
  about them is sent), `GET /v1/repositories/{id}/issues` and a card's sub-issues, `GET /v1/issues`, `GET /v1/projects`, `GET
  /v1/lessons`, `GET /v1/judgements`, `GET /v1/replays`, `GET /v1/exports`, `GET /v1/chats`, `GET /v1/grillings`,
  `GET /v1/places`, `GET /v1/notices`, `GET /v1/automations`, `GET /v1/research`, `GET /v1/ports`, and the report of `GET
  /v1/retros/{date}` (its errors and merged cards; the analyst's `run_id` is null when its day touched one, and the
  Retro's `warnings` are left out when anything was). `GET /v1/usage` and `GET /v1/judgements/kinds` answer `404` for such a `repository`.
- A Retro's proposal to the shared configuration (`harness` or `settings`) is shown to everyone without quoting the
  project: its `cards` leave out the project's cards, and in its `title`, `body` and `diff` the project's name and key
  and the titles of its cards read `a private project`, in `GET /v1/proposals`, a Retro's view, the answers of the
  routes that try, merge or dismiss it, and the title and data of its inbox item (`proposal`). A `repository` proposal
  is shown when the repository is.
- A lesson learned from a private project's cards is put in the prompt of, and found by the `lessons` tool for, only
  that project's runs.
- Its wiki pages (`projects/<key>/...`, see [the wiki](#the-wiki)) are left out the same way: the page, its history and
  every older version of it, a write, delete, move or proposal naming one (as the page, or as the target of a move),
  `POST /v1/wiki/from-template` into its folder, and the merge or dismissal of an agent's proposal for one all answer
  `404`; the page list and the search leave them out, and so does the inbox (`wiki_proposal`).
- An owner who is not a member still sees the project's record in `GET /v1/projects` and `GET /v1/projects/{id}`
  (`member` false, `open_cards` 0), can rename it, and names its members with `PUT /v1/projects/{id}/access`, themselves
  included; nothing of its board.

The run token of the Moebius agent and of a Grilling partner sees what the person whose chat it runs in sees; a run
without such a person sees no private project. The training export leaves private projects out unless asked (see
[the training export](training-export.md)).

### `GET /v1/projects`

Every project, in the order they were made: `[ProjectView]`.

### `POST /v1/projects`

An owner, or the Moebius agent, makes a project with no repository yet. Request body: `{"name": string, "key":
string}`. The name is 1 to 80 characters, without a control character or one of `/\<>:"|?*`, does not start with a dot
or a space, does not end with a dot or a space, and is not a device name of Windows (`nul`, `com1`, ...), since it is
also the folder of the project's settings, `projects/<name>/` of the harness configuration repository. The project gets
[its own files](#a-projects-own-files) at once. Response: the `ProjectView`. Errors: `409` for a key another project has
or a repository has as its name, or a name another project has in any case; `422` for a key that is not one or a name
that is not one.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "Storefront", "key": "SHOP"}' http://127.0.0.1:7171/v1/projects
```

### `GET /v1/projects/{id}`

One project: the `ProjectView`. Errors: `404` for no such project, or a private one the person is no member of and,
being no owner, does not see.

### `PATCH /v1/projects/{id}`

An owner, or the Moebius agent, renames a project. Request body: `{"name": string, "key"?: string}`; the key stays as it
was, and `key` is accepted only when it is the project's own. The project's settings move with it: `projects/<old
name>/` becomes `projects/<new name>/` in one commit of the harness configuration repository, pushed to `origin` before
the name changes (decision kind `rename_project`; see [harness
configuration](harness-configuration.md#project-settings)). Response: the `ProjectView`. Errors: `404` for no such
project, `409` for a name another project has in any case, for `projects/<new name>/` being there already, or a push
that is refused; `422` for a name that is not one or a `key` that is not the project's (`a project keeps its key <key>:
the keys of its cards would change`).

### `GET /v1/projects/{id}/scripts`

The script of every column of the project's board, left to right, each as `GET /v1/projects/{id}/scripts/{column}`
answers it: `[{"column", "path", "source", "text"}]`. Errors: `404` no such project.

### `GET /v1/projects/{id}/scripts/{column}`

The script a column runs for the cards on a project's board: the project's own `<file>.luau`, a file of
`projects/<project name>/` in the harness configuration repository (see [column
scripts](column-scripts.md#project-overrides)), or the built-in one. `{column}` is the column's name as the board
shows it, in any case and URL-encoded (`In%20progress`); `Ready`, the old name of `Not started`, finds that column too.
Response: `{"column": string, "path": string, "source": "project" or "built-in", "text": string}`. `path` is where the
project keeps its own script for the column, from the root of the harness configuration repository
(`projects/demo/ready.luau` for `Not started` of the project `demo`; the files are `backlog`, `ready`, `code` for `In
progress`, `review`, `merge` and `done`), whichever `source` is. Errors: `404` no such project or column.

### `PUT /v1/projects/{id}/scripts/{column}`

A person, or the Moebius agent, replaces the column's script for the project. Request: `{"text": string, "reason":
string}`; `reason` is optional and is kept with the decision. The script is checked as `moebius check-script` checks it,
in the column, against a made-up card, and refused when the check fails. Then `projects/<project name>/<file>.luau` is
committed (`Change the <Column> column's script of <project>`) on the default branch of the harness configuration
repository's clone and pushed to its `origin`, as an edit of an agent is. The decision is recorded (kind `edit_script`,
decider `person` or `agent:Moebius`; its context has `project`, `column`, `path`, `text`, `change` and, when given,
`reason`) and the event `script_changed` is sent. Text that changes nothing makes no commit and no decision. Response:
the script as `GET` answers it, now with `source` `project`. Errors: `404` no such project or column; `409` the server
has no `[harness]`, or its push is refused or its clone cannot be brought up to date: the commit is undone and the
message is git's, with keys redacted; `422` the check failed, and the message is the check's (it starts with the
script's file name), or the project's name cannot be a folder (a project from before such names were refused); `500` any
other failure of git or the store.

```sh
curl -s -X PUT -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"text": "function on_card(card) moebius.wait(\"held\") end", "reason": "hold the cards"}' \
  "http://127.0.0.1:7171/v1/projects/1/scripts/Not%20started"
```

### `DELETE /v1/projects/{id}/scripts/{column}`

Removes the project's own script for the column, committed and pushed the same way (`Use the built-in script for the
<Column> column of <project>`, kind `edit_script`), so the column runs the built-in script again. Response: the script
as `GET` answers it, now the built-in one. Errors: `404` no such project or column, or the project has no script of its
own for the column; `409` and `500` as for `PUT`.

### `PUT /v1/projects/{id}/access`

An owner, never an agent, makes a project private or public and names its members. Request body: `{"private": bool,
"members": [string], "reason"?: string}`; `members` are handles, with or without the `@`, each kept once, and replace
the project's list. A private project is seen only by the people it names: an owner who does not name themselves keeps
its record and loses its board. A public project is everyone's, and keeps its list for when it is made private again.
The decision is recorded (kind `edit_project_access`, decider `person`; its context has `project` the key, `private`,
`previous_private` and `members`, the handles joined with `, `) and `project_changed` is sent. Response: the
`ProjectView` as the owner now sees it. Errors: `403` for anyone but an owner; `404` for no such project; `422` for a
handle no person has.

```sh
curl -s -X PUT -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"private": true, "members": ["lea", "owner"]}' http://127.0.0.1:7171/v1/projects/2/access
```

### `POST /v1/projects/{id}/repositories`

An owner, or the Moebius agent, puts a repository among the repositories of a project; the repository is in its home
project and in this one, and its issues are read into the home project as before. Request body: `{"repository":
number, "reason"?: string}`. Nothing changes when the repository is one of the project's already. The decision is
recorded (kind `edit_project_repositories`; its context has `project` the key, `change` `add` and `repository` the
name) and `project_changed` is sent. Everyone who sees the project sees its repositories from then on, also when
the repository's home project is private. Response: the `ProjectView`. Errors: `404` for no such project or repository, or
one the person does not see.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"repository": 1}' http://127.0.0.1:7171/v1/projects/2/repositories
```

### `DELETE /v1/projects/{id}/repositories/{repository}`

Takes the repository out of the project's repositories (decision `edit_project_repositories`, `change` `remove`;
`project_changed` is sent). Response: the `ProjectView`. Errors: `404` for no such project or repository, or a
repository the project does not have; `422` for the repository's home project, which keeps it until the repository is
removed.

### The wiki

The wiki ([ADR 0022](../adr/0022-the-wiki-is-a-git-repository.md)) is a git repository of Markdown pages that the
server clones from `[wiki] url` (see [configuration](configuration.md#wiki)) and keeps in step with its `origin`. Every
route here answers `409` (`the server has no wiki: set [wiki] url in moebius.toml`) while the server has none.

A page is named by its **path**, its file's path in the repository without `.md`: `Handbook`, `Team/On call`. The
organisation's pages are at the top; a project's are in `projects/<key>/`, the folder of its [key](#projects), such as
`projects/SHOP/Payments`. Each part of a path is 1 to 120 bytes, does not start with a dot or start or end with a space,
and has no control character and none of `\ : * ? " < > |`; under `projects/` a page is in the folder of a key, and
writing or moving a page there needs a project with that key. In a URL, the path is the rest of it, with each part
percent-encoded: `/v1/wiki/pages/Team/On%20call`. Files of the repository that are not `.md`, or whose path is not a page's,
are left alone and not listed. The clone checks a link committed to the repository out as a plain file
(`core.symlinks` is false), and a page whose path goes through a link is not listed and is refused (`422`), so no route
reads or writes outside the clone.

An edit is a commit on the clone's checked-out branch with the person as its author (`<name> <<handle>@moebius>`;
Moebius is the committer), made after the clone has followed `origin` and then pushed to `origin`. When the push fails,
the commit is undone and the clone is as it was: `409` with git's message, any key in it redacted. Text that changes
nothing makes no commit. Every commit, and a refresh that brings commits pushed to `origin` from elsewhere, sends
`wiki_changed`; the server refreshes the wiki every 30 seconds, and a failed refresh sends `wiki_error`. Only a person
edits the wiki directly: an agent's run token is refused (`403`) on the routes above that write, and proposes its edit
with `POST /v1/wiki/proposals` instead.

**Private projects.** The pages of a private project ([ADR 0018](../adr/0018-more-than-one-person.md)) are seen only by
its members, owners who are not members included: for anyone else every route that names such a page answers `404`
(reading it or an older version, its history, writing, deleting, moving from or to it, proposing it, making it from a
template, and merging or dismissing its proposal), `GET /v1/wiki/pages` and the search leave it out, and so does the
inbox. The Moebius agent sees what the person of its chat sees; an agent without one sees none but the pages of the project of its run's card. The recorded `merge_proposal` decision about such a page holds none of its title or diff (they read `a private project` and nothing). A page moved out of a private project keeps its history, so older versions stay readable under the new name. Page names spelled `Projects/...` or `PROJECTS/...` are refused. A run's prompt holds
the pages its issue links to with `[[...]]`, but a private project's page only for a card of that project. The
organisation's pages, those of a public project and a folder of a key no project has are seen by everyone.

A `PageView` is `{"path": string, "name": string, "text": string, "version": Version}`: `name` the last part of the
path, `text` the Markdown, `version` the last commit that changed the page. A `Version` is `{"commit": string, "author":
string, "handle": string | null, "at": number, "message": string, "path": string}`: the full commit id, the author's
name, their handle when a person of Moebius made the commit (`null` for one pushed from elsewhere), the time in seconds,
the first line of the message, and the page's path in that commit, which differs from today's before a rename or move.

### `GET /v1/wiki/pages`

Every page, sorted by path: `[{"path": string, "name": string}]`; `[]` while the repository has no commit.

### `GET /v1/wiki/pages/{*path}`

The page: its `PageView`. With `?at=<commit>` (the full id, or at least its first 7 characters), the page as that
commit of its history left it, even after it was deleted. Errors: `404` for no such page, or a commit that is not in its
history; `422` for a path that is not a page's.

### `PUT /v1/wiki/pages/{*path}`

A member writes a page, making it, and its folders, when it is not there. Request body: `{"text": string, "message"?:
string}`; the text gets a newline at its end, and `message`, its first line, is the commit's (by default `Add <path>` or
`Update <path>`). Response: the `PageView` as it now is. Errors: `409` when the push fails, `422` for a path that is not
a page's or under `projects/` a key no project has.

```sh
curl -s -X PUT -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"text": "# On call\n\nWho answers at night.", "message": "Say who answers"}' \
  http://127.0.0.1:7171/v1/wiki/pages/Team/On%20call
```

### `DELETE /v1/wiki/pages/{*path}`

A member deletes a page, a commit `Delete <path>`; folders left empty go with it. Its history stays and its versions can
still be read with `?at=`. Response: `null`. Errors: `404` for no such page, `409` when the push fails.

### `POST /v1/wiki/moves`

A member moves a page. Request body: `{"from": string, "to": string}`, two paths; a rename is a move within the same
folder (`Rename <from> to <name>`), any other a `Move <from> to <to>`. The history follows the page. Response: the
`PageView` at `to`. Errors: `404` for no page at `from`, `409` for a page at `to` already or a failed push, `422` for a
path that is not a page's or `to` the same as `from`.

### `GET /v1/wiki/history/{*path}`

The commits that changed the page, newest first, following it through its renames and moves: `[Version]`. A deleted
page keeps its history, its deletion first. Errors: `404` for a page that never was.

### `GET /v1/wiki/search`

Finds pages by the words of their title and text: `?q=<words>`, optionally `&project=<key>` for only the pages under
`projects/<key>/` (without it, the organisation's pages and every project's), and `&limit=<n>` (1 to 100; 20 without
one). Every word of `q` must be in the page, in any order, the last one may be only its beginning (`refu` finds
`refunds`), and a word matches its other forms (`refund` finds `refunds`). A word of the title counts far more than one
of the text, so a page called for the words comes before one that mentions them. Response: `[SearchHit]`, the best
first, `{"path": string, "name": string, "project": string | null, "snippet": string}`: the page's path and name, the
key of its project (`null` for the organisation's pages) and the words around the match as plain text. An empty `q`
finds nothing (`[]`).

**Only what the asker may see is returned**: a page under `projects/<key>/` of a private project the asker is no member
of is never in the answer, and does not take the place of a page that is. The index is derived from the pages
and kept in the store; it is made again at the next search after the wiki's head moved (`wiki_changed`), so an edited,
moved or deleted page, and one pushed from elsewhere, is found as it is now.

### `POST /v1/wiki/proposals`

An agent's edit of a page, or a member's who would rather it were reviewed (the run token of the Moebius agent is
accepted). Request body: `{"path": string, "text": string, "message"?: string}`, as `PUT` takes them. Nothing is
pushed and no page changes: the clone gets a branch `proposal/run-<run id>-<n>` (`proposal/<handle>-<n>` for a person)
that holds the change as one commit, authored `<name> (agent) <agent@moebius>` for an agent, and the inbox gets an item
of the kind `wiki_proposal` with the title `Wiki: <message>` and no card. Response: that `InboxItem`, whose `data` is
`{"branch": string, "page": string, "message": string, "by": string, "commit": string, "diff": string}`; `diff` is the
page's change, cut at 40 000 bytes. Errors: `409` while the wiki has no page yet (a person writes the first), `422` for
a path that is not a page's, under `projects/` a key no project has, or a text the page has already, `403` for a column
script.

### `POST /v1/wiki/proposals/{id}/merge`

A member merges the proposal of the inbox item `id`. The clone follows `origin`, the branch is merged into the clone's
branch with the commit `Merge the proposal: <message>` and pushed to `origin`; the branch is deleted, the item resolved,
the decision recorded as `merge_proposal` (the kind the Retro's proposals use, with `kind` `wiki_proposal` in its context) and `wiki_changed` sent. Request body: `{"reason"?: string}`. Response:
`{"commit": string}`, the commit the pages are now read at. Errors: `404` for no open `wiki_proposal` item `id`, `409`
for a merge that conflicts with what was pushed since (the page changed meanwhile) or a push that fails; both leave the
clone and the item as they were.

### `POST /v1/wiki/proposals/{id}/dismiss`

A member dismisses the proposal: its branch is deleted, the item resolved, and the wiki is as it was. Request body
`{"reason"?: string}`. Response: `null`. Errors: `404` as for merging.

### `POST /v1/wiki/refresh`

Fetches `origin` and fast-forwards the clone now, rather than at the next refresh, so that what someone pushed there is
read. Response: `null`. Errors: the fetch's failure.
### Intake

A project can take requests (ADR 0019). An owner turns it on with `PUT /v1/projects/{id}/intake`; until then the project
has no form, and a request to it is refused with `422`. A request is what a person who may see the project, a viewer too,
writes in their own words; Moebius starts a grilling on it (the grilling of [Grilling](../guide/grilling.md), here a chat of
the origin `automation`, so that it belongs to no one and the members read and answer it; the requester is never asked
to wait) in the project's first repository, else its own files. When the chat publishes or ends, the request gets a
draft issue: the first issue of `issues.json` (the others are listed at the end of its body), or, for a report, an
empty result or a grilling that could not be started, the person's own words (the first line is the title). A request's
grilling puts no `grilling_issues` item in the inbox: the draft is an inbox item `intake_request` instead (`data.request`,
`data.project`, `data.requester`; no card), resolved when the request is decided.

An `IntakeRequest` is `{"id", "project_id", "requester": handle, "text", "title", "body", "status", "run_id": number
or null, "card_id": number or null, "reason", "created_at", "decided_at": number or null}`. `status` is `grilling` while the
chat works on it (`title` and `body` are empty), `drafted` once it has a draft a member may edit, `accepted` with its
`card_id`, or `declined` with the `reason`. A viewer reads only their own requests (another's is `404`); a member or an
owner reads every request. Every change sends `intake_changed` with `{"project": id, "request": id, "status": string}`
and no text. Accepting and declining are recorded as `accept_request` and `decline_request` decisions.

#### `PUT /v1/projects/{id}/intake`

An owner turns intake on or off. Request: `{"enabled": bool}`. Requests already made stay. Response: the `ProjectView`.
Errors: `404` for no such project.

#### `POST /v1/projects/{id}/requests`

A person writes a request. Request: `{"text": string}`, 1 to 8000 characters after trimming, redacted of secrets.
Response: the `IntakeRequest`, `grilling` while a chat drafts it. Errors: `404` for no such project, `422` for a project
without intake or a blank or too long text.

#### `GET /v1/projects/{id}/requests`

The requests the asker may read, the newest first: `[IntakeRequest]`. Errors: `404` for no such project.

#### `GET /v1/requests/{id}`

One request: the `IntakeRequest`. Errors: `404` for no such request, or for another person's when the asker is a viewer.

#### `PUT /v1/requests/{id}`

A member edits the draft issue, which also gives a request still being grilled its draft at once. Request: `{"title":
string, "body"?: string}`. Response: the `IntakeRequest`. Errors: `404`; `422` for a blank title or a request that was
decided.

#### `POST /v1/requests/{id}/accept`

A member accepts a `drafted` request, as it is or with `{"title"?: string, "body"?: string}` as edited. A confirmed card
of the project is made in the project's first repository (opened on its forge first, as a person's issue is), written by
the requester, who follows it and is told in the Inbox's Mentions tab ("Your request was accepted"). The request's chat is
ended and its proposal dropped. Response: the `IntakeRequest`, `accepted`. Errors: `404`; `422` for a request that is not
`drafted`, which includes one decided at the same moment by another member (only one card is made).

#### `POST /v1/requests/{id}/decline`

A member declines a request that is `grilling` or `drafted`. Request: `{"reason": string}`; a blank reason is `422`. The
requester is told the reason in the Inbox's Mentions tab and reads it in the request. Response: the `IntakeRequest`,
`declined`. Errors: `404`; `422` for a request that was decided already.


### `POST /v1/wiki/from-template`

A member makes a page from a page [template](#templates). Request body: `{"template": string, "path": string}`. The page
at `path` has the template's text and is a commit in the wiki like any write. Response: the `PageView`. Errors: `404` for
no such template, `409` for a page at `path` already or a server without a wiki, `422` for a path that is not a page's.

### Templates

Templates are kept in the harness configuration repository (see [harness configuration](harness-configuration.md#templates)):
an issue template for each kind of issue, and page templates for the wiki. Everyone reads them; a person who is a member
edits them, and an edit is a commit pushed to `origin` and recorded as a decision (kind `edit_template`, decider
`person`). A server without a harness configuration repository lists none (`editable` false) and refuses an edit with `422`.

### `GET /v1/templates`

`{"editable": bool, "issues": [{"kind": text, "title": text, "body": text, "fields": {name: value}}], "pages": [{"name":
text, "text": text}]}`.

### `PUT /v1/templates/issues/{kind}`

Sets the template of a kind (matched in any case). Request body: `{"title": text, "body": text, "fields": {name: value}}`,
each optional. Every field named has to be one of the kind's. Response: the template as listed, with `commit` (the short id;
left out when nothing changed). Errors: `404` for no such kind, `422` for a field the kind lacks.

### `DELETE /v1/templates/issues/{kind}`

Deletes it; response `{"commit": text}`. `404` when the kind has no template.

### `PUT /v1/templates/pages/{name}` and `DELETE`

Sets or deletes a page template. A name is letters, digits, `-` and `_`, up to 64 (`422` otherwise). Request body:
`{"text": text}`. Response: `{"name", "text", "commit"}`; a delete answers `{"commit"}` and `404` for no such template.
### Milestones

A milestone (ADR 0019) is a goal of the organisation, not of a project: it has a name (unique in any case, 1 to 80
characters, no control character), a target date (`YYYY-MM-DD`), a description (up to 10 000 characters) and holds
cards of any projects. A card is in any number of milestones. A milestone's progress is how many of its cards are done,
a card being done when its issue is closed or it is in `Done`.

**A person counts and sees only the cards of projects they may see.** Every card and every number below is worked out
for the asker: a card of a project they may not see is not listed, not counted in `cards`, `done` or `projects`, and
its `GET /v1/cards/{id}/milestones` and `PUT` answer `404`, as if there were no card. A private project is
seen only by its members (see [private projects](#private-projects)); the check is `App::may_see_project`.

A `MilestoneView` is `{"id": number, "name": string, "target_date": string, "description": string, "created_at":
number, "cards": number, "done": number, "projects": [{"id": number, "name": string, "key": string, "cards": number,
"done": number}]}`; `projects` are the visible projects the milestone holds cards of, in the order the projects were
made. A `MilestoneDetail` is a `MilestoneView` whose `cards` is a list instead of a count: `[{"id", "project_id",
"repository_id", "number", "title", "column", "done"}]`, in the order the cards were made. A milestone made or edited
sends `milestone_changed`, a deleted one `milestone_removed`, and a card's set of milestones changing
`card_milestones_changed`.

#### `GET /v1/milestones`

Every milestone with its progress, the nearest target date first: `[MilestoneView]`.

#### `POST /v1/milestones`

A member or an owner makes a milestone. Request: `{"name": string, "target_date": string, "description"?: string}`.
Response: the milestone, `{"id", "name", "target_date", "description", "created_at"}`. Errors: `409` for a name another
milestone has in any case; `422` for a name, a date that is not one (`2027-02-30`) or a description that is not valid.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "Launch 2.0", "target_date": "2027-01-31"}' http://127.0.0.1:7171/v1/milestones
```

#### `GET /v1/milestones/{id}`

One milestone with the cards the asker may see: the `MilestoneDetail`. Errors: `404` for no such milestone.

#### `PATCH /v1/milestones/{id}`

Replaces a milestone's name, target date and description; the request is the one of `POST`. Setting what is set changes
nothing and sends no event. Response: the milestone. Errors: as `POST`, and `404` for no such milestone.

#### `DELETE /v1/milestones/{id}`

Deletes a milestone and answers it; the cards it held stay. Errors: `404` for no such milestone.

#### `GET /v1/cards/{id}/milestones`

The milestones the card is in, the nearest target date first: `[milestone]`. Errors: `404` for no such card, or a card
of a project the asker may not see.

#### `PUT /v1/cards/{id}/milestones`

Replaces the milestones the card is in. Request: `{"milestones": [id]}`; repeats count once and `[]` takes the card out of
all. The event `card_milestones_changed` is sent when the set changed. Response: the milestones the card is in. Errors:
`404` for no such card or milestone.

### `POST /v1/repositories`

Adds a repository: the server clones `url` into its data directory and reads the clone's checked-out branch as the
default branch.

Request:

| Field | Type | Meaning |
|---|---|---|
| `name` | string | Required. Letters, digits, `-`, `_` and `.`, and not starting with a dot. Must be unique regardless of case, and no project's key. |
| `url` | string | Required. Anything `git clone` takes: a URL or a path on the server. Without a forge, an absolute path that does not exist, or an empty folder, is first made a new repository: branch `main` with one empty commit, "Start the repository". |
| `forge` | string | `none` (the default), `github`, `gitlab` or `forgejo`. |
| `github` | string | `owner/repo` on GitHub. Required with the forge `github`, not allowed otherwise. Both parts are made of letters, digits, `-`, `_` and `.`. |
| `project` | string | The project's path on GitLab, groups first: `group/project` or `group/subgroup/project`. Required with the forge `gitlab`. At least two parts, each made of letters, digits, `-`, `_` and `.` and not starting with a dot. With the forge `forgejo`, the repository as `owner/repo`: exactly two such parts. Not allowed otherwise. |
| `trusted` | array of strings | Logins on the forge (GitHub logins, GitLab usernames, Forgejo logins) whose issues need no approval. Only with a forge. |

Response: the `Repository`. Errors: `422` for an invalid name, an absolute path to a folder with files that is not a git
repository (nothing is created then), a `github` that is not `owner/repo`, a `project` that is not a path as above, or
a combination other than "no forge, no `github`, no `project`, no `trusted`", "forge `github` with `github`" , "forge
`gitlab` with `project`" and "forge `forgejo` with `project`"; `422` also when the name is taken by a repository or a project (nothing is cloned or created then); `500` when the clone
fails.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"name": "demo", "url": "/srv/git/demo"}' http://127.0.0.1:7171/v1/repositories
```

For GitHub, with the App or an access token set up, the clone authenticates as the App's installation or as the token's user, so a private repository works:

```json
{"name": "widgets", "url": "https://github.com/acme/widgets.git", "forge": "github", "github": "acme/widgets", "trusted": ["leon"]}
```

For GitLab, with `[gitlab]` set up, the clone sends the access token when `url` is on that GitLab, so a private project
works:

```json
{"name": "tools", "url": "https://gitlab.com/acme/platform/tools.git", "forge": "gitlab", "project": "acme/platform/tools", "trusted": ["leon"]}
```

For Forgejo, such as Codeberg, with `[forgejo]` set up, the clone sends the access token when `url` is on that forge:

```json
{"name": "gadgets", "url": "https://codeberg.org/acme/gadgets.git", "forge": "forgejo", "project": "acme/gadgets", "trusted": ["leon"]}
```

### `PUT /v1/repositories/{id}`

A person, or the Moebius agent, replaces the logins whose issues on a repository on a forge need no approval.
Request: `{"trusted": [string], "reason": string}`; `reason` is optional and is kept with the decision. Each login is
trimmed and kept once (in any case); a login must be made of letters, digits and `-_.[]`. The decision is recorded
(kind `edit_repository`, decider `person` or `agent:Moebius`; its context has `repository`, `trusted` and
`previous_trusted`, the logins joined with `, `) and the event `repository_updated` is sent. A list that changes nothing
records nothing. The new list counts for cards checked from then on; a card already approved or waiting stays as it is.
Response: the `Repository`. Errors: `404` no such repository; `422` the repository is on no forge, or a login is blank
or has other characters.

### `DELETE /v1/repositories/{id}`

A person, or the Moebius agent, removes a repository from Moebius: the repository with every card of its board and what
belongs to them (their runs and their transcripts, usage, judgements, replay shadows, inbox and dependencies), its
changes, its chats and grillings, the open inbox items that name its changes (`ready_to_merge`), grillings
(`grilling_issues`, `grilling_report`) or proposals (`proposal`) without belonging to a card, its lessons, its research, its automations, its localize index and its plugins' project settings; and its clone and
worktrees under the server's data directory. The decision is recorded (kind `delete_repository`, decider `person` or
`agent:Moebius`; its context has `repository`) and the event `repository_removed` is sent.

Request: `{"reason": string}`; `reason` is optional and is kept with the decision. A body is not required.

Response: the `Repository`, as it last was. Errors: `404` no such repository; `409` one of its runs is still going —
stop that run first, nothing is removed then; `422` for the repository that holds a project's own files, which goes only
with its project.

```sh
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" http://127.0.0.1:7171/v1/repositories/2
```

### `POST /v1/repositories/{id}/issues`

Creates an issue from a person in the app. Its card starts in `Backlog`, with the author `person:<handle>` of the person who made it.

Without a forge the card is trusted and moves on. On a repository with a forge the issue is opened there first, the same
way a confirmed grilling opens its issues, and the card gets the issue's number right away, so that the poll finds it
existing. The card is `confirmed`, since a person wrote it, and so moves on from `Backlog` without approval, whatever the
repository's `trusted` list says.

Request: `{"title": "...", "body": "...", "parent": number, "labels": [text], "blocked_by": [number], "kind": text,
"fields": {name: value}, "use_template": bool}`. `title` is required and must not be empty or blank (unless `use_template`
supplies one); the others are optional. `kind` and `fields` are set as `PUT /v1/cards/{id}/kind` and `/fields` do, after
the issue is made, so a kind the project does not offer or a value a field refuses is a `warnings` entry. With
`use_template` (which needs `kind`), the [template](#templates) of the kind fills an empty `title` or `body` and the fields
`fields` does not give; `404` when the kind has no template. `fields` without `kind` is `422`. `parent` and `blocked_by` are issue numbers of the same
repository and are set as `PUT /v1/cards/{id}/parent` and `POST /v1/cards/{id}/blockers` do, `labels` as `PUT
/v1/cards/{id}/labels` does.

What `parent` and `blocked_by` name is checked before anything is created: the issues have to exist in the repository,
and a blocker must not wait for the parent (it would make the two wait for each other). If the issue is created and a
later step is still refused by the forge (a label, say), the issue exists and is the answer, with a `warnings` field.

Response: the `Card`, and `warnings` (array of text, one for each step that failed after the issue was created; left out
when nothing failed). Errors: `404` no such repository, or no such parent or blocker; `422` the title is empty, or a
blocker would make a cycle; `500` the forge refuses to open the issue (no card is made).

### `GET /v1/repositories/{id}/issues`

The issues of a repository, oldest first, as `Issue`s. Every filter that is given must match; they are query
parameters: `parent` (an issue number: only its sub-issues), `label` (an exact label), `state` (`open` or `closed`),
`claimed` (`true` or `false`), `archived` (`true` for the issues whose card is archived, `false` for the others; archived
issues are listed too without it) and `blocked` (`true` for the issues an issue that is not done blocks, `false` for the
others). The frontier of a map of tickets, the open sub-issues that nothing blocks and nobody claimed, is
`?parent=<map>&state=open&blocked=false&claimed=false`. Errors: `404` no such repository.

### `GET /v1/repositories/{id}/settings`

The settings of the repository, what is about its code: the files of `repos/<repository name>/` in the harness
configuration repository (see [harness configuration](harness-configuration.md#repository-settings)); the column scripts
are its projects' (`GET /v1/projects/{id}/scripts`).
Response: `{"path": string, "editable": bool, "files": [{"file": string, "exists": bool, "text": string}],
"leftover": [string]}`. `path` is where they are (`repos/demo`). `editable` is false when the server has no
`[harness]`, where nothing can be saved and no repository has any file. `files` has an entry for each of `rules.md`,
`review.md`, `checks`, `test-first` and `localize`, in that order, with the file's text (empty when it does
not exist; a `test-first` that exists is empty and turns test first on). `leftover` lists the files of a `.moebius/`
folder that the repository's default branch still has, which are not read: move them to the settings. Errors: `404` no such repository.

### `PUT /v1/repositories/{id}/settings/{file}`

A person, or the Moebius agent, writes one of the files `rules.md`, `review.md`, `checks`, `test-first`
and `localize` of the repository. Request: `{"text": string, "reason": string}`; `reason` is optional and is kept with the
decision. A final new line is added to a text without one. `rules.md` is checked as `PUT /v1/rules` checks the server's
(a `## ` line needs a name and no name may be given twice); text that
fails is refused with `422` and a message that starts with the file's name, and nothing is committed. Otherwise the file
is committed (`Update <file> of <repository>`) on the default branch of the harness configuration repository's clone and
pushed to its `origin`. The decision is recorded (kind `edit_repository_rules` for `rules.md`, else `edit_project_file`,
decider `person` or `agent:Moebius`; its context has `repository`, `path`, `text`, `change` and, when given, `reason`)
and the event `project_settings_changed` is sent. Text that changes nothing makes no commit and no decision. Response: `{"file",
"exists", "text", "commit"}`, where `commit` is the short id of the commit, left out when nothing changed. Errors: `404`
no such repository; `409` and `500` as for `PUT /v1/projects/{id}/scripts/{column}` (`409` also when the server has
no `[harness]`); `422` the file is not one of the five (column scripts are a project's, with their own route), the text fails its check,
or the repository's name is a device name of Windows (`con`, `nul`, `aux`, `prn`, `com1` to `com9`, `lpt1` to `lpt9`, in
any case and with any extension), for which no `repos/<name>/` directory can be made.

### `DELETE /v1/repositories/{id}/settings/{file}`

Removes the file from the repository's settings, committed and pushed the same way (`Remove <file> of <repository>`, same kinds); with
`test-first` it turns test first off. Optional body `{"reason": string}`. Response: the file as `PUT` answers it, now with
`exists` false. Errors: `404` no such repository or the project has no such file; otherwise as for `PUT`.

### `GET /v1/repositories/{id}/plugin-settings`

The project settings of the repository: what each plugin asks of every repository in its manifest
([`project_settings`](plugin-api.md#plugin-settings)), with the value this repository has. Only plugins added to the
server that declare some are listed, by name; the web app shows them on the repository's page. Response:
`{"repository": number, "plugins": [{"plugin": string, "version": string, "settings": [{"key": string, "label": string,
"description": string, "input": "line" or "code", "language": string or missing, "value": string}], "enabled": boolean, "ui": boolean}]}`,
`value` empty when the repository has none, `enabled` false for a plugin that is [turned off](configuration.md#turning-a-plugin-off-and-on) (its settings are kept and can be changed), and `ui` true when the plugin brings its own page for its settings
([`GET /v1/plugins/{plugin}/settings-ui`](#get-v1pluginspluginsettings-ui)). A plugin that is not running is left out, but its values are kept and come back with it. A server without `[harness]`
has no values.
Errors: `404` no such repository.

```json
{"repository": 1, "plugins": [{"plugin": "podman-sandbox", "version": "0.2.0", "settings": [{"key": "dockerfile", "label": "Dockerfile", "description": "The image of the runs of this repository.", "input": "code", "language": "dockerfile", "value": "FROM docker.io/library/node:24-bookworm\n"}], "enabled": true, "ui": false}]}
```

### `PUT /v1/repositories/{id}/plugin-settings/{plugin}`

A person replaces the values of one plugin's project settings for the repository. Request: `{"values": {string:
string}, "reason": string}`; `values` holds every setting by key, a key left out or an empty value being no value, and
`reason`, optional, is kept with the decision. A key the plugin does not declare is refused, as is a value over 256 KiB
and a line break in a setting whose `input` is `line`. The decision is recorded (kind `edit_project_settings`, decider
`person`; its context has `repository`, `plugin` and `keys`, the keys that now have a value, joined with `, `; not the
values). Values that change nothing record nothing. The values are the files `plugins/<plugin>/<key>` of the repository's [settings](harness-configuration.md#repository-settings), saved as one commit pushed to `origin` (the decision's context also has `change` and `path`), and the event `project_settings_changed` is sent with `file` `plugins/<plugin>`. A launcher gets the new values at the next launch. Response: the
plugin's entry as `GET` answers it. Errors: `403` the Moebius agent asked: a launcher's settings say what a run may do,
so only a person changes them; `404` no such repository, or no plugin of that name with project settings is running; `409` the server has no `[harness]` (the values are files of its repository), or `origin` refused the push;
`422` a key, a value too long or a line break, as above.

The values are kept as written, without redaction, and handed to the plugin: they are not a place for keys and
tokens.

### `GET /v1/repositories/{id}/tree`

A directory of the repository as it is on its default branch (the same branch column scripts are read from: `origin`'s
for a repository on GitHub, the clone's own otherwise). `path` is the directory from the repository's root, with `/`
between directories, given as `?path=src/lib`; left out or empty, it is the root. Response: `{"path": string, "entries": [{"name": string, "path":
string, "kind": "dir" or "file", "size": number}]}`: `path` is the directory without leading or trailing `/`, an
entry's `path` is from the root, `size` is in bytes (0 for a directory); directories come first, each group sorted by
name. Submodules are left out. Errors: `404` no such repository or directory (a file is not one); `422` the path is
absolute, climbs out with `..`, or names `.git` (in any case, or as `.git.`, `.git `, `GIT~1`).

### `GET /v1/repositories/{id}/files`

The paths of every file on the repository's default branch, for the app's "Go to file". Optional `?query=`: only the
paths that contain the characters of the query in that order, ignoring case and surrounding blanks (the app ranks them
itself). Response: `{"paths": [string]}`, from the repository's root, sorted; submodules are left out, and a clone
without a commit has none. Errors: `404` no such repository.

### `GET /v1/repositories/{id}/search`

A full-text search of the files on the default branch, run with `git grep` in Moebius's clone, ignoring case, skipping
binary files. `?q=` is the text to find (required, not blank); `?regex=true` takes it as an extended regular
expression instead of the text itself; `?limit=` is the most hits answered (default 200, at most 1000). Response:
`{"files": [{"path": string, "hits": [{"line": number, "text": string}]}], "total": number, "truncated": bool}`: files
in path order, hits in line order (`line` from 1, `text` cut after 300 characters), `total` the hits in `files`, and
`truncated` true when there are more than the limit. Errors: `404` no such repository; `422` `q` is blank or the
regular expression is invalid (the message is git's).

### `GET /v1/repositories/{id}/files/{*path}`

A file of the repository as it is on its default branch; `{*path}` is the rest of the URL, from the repository's root
(`/v1/repositories/1/files/src/main.rs`). Response, always one shape: `{"path": string, "size": number, "text": string}`
for a UTF-8 file of at most 1 MiB (1048576 bytes); a larger file has `"too_large": true` and a file that is not UTF-8
text, or holds a NUL byte, has `"binary": true`, both without `text`. `binary` and `too_large` are left out when they
are false. Errors: `404` no such repository or file (a directory is not one); `422` as for the tree.

### `PUT /v1/repositories/{id}/files/{*path}`

A person, or the Moebius agent, writes the file, creating it and its directories when they are new. Request: `{"text":
string, "message": string, "reason": string}`; `message` is the commit's message, `Update <path>` when it is left out or
blank, and `reason`, optional, is kept with the decision. The file is committed on the default branch of Moebius's
clone, the same way `PUT /v1/projects/{id}/scripts/{column}` commits a script: for a repository on GitHub the clone
is first brought up to date with `origin` and the branch is pushed with the installation's token (or the access token). The decision is
recorded (kind `edit_file`, decider `person` or `agent:Moebius`; its context has `repository`, `path` and `change`, the
commit's message) and the event `file_changed` is sent. Text that changes nothing makes no commit and no decision.
Response: the file as `GET` answers it. Errors: `404` no such repository; `409` as for a script: the clone cannot be
brought up to date or the push is refused, the commit is undone and the message is git's, with keys redacted; `422` the
path is empty, absolute, climbs out with `..`, names `.git` (in any case, or as `.git.`, `.git `, `GIT~1`), is a directory, or goes through a symbolic link in the
clone.

### `PUT /v1/cards/{id}`

A person, or the Moebius agent, changes a card's title and body, in any column. Request: `{"title": string, "body":
string, "reason": string}`; `body` and `reason` are optional (`body` left out is empty), and `reason` is kept with the
decision. The title is trimmed and must not be blank. On a repository with a forge the issue is changed there first
(for GitHub, `PATCH /repos/{owner}/{repo}/issues/{number}` with the installation's token or the access token; for GitLab,
`PUT /projects/{path}/issues/{number}` with the access token; for Forgejo, `PATCH /repos/{owner}/{repo}/issues/{number}`
with the access token), and "blocked by #n" and "Part of #n" in the
new body become the card's blockers and parent, as the poll reads them (what the old body said no longer counts, and the
blockers and the parent the forge keeps itself stay); without a forge they stay as they are. Then the
card is stored, the decision is recorded (kind `edit_card`, choice `edit_card`, decider `person` or `agent:Moebius`,
the card's id; its context has `title`, `body`, `previous_title` and `previous_body`) and the event `card_updated` is
sent. A title and body that change nothing record nothing. Response: the `Card`. Errors: `404` no such card; `422` the
title is blank; `500` the forge refused the change, and then the card is as it was and nothing is recorded.

### The issue tracker

An issue has more than a title and a body: a parent, blockers, duplicates and related issues, labels, a claim, comments and a state. All of it is read
and written here, the same on every forge and without one, so that a planning session or a person works on issues through
the API, `"$MOEBIUS" issue` and the web app alike. Issues are addressed by their card's id in the routes; the relations
(`parent`, a blocker) name issue numbers of the same repository, as `Card.number` is. Every change is announced as its own
event (see [the events](#get-v1events)).

Where each thing lives depends on the forge; what a forge cannot express, Moebius keeps itself:

| | Without a forge | GitHub | GitLab | Forgejo (and Gitea) |
|---|---|---|---|---|
| Parent | Moebius | Sub-issues (`parent_issue_url`, `POST /issues/{n}/sub_issues`) | A line `Part of #n` in the description | A line `Part of #n` in the description |
| Blockers | Moebius | Issue dependencies (`blocked_by`), and "Blocked by #n" lines | Issue links of the type `is_blocked_by` (GitLab Premium and above; without them GitLab links the issues as `relates_to`, which blocks nothing, so Moebius writes the line "Blocked by #n" in the description instead, and takes it out again), and the lines | Issue dependencies, and the lines |
| Labels | Moebius | Labels (made when they do not exist) | Labels (made when they do not exist) | Labels, by name (a label has to exist in the repository) |
| Comments | Moebius | Comments | Notes written by people (GitLab's own notes are left out) | Comments |
| State | Moebius | Open or closed | Opened or closed | Open or closed |
| Claims | Moebius | Moebius | Moebius | Moebius |

- **Polling keeps it current both ways.** What a change through the API writes to the forge is read back by the next poll,
  and what someone changes on the forge becomes the card's parent, labels, blockers, comments and state. The poll acts on
  what the forge *changed*: Moebius keeps, for each of those, what the forge was last seen to say, and a poll that says
  the same is not a change. After Moebius writes to the forge it takes what it wrote for what the forge says, so that
  someone who puts the old value back before the next poll is noticed; a snapshot whose read began before the write
  ended (a slow poll) is left alone, because it may not have seen the write. What the poll could not apply (an edge
  that would make issues wait for each other, see below) is reported as `forge_error` and not taken as seen, so it is
  tried again when the issue is read again. The first poll after an upgrade announces nothing for issues that are
  closed and in `Done` already.
  A blocker, label or parent that a poll no longer finds on the forge, where it did
  before, is dropped from the card. A forge that cannot list blockers (GitLab links or Forgejo dependencies turned
  off, say, or a read that failed) leaves the card with the blockers it had from the forge, and changes only those the
  description names. A poll that cannot read an issue's comments reports `forge_error` and reads them at the next poll.
  What the poll reads besides the issue list costs requests only for the open issues it syncs (not the closed ones with
  no card, nor the ones a person deleted): the blockers of an issue are read when the issue changed, or, because GitLab
  does not always change an issue's time when a link changes, when what was read is ten minutes old, which is checked at every poll, five issues at a time, oldest first, also
  when the forge answered that nothing changed. A comment is read when the forge's count differs from the comments
  Moebius stored, and when the issue changed since its comments were read and it has any (a comment deleted and another
  written between two polls leave the count as it was, and the issue's list says no more of them); comments edited or
  deleted on the forge are not changed or removed in Moebius.
- **Blockers** from the forge's own dependencies and from "Blocked by #n" lines in the description are added together.
  Removing one through the API also takes the line out of the description. A blocker the forge says is closed counts as
  done even when Moebius has no card for it (it was closed before the repository was added). Dependency lists are read
  in all their pages.
- **Only issues a person can have vouched for are sub-issues.** An issue says "Part of #n" (or has a parent on the
  forge) whoever writes it, so a parent counts as its sub-issues only those whose author is trusted, that a person
  approved or that a confirmed grilling made. Another issue is shown with the parent it names, but is not in
  `Issue.children`, in `card.children` or in the filter `parent`, so it cannot stall a parent or close it, and its edges are left out of what makes issues wait for each other. A parent in
  another repository (GitHub allows it) is ignored: the number would be that of another issue here.
- **Nothing makes issues wait for each other.** An issue cannot be its own parent or blocker, nor be blocked by an
  ancestor, nor have a parent that waits for it, directly or through others; what the API refuses with `422`, the poll
  ignores when the forge says it.
- **Claims are Moebius's own and advisory.** A run is not a forge user, so a claim is not an assignee: nothing is written
  to the forge and the assignees the forge has are not read. A claim names a person (`person:<handle>`), the Moebius agent
  (`agent:<name>`) or a name a session gave itself, which is free text: the claim keeps honest sessions from taking the
  same issue, it does not keep anyone out, and the default Kanban does not look at it.
- **Comments** read from the forge are stored once each, by the id they have there, and are read again when the forge's
  count of an issue's comments differs from what Moebius has (and from what the forge said when they were last read, so
  that a comment deleted on the forge is read once and not at every poll). A comment that is edited or deleted on the
  forge is not changed or removed on the card. A comment written through the API is written to the forge as the forge's
  own identity (the GitHub App, the access token and the user it belongs to) and kept with the author who wrote it here; if the poll read it from
  the forge first, the stored one is the answer.
- **Closing** an issue closes it on the forge and moves its card to `Done`, so that issues it blocks are not blocked any
  more; a closed issue is its card in `Done`. A forge that reports an issue closed does the same, and one that reports a
  closed issue open again moves its card back to `Backlog`. A card in `Done` because its change was merged is not reopened
  by an issue the forge left open.

### `GET /v1/cards/{id}`

One issue: the `Issue` of the card, with its state, the blockers that are not done and its sub-issues. Errors: `404` no
such card.

### `GET /v1/cards/by-key/{key}`

The issue a card key names, as `GET /v1/cards/{id}` answers it. A card is named by its project's key and its number in
the project (`WEB-24`), unique across projects; the key may be written in any case. A new card takes the number of its
issue as its number in the project while the project has no card with that number, else the next free one, so that a
project over several repositories never has two `WEB-7`; a card that was there before projects keeps its number. A
card of a forge keeps the forge's number beside its key (`number`). The project's key stays when it is renamed. Errors:
`404` no such card or project; `422` the text is not a key and a number.

### `PUT /v1/cards/{id}/parent`

Makes the issue a sub-issue of another, or gives it none. Request: `{"parent": number or null}`, the issue number of the
parent. On GitHub the sub-issue is made there; on GitLab and Forgejo the line `Part of #n` of the description is written
(replacing the one it has) with the issue's edit, and the card's body shows it. The event `card_parent_changed` is sent;
setting what is set changes nothing. The description is rewritten from the one the forge has now, so that an edit made there
since the last poll is kept. Response: the `Card`. Errors: `404` no such card or parent; `422` the issue as its own parent,
a parent that is below it, or a parent that waits for the issue (it is blocked by it, or by an issue that is); `500` the
forge refused.

### `PUT /v1/cards/{id}/labels`

Replaces the issue's labels, on the forge too. Request: `{"labels": [text]}`; blank labels and repeats are dropped. The
event `card_labels_changed` is sent. Response: the `Card`. Errors: `404` no such card; `500` the forge refused (a Forgejo
repository without such a label, for one).

### Kinds of issues

An owner defines kinds of issues for the organisation, such as a bug, a feature or a task, each with fields of its own; a
project picks the kinds it offers, and a card of the project has at most one of them, with values for its fields. A
field belongs to its kind, so one field is set on the cards of every project that picked the kind, and a question across
projects can filter and sort by it. Column scripts see them as `card.kind` and `card.fields` (see [Column
scripts](column-scripts.md)). A `KindView` is `{"id": number, "name": string, "fields": [{"id": number, "name": string,
"type": "text" | "number" | "choice" | "date" | "person", "options": [string]}], "projects": [number]}`: `fields` in the
order the owner wrote them, `options` the values of a `choice` (empty for the other types), `projects` the ids of the
projects that offer the kind.

A name of a kind, a field or an option is 1 to 60 characters with no space around it and no control character. The name of a
kind is unique regardless of case, and so is a field's within its kind; a kind has at most 30 fields; a `choice` has at
least one option, no two the same regardless of case, and no other type has options. A value is checked against its
field: `text` is a string of at most 2000 characters, `number` a JSON number, `choice` one of the options (in any case; it is kept as the option is
written), `date` a date that exists as `YYYY-MM-DD`, and `person` the handle of a person (with or without the `@`; kept
as the person's handle), who is not disabled unless the card already had them. A card that moves to a project that did
not pick its kind (a project label on its issue) loses the kind and the values of its fields. Removing a repository also
removes its home project's picks of kinds.

### `GET /v1/kinds`

Every kind, in the order they were made: `[KindView]`.

### `POST /v1/kinds`

Makes a kind. Request: `{"name": text, "fields": [{"name": text, "type": text, "options": [text]}]}` (`fields` and
`options` may be left out). Sends `kind_changed`. Response: the `KindView`. Errors: `409` a kind with that name; `422` a
name, field or option that is refused as above.

### `PUT /v1/kinds/{id}`

Replaces the kind's name and fields with the request's, which is the body of `POST /v1/kinds`. A field is the same field
while its name is, regardless of case: it keeps the values cards hold, and its type cannot change (remove the field and
add it again for that, which drops its values). A field the request leaves out is removed with the values cards hold for
it, and a choice that lost an option loses the values that were that option. Sends `kind_changed`, and
`card_kind_changed` for each card of the kind. Response: the `KindView`. Errors: `404`; `409` the name is another
kind's; `422` as above, or a field whose type changed.

### `DELETE /v1/kinds/{id}`

Deletes a kind with its fields and the projects' picks of it, when no card has it. Sends `kind_removed` and
`project_kinds_changed` for each project that offered it. Response: `{}`. Errors: `404`; `409` a card has the kind.

### `PUT /v1/projects/{id}/kinds`

Replaces the kinds the project offers. Request: `{"kinds": [name]}`, in any case; repeats are dropped and `[]` offers
none. A project that has picked no kind offers none, and its cards have none. Sends `project_kinds_changed`. Response:
the `ProjectView`. Errors: `404` no such project; `409` the project drops a kind one of its cards has; `422` no kind has a
name.

### `PUT /v1/cards/{id}/kind`

Gives the card a kind, or none. Request: `{"kind": name}` in any case, or `{"kind": null}`. The kind has to be one the
card's project offers. A card that changes kind loses the values of its earlier kind's fields; giving it the kind it has
changes nothing. Sends `card_kind_changed`. Response: the `Card`. Errors: `404` no such card; `422` no kind has the name, or
the project does not offer it.

### `PUT /v1/cards/{id}/fields`

Sets values of fields of the card's kind. Request: `{"fields": {name: value}}`, each name a field of the kind (in any
case) and each value as its type says, or `null` to clear the field; fields not named stay as they are. Nothing is set
when any entry is refused. Sends `card_kind_changed`. Response: the `Card`. Errors: `404` no such card; `422` the card
has no kind, a field the kind does not have, or a value its field refuses.

### `GET /v1/issues`

The cards of every project, oldest first, as `Card`s, across the repositories and projects of the server. Every filter
that is given must match; they are query parameters: `project` (a project id), `kind` (a kind's name, in any case),
`state` (`open` or `closed`) and `field.<name>` (a field's name, in any case) with the value the card's field must
have, which may be repeated for several fields. A `number` field is compared as a number, any other as text regardless of case; a card
that has no value for the field does not match. `sort` names a field to order the cards by (numbers by value,
anything else by its text), with `order=desc` for the other way round: a card with no value for it comes after those that
have one, whichever the order. Errors: `404` no such project or kind; `422` a field no kind has, a number that is not
one, any other parameter, or a `state` or `order` that is neither of its two words.

### Assignees

A card and an item of the inbox can be given to people (ADR 0018). Moebius keeps assignees itself: nothing is written to
a forge and the assignees a forge has are not read. They are the handles of people of the organisation, which column
scripts see as `card.assignees`; what a card's assignees mean is up to the column's script, and the default `ready.luau`
starts no work on a card that has one (see [Column scripts](column-scripts.md)). A person who was disabled after being
assigned stays assigned until someone changes it. A member or an owner assigns, never an agent.

### `PUT /v1/cards/{id}/assignees`

Replaces the people the card is given to. Send it only when the whole set is meant: a client that builds the set from
a card it loaded earlier loses a change made meanwhile, so to add or take off one person use the routes below. Request: `{"assignees": [handle]}`, each with or without the `@` and in any
case; repeats are dropped, and `[]` gives the card to no one. The event `card_assignees_changed` is sent; setting what is
set changes nothing. Response: the `Card`. Errors: `404` no such card; `422` a handle that is nobody's, or a disabled
person who was not assigned already.

### `PUT /v1/inbox/{id}/assignees`

The same for an item of the inbox, resolved or not; the event is `inbox_assignees_changed`. Response: the `InboxItem`.
Errors: `404` no such item; `422` as above.

### Priority and due date

A card has a priority (`urgent`, `high`, `medium`, `low` or `none`) and an optional due date. Both are kept as labels of
the card, `priority:<name>` and `due:YYYY-MM-DD`, so the forge shows and sets them as it does any label: setting one in
Moebius writes the label to the forge (replacing the card's earlier `priority:` or `due:` label), and a label set on the
forge becomes the card's priority or due date after the next poll. A card with several `priority:` labels has the most
urgent, and one with several `due:` labels the earliest; a label that names no priority or no real date is just a label.
Column scripts see `card.priority` and `card.due` (see [Column scripts](column-scripts.md)); the board settles urgent
cards first, then those due first, then the oldest, so they take the free run slots first. A member or an owner sets
them, never an agent. The event is `card_labels_changed`, as for any change of labels.

### `PUT /v1/cards/{id}/priority`

Sets the priority. Request: `{"priority": "urgent"}`, one of `urgent`, `high`, `medium`, `low` or `none` in any case;
`none` takes the label away. Response: the `Card`. Errors: `404` no such card; `422` a name that is not a priority.

### `PUT /v1/cards/{id}/due`

Sets the due date. Request: `{"due": "2026-12-01"}`, a date that exists as `YYYY-MM-DD`, or `{"due": null}` for none.
Response: the `Card`. Errors: `404` no such card; `422` a text that is not a date.

### Estimates

A card has an estimate in points, a whole number from 1 to 1000, or none. It is kept as the card's `estimate:<points>`
label, so the forge shows and sets it as it does any label (the smallest `estimate:` label with a valid number is the
estimate, and one that holds anything else is just a label), and column scripts see it as `card.estimate`. A member or an
owner sets it, never an agent; the Moebius agent only proposes one. The event is `card_labels_changed`. After a Retro's
day, the report lists the cards whose estimate was furthest off (see [`POST /v1/retros`](#post-v1retros)).

### `PUT /v1/cards/{id}/estimate`

Sets the estimate. Request: `{"estimate": 5}`, a whole number from 1 to 1000, or `{"estimate": null}` for none.
Response: the `Card`. Errors: `404` no such card; `422` a number outside 1 to 1000, or a body that is not a whole number or null.

### `GET /v1/cards/{id}/estimate/proposal`

The estimate the Moebius agent proposes: the cards of the card's repository that have an estimate and look like it
(the words of titles and bodies, as for `GET /v1/cards/{id}/similar`) are compared, at most five, the most alike first,
and the proposal is the median of their estimates (the higher of a middle pair). Nothing is set. Response:
`{"estimate": number or null, "compared": [{"id", "number", "title", "estimate", "score"}]}`; `estimate` is null and
`compared` empty when no card with an estimate looks like this one. The agent names the `compared` cards when it
proposes. Errors: `404` no such card.

### `PUT /v1/cards/{id}/assignees/{handle}` and `DELETE /v1/cards/{id}/assignees/{handle}`

Give the card to one more person, or take it off one. `{handle}` is a handle with or without the `@`, in any case; there
is no body. The change is applied to the people the card has when the server handles it, so two made in quick succession
(from the palette and the sidebar, say) both stay. Adding someone already on the card, or taking off someone who is not,
changes nothing and sends no event; otherwise `card_assignees_changed` is sent with the whole new set. A disabled person
can be taken off. Response: the `Card`. Errors: `404` no such card; `422` a handle that is nobody's, or adding a disabled
person who was not assigned already.

### `PUT /v1/inbox/{id}/assignees/{handle}` and `DELETE /v1/inbox/{id}/assignees/{handle}`

The same for an item of the inbox; the event is `inbox_assignees_changed`. Response: the `InboxItem`. Errors: `404` no
such item; `422` as above.

### Mentions and following

What concerns a person is a **notice** of their own, shown in the Inbox under Mentions with a read mark that is theirs
alone (ADR 0018; there is no bell). Each happening tells exactly the people it concerns, one notice each, and never
the person who caused it where that is known:

- **assigned**: the people newly given a card or an inbox item (`PUT .../assignees`); those already given it are not
  told again. A person who gives something to themselves is not told of it.
- **mentioned**: the people a comment names with `@handle` (a handle after an `@` that starts a word, in any case;
  an address such as `me@example.com` names no one), written through the API or read from the forge (a comment more than a day old is history, and tells no one). A handle that is
  nobody's or is disabled tells no one. A person mentioned twice is told once. A mention in a page is told the same
  way, by the wiki that writes it.
- **changed**: the followers of a card when it moves to another column (except the person who moved it, when a person did; the board and
  scripts tell every follower. Closing a card moves it to `Done`, and reopening it moves it back) and when it is commented on. A follower who is mentioned in that comment gets the
  mention alone.

A person follows the cards they wrote and the cards they are given, until they stop, and any other card they choose to
follow. Disabled people are told nothing. Notices go with their card when it is deleted. An agent has no notices.

### `GET /v1/notices`

The notices of the person asking, newest first, at most 200. Response: an array of `Notice`: `id`, `kind` (`assigned`,
`mentioned` or `changed`), `by` (the handle of the person who caused it, or null), `card_id` and `inbox_item_id` (each
or null), `title` (of the card or item), `body` (the start of the comment, or what changed), `at` and `read`.

### `POST /v1/notices/{id}/read`

Marks one of the asking person's notices read; reading it again changes nothing. Sends `notice_read` to that person.
Response: the `Notice`. Errors: `404` no such notice, which is also the answer for another person's.

### `GET /v1/cards/{id}/followers`

The handles of the people who follow the card, in the order the people were added. Errors: `404` no such card.

### `PUT /v1/cards/{id}/follow`

The person asking follows the card or stops following it. Request: `{"following": true or false}`. Response: the
handles of the followers now. Errors: `404` no such card.

### `POST /v1/cards/{id}/blockers`

Makes an issue block this one, on the forge too. Request: `{"blocker": number}`, the issue number of the blocker, which
must have a card. The event `card_blocking_changed` is sent; adding what is there changes nothing. Response: every blocker
of the issue, as numbers, done or not. Where the forge keeps no blocking (GitLab without Premium links the issues as
`relates_to`, which blocks nothing), the line "Blocked by #n" is written to the description instead. Errors: `404` no
such card or blocker; `422` the issue as its own blocker, or a blocker that waits for this issue directly or through
others (a cycle: it is blocked by this issue, or it is an ancestor of it); `500` the forge refused.

### `DELETE /v1/cards/{id}/blockers/{number}`

Stops the issue `number` from blocking this one, on the forge too; a line of the description that says it is blocked by it is
taken out of the description. Response: the blockers that are left, as numbers. Errors: `404` no such card, or that issue
is not a blocker; `500` the forge refused.

### Duplicates and related issues

Besides a parent and blockers, an issue can be a duplicate of another, the original, or be related to others. Moebius
keeps these links itself (a forge has none that all of them share); closing the duplicate and the comment that points to
the original reach the forge as for any issue. The links name issue numbers of the same repository, as `parent` does, and
show in the `Issue` as `duplicate_of`, `duplicates` and `related`. Every change is announced as `card_links_changed` for
each issue whose links changed.

### `PUT /v1/cards/{id}/duplicate`

Marks the issue a duplicate of another: closes it (the decision is recorded as for `close`, with the reason `Duplicate of
#n: <title of the original>`), writes the comment `Duplicate of #n.` and keeps the link. Request: `{"original": number}`.
Marking it a duplicate of another issue moves the link; marking what is marked changes nothing. Nothing is reopened or
closed besides this issue. Response: the `Card`. Errors: `404` no such card or original; `409` a run of the card is going
(block the card to stop it first); `422` the issue as its own original, an original that is a duplicate itself (mark the
issue as a duplicate of that one), or an issue that has duplicates of its own (move them first); `500` the forge refused.

### `DELETE /v1/cards/{id}/duplicate`

Takes the duplicate link off the issue. The issue stays closed; reopening it is a step of its own (`reopen`). Response:
the `Card`. Errors: `404` no such card, or it is no duplicate.

### `POST /v1/cards/{id}/related`

Relates the issue to another, in both directions, so that each shows the other in `related`. Request: `{"related":
number}`. Relating what is related changes nothing. Response: the `links` of the issue, `{"duplicate_of", "duplicates",
"related"}`. Errors: `404` no such card or issue; `422` the issue as related to itself.

### `DELETE /v1/cards/{id}/related/{number}`

Stops the issue `number` from being related to this one, in both directions. Response: the links as above. Errors: `404`
no such card, or that issue is not related to it.

### `GET /v1/cards/{id}/similar`

The open issues of the card's repository that are likely the same as this issue, the most alike first, at most five, each
`{"id", "number", "title", "state", "score"}`. The issue is compared with each by the words of their titles and of their
titles and bodies together (lower case, without words of under three letters and common ones, a plural as its singular):
two issues are alike when at least half of the words of either are the words of both, and at least two words are shared
(one when both titles are that one word). `score` is that share, from 0.5 to 1. When an issue is made through
`POST /v1/repositories/{id}/issues`, the Moebius agent writes the result as a comment (author `agent:Moebius`) that lists
them, and says nothing when there are none. Nothing is ever closed for it: only a person, or the agent told to by one,
marks a duplicate. Errors: `404` no such card.

### `POST /v1/cards/{id}/claim`

Claims the issue, so that two sessions do not take the same one. Request: `{"by": text}`, optional: the name that claims,
`person:<handle>` or `agent:<name>` for the caller when left out; a claim is advisory (see above). Claiming again as the claimer changes
nothing. The event
`card_claimed` is sent. Response: the `Card`. Errors: `404` no such card; `409` somebody else claimed it, and the message
says who.

### `DELETE /v1/cards/{id}/claim`

Releases the claim. A person releases any claim; the Moebius agent only the one it holds, as `agent:Moebius` or under the name
it claimed with, which it gives as the optional request `{"by": text}`. The event `card_released` is sent. Response: the
`Card`. Errors: `404` no such card; `403` the claim is somebody else's and the caller is the agent; `409` it is not
claimed.

### `POST /v1/cards/{id}/close`

Closes the issue, on the forge too: its card moves to `Done` (the event `card_moved`) and the issues it blocks are not
blocked by it any more. Request: `{"reason": string}` (optional). The decision is recorded (kind `close_issue`, with the
title) and the event `card_closed` is sent. Closing a closed issue changes nothing. The card's outcome (see
[the training export](training-export.md)) is `closed`, not `merged`, unless a change of the card was merged. Response: the `Card`. Errors: `404` no
such card; `409` a run of the card is going (block the card to stop it first); `500` the forge refused.

### `POST /v1/cards/{id}/reopen`

Reopens a closed issue, on the forge too: its card moves from `Done` back to `Backlog`, where the Kanban takes it up as
it does a new issue. Request: `{"reason": string}` (optional). The decision is recorded (kind `reopen_issue`) and the event
`card_reopened` is sent. Response: the `Card`. Errors: `404` no such card; `409` the issue is open; `500` the forge refused.

### `GET /v1/cards/{id}/comments`

The comments of the issue, oldest first, as `Comment`s. Errors: `404` no such card.

### `POST /v1/cards/{id}/comments`

Writes a comment, on the forge too. Request: `{"body": text}`, which must not be blank; secrets are redacted from it. The
event `card_commented` is sent. Response: the `Comment` (the one the poll stored, when it read it from the forge first).
Errors: `404` no such card; `422` the comment is blank; `500` the forge refused (nothing is stored).

### `POST /v1/cards/{id}/block`

A person, or the Moebius agent, holds a card, so that work on it stops and stays stopped. The card's `held` is set to
`{"reason", "by", "at"}` (`by` is `person:<handle>` or `agent:Moebius`), the card's runs that have not ended are stopped with the
stop of `POST /v1/runs/{id}/stop` (each recorded as a `stop` decision), and the reason it waited for is cleared. A run
the board is still starting cannot be stopped until its harness has registered it, so the block keeps trying for up to
10 seconds; if a run still cannot be stopped by then the card is released again, nothing is recorded and the answer is
`409`. The card stays in its column. The board engine then calls no column script for it and starts no run for it, so a script can
neither move it nor start work on it, until `POST /v1/cards/{id}/unblock`. A held card is still moved by what a person
does to it (approving, merging, editing) and by its issue closing on the forge. `held` is in the `Card`, in the snapshot
and, as `card.held`, in [column scripts](column-scripts.md); it is kept when the server restarts.

The decision is recorded (kind `block_card`, choice `block_card`, decider `person` or `agent:Moebius`, the card's id; its
context has `title` and `reason`) and the event `card_updated` is sent.

Request: `{"reason": string}`; `reason` is optional, is trimmed, and is shown to whoever sees the card held. A body is
not required.

Response: the `Card`. Errors: `404` no such card; `409` the card is held already, or a run could not be stopped in time
(the card is not held then).

### `POST /v1/cards/{id}/unblock`

A person, or the Moebius agent, releases a held card: its `held` is null again and the next tick of the board treats it
like any other card. If the block stopped the card's latest run, the card starts afresh, as after a retry
(`POST /v1/cards/{id}/retry`): that run counts toward no limit and is no reason to call the card stuck, so the default
scripts carry on with it. A run a person stopped before the block still waits for a retry. The decision is recorded (kind
`unblock_card`, the card's id; its context has `title`, `reason`, `held_reason` and `held_by`) and the event
`card_updated` is sent.

Request: `{"reason": string}`; `reason` is optional and is kept with the decision. A body is not required.

Response: the `Card`. Errors: `404` no such card; `409` the card is not held.

### `POST /v1/cards/{id}/approve`

A person approves an issue that waits in the backlog: the decision is recorded (kind `approve_issue`, decider `person`),
the card's inbox items are resolved, and the card moves to `Not started`.

Request: `{"reason": "..."}`; `reason` is optional.

Response: the `Card`. Errors: `404` no such card; `422` the card is not in `Backlog`.

### `DELETE /v1/cards/{id}`

A person removes a card, as removing a repository removes each of its cards: its runs with their transcripts, usage and
replays, its changes, inbox items, dependencies and moves, its attempts, trials and judgements, and the uses of lessons
for it; and, in the repository's clone, its worktrees that are left and its `moebius/` branches (`moebius/run-<id>` of
each run, the branch of its change `moebius/card-<number>` and its trial branches). A run that is going is stopped first
and the card is deleted once it has ended; if it has not ended after 30 seconds nothing is removed and the card is
released again. A card whose
number other cards name as a blocker leaves them waiting, as for any issue without a card.

The issue is not touched. On a repository with a forge the card's number is remembered as deleted, so the poll never makes
a card for that issue again, open or closed; the issue stays open on GitHub, GitLab or Forgejo, and so does its pull or
merge request. A card made by Moebius itself, in a repository without a forge, is simply gone and its number may be given
again. Removing the repository forgets the numbers.

The decision is recorded (kind `delete_card`, decider `person`; it belongs to no card, since there is none any more, and
its context has `title`, `number` and `repository`) and the event `card_deleted` is sent.

Only a person may delete a card: the Moebius agent's run token gets `403`.

Request: `{"reason": string}`; `reason` is optional and is kept with the decision. A body is not required.

Response: the `Card`, as it last was. Errors: `403` the request is the Moebius agent's; `404` no such card; `409` a run of
the card has not ended after being stopped.

### `POST /v1/changes/{id}/merge`

A person merges an open change: the decision is recorded (kind `merge`, decider `person`), the forge merges it (a
no-fast-forward merge into the default branch of the clone for a repository without a forge; the GitHub API for a pull
request; the GitLab API for a merge request, with the merge commit's message; the Forgejo API for a pull request, with a
merge commit titled with the message), the change becomes `merged`, its inbox items are resolved and its card, if it has one, moves to `Done`. A
change from a chat has no card: its `ready_to_merge` item is resolved and the commit message names what it carries,
`Merge <branch>: <the branch's last commit's subject>`, or `Merge <branch>` when the branch's last commit has no subject. A
workflow run's change is merged the same way, and the change that belongs to the run becomes `merged`.

A change of the harness configuration repository (`harness_repository` true) is merged into the default branch of its
clone with a merge commit, which is pushed to `origin`; the agents are read again, so the next run uses what the change
brought. A push that `origin` refuses undoes the merge and is a `500`. The decision is recorded as for any change, with
the repository `harness`.

Request: `{"reason": "..."}`; `reason` is optional, but the body is not: send `{}`.

Response: `{"commit": "<sha of the merge commit>"}`. Errors: `404` no such change; `422` the change is not `open`; `409` the
branch does not merge cleanly; `500` the forge failed.

### `POST /v1/changes/{id}/reject`

A person rejects an open change whose card waits in `Ready to merge`: the decision is recorded (kind `reject`, decider
`person`), the card's `ready_to_merge` inbox item is resolved, and the card moves back to `In progress` with the reason as
its `note`, so that the next Coder round is told (without a reason the note says a person rejected the change). The change
stays open: the next round updates it, and on GitHub its pull request stays. A workflow run's change has no card: rejecting
it records the rejection and closes the change, so the run's change becomes `closed` and its `ready_to_merge` item is
resolved.

Request: `{"reason": "..."}`; `reason` is optional, but send `{}` without one.

Response: the `Card`, or `null` for a workflow run's change. Errors: `404` no such change; `422` the change is not `open`
or its card is not in `Ready to merge`.

### `GET /v1/changes/{id}/diff`

What a change changes: `git diff <default branch>...<branch>` in the repository's clone (the harness configuration
repository's for a change of that), as a unified diff.

Response: `{"diff": string, "truncated": boolean}`. `diff` has secrets removed and is empty when nothing changed or the
branch is gone; a diff over 2 000 000 bytes is cut and `truncated` is true. Errors: `404` no such change.

### `POST /v1/cards/{id}/retry`

A person retries a card that waits in the inbox as stuck or whose pull request was closed (it reached its round limit or its token budget, or a person
stopped its run): the decision is recorded (kind `retry`, decider `person`), the card's inbox items are resolved, its
waiting reason is cleared and its `retried_after` is set to its latest run, so that its `rounds`, `reviews` and `tokens`
(see [column scripts](column-scripts.md)) count from the runs after it. The board then starts work on the card again.

A card whose pull request was closed on the forge without a merge waits in `Ready to merge` with an open `change_closed`
inbox item. Retrying it does the same, and moves the card to `In progress` with the note `The pull request was closed
without a merge.`; the closed change stays closed and the next publish opens a new change and pull request.

Request: `{"reason": "..."}`; `reason` is optional, but send `{}` without one.

Response: the `Card`. Errors: `404` no such card; `422` the card has neither an open `stuck` nor an open `change_closed` inbox item.

### `GET /v1/runs/{id}`

One run, for the live view. Response: the `Run`. Errors: `403` the run is a chat of another person that is not shared;
`404` no such run.

### `GET /v1/runs/{id}/events`

The stored events of a run, in order. A client loads a long run in pieces and then follows `GET /v1/events` from the last
`seq` it has: every `run_event` on the stream carries the `seq` the event was stored under, so the client drops those
it already has and sees every event exactly once.

A page is read either forwards or backwards, never both at once. Forwards: `after` (default 0) returns only events with
a `seq` above it. Backwards, for a page that opens a long chat at its last message: `tail` (default false) returns the
newest events, and `before` returns those with a `seq` below it; each is answered oldest first, so the page that follows
it asks for the `seq` of its first event. `limit` (default 500, at most 5000, at least 1) is the most either returns.

Response: an array of `{"seq": number, "kind": string, "data": object, "at": number}`, oldest first, where `kind` and
`data` are those of the `run_event` (below). Errors: `404` no such run; `422` more than one of `after` (above 0),
`before` and `tail` is given.

### `GET /v1/cards/{id}/runs`

The card's runs, oldest first. Response: an array of `Run`. Errors: `404` no such card.

### `GET /v1/cards/{id}/history`

The card's timeline, oldest first (entries of the same second are in the order of the kinds below). Response: an array
of `{"at": number, "kind": string, "data": object}` with these kinds:

| Kind | When `at` is | `data` |
|---|---|---|
| `moved` | The card moved to another column. Moves from before moves were recorded are not in the history. | `{"from": column, "to": column, "note": string or null}`, the card's `note` when it moved |
| `run_started` | The run was stored. | `{"run": id, "agent": string, "purpose": string}` |
| `run_ended` | The run ended. | `{"run": id, "status": "succeeded" \| "failed" \| "stopped"}` |
| `decided` | A decision was recorded for the card. | `{"judgement": id, "kind": string, "choice": string, "decider": string, "reason": string, "outcome": string or null}` |
| `attempt_judged` | An attempt of the card was judged by its tests and checks. | `{"run": id, "model": string or null, "result": "passed" \| "failed", "output": string}` |
| `attempts_settled` | The card's attempts were settled: one picked, or none passed. | `{"runs": [id], "picked": id or null, "reason": string or null}`, the reason the picked attempt was kept |
| `inbox_added` | An inbox item about the card was added. | `{"item": id, "kind": string, "title": string}` |
| `inbox_resolved` | That item was resolved. | `{"item": id, "kind": string, "title": string}` |

Errors: `404` no such card.

### `GET /v1/judgements`

The recorded decisions (what a person or a decider chose at a decision point), newest first. Query: `card` returns only
those about that card, `run` only those about that run (a chat has no card, so this is how its tool calls' deciders are
found), `kind` only those of that kind (such as `retry`, `merge` or `tool_call`); `limit` (default 100, at most 1000, at
least 1) is the most it returns. A card or run that does not exist simply has none.

Response: an array of `{"id": number, "kind": string, "choice": string, "reason": string, "decider": string,
"outcome": string or null, "card": number or null, "run": number or null, "at": number}`. The blobs of the question's
context are not included. For a refused tool call that was replayed, `outcome` is the replay's sentence, `replayed: <how
it ended>; <what happened to the files>` (see [the replays](harness-configuration.md#the-replays)).

### `POST /v1/judgements/{id}/replay`

Runs the call of a refused `tool_call` judgement again in a throwaway copy of its worktree, now, at any hour, as the
schedule does inside `[trial_window]`. The call runs once; the copy is removed; the judgement's `outcome` becomes the
replay's sentence; the event `replayed` is sent. Request: no body. Response: the `Replay`:

```json
{"id": 1, "judgement": 12, "run": 9, "exit": 0, "timed_out": false, "output": "", "created": [],
 "changed": ["src/lib.rs"], "deleted": ["src/a.rs", "src/b.rs"], "at": 1790000000}
```

`exit` is the exit code, null when the call was killed (`timed_out` says whether for the time) or did not run (then
`output` says why). `output` is what the call printed, secrets removed, the last 4 KB. `created`, `changed` and
`deleted` are paths relative to the worktree. Errors: `404` no such judgement; `409` it was replayed already, or has an
outcome already; `422` it is not a refused `tool_call`, or its call is not reversible, or its run or repository is gone,
with the reason.

### `GET /v1/replays`

The stored replays, newest first, at most 1000. Query: `judgement` returns the replay of that judgement, `run` those of
that run's calls. Response: an array of `Replay`.

### `DELETE /v1/replays/{id}`

A person deletes a replay: its record, and whatever of its copy is left under `<data_dir>/replays/`. The judgement it
replayed keeps its outcome (`replayed: ...`), and the schedule does not replay that judgement again; `POST
/v1/judgements/{id}/replay` still can. The decision is recorded (kind `delete_replay`, decider `person`, context: `replay`,
`judgement` and `run`) and announced as `replay_deleted`. Request: `{"reason": string}` (optional). Response: the
deleted `Replay`. Errors: `403` for an agent's run token (only a person deletes); `404` no such replay.

### `GET /v1/judgements/kinds`

What the store holds now, per kind of judgement, kinds in alphabetical order. Response: an array of `KindCounts`:
`{"kind": string, "count": number, "with_outcome": number, "by_person": number, "held_out": number}`. `count` is the
judgements of the kind, `with_outcome` those with an outcome, `by_person` those whose decider is a person (`person` or `person:<handle>`), `held_out`
those in the held-out tenth (see [the training export](training-export.md#the-split)).

Query: `repository` (a repository id) and `agent` narrow the counts to the judgements made for a card of that repository
or in a run of it (`repository`), and to those made in a run of that agent (`agent`); both together keep the ones that are
both. Without them every judgement counts. A judgement made with no card and no run is left out once either is set.

### `POST /v1/exports`

Writes the training-set export now (see [the training export](training-export.md)) to `<data_dir>/exports/<id>/` and
records the person's decision to do so as a judgement of kind `export` (decider `person`, context: `export` the id,
`kinds` the kinds written, joined by commas, and `counts` the `KindCounts` as JSON text). Request: `{"kinds": [string],
"include_private": bool}`, the kinds to export, without `kinds`, or with `null`, every kind; and whether the judgements
of the private projects the person is a member of are written too (`false` when left out: no private project's
judgement is). The body is required, `{}` is the smallest. Response: the `Export`:

```json
{"id": 1, "at": 1790000000, "kinds": [{"kind": "merge", "count": 12, "with_outcome": 12, "by_person": 12, "held_out": 1}], "private": []}
```

`private` holds the ids of the private projects whose judgements the export holds; only their members see the export
(in the list, and its files). An export made before projects could be private has `private` null and is seen only by
someone who sees every project. Making a project private later does not take back an export already made.

`id` counts up from 1. The export holds the judgements there were when it read them, not the judgement of its own
decision. Errors: `422` a kind has no judgements (no file is written for any kind).

### `GET /v1/exports`

The exports made through the API, newest first. Response: an array of `Export`.

### `DELETE /v1/exports/{id}`

A person deletes an export: its directory `<data_dir>/exports/<id>/` with all its files. The decision is recorded (kind
`delete_export`, decider `person`, context: `export` the id and `kinds` joined by commas) and announced as
`export_deleted`. Request: `{"reason": string}` (optional). Response: the deleted `Export`. Errors: `403` for an agent's
run token; `404` no such export.

### `GET /v1/exports/{id}/{file}`

Downloads a file of export `id`, `{file}` being `<kind>.jsonl` (such as `tool_call.jsonl`), streamed, as `application/x-ndjson`. The body is the file as the
export wrote it. Errors: `404` no such export or no such file in it.

### `GET /v1/usage`

What the model calls used: tokens, calls, how long they took, and where they went. Query: `days` (a whole number from 1
to 365, default 1) is the window, counted in local days of the server, today included: `1` is since local midnight (the
same "today" the day budget counts), `7` the last seven days. Anything else is `422`. Tokens are input plus output tokens.

Two more parameters narrow every figure of the answer (the totals, `days`, `models`, `agents`, `activities`,
`tool_waits` and `classes`) to the usage of some runs: `repository` (a repository id) keeps the runs of that repository,
a card's or a chat's, and `agent` keeps the runs of that agent; both together keep the runs that are both. Usage that
belongs to no run is left out once `repository` is set, and `agent=No run` keeps exactly that usage. The budget of the
day in `totals.day_budget` is not narrowed. Without them every run counts.

Response:

```json
{
  "today": "2026-10-28",
  "days_in_window": 7,
  "totals": {"tokens": 1840000, "calls": 1284, "cached_share": 0.61, "model_ms": 11520000, "day_budget": 8000000},
  "days": [{"date": "2026-10-27", "tokens": 2520000, "model": "claude-sonnet-5-5"}],
  "models": [{"model": "claude-sonnet-5-5", "tokens": 960000, "calls": 690, "input_tokens": 700000, "cached_tokens": 520000}],
  "agents": [{"agent": "Coder", "calls": 702, "tokens": 1120000, "input_tokens": 1000000, "cached_tokens": 610000, "model_ms": 7080000}],
  "activities": [{"activity": "bash", "model_ms": 2140000, "calls": 214, "output_tokens": 61000}],
  "tool_waits": [{"tool": "bash", "calls": 31, "seconds": 2460}],
  "classes": [{"class": "strong", "model": "claude-sonnet-5-5", "tokens": 420000, "calls": 300}]
}
```

- `today`: the server's date, `YYYY-MM-DD`; the last day of the heatmap.
- `totals` covers the window. `cached_share` is the cached input tokens divided by the input tokens (0 to 1, and 0
  when there was no input). `model_ms` is the time the model calls took, in milliseconds, each measured from the request
  to the end of the response or stream. `day_budget` is `day_tokens` of `moebius.toml`, or `null` without one.
- `days` does not depend on `days_in_window`: it holds each of the last 365 local days that had usage, oldest first, with
  the tokens of the day and its leading model (the one with the most tokens that day; the first by name on a tie).
  Days without usage are not listed.
- `models` holds each model used in the window, the one with the most tokens first: tokens, calls (one per model call),
  input tokens (part of the tokens) and cached tokens (part of the input tokens).
- `agents` holds each agent whose runs used tokens in the window, the one with the most tokens first: calls, tokens,
  input tokens, cached tokens and model time. Usage that belongs to no run is counted under the agent `No run`. The usage of a deleted chat, research run or other deleted run belongs to no run from then on, so the per-agent view moves it to `No run`; the totals do not change.
- `activities` holds what the model did with its time in the window, the activity with the most model time first: the
  `activity` is a tool's name, `answering` or `unknown`. A model call that asked for `n` tools gives each of them `1/n`
  of its duration and of its output tokens (the remainder of a division, whole milliseconds or tokens, goes to the first
  tools; a tool named twice by one call gets both shares). A call that asked for no tool is `answering`. `model_ms` and
  `output_tokens` are the sums of those shares; `calls` counts a call once for each distinct tool it named, so it can
  add up to more than the calls of the window. A call recorded before the router read tools is `unknown`; `unknown` is
  listed only when the window holds no other call. What the router reads from answers is in `router.md`.
- `tool_waits` holds the tools whose calls waited longest in the window, at most 10: for each tool name, the number of
  calls and the seconds between their `tool_execution_start` and `tool_execution_end` run events, summed. Events
  are stamped in whole seconds. A call that has not ended is not counted.
- `classes` holds the calls of the window whose request named a model class (see [the router](router.md#model-classes)):
  for each class and the model of it that answered, the tokens and the calls, by class name and then the model with
  the most tokens first. The same calls are counted under their model in `models` too.

Errors: `422` `days` is not from 1 to 365.

### `GET /v1/runs/{id}/diff`

What a run changed, as a unified diff: while the run has a worktree, everything in it since the run started, uncommitted
work included; afterwards the run's branch (`moebius/run-<id>`, `moebius/chat-<id>` for a chat, or
`moebius/workflow-run-<workflow run id>` for an agent of a workflow) against the commit the
run started from. Response: `{"diff": string, "truncated": boolean}`, as for [a change](#get-v1changesiddiff); the diff
is empty for a chat that has no repository. Errors: `404` no such run.

### `POST /v1/runs/{id}/steer`

A person sends a message to a running run: the harness passes it to the agent (Pi-Bolt's `steer` command; Claude Code
is given it as its next user message, which it answers before the run ends), the run
stores a `steered` event with `{"message": ...}` (secrets removed), and the decision is recorded (kind `steer`, decider
`person`, the message in its context).

Request: `{"message": "...", "context": "..."}`; `message` is required and must not be blank. `context` is optional and
works as in `POST /v1/chats`: the agent is told it before the message (secrets removed, at most 4 000 characters), and
the `said` event holds only the message.

Response: the `Run`. Errors: `404` no such run; `422` the message is blank or the run is not running.

### `POST /v1/runs/{id}/stop`

A person stops a running run: the decision is recorded (kind `stop`, decider `person`) and the harness asks the agent to
abort (Pi-Bolt's `abort` command; a Pi-Bolt that has not exited after 10 seconds is killed; Claude Code is killed at
once; the agent's own process group goes with it, so the dev servers and watchers it started end too). The run ends as
`stopped`, also when Pi-Bolt had to be killed,
which publishes `run_ended`. The card is not retried by itself: its script puts it in the inbox as stuck ("stopped by a
person"), and a person decides with retry.

Request: `{"reason": "..."}`; `reason` is optional, but send `{}` without one.

Response: the `Run`, as it was when the request was made (still `running`). Errors: `404` no such run; `422` the run is not
running; `409` the run is a chat whose agent is starting after a message woke it (try again in a moment).

A chat that is stopped, also while it waits, ends as `stopped`; what it left in its repository's worktree is committed
on its branch first. A message written to it afterwards starts its agent again (see `POST /v1/chats/{id}/messages`).

### `GET /v1/runs/{id}/terminal`

A shell in the run's worktree, for the person to look around and run commands. It upgrades to a WebSocket (with the
token, like every route) and starts the person's shell (`$SHELL`, else `/bin/sh`) on a pseudo-terminal in the worktree
of the run or chat, with `TERM=xterm-256color`.

- Binary messages carry bytes both ways: from the client to the shell's input, from the shell's output to the client.
- A text message `{"type": "resize", "cols": N, "rows": M}` sets the size of the terminal in cells (the size is 80 by
  24 until then).
- When the shell exits the server closes the socket; when the socket closes the server kills the shell and every
  process of its session, background jobs included (on Windows, its process tree).
- A run has at most 4 terminals open at a time.
- It is a person's shell, so the guard (which checks the tool calls of agents) does not look at it, and nothing the
  person types or sees is stored. Opening one stores a `shell_opened` run event, `{"who": "person"}`.

The worktree exists while the run or chat is `starting`, `running` or `waiting`. Errors, answered before the upgrade:
`404` no such run; `422` the run has no worktree (it is over) or already has 4 terminals open.

### `GET /v1/chats/{id}/tui`

Pi-Bolt's own terminal interface for a chat, for the person who prefers it to the conversation view. It upgrades to a
WebSocket and works like [the shell of a run](#get-v1runsidterminal), with the same binary and resize messages, but the
program on the pseudo-terminal is `pi-bolt` in its interactive mode, in the chat's worktree, started as the chat's own
process is: the same agent, model, provider (the router, with the chat's run token), instructions, skills, MCP servers,
extensions and the Moebius bridge, so every tool call is still checked and recorded as a decision of the chat's run.
Only `--mode rpc` is left out.

The chat and its terminal are one conversation:

- Opening it ends the chat's own Pi (idle, as the chat waits; the chat stays `waiting`), together with every process its
  agent started (a dev server, a watcher), and starts the interactive one
  with `--continue` on the chat's own agent directory and Pi sessions (`runs/<id>/agent`, `runs/<id>/sessions`), so it
  continues the chat's session with everything said so far.
- While it is open, Moebius reads what Pi adds to the session file (the newest one in the chat's sessions, every half
  second) and records it as the chat's run events, in the shapes Pi's RPC records have, so the chat view, the Fleet and
  the event stream show it as it happens: the person's message as a `said` event followed by `agent_start`; each
  thinking and text part of an answer as a `message_update` with a `thinking_delta` or `text_delta`
  (`{"assistantMessageEvent": {"type", "contentIndex", "delta"}}`); each tool call as a `tool_execution_start`
  (`{"toolCallId", "toolName", "args"}`) and its result as a `tool_execution_end`
  (`{"toolCallId", "toolName", "result": {"content"}, "isError", "durationMs"}`); an answer that does not end in a tool
  call as `agent_end` (`{"messages": [the answer]}`, which also makes it the run's summary) and `agent_settled`
  (`{"aborted": false}`). So the chat's status follows the agent as it does in the chat: `running` while it answers,
  `waiting` while the terminal waits for the person. What the session held before the terminal opened is not recorded
  again, and secrets are removed as from every event. As the file is read every half second, the `tool_check` event of
  a call made in the terminal (recorded when the bridge asks) can come before the events of the message that made it.
- Closing it (the socket closes, or the program exits) ends Pi-Bolt and its whole session, background jobs included, and starts the chat's own Pi
  again with `--continue` on the same session, without a message; the chat is `waiting` and its next message continues
  the whole conversation, the terminal's part included. The terminal's place is given back only then, so a message sent
  as it closes is answered `409`, never lost.
- Ending or stopping the chat while the terminal is open closes the terminal, and the chat ends (committed and offered
  as when it ends otherwise) without its own Pi starting again.

A chat has one terminal open at a time. While it is open the chat takes no message: `POST /v1/chats/{id}/messages`
answers `409`, as what is said goes through the terminal. It opens only while the chat is `waiting`, that is, between
turns; while the agent works it is `409`. The opening is recorded as a `tui_opened` run event `{"who": "person"}`.

Errors, answered before the upgrade: `401`; `403` for the run token of an agent (the terminal stays with a person);
`404` no such run; `409` the chat is not waiting, or has its terminal open already; `422` the run is not a chat or it
has ended.

### `GET /v1/terminal`

A shell on the server itself, for the person to look at `moebius.toml` and the machine Moebius runs on. It works like
[the shell of a run](#get-v1runsidterminal), with the same messages, but starts in the directory of the server's
configuration file (`moebius.toml`), not in a worktree.

- At most 4 terminals are open on the server at a time.
- A run has no part in it, so opening one is stored as a judgement of kind `open_terminal` (decider `person`, context
  `directory`: the directory the shell started in), never what is typed or seen.

Errors, answered before the upgrade: `403` for the run token of an agent (the shell stays with a person); `422` when 4
terminals are open already or the directory is gone.

### `GET /v1/repositories/{id}/terminal`

A shell in a repository's clone on the server, for the person to run commands in the project (the project page's
Terminal tabs). It works like [the shell of a run](#get-v1runsidterminal), with the same messages, but starts in the
clone of repository `id`, not in a worktree.

- At most 4 terminals are open on one repository at a time.
- A run has no part in it, so opening one is stored as a judgement of kind `open_terminal` (decider `person`, context
  `directory`: the clone, and `repository`: its name), never what is typed or seen.
- The ports its programs listen on are listed by `GET /v1/ports` as a terminal's, with no run.

Errors, answered before the upgrade: `401`; `403` for the run token of an agent (the shell stays with a person); `404`
no such repository; `422` when 4 terminals are open on it already or its clone is gone.

### `GET /v1/ports`

The TCP ports that processes of runs, chats and terminals listen on, as an array of `{"port": integer, "pid": integer,
"process": string, "kind": "run" \| "chat" \| "terminal", "run": id or null}`, by port. A port is listed when a socket in
the listening state on `127.0.0.1`, `::1` or every address has a process that belongs to a run, a chat or a terminal: a
terminal's when the process, or one that started it, is a shell a person opened (`run` is the run or chat it was opened
on, null for `GET /v1/terminal`), else the run's or chat's when the process, or one that started it, works in its
worktree. `process` is the program's name. It is read from `/proc` (Linux); on any other system the list is empty.

### `GET /v1/ports/{port}/tunnel`

Carries one TCP connection to `127.0.0.1:<port>` on the server (`[::1]` when only that answers). It upgrades to a
WebSocket: every binary message the client sends is written to the connection and every byte the connection sends comes
back as a binary message; either side closing ends both. The server listens on no new interface: a client that forwards the port listens
on its own machine and opens one tunnel for each connection made to it, with the same address and token as for every
other route, so it works over an SSH tunnel too. A person may open a tunnel to any port of the server's loopback address,
detected or asked for by hand.

Errors, answered before the upgrade: `401`; `403` for the run token of an agent; `422` for port 0; `502` when nothing
answers on the port within 5 seconds.

### `GET /v1/runners`

The machines that run harnesses over SSH, the `[[runners]]` of `moebius.toml` (see [configuration](configuration.md#runners)),
by name. Response: an array of `{"name": string, "host": string, "user": string or null, "max_runs": integer, "busy":
integer}`; `busy` is how many runs go on the machine now. Nothing secret (the key file) is included.

### `GET /v1/agents`

The agents the server can run, for choosing one to chat with: the Moebius agent first and the Grilling partner second,
then the others by name, the Coder and the Reviewer among them whether or not they have files. A built-in agent takes its
model from its own `[agents.<name>]` or from `[agents.Coder]`; with neither its `model` is empty (always so for a Coder
without a model: it needs one, and a card that starts it fails with a clear error), and a chat with it runs on the `model`
it is started with. Response: an array of `{"name": string, "model":
string, "protocol": "openai" or "anthropic", "thinking": string, "token_budget": integer, "harness": "pi-bolt" or
"claude-code", "built_in": boolean, "error": string}`; `harness` is the harness the agent runs on;
`thinking` and `token_budget` are the agent's, left out when its configuration does not set them; `built_in` is true for the Moebius
agent, the `Grilling-partner`, the `Coder` and the `Reviewer`, which Moebius brings itself. `error` is left out unless the agent's files in the [harness
configuration](harness-configuration.md) are broken: such an agent has an empty `model`, cannot be run, and `error` says
why (the built-in agents still run, on the Coder's model). Instructions, skills, extensions and anything secret are not
included.

### `GET /v1/agents/{name}`

An agent of the harness configuration repository, as its files say: `{"name": string, "settings": {"model", "protocol",
"thinking", "token_budget", "harness", "runner", "skills", "extensions", "mcp", "engineering_skills"}, "instructions": string}` (`thinking`,
`token_budget`, `harness` and `runner` are left out when not set; no `harness` is `pi-bolt`, no `runner` is the server itself; `engineering_skills` is left out when it is `true` (the engineering skills of the agent's name, if it has any), is `false` for none, and is a list of names (`tdd`, `diagnosing-bugs`, `code-review`, `grilling`, `to-tickets`, `wayfinder`, `domain-modeling`, `research`, `prototype`, `moebius-autonomous`) for exactly those: see [engineering skills](harness-configuration.md#engineering-skills). A `PUT` with any other name is refused with `422`, and so is a file with one). The files hold no secrets and are returned in full. Errors: `404` no such agent; `422` its files
are broken (the message says how), or the server has no `[harness]` and its agents are in `moebius.toml`.

A built-in agent (`Moebius`, `Grilling-partner`, `Coder`, `Reviewer`, `Retro-analyst`, `Autoresearcher`) also has `"built_in":
{"instructions": string, "skills": [{"name": string, "description": string}], "engineering_skills": [...]}`: the instructions it is always given
before its own, `engineering_skills` the skills of the same shape it is given besides by default, that is while its `engineering_skills` is not `false` or a list of its own (the Grilling partner's `grilling`, `to-tickets`, `wayfinder`, `domain-modeling`, `research`, `prototype` and `moebius-autonomous`, the Coder's `tdd`, `diagnosing-bugs` and `moebius-autonomous`, the Reviewer's `code-review` and `moebius-autonomous`; empty for the others), and the built-in skills it always has before its own, which nobody can change. The Moebius agent's are
the `moebius` skill and the task skills `moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions`, `moebius-plugins` and `moebius-report`; the Coder and the Reviewer have none; the others have the `moebius` skill. A built-in agent is answered
without files (then `settings` and `instructions` are left out) and without `[harness]`. `PUT` saves only the agent's
files and answers without `built_in`. A new agent that starts from an existing one (the Agents page, `moebius agent save
--template`) is read with this route and saved with `PUT`: for the Coder or the Reviewer the copy's instructions are
the built-in ones followed by the agent's own, since the copy is not built in.

### `PUT /v1/agents/{name}`

A person adds or changes an agent of the harness configuration repository. Request: `{"settings": {...}, "instructions":
string}`, the shape `GET` answers without `name`; `settings.model` is required and unknown keys are refused;
`instructions` may be left out. The edit is validated as a file read is and, when valid, written, committed on the
default branch and pushed to `origin`; the decision is recorded (kind `edit_agent`, decider `person`), and the agent is
used from then on. Response: the agent as `GET` answers it, with `commit`, the short id of the commit the edit made (left
out when the edit changed nothing and so made no commit). Errors: `422` an invalid name or settings (the message says
what), or no `[harness]`; `409` a push that `origin` refused, and then nothing changed and the message is git's, with keys
redacted (see [harness configuration](harness-configuration.md)); `500` any other failure of git.

### `DELETE /v1/agents/{name}`

A person deletes an agent of the harness configuration repository, committed and pushed the same way (kind `edit_agent`).
Response: `null`. Errors: `404` no such agent; `422` no `[harness]`; `409` and `500` as for `PUT`.

### `GET /v1/rules`

The rules page: what the guard refuses and why. Response:

```json
{
  "patterns": [{"name": "no-force-push", "description": "git push --force rewrites history on the remote"}],
  "prose": {
    "source": "harness",
    "editable": true,
    "text": "## no-vendoring\nDo not vendor.\n",
    "rules": [{"name": "no-vendoring", "text": "Do not vendor."}],
    "error": null
  },
  "repositories": [{"repository": "demo", "repository_id": 1, "text": "...", "rules": [{"name": "...", "text": "..."}], "error": null}],
  "refusals": {"no-force-push": 2}
}
```

- `patterns`: the built-in rules in the order they are tested, each with its name and what it refuses (the reason a
  refusal gives).
- `prose`: the server's rules file as it is now. `source` is `harness` for the `rules.md` of the harness configuration
  repository, `file` for the `rules_file` of the configuration, and `null` when the server has neither (then `text` is
  empty and there are no rules). `editable` is true exactly when `source` is `harness`. `text` is the file's text (empty
  when it does not exist), `rules` the rules as the parser reads them. `error` is `null` unless the file cannot be used:
  it cannot be read, a `## ` line has no name, or a name is repeated (the repeat is dropped by the parser).
- `repositories`: every repository, in the order of their ids, with the `rules.md` of its project (see [harness
  configuration](harness-configuration.md#repository-settings)), each with `repository` (its name), `repository_id` (to save
  the file with `PUT /v1/repositories/{id}/settings/rules.md`), its text (without the final new line), its rules and
  `error` as for `prose`. A repository without the file, or a server without `[harness]`, has an empty `text` and no
  rules, so that a client can add its first rule.
- `refusals`: for each rule name, the number of `tool_call` judgements in the last 7 days that refused a call because of
  it, from the reason `<rule>: <why>` of a built-in pattern or of a rule in prose. A refusal that names no rule (the
  teacher failing, no teacher, a rules file that cannot be read) is not counted. A rule with no refusals is left out.

### `PUT /v1/rules`

A person replaces the server's rules in prose: the `rules.md` of the harness configuration repository, or without
`[harness]` the server's `rules_file`. Request: `{"text": string}`, the whole `rules.md`. The text must be fit to be saved: a `## ` line must have a name and a name must not be given twice (text
before the first `## ` line is free). It is written to the file (a final new line is added if missing); with `[harness]`
it is committed on the default branch with the message `Update the rules` and pushed to `origin`. Then the decision is
recorded (kind `edit_rules`, decider `person`, the text in its context, and `file` for a `rules_file`). Every tool call reads the file afresh, so the next one is
checked against the new rules. Response: the same as `GET /v1/rules` after the change, with `commit`, the short id of the
commit (left out when the text changed nothing, and always for a `rules_file`). Errors: `422` the text is not fit to be saved (the message says which line);
`409` a push that `origin` refused, and then nothing changed and no decision was recorded; `500` any other failure of git (see
[harness configuration](harness-configuration.md)).

### `GET /v1/workflows`

The workflows of the harness configuration repository, in name order. A workflow is a file `workflows/<name>.toml` in
that repository (see [workflows](workflows.md)). Response: an array of
`{"name": string, "description": string, "trigger_kind": string, "node_count": number}`; `trigger_kind` is the `kind` of
the first trigger, empty for a workflow with none. Errors: `409` the server has no `[harness]`; `422` a workflow file
that cannot be used (the message names its file).

### `GET /v1/workflows/{name}`

One workflow. Response: `{"name": string, "file": string, "definition": object}`, the text of the file exactly as it is
saved and the definition parsed from it. Errors: `422` the name or the file cannot be used (there is no such workflow, or
it is not valid); `409` the server has no `[harness]`.

### `PUT /v1/workflows/{name}`

A person adds or changes a workflow. Request: exactly one of `{"file": string}`, the whole TOML file, or
`{"definition": object}`, the parsed form `GET /v1/workflows/{name}` answers; unknown keys are refused. The definition is
parsed and validated before anything is written, so every node keeps its `position` and a canvas round-trips; an invalid
definition is refused with the validation message and the file is untouched. A valid one is written as TOML to
`workflows/<name>.toml`, committed on the default branch with the message `Add the workflow <name>` or
`Update the workflow <name>` and pushed to `origin`; the decision is recorded (kind `edit_workflow`, decider `person`).
Response: the workflow as `GET /v1/workflows/{name}` answers it, with `commit`, the short id of the commit (left out when
the definition changed nothing). Errors: `422` an invalid name or definition (the message says why); `409` the server has
no `[harness]`; `409` and `500` as for `PUT /v1/agents/{name}`.

### `DELETE /v1/workflows/{name}`

A person deletes a workflow: the file is removed, committed on the default branch with the message
`Delete the workflow <name>` and pushed to `origin`, and the decision is recorded (kind `edit_workflow`, decider
`person`). Response: `{"name": string, "commit": string}` (`commit` is left out when there was nothing to commit). Errors:
`404` no such workflow; `409` a run of the workflow is queued, running or waiting, or the server has no `[harness]`; `422` an
invalid name; `409` and `500` as for `PUT /v1/agents/{name}`.

### `POST /v1/workflows/runs/{id}/cancel`

Cancels a run of a workflow and ends it `cancelled`: a queued run never starts, a run going stops at its current node,
and a run waiting on a person leaves its inbox item withdrawn. The run's worktree and branch are left as they are (see
[cancelling a run](workflows/cancel.md)). Request: no body. Response: the run as the server has it now,
`{"id": number, "workflow": string, "repository_id": number, "trigger": string, "status": string, "value": any,
"created_at": number, "started_at": number or null, "finished_at": number or null, "error": string or null,
"worktree": string or null}`, its
`status` `cancelled` and its `error` `cancelled by a person`. Errors: `404` no such run; `409` the run has ended
already, and the message names the status it ended as.

### `POST /api/hooks/{path}`

Starts a run of the workflow whose `webhook` trigger names this path, in the repository that trigger names. No token:
what opens the door is the secret the trigger names, when it names one. The body is one JSON value and is the value the
run starts with; a body that is not JSON is refused `400` saying so. The answer is `202` with
`{"run": number, "workflow": string}` as soon as the run is stored, its nodes running on a thread of their own. A path
with no hook on it, a credential that is missing or wrong, and a secret the trigger names that the server does not
have are answered alike, with `404` and the plain text `no hook on this path`, so nothing here says which paths have
hooks. At most eight runs of one workflow may be queued or running at once through its hook; past that the answer is
`429` saying to try again later, never a dropped delivery. Errors: `400` a body that is not JSON; `404` no hook this
delivery opens, including a path claimed by two workflows that the delivery cannot open; `422` a trigger that names no
repository, or two workflows claiming one path that the delivery opens; `429` past the ceiling. See the
[`webhook` trigger](workflows/webhook-trigger.md).

### `POST /api/hooks/{repository}/{path}`

The same door for a delivery that names which repository the run belongs to. The repository in the address wins over
the one the trigger names; a repository the server does not have is refused `422`.

### `GET /v1/workflows/{name}/run-inputs`

The inputs the `manual` trigger of a workflow names, in the order its file names them, so a form builds itself from
them; see the [manual trigger](workflows/manual-trigger.md).
Response: `{"trigger_kind": "manual", "inputs": [{"name": string, "type": string, "required": boolean,
"default": value, "choices": [string]}]}`, where `type` is `text`, `number`, `boolean`, `choice` or `repository`,
`default` is left out when the input has none and `choices` when it is not a `choice`. Errors: `422` the workflow file
cannot be used, as for `GET /v1/workflows/{name}`; `409` the workflow's trigger is not `manual`, or the server has no
`[harness]`.

### `POST /v1/workflows/{name}/runs`

A person starts a run of the workflow by hand. Request: `{"repository": string, "inputs": object}`, both optional and
any other key refused. `inputs` gives the trigger's inputs by the names it names them; a value is checked against the
type its input names, a default stands in for an input that was not given, and an input that was given nothing and has
no default is left out of the value the run starts with. The run belongs to the repository `repository` names, or, when
the trigger names an input of type `repository`, to the repository that input names. A run started here is recorded with
`manual` as the kind of the trigger that started it, and goes on off the request path as any other run does.
Response: `{"id": number, "workflow": string, "repository": string, "trigger": "manual", "status": string,
"value": object}`, the run as stored and the value its nodes start from. Errors: `422` an input the trigger does not
name, a value that is not of the type its input names, a required input with neither a value nor a default, a
repository that is not known, or a request giving `repository` when the trigger names an input of type `repository` —
every message names the input or the repository it refused; `409` the workflow's trigger is not `manual`, or the server
has no `[harness]`; `422` a workflow file that cannot be used, as for `GET /v1/workflows/{name}`.
### `POST /v1/workflows/runs/{run_id}/resume`

A person decides on a workflow run that waits for one, which is what the `approval` node leaves (see
[the approval node](workflows/approval.md)). Request: `{"decision": "approve" | "edit" | "reject", "value": any, "reason":
string}`, where `value` is what an `edit` replaces the run's value with and `reason` why a `reject` refuses it (`rejected`
when left out); unknown keys are refused. The decision is recorded in the waiting node's row and the run's `workflow_approval`
inbox item is resolved, and the run runs on: with the value it had for an `approve`, with the person's for an `edit`, or ended
`failed` with the reason for a `reject`. Response: `{"run": id, "status": string}`, the run and the status it went to. Errors:
`404` no such run; `409` the run does not wait, which is how a second decision on a finished wait is refused; `422` a
`decision` that is none of the three, or an `edit` without a `value`.

### `GET /v1/skills`

The skills an agent's `skills` can name, for an editor to offer: an array of `{"name": string, "description": string,
"built_in": boolean, "source": {"url": string, "commit": string}}`, the built-in skills first (`moebius`, then the task skills `moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions`, `moebius-plugins` and `moebius-report`, then `moebius-autonomous`), then the
engineering skills (`tdd`, `diagnosing-bugs`, `code-review`, `grilling`, `to-tickets`, `wayfinder`, `domain-modeling`, `research`, `prototype`) with `built_in` true and a `source`, the repository they were copied from and the commit, unless the harness configuration repository has a skill of that name, then the
directories of `skills/` in the harness configuration repository by name; a directory with the name of a reserved built-in skill
is left out (one with the name of an engineering skill replaces it). `description` is the `description` of the frontmatter of the skill's `SKILL.md`, empty when there is
none. Without `[harness]` the array is empty.

### `GET /v1/skills/{name}`

A skill of the harness configuration repository, or a built-in skill: `{"name": string, "description":
string, "text": string, "files": [string], "source": {"url": string, "commit": string}}` (`source` only for an engineering skill that the repository does not replace). `text` is the `SKILL.md` after its frontmatter; `files` are the paths of the
skill's other files, relative to its directory, sorted (each is read with `GET /v1/skills/{name}/files/{*path}`). Errors: `404` no such skill;
`409` the server has no `[harness]`: skills and MCP servers are edited in a harness configuration repository.

### `PUT /v1/skills/{name}`

A person adds or changes a skill. Request: `{"description": string, "text": string}` (`text` may be left out);
unknown keys are refused. The name is made of letters, digits, `-` and `_`, and the description is one line and not
blank. It writes `skills/<name>/SKILL.md` with the frontmatter `name` and `description`, then `text`; the skill's other
files are left as they are. The edit is committed (`Add the skill <name>` or `Update the skill <name>`) on the default
branch and pushed to `origin`, the decision is recorded (kind `edit_skill`, decider `person`), and the agents are read
again. Response: the skill as `GET` answers it, with `commit`, the short id of the commit (left out when nothing
changed). Errors: `409` no `[harness]`, or the skill is built in (`moebius` or a task skill) and read only; `422` an invalid
name or description; `409` and `500` as for `PUT /v1/agents/{name}`.

### `DELETE /v1/skills/{name}`

A person deletes a skill and its directory, committed and pushed the same way (`Delete the skill <name>`, kind
`edit_skill`). Response: `null`. Errors: `404` no such skill; `409` no `[harness]`, the skill is built in, or an agent
names it (the message names the agents); `409` and `500` as for `PUT`.

### `GET /v1/skills/{name}/files/{*path}`

One of a skill's other files; `{*path}` is the rest of the URL, from the skill's directory
(`/v1/skills/pdf/files/scripts/fill.py`). Response: the shape `GET /v1/repositories/{id}/files/{*path}` answers,
`{"path": string, "size": number, "text": string}`, with `binary` or `too_large` instead of `text` for a file that is
not UTF-8 text or is larger than 1 MiB. A built-in skill's files are read too. Errors: `404` no such skill or file (a
directory is not one); `409` no `[harness]`; `422` the path is empty, is `SKILL.md`, is absolute, climbs out with `..`,
names `.git` (in any case, or as `.git.`, `.git `, `GIT~1`), or goes through a symbolic link.

### `PUT /v1/skills/{name}/files/{*path}`

A person, or the Moebius agent, writes one of a skill's other files, creating it and its directories when they are
new. Request: `{"text": string}`; unknown keys are refused. The skill must exist (its `SKILL.md` is written with `PUT
/v1/skills/{name}`). The file is committed (`Add <path> of the skill <name>` or `Update <path> of the skill <name>`) on
the default branch and pushed to `origin`, and the decision is recorded (kind `edit_skill`, its context `skill`, `file`
and `text`). Text that changes nothing makes no commit and no decision. Response: the skill as `GET /v1/skills/{name}`
answers it, with `commit` (left out when nothing changed). Errors: `404` no such skill; `409` no `[harness]`, or the
skill is built in; `422` the path, as for `GET`, or it is a directory; `409` and `500` as for `PUT /v1/agents/{name}`.

```sh
jq -Rs '{text: .}' < fill.py > body.json
"$MOEBIUS" api PUT /v1/skills/pdf/files/scripts/fill.py --body body.json
```

### `DELETE /v1/skills/{name}/files/{*path}`

A person, or the Moebius agent, deletes one of a skill's other files, and the directories it leaves empty, committed and
pushed the same way (`Delete <path> of the skill <name>`, kind `edit_skill`). Response: the skill as `PUT` answers it.
Errors: `404` no such skill or file; `409` no `[harness]`, or the skill is built in; `422` the path, as for `GET`; `500`
as for `PUT` (a refused push is a `409` too).

### `GET /v1/mcp-servers`

The servers of `mcp.toml` in the harness configuration repository, by name, for an editor to offer: an array of
`{"name": string, "kind": "stdio" or "remote", "error": string}`. `error` is left out unless the server cannot be used
(then `kind` is `unknown` when the table is too broken to tell). Commands, arguments, environments, URLs and headers are
never included. The array is empty without `[harness]` or when `mcp.toml` cannot be read.

### `GET /v1/mcp-servers/{name}`

A server of `mcp.toml` as its table says: `{"name": string, "command": string, "args": [string], "env": {name:
string}}` for a stdio server, or `{"name": string, "url": string, "headers": {name: string}}` for a remote one; empty
`args`, `env` and `headers` are left out. A value holds its `${NAME}` references as written, never what they stand for.
Errors: `404` no such server; `409` no `[harness]`; `422` the server or `mcp.toml` cannot be used (the message says why).

### `PUT /v1/mcp-servers/{name}`

A person adds or changes a server. Request: the shape `GET` answers, without `name`. It is checked as reading `mcp.toml`
checks it (see [harness configuration](harness-configuration.md#mcptoml)): one of `command` and `url`, only the keys
that belong to it, and `${NAME}` only for a `NAME` in `mcp_env` of `[harness]`. The server's table in `mcp.toml` is
rewritten, keeping the file's other tables and comments, committed (`Add the MCP server <name>` or `Update the MCP
server <name>`) and pushed, the decision is recorded (kind `edit_mcp_server`, decider `person`), and the agents are read
again. Response: the server as `GET` answers it, with `commit`. Errors: `409` no `[harness]`; `422` the server would not
read, with the reason; `409` and `500` as for `PUT /v1/agents/{name}`.

### `DELETE /v1/mcp-servers/{name}`

A person deletes a server from `mcp.toml`, committed and pushed the same way (`Delete the MCP server <name>`, kind
`edit_mcp_server`). Response: `null`. Errors: `404` no such server; `409` no `[harness]`, or an agent names the server
(the message names the agents); `409` and `500` as for `PUT`.

### `GET /v1/extensions/search`

The packages of the catalog of pi.dev: the npm registry's search (`[packages] npm_registry`) for the keyword `pi-package`
and the words of `q`, at most 40. Query: `q` (words; none finds the most relevant packages) and `type` (`extension`,
`skill`, `theme` or `prompt`; only packages whose keywords, with or without the prefix `pi-`, name it). Response: an
array of `{"name", "version", "description", "kinds": [string], "weekly_downloads": number or null, "npm", "homepage",
"repository", "date"}`; the links and date are null when the registry gives none. Allowed to a person and to the Moebius
agent. Errors: `422` an unknown `type`; `422` the registry cannot be reached or answers something else.

### `GET /v1/extensions/info`

What the package of a source is, fetched without its dependencies into a throwaway directory. Query: `source`, such as
`npm:pi-web-access`, `npm:@scope/name@^1.2.0` or `git:github.com/owner/repo@v1`. Response: `{"name", "source", "version",
"resolved", "integrity" (null for git), "description", "resources": {"extensions", "skills", "prompts", "themes"}
(paths in the package), "dependencies": [string], "install_scripts": [string] (scripts npm would run and Moebius does
not), "files": number, "installed": the installed version or null}`. Errors: `404` no such package, version or ref;
`422` the source is not one, or the download does not match its integrity.

### `GET /v1/extensions/packages`

The packages the harness configuration repository vendors (`packages.lock`), by name: an array of `{"name", "source",
"version", "resolved", "integrity", "sha256", "resources", "install_scripts", "agents": [string] (the agents that name a
resource of it), "error": string or null}`. `error` says why the files cannot be read. Errors: `409` no `[harness]`.

### `POST /v1/extensions/packages`

Proposes installing a package. Request: `{"source": string, "agent": string}`; `agent` is optional and names an agent
that is given the package's extensions and skills as `pkg:<name>` in the same change. The package is fetched (an npm
download whose SHA-512 is not the registry's `dist.integrity` is refused; no install script runs), put on a branch
`moebius/ext-add-<name>-<n>` of the harness configuration clone, checked as a chat's branch is, pushed and offered as a
change of the harness configuration repository with a `ready_to_merge` inbox item (see [Packages](harness-configuration.md#packages)).
Nothing is installed until a person merges the change (`POST /v1/changes/{id}/merge`). Allowed to a person and to the
Moebius agent; the agent's token cannot merge it. Response: `{"change": Change, "package", "action": "add", "source",
"from": null, "to": version, "resolved", "integrity", "resources", "agents", "changelog": "", "files": ["A packages/..."],
"dependency_files": number, "diff": string}`. `files` are the files of the package, without what npm installed below
`node_modules`, which `dependency_files` counts. Errors: `404` no such package, version or ref; `409` the package is
installed already, or no `[harness]`; `422` the source or version is not valid, the integrity does not match, npm or the
registry failed, the agent does not exist or the package has nothing to give it, or the branch is not fit.

### `POST /v1/extensions/packages/{name}/update`

Proposes updating one package to the newest version its source allows: for npm the newest release within the range that
was chosen when it was installed (the `latest` tag for none), for git the newest commit of the ref (a commit is pinned).
`{name}` is the package's name, with a `/` written `%2F`. Request body: `{}`. Response: `{"offered": [Offered], "up_to_date":
[name], "failed": [{"package", "error"}]}`, where `Offered` is the response of `POST /v1/extensions/packages` with
`"action": "update"`, `from` and `to` the versions, `changelog` the sections of the package's changelog (`CHANGELOG.md`,
or `pi.changelogPath`) after `from` up to `to` (headings `## [x.y.z]` or `## x.y.z - date`), `files` the changed files
as `A`, `M` or `D` and a path, and `diff` the diff of them (cut off at 100,000 characters). The open `extension_update`
inbox item of the package is resolved. Errors: `404` no such package; for one package asked alone, the other errors of
`POST /v1/extensions/packages` instead of `failed`.

### `POST /v1/extensions/update`

Proposes updating every installed package, as the route above does for one: the response has an `offered` entry for each
package with a newer version, `up_to_date` for the others, and `failed` for those whose newer version could not be fetched.

### `DELETE /v1/extensions/packages/{name}`

Proposes removing a package: its directory, its entry in `packages.lock` and its names out of the agents that have them,
on a branch `moebius/ext-remove-<name>-<n>` offered as a change. Response: `Offered` with `"action": "remove"`, `from`
the version and `agents` the agents that lose a name. Errors: `404` no such package; `409` no `[harness]`.

A change that adds, updates or removes a package is merged by a person only: `POST /v1/changes/{id}/merge` (and `POST
/v1/proposals/{id}/merge` for a proposal of the Retro) answers `403` with `only a person can merge a change that
installs, updates or removes a package` to a run token when the change touches `packages/` or `packages.lock`. An
extension is code that runs in every Pi process of the agents that name it. The check is made again, on the commit that
is merged, when the branch is merged, so a commit added to the branch in between does not get past it. The `agent` of
`POST /v1/extensions/packages` must be an agent name (`422` otherwise).

### `POST /v1/uploads`

Stores a zip file on the server, for the Moebius agent to import skills or MCP servers from (the web app's Import
button). Request: the file itself as the body, with `Content-Type: application/zip`, at most 20 MiB (20,971,520
bytes). The file is written to `<data_dir>/uploads/<id>.zip`, where `id` is 32 random hex digits, and kept until
someone removes it. Response: `{"id": string, "path": string}`, `path` being where the file is on the server. Allowed to
a person and to the Moebius agent's run token. Errors: `413` the body is larger than the limit; `415` another content
type; `422` an empty body; `500` the file cannot be written.

```sh
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H 'content-type: application/zip' --data-binary @skills.zip \
  http://127.0.0.1:7171/v1/uploads
```

### `GET /v1/models`

The models the server uses and where it reaches them. Never a key: an upstream or the decision model says only whether it
has one. Response:

```json
{
  "upstreams": [{"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_set": true}],
  "agents": [{"name": "Coder", "model": "qwen3.8-flash-next", "protocol": "openai", "thinking": "medium", "token_budget": 2000000, "built_in": false}],
  "teacher": {"model": "qwen3.8-flash-next", "protocol": "openai", "timeout_seconds": 20},
  "decision_model": {"url": "http://127.0.0.1:8081/v1", "model": "moebius-decider", "timeout_seconds": 20, "api_key_set": false}
}
```

- `upstreams` are the `[[router.upstreams]]` the server runs with, in the order of the configuration. `api_key_set` is
  true when the upstream has an `api_key_env` or an `api_key_secret`; the server does not start when that variable is not
  set or no key is stored under that name. Neither the key nor the name of the variable or the secret is shown. The configuration names no models an upstream serves, so none are listed.
- `oauth` is `null`, or for an upstream that logs in with OAuth (see [the router](router.md#oauth-logins))
  `{"flow": "device" | "code", "logged_in": boolean, "warning": string, "preset": string | null, "needs": string | null}`
  (`preset` the built-in provider, `needs` what the upstream needs that the router does not speak yet, so that its
  requests fail): `logged_in` is true when a refresh token is
  stored for it (whether it still holds, `GET /v1/models/upstreams/{place}/login` tells), and `warning` the text the
  person confirms before a login starts.
- `agents` are the agents of [`GET /v1/agents`](#get-v1agents), with the same fields.
- `teacher` and `decision_model` are `null` without a `[teacher]` or `[decision_model]` table.
- A user name and password in a URL are removed from `base_url` and `url`.

### `POST /v1/models/upstreams/{place}/login`

A person starts the OAuth login of the upstream at `place` (from 0, in the order of `[[router.upstreams]]`). Request:
`{"accept_terms": true}`, which says the person confirmed the upstream's `warning` (that using a subscription through a
third-party tool may break the provider's terms). The login goes on in the background (see
[the router](router.md#oauth-logins)); a login started before for the same upstream is replaced. The confirmation is
recorded as a decision (kind `oauth_login`, decider `person`, the upstream's URL and the warning in its context).
Response, a login:

```json
{"state": "waiting", "flow": "device", "user_code": "WDJB-MJHT", "verification_uri": "https://login.example.com/device",
 "verification_uri_complete": null, "authorize_url": null, "message": "Open https://login.example.com/device and enter the code WDJB-MJHT."}
```

- `state` is `logged_out`, `waiting`, `logged_in` or `failed`.
- For `device`, `user_code` is entered at `verification_uri`; `verification_uri_complete`, when the provider gives it,
  is the page with the code filled in. For `code`, `authorize_url` is the provider's page, which the app opens in the
  browser. The others are `null`.
- `message` says in plain words where the login stands. No token is ever in a login.

Errors: `403` an agent's token; `404` there is no upstream at `place`; `422` `accept_terms` is not `true`, or the
upstream has no `oauth` table; `502` the provider's device URL refused or could not be reached.

### `POST /v1/models/upstreams/{place}/login/code`

A person finishes the waiting `code` login of the upstream at `place` with what the browser showed. Request:
`{"pasted": string}`: the code, `code#state`, or the whole address the browser ended on (see
[the router](router.md#oauth-logins)). Response: the login as it stands (`waiting`; the exchange of the code goes on in
the background and `GET` below tells how it ended). Errors: `403` an agent's token; `404` no upstream; `409` the upstream
has no login waiting for a code (none started, a `device` login, or one that ended); `422` nothing in `pasted` is a code,
or its `state` is not the waiting login's (the login goes on waiting).

### `GET /v1/models/providers`

The providers built in, which an upstream's `preset` names, in the order the Models page offers them: `[{"name":
"anthropic", "title": "Anthropic (Claude Pro/Max)", "protocol": "anthropic", "base_url": "https://api.anthropic.com",
"needs": null}]`. `needs`, when not `null`, is what the provider's upstream needs that the router does not speak yet (every built-in provider is served now, so it is `null`). The
app adds an upstream for a provider with `PUT /v1/settings` (`router.upstreams.<n>`: `protocol`, `base_url` and
`oauth.preset`), which takes effect without a restart.

### `GET /v1/models/upstreams/{place}/login`

Where the OAuth login of the upstream at `place` stands, as a login (above). While a started login waits, or after it
failed, that login is answered. Otherwise the server checks whether the stored login holds: `logged_out` when no refresh
token is stored or the provider refuses it, `logged_in` when the upstream lists its models with the access token
(`Logged in: the upstream lists 2 models.`), `failed` when it does not. The web app asks every second while a login
waits. Errors: `404` and `422` as above.

### `GET /v1/models/available`

The models the router's upstreams serve, for the pickers of the Models and Agents screens. The server asks every
upstream at the same time, each with a limit of 10 seconds, with the key it sends in the router (see
[the router](router.md#listing-models)). Query: `refresh=true` asks the upstreams again; without it the last answer is
returned if it is less than 5 minutes old. `decision=true` also asks the endpoint of `[decision_model]`, never kept, for
the Models screen's decision model. Response:

```json
{
  "models": [
    {"id": "claude-sonnet-4-5", "protocol": "anthropic", "upstream": "https://api.anthropic.com"},
    {"id": "qwen-local", "protocol": "openai", "upstream": "http://127.0.0.1:8080",
     "levels": ["none", "minimal", "low", "medium", "xhigh"],
     "thinking_levels": ["off", "minimal", "low", "medium", "xhigh"],
     "thinking": {"off": "none", "high": "xhigh", "max": "xhigh"}}
  ],
  "errors": [{"upstream": "https://api.example.com", "message": "401"}],
  "classes": [{"name": "strong", "models": ["claude-sonnet-4-5", "qwen-local"], "protocols": ["anthropic"]}],
  "decision_models": ["moebius-decider"]
}
```

- `models` are sorted by `protocol`, then `id`; the same model of the same upstream is listed once. Each may carry
  `levels`, the reasoning level names its endpoint said it takes, easiest first (see
  [the router](router.md#reasoning-level-names)); a model whose upstream does not say has no `levels` at all, and takes
  the level names themselves. Two more fields are the server's own working-out, so that a client does not repeat it:
  `thinking_levels`, the Moebius reasoning levels to offer for the model (only the ones its endpoint names, and the ones
  `[router.model_thinking]` gives a value; absent when every level is offered), and `thinking`, the value a run sends
  for each level whose own name the model does not take (`[router.model_thinking]` when it names the model, else
  worked out from `levels`; absent when nothing is renamed). A level not in `thinking` is sent by its own name. Both are
  worked out as the answer is given, so an edit of `[router.model_thinking]` shows at once. A model class has neither,
  and takes every level by its own name. When two upstreams list the same id, a client uses the first.
- `classes` are the model classes of `[router.classes]` by name (see [the router](router.md#model-classes)), which may be
  named wherever a model is: each with its models, first tried first, and the protocols `models` lists any of them
  under (empty when none is listed). They are the router's as the request is answered, never kept.
- `decision_models` are the ids the decision model's endpoint lists at `<url>/models`, with its key; empty without
  `decision=true` or without a decision model. An endpoint that cannot be asked, or whose key is not set, is in
  `errors` under its `url`.
- An upstream that cannot be asked is in `errors` with a short `message`: an HTTP status such as `401`, `timed out`,
  `could not connect`, `the answer is not JSON` or `the answer has no model list`. It does not fail the request.
- No key is in the answer. A user name and password in an upstream's URL are removed.

### `GET /v1/plugins/settings`

The app settings of the server: what each plugin asks of the whole server in its manifest
([`app_settings`](plugin-api.md#plugin-settings)), with the value the server has. Only plugins added to the server that
declare some are listed, by name; the web app shows them on the Plugin settings page (`/plugin-settings`). Response: `{"plugins": [...]}`, each
entry as in [`GET /v1/repositories/{id}/plugin-settings`](#get-v1repositoriesidplugin-settings). A plugin that is not running is left
out, but its values are kept and come back with it.

### `PUT /v1/plugins/{plugin}/settings`

A person replaces the values of one plugin's app settings, as
[`PUT /v1/repositories/{id}/plugin-settings/{plugin}`](#put-v1repositoriesidplugin-settingsplugin) does for a repository, with the
same request, checks and errors (but for `404`: no plugin of that name with app settings is running). The decision has
the kind `edit_app_settings`, and its context has `plugin` and `keys`. A launcher gets the new values at the next launch.
Response: the plugin's entry as `GET` answers it.

### `GET /v1/plugins/{plugin}/settings-ui`

The JavaScript module of a plugin's own page for its settings ([`settings_ui`](plugin-api.md#a-page-of-its-own) of its
manifest), as the plugin gave it, for the web app to hand to the sandboxed frame it draws the page in (see [a page of its
own](plugin-api.md#a-page-of-its-own)), in place of the fields. The answer is `text/javascript` with
`Cache-Control: no-store`, so a new version of the plugin is drawn on the next visit, and `X-Content-Type-Options: nosniff`. The same page serves both scopes.
Errors: `404` no plugin of that name with a page is running.

### `GET /v1/start`

Whether the server is ready for a first card, for the Get started screen and `moebius doctor`. The steps are always the
same five, in the order a person takes them. Response:

```json
{
  "done": false,
  "steps": [
    {"key": "upstream", "title": "Connect a model provider", "done": true, "detail": "3 models are available."},
    {"key": "pi_bolt", "title": "Install Pi-Bolt", "done": false, "detail": "Could not run pi-bolt: it was not found. Set the program on the Settings page."},
    {"key": "harness", "title": "Keep agents, skills and MCP servers in a repository", "done": true, "detail": "Agents, skills, MCP servers and rules come from /srv/moebius/harness."},
    {"key": "coder", "title": "Choose the Coder's model", "done": true, "detail": "The Coder uses qwen."},
    {"key": "repository", "title": "Add a repository", "done": false, "detail": "No repository is added yet."}
  ],
  "warnings": []
}
```

- `done` is true when every step is.
- `upstream` is done when the router's upstreams list at least one model (the list `GET /v1/models/available` answers,
  without asking again if it is less than 5 minutes old). Without an upstream the detail is `Add an upstream on the
  Models page.`; when every upstream fails, it names each one as `Could not ask <url>: <message>.`
- `pi_bolt` is done when `pi_bolt.program` exists and `<program> --version` exits with status 0 within 10 seconds.
- `harness` is done when the server runs with `[harness]` (see [harness configuration](harness-configuration.md)), and
  the detail names its URL. Without it the detail says what `POST /v1/harness/setup` does, which the web app's
  "Set up" button sends.
- `coder` is done when the agent the default Kanban starts in the code column, `Coder`, has a model.
- `repository` is done when at least one repository is added.
- `detail` is a sentence in plain words. It holds no key and no URL credentials.
- `warnings` are sentences the person should read even when every step is done: one for each upstream that logs in with
  OAuth, `<url> logs in with OAuth.` followed by the warning about the provider's terms. `moebius doctor` writes them
  after the steps.

### `GET /v1/plugins`

The plugins the plugin host has loaded, which answer the decisions (see [the plugin API](plugin-api.md)). Response:

```json
{
  "time_limit_seconds": 30,
  "plugins": [
    {"name": "guard", "version": "0.1.0", "kinds": ["decider"], "decides": ["tool_call"], "process": null,
     "enabled": true},
    {"name": "allow-reads", "version": "0.1.0", "kinds": ["decider"], "decides": ["start_card"],
     "process": {"program": "/srv/plugins/allow-reads", "state": "running", "restarts": 0, "error": null,
                 "log": "/srv/.moebius/plugins/allow-reads.log"},
     "enabled": true},
    {"name": "box", "version": "0.2.0", "kinds": ["launcher"], "decides": [], "process": null, "enabled": false}
  ]
}
```

`time_limit_seconds` is how long the host lets a call into any plugin take before it counts as refused. For each plugin,
by name, `kinds` are the kinds of its manifest (`forge`, `harness`, `runner`, `decider`, `router`, `view`) and `decides`
the kinds of decision it answers now, in alphabetical order: those routed to it, and those the decision model took over
with a merged takeover.

`process` is `null` for a plugin compiled in. For a plugin in its own process (`[[plugins]]` in
[the configuration](configuration.md#plugins)) it holds the `program` that runs; its `state`, one of `starting`,
`running` (it takes calls), `restarting` (it stopped and waits to be started again; calls are refused) and `stopped`;
how many times it was started again (`restarts`); why it stopped the last time (`error`, with secrets redacted, or
`null`); and the file its standard error goes to (`log`). Until such a plugin has sent its manifest, its `version` is
empty and its `kinds` are empty.

`enabled` is false for a plugin that is turned off (`plugins.enabled`, or `enabled` in the table `[guard]` or `[localize]` for those built-in deciders, see [the configuration](configuration.md#turning-a-plugin-off-and-on)):
it is listed from the manifest it last sent, its `process` is `null`, and it decides nothing.

### `PUT /v1/plugins/{plugin}/enabled`

A person turns the plugin `plugin`, one that `[[plugins]]` names or one of the built-in deciders `guard` and
`localize`, off or on, at once and without a restart. Request:
`{"enabled": boolean, "unwrap_runs": boolean (optional, default false), "allow_unchecked": boolean (optional, default false)}`.
For a built-in decider `enabled` false writes `enabled = false` to its table (`[guard]`, `[localize]`) and
`true` removes the key. `allow_unchecked` confirms turning off the guard, after which tool calls are not checked. `enabled` false writes `enabled = false` to the
plugin's entry, `true` removes that key; the entry and its other keys stay, and so do its settings. The server stops or
starts the plugin, gives its kinds back to or takes them from the deciders, and builds the launchers again from the
launchers that are on (see [turning a plugin off](configuration.md#turning-a-plugin-off-and-on)); a run that started keeps
the launcher it started with. The decision has the kind `enable_plugin` or `disable_plugin`, and its context has `plugin`
and `unwrapped_runs`. The event `plugin_enabled_changed` is published. Response:

```json
{"name": "box", "enabled": false, "changed": true, "unwrapped_runs": true, "unchecked_calls": false,
 "note": "runs now start unwrapped: no launcher is on after box was turned off"}
```

`changed` is false, and nothing is written, when the plugin runs in that state and the file says so too. It is true
when either changes: a hand edit that the server has not applied (it did not leave no launcher on without a
confirmation) is applied, or the file is put in step with what runs. `unwrapped_runs` is true, with a `note`, when the
plugin was the last launcher that was on, so that runs start unwrapped from now on. `unchecked_calls` is true, with a
`note`, when the plugin is the guard and it is off: tool calls are allowed unchecked from now on. Errors: `403` a run
token (a person only); `404` no `[[plugins]]` entry or built-in decider of that name; `409` no launcher would be on and
`unwrap_runs` is not true, or the guard would be turned off and `allow_unchecked` is not true; `422` an unknown field, or turning it off or on would leave the launchers wrapping some agents and not others (the
message names the agents whose runs would start unwrapped), or the file would not be valid, or the plugin cannot be
turned on (it cannot be started at all, or a launcher cannot name its leftovers; the message gives the reason, and the
file, the plugin and the launchers stay as they were, and nothing is published). A plugin that starts and then fails is
on and refused, as at the server's start.

### Packages

The package manager (see [packages](packages.md)). A package is a `plugin`, `workflow` (a Kanban), `rules` or `skills` package.

#### `GET /v1/packages/search`

Query `q`: text; the packages whose name, description or kind contains it (case does not matter), all without one.
Every registry in [`[[registries]]`](configuration.md#registries) is read afresh (cloned or fetched). Response:

```json
{
  "packages": [
    {"name": "gate", "kind": "plugin", "version": "1.1.0", "description": "Holds cards marked wip.",
     "registry": "moebius-plugins", "compatible": true, "installed": null}
  ],
  "problems": [{"registry": "acme", "message": "cannot read index.toml: ..."}]
}
```

One package for each name and registry. `version` is the newest that works with this Moebius, or the newest of all with
`compatible` false when none does. `installed` is the installed version or null. A registry that cannot be cloned or
whose index is not valid is in `problems` (secrets redacted), and the others are answered.

#### `GET /v1/packages`

What is installed: an array of the lock's entries (`name`, `version`, `kind`, `registry`, `url`, `sha256`, and
`program`, `repository`, `change`, `files`, `rules` where they apply; see [the lock](packages.md#the-lock-data_dirpackageslock))
with `update_available`: the newest version the nightly check found, or null, and for a plugin `enabled`, false when it
is turned off. Nothing is fetched.

#### `GET /v1/packages/{name}`

Every version of the package in the first registry that has it, newest first. Response: `{"name", "registry",
"installed": the lock's entry or null, "versions": [...]}`; each version is the index entry (`version`, `kind`,
`description`, `license`, `homepage`, `moebius`, `targets`, `plugin` with its `settings`, `downloads`, `files`) with
`compatible`, whether it works with this Moebius. Errors: `404` no registry has it; `502` a registry could not be read
and none has it.

#### `POST /v1/packages`

Installs a package (see [installing](packages.md#installing)). Request: `{"name", "version"?, "registry"?, "repository"?}`;
`version` is exact, without it the newest that works with this Moebius; `repository` is a repository's name, for a
Kanban (`workflow`) or rules package. Response, here and for the two routes below, an `Outcome`: `name`, `version`, `kind`,
`action` (`installed`, `updated`, `removed` or `unchanged`), `restart_needed` (a launcher is read when the server
starts), `change` (the change that proposes the files of a Kanban (`workflow`), rules or skills package; absent for a plugin) and
`plugin` (a plugin's `program`, `state`, `restarts` and `error`, as `GET /v1/plugins` has them; absent for the others) and `leftover` (after removing a plugin, the folder of its
files when it could not be deleted; the removal is done all the same; absent otherwise).
Errors: `403` a run token asks for a plugin (the Moebius agent may propose the other kinds); `404` no registry has the
package; `409` it is installed; `422` the version does not exist or does not work with this Moebius, no download fits
this target or the sha256 differs, there is no such registry or repository, or the files cannot be proposed (skills
without a harness configuration repository); `502` a registry or a download could not be reached. Recorded as the
decision `install_package`.

#### `POST /v1/packages/{name}/update`

Updates to the newest version of the package's registry that works with this Moebius; `unchanged` when it has it.
Request body: none. Person only (`403` for a run token). Errors: `404` it is not installed; `422` the new version does not
start (the old one stays) or is refused as an install is. Recorded as `update_package`.

#### `DELETE /v1/packages/{name}`

Removes the package. Person only. Error: `404` it is not installed. Recorded as `remove_package`.

### `GET /v1/settings`

Every key of [`moebius.toml`](configuration.md) with its value in the file, or its default, for the Settings and
Models pages to build their forms from. The file is read afresh, so a change made by hand shows. Response:

```json
{
  "file": "/srv/moebius/moebius.toml",
  "values": {
    "listen": {"value": "127.0.0.1:7171", "default": true},
    "board.round_limit": {"value": 7, "default": false},
    "budgets.day_tokens": {"value": null, "default": true},
    "router.upstreams": {"value": [{"protocol": "anthropic", "base_url": "https://api.anthropic.com", "api_key_secret": "anthropic"}], "default": false},
    "agents": {"value": {"Coder": {"model": "qwen3.8-flash-next", "token_budget": 2000000}}, "default": false}
  },
  "secrets": ["anthropic"],
  "restart_needed": false,
  "restart_keys": []
}
```

- `file` is the absolute path of the configuration file.
- `values` has an entry for every key of the reference, by its dotted name (`board.round_limit`; `harness.mcp_env`;
  `decision_model.shadow`), with the whole `router.upstreams` (an array of objects) and the whole `agents` (an object of
  agents by name). `value` is what the file says, as JSON, or the default when `default` is true; `null` when the key
  has no default and the file does not set it. Paths are as written in the file.
- `secrets` are the names of the stored API keys, never the keys. An upstream or the decision model names one with
  `api_key_secret`, and an environment variable with `api_key_env`; neither the variable's value nor a key is ever
  in an answer. A user name and password in `harness.url`, `decision_model.url` and the upstreams' `base_url` are
  removed.
- `restart_keys` are the keys whose value in the file is not the one the running server started with and that the
  server uses only after it restarts (see [Editing the file from the app](configuration.md#editing-the-file-from-the-app));
  `restart_needed` is true when there are any.

### `PUT /v1/settings`

A person changes `moebius.toml`. Request: `{"set": {...}, "remove": [...], "unwrap_runs": boolean, "allow_unchecked": boolean}`,
all optional. `unwrap_runs` (default false) confirms a change that leaves no launcher on, so that runs start unwrapped
(see below); `allow_unchecked` (default false) confirms a change that turns the guard off, so that tool calls are not
checked.

- `set` maps a dotted key to its new value: a JSON string, number, boolean or array of them for a key, an object for a
  table (`"teacher": {"model": "m"}` replaces the table, `"agents.Coder": {...}` replaces that agent), an array of
  objects for the router's upstreams (`"router.upstreams": [...]` replaces them all). One upstream is addressed by its
  position: `router.upstreams.0` replaces the first, the position after the last adds one. A key or table that is not
  in the file is added. A value is never `null`: remove the key instead.
- `remove` lists dotted keys or tables to take out (`budgets.day_tokens`, `teacher`, `agents.Coder`,
  `router.upstreams.1`); one that is not in the file is ignored. Removals are made before the sets.

The change is made to the text of the file, so its comments and the order of its keys stay. The result is read as
`moebius serve` would read it; if that fails (an unknown key, a value of the wrong type or out of range, an environment
variable of an upstream that is not set, a secret that is not stored, a GitHub App key file, a GitHub, GitLab or Forgejo
token file that cannot be read, a token secret that is not stored or a token that is empty, `[agents.*]` together with
`[harness]`) the
answer is `422` with the message of the parse and nothing is written. Otherwise the file is written in one step (a
temporary file in its directory that replaces it), the decision is recorded (kind `edit_settings`, decider `person`,
the `set` and `remove` in its context; a URL has no user name and password there, and a key is never part of a
change), and the keys that are live take effect at once. Response: `{"restart_needed": boolean, "restart_keys":
[string]}`, as in `GET /v1/settings`: true with the keys that the running server uses only after it restarts. A
`plugins.<n>.enabled` set or removed takes effect at once, by a person only (`403` for a run token). A change that turns
off the last launcher that is on is refused with `409` unless `unwrap_runs` is true, and one that sets `guard.enabled`
to false likewise unless `allow_unchecked` is true (`guard.enabled` and `localize.enabled` take effect
at once); a file that a hand edit left in
that state does not refuse other changes: the launcher or the guard stays on until the edit is confirmed (see
[turning a plugin off](configuration.md#turning-a-plugin-off-and-on)) and the event `plugin_enable_refused` is published.
A plugin that cannot be turned on makes the whole change a `422` with the reason, and the file is left as it was.
Errors: `403` as above; `409` as above; `422` as above, a key that is not a key of the configuration, or a plugin that
cannot be turned on; `500` the file cannot be read or written.

### `PUT /v1/secrets/{name}`

Stores an API key under `name` (letters, digits, `-` and `_`) in `secrets.toml` of the data directory, which only its
owner can read, replacing a key stored under the name before. Request: `{"value": string}`, not blank. The key is not
returned, stored anywhere else or logged. Response: the names of the stored keys. Errors: `422` the name or the key is
not allowed.

### `DELETE /v1/secrets/{name}`

Removes the key stored under `name`. Response: the names of the keys left. Errors: `404` there is none. A key that
`api_key_secret` still names is not checked: the next start fails until the name is stored again.

### `GET /v1/outgoing-webhooks`

The outgoing webhooks, oldest first: `[{"id", "url", "kinds", "projects", "created_at"}]`, `projects` being the names of the
repositories the hook names (empty for none). Never the secret. See [Outgoing webhooks](configuration.md#outgoing-webhooks).

### `POST /v1/outgoing-webhooks`

Adds a hook. Body: `{"url": "https://...", "kinds": ["card_moved"], "projects": ["name"]}`; `url` is an `http` or `https`
address, `kinds` is at least one event kind of [the events](#events) (lowercase letters, digits and `_`),
`projects` names repositories and may be left out. Response: the hook as above and its `secret`, a new random `whsec_...`
shown this once and kept as `outgoing-webhook-<id>` in `secrets.toml`. Errors: `403` to a run token; `422` the url, the
kinds or a project is not allowed.

### `DELETE /v1/outgoing-webhooks/{id}`

Removes the hook and its secret. Answers `204`. Errors: `403` to a run token; `404` there is no such hook.

### `PUT /v1/repositories/{id}/private`

An owner makes a repository private, or public again. Request: `{"private": bool}`. Response: `{"id", "private"}`. The
events of a private repository go only to an outgoing webhook that names it (see
[Outgoing webhooks](configuration.md#outgoing-webhooks)). Errors: `403` to a run token; `404` no such repository.

### `POST /v1/webhooks/{forge}`

A forge's webhook delivery (`github`, `gitlab` or `forgejo`; see [Webhooks](configuration.md#webhooks)). It needs no
token: the forge proves the delivery with `X-Hub-Signature-256` (GitHub), `X-Gitlab-Token` (GitLab) or `X-Gitea-Signature`
(Forgejo), checked in constant time against the secret `webhook-<forge>`. The body is JSON from the forge and is not
stored or used as data; it only names the repository to poll. Answers `202` with an empty body when the delivery is
verified, and the poll of the forge runs at once (for the repository `repository.full_name`, GitLab's
`project.path_with_namespace`, names, or for all of the forge's repositories when it names none). Errors: `401` the
signature is missing, malformed or wrong, and nothing changed; `404` the forge is unknown, is not set up, or has no
secret stored.

### `GET /v1/webhooks`

For each forge that is set up, its webhook as the Settings page shows it: `[{"forge": "github", "path":
"/v1/webhooks/github", "header": "X-Hub-Signature-256", "secret": string or null}]`. `path` is appended to the address the
forge reaches the server at; `header` is where the forge puts its proof; `secret` is the secret to set on the forge, null
until one is made or stored. This is the one answer that holds a secret, so only a person may read it (`403` to a run
token).

### `POST /v1/webhooks/{forge}/secret`

Makes a new random secret for the forge's webhook, stores it as `webhook-<forge>` and replaces the one before, which stops
working at once. No request body. Response: the forge's entry of `GET /v1/webhooks`. Errors: `403` to a run token; `404`
the forge is unknown or not set up.

### `POST /v1/server/restart`

Restarts the server. It answers `202` with an empty body, and a moment later stops the board, the schedule and the Pi
processes of the runs going (the next start fails those runs, as it does after any stop, so the board starts them
again) and starts the same program again with the same arguments, on the same configuration file. Clients lose their
connection and make it again. A server that is not the `moebius` binary only stops serving.

### `POST /v1/harness/setup`

A person moves the agents and rules of `moebius.toml` into a new harness configuration repository. No request body.
The server makes `harness/` next to `moebius.toml` a git repository on `main` and commits in it "Set up the harness
configuration repository": every `[agents.<name>]` as `agents/<name>.toml` and `agents/<name>.md`, each skill directory
an agent names copied to `skills/<directory name>/` and each extension file to `extensions/<file name>` (the agents'
`skills` and `extensions` name the copies), the rules file as `rules.md` (empty without one), an `mcp.toml` without
servers and a `README.md`; the agents are read back as the server reads them first (see [a repository on the
server](harness-configuration.md#a-repository-on-the-server)). It then writes `moebius.toml` in one step with `[harness]
url` set to the repository's absolute path and without `[agents.*]` and `rules_file`, its other keys and comments kept,
and records the decision (kind `setup_harness`, decider `person`, the file, the repository, the agents and where each
skill and extension was copied in its context). The file is saved as `PUT /v1/settings` saves a change, so the result
must read as `moebius serve` reads it and the keys that are live take effect at once; the running server is not
otherwise changed: it uses the repository once it restarts (`POST /v1/server/restart`, which the web app sends
next).

Response: `{"restart_needed": true, "restart_keys": ["harness.url"]}`, as `PUT /v1/settings` answers. Errors: `403` a
run token; `409` the file has `[harness]` already, or `harness/` exists and is not empty; `422` an agent cannot be moved
(a skill or extension path that is not there, a skill named like a built-in one, two skills or extensions with the same
name), or the file cannot be used as `moebius serve` reads it (it does not parse, an environment variable of an upstream
is not set, a secret is not stored); `500` a file cannot be read or written. Whatever fails before the file is written
leaves `moebius.toml` and `harness/` as they were; a write that fails leaves the repository the server made in
`harness/`.

### `POST /v1/chats`

Starts a chat: an interactive run without a card. The agent answers the first message as a stream of run events, and
the chat then waits (status `waiting`) for the next message. It works in a new worktree on the branch `moebius/chat-<run id>`
from the repository's default branch when `repository` is given, so that what the agent builds can be offered as a
change (see `POST /v1/chats/{id}/offer` and `POST /v1/chats/{id}/end`), or otherwise in an empty directory `chats/<run id>` under the data directory, which is removed when the
chat ends. The chat's run uses the agent's configuration like any run; its `token_budget` counts the chat's own tokens, the
daily budget applies, and a chat does not count against the board's run limit.

A chat lasts until it is ended or stopped. A chat that has waited 30 minutes for its next message, with no terminal open,
has its Pi ended to free what it holds, together with every process its agent started (a dev server, a watcher); the chat stays `waiting` with its worktree, and the next message starts its Pi
again with `--continue` on the chat's own session, so the agent answers it with the whole conversation. Ending, stopping
or opening the terminal of such a chat works as for any waiting chat.

Without `message` the chat's Pi starts with no prompt and the chat is `waiting` at once, so its terminal
(`GET /v1/chats/{id}/tui`) can be opened straight away; its first message, written later, gives it its title.

A chat runs on `model` and with the reasoning level `thinking` in place of its agent's when they are given: its Pi is
started with `--model` and `--thinking` of them every time (the first time, after a terminal, after waiting long), and
the router serves the model as for any run. The protocol to the model stays the agent's, unless the router's upstreams
list the model (see `GET /v1/models/available`) only under the other protocol: then the chat speaks that one. A
`model` that names a model class is listed under the protocols of its models. The
choice is stored as the chat's first `settings_changed` run event and listed with the chat. Pi's `models.json` marks
the model as one that reasons when the level is not `off`, as Pi otherwise turns any level off.

Request:

| Field | Type | Meaning |
|---|---|---|
| `agent` | string | An agent of the configuration, `Moebius` or `Grilling-partner`. Without it the chat is with the Moebius agent (see [configuration](configuration.md)). With `Grilling-partner` the chat is a grilling (see `POST /v1/grillings/{id}/confirm`) and `repository` is required. |
| `message` | string | The first message, not blank, sent to the agent as it is, with no issue prompt around it. Optional: without it the chat waits for its first message (required with `continues`). |
| `model` | string | A model to run the chat on in place of the agent's, not blank. Optional. |
| `thinking` | string | A reasoning level to run the chat with in place of the agent's: `off`, `minimal`, `low`, `medium`, `high`, `xhigh` or `max` (the levels Pi-Bolt takes). Optional. |
| `repository` | number | The id of a repository to work in. |
| `harness` | boolean | Work in the harness configuration repository instead (default false): in a worktree on the branch `moebius/chat-<run id>` of its default branch, so that the agents, skills, extensions, MCP servers and rules can be built as files and merged (see [changing it in a chat](harness-configuration.md#changing-it-in-a-chat)). The run has `harness_repository` true. Not together with `repository`. |
| `continues` | number | The id of an ended chat this one goes on from. Its agent, place (repository or harness configuration repository), model and reasoning level are used unless `agent`, `repository`, `model` or `thinking` name others, and the agent is given what was said in it (the person's messages and the agent's answers, at most the last 20 000 characters) before `message`. The new chat's `said` event holds only `message`. |
| `context` | string | Where the person is in the app (the page, the repository, the open card or chat, the text being edited), as the web app describes it. The agent is told it before `message`, under the line `[Where the person is in the Moebius app. Use it to understand what they mean; do not repeat it.]`, with secrets removed and cut to 4 000 characters; the `said` event holds only `message`. Optional; blank is the same as none. |
| `origin` | string | Where the chat is started: `chat` (the default), on the Chat page or by any client, `automation`, as an automation's chats are (see `POST /v1/automations`), or `assistant`, a conversation in the web app's Moebius agent of the right column, which the person reads there. `GET /v1/chats` leaves out the chats of origin `assistant` unless asked for them with `all=true`; what an app does with them is its own (the web app lists them on its Chat page with every other chat and counts them unread until they are read, the web app keeps them out of its Chat page). The run, its events and every other chat route work as for any chat. |

The chat belongs to the person who started it (`owner_id` of the `Run`, `owner` and `mine` of the `Chat`); another person
reads it only once its owner shared it (`POST /v1/chats/{id}/share`). A chat an agent's run token or an automation
starts has no owner and belongs to everyone, as does a chat from before chats had owners.

Response: the `Run` (`purpose` `chat`, `card_id` null). Errors: `404` no such repository or no run `continues`; `422`
the message is blank or missing with `continues`, `model` is blank, `thinking` is not one of the levels, the agent does not exist, it is a built-in agent and neither its own `[agents.<name>]` nor
`[agents.Coder]` is configured (the message says so), it is the Grilling partner and there is no `repository`, or
`continues` is not a chat or has not ended (write to it with `POST /v1/chats/{id}/messages`), `origin` is none of `chat`,
`assistant` and `automation`, `harness` is set together with `repository`, or `harness` is set and the server has no `[harness]`; `500` the worktree cannot be made.

### `GET /v1/chats`

The chats the person may read, newest first: those they started and those others shared (see `POST
/v1/chats/{id}/share`), without those of origin `assistant` (the web app's Moebius agent of the right column). Query: `all=true`
lists those too, each with its `origin`; the web app asks for them so that its Fleet page shows the agents they
keep working and "Open in Chat" can show one. `scope=mine` keeps the chats the person started, `scope=shared` those of
others that are shared with them (and the chats without an owner), `scope=all` (the default) both; any other value is
`422`. The Retro is no person's list: it reads the chats of every person. Response: an array of `Chat`. A chat's history is its run's events, read with
`GET /v1/runs/{id}/events` and followed on `GET /v1/events`.

### `POST /v1/chats/{id}/messages`

A person writes to a chat. The message is stored as a `said` run event (secrets removed) and sent to the agent as a
prompt: when the agent is waiting it answers at once; when it is working the message is delivered when it is done with
the current turn (a follow-up). To change what a working agent is doing, steer it with `POST /v1/runs/{id}/steer`.

A chat that ended because its agent went away is taken up again by the message, so that a person carries on in the chat
they are reading rather than starting a new one. The agent went away between turns (`status` `stopped`) when the server
stopped it, when it exited, and when a person pressed Stop (`POST /v1/runs/{id}/stop`); it went away in the middle of a
turn (`status` `failed`) when the agent's process failed, or the server stopped while it was answering. A
message written to such a chat starts an agent again in every case, so neither Stop nor a failed run keeps a chat from
being written to. The same `Run` is no longer
ended (`ended_at` null, `status` `running`), its worktree is made again on the branch it already has (for a chat with
no repository, its empty directory is simply made again; a directory or registration left over is cleared away), and
its agent starts on a prompt that holds what was said in the chat before — at most the last 20 000 characters of it —
and then the message. The agent has a run token of its own; the old one stays withdrawn. Two messages written at once to
such a chat start one agent, not two. Events: the chat publishes `run_started` again, and `run_ended` a second time
when the woken agent goes away, for the same run id; a client reads the latest of them as the chat's state. A chat that
ended any other way (`succeeded`) is ended for good and is continued by a new chat (`POST /v1/chats` with
`continues`), as is a grilling, whose issues are already read (`422`). `422` also when the chat cannot start again
(its branch is gone, or git cannot make its worktree): the chat stays as it ended, and the error says why; when the branch
is gone the message is not kept, but a message whose agent fails to start after the message was stored stays in the
chat, so a retry repeats it. A blank message to such a chat is the same ask with no new words: its agent starts on what
the chat already holds and no `said` event is written. It is told what was said in the chat and the final answers, and
that work it had begun and not committed may be lost, because a failed turn commits nothing and its worktree is removed:
it checks the state of the branch before going on. A chat with nothing said in it has nothing to answer again, and a blank
message to it is refused (`422`). A failure that is permanent (a launcher refusing to start the agent, a missing model)
fails again on each Retry, and adds a `moebius_error` event each time.

While such a chat is being taken up, from the moment it is `running` until its agent is up, the chat answers a
message with `409` (`chat N is starting; try again in a moment`) and records nothing, so the message can be sent again
a moment later.

Request: `{"message": "...", "context": "..."}`; `message` is required. A blank message is refused unless the chat's
agent went away (`stopped` or `failed`); a blank one to such a chat asks its agent to answer the last thing said again and stores nothing in the
chat, which is what a Retry asks for. `context` is optional and works as in `POST /v1/chats`: the agent is told it
before the message (secrets removed, at most 4 000 characters), and the `said` event holds only the message.

Response: the `Run`. Errors: `404` no such run; `422` the run is not a chat, the chat has ended for good, its agent
cannot start again, it is the Grilling partner (its issues are already read), the message is blank and the chat
is not one whose agent went away, or it is blank and nothing was said in the chat; `409` the chat is in its terminal, or its agent is starting (see above).

The chat is `running` from the request on, before the agent says it has started, until the agent has answered.

### `POST /v1/chats/{id}/settings`

Changes the model or the reasoning level a chat runs with, in place of its agent's (see `POST /v1/chats`):
`{"model": "...", "thinking": "..."}`, each optional and kept as it was when not given. The agent answers the next
message with them. The chat must be `waiting`, between turns, or ended with its agent gone (`stopped` or `failed`): while its agent works
the answer is `409`, and a chat that ended any other way is ended for good.

The change is stored as a `settings_changed` run event holding what the chat now runs with, `{"model", "protocol",
"thinking"}` (each null while the agent's is used; `protocol` is `openai` or `anthropic`, chosen as in
`POST /v1/chats`), and the latest such event counts. Then the chat's Pi is told:

- When only the reasoning level changed and the Pi's model reasons (its level was not `off`, nor is the new one), Pi is
  sent `{"type": "set_thinking_level", "level"}` on its RPC channel, and its answer is recorded as a `response` event.
- When the model changed (Pi's `set_model` knows only the models of its `models.json`, which holds the one it was
  started with), or the level turns reasoning on or off, the chat's Pi is ended and started again with `--continue`
  on the same session and the new `--model` and `--thinking`; the chat stays `waiting` and keeps its conversation.
- A chat whose Pi was ended after waiting long, or whose terminal is open, has no Pi to tell: its Pi is started with
  the new settings when the next message comes or the terminal closes.
- A chat whose agent went away (`stopped` or `failed`) has none to tell either: the agent its next message makes (see
  `POST /v1/chats/{id}/messages`) starts with the new settings, so a person leaves a chat on a better model as they
  leave it.

Response: the `Run`. Errors: `404` no such run; `409` the chat's agent is working; `422` the run is not a chat, the chat
ended for good (`succeeded`), `model` is blank or `thinking` is not one of `off`, `minimal`, `low`,
`medium`, `high`, `xhigh` and `max`.

### `POST /v1/chats/{id}/offer`

Offers what the chat's worktree changes as a change, without ending the chat: what the agent left in the worktree is
committed on the chat's branch in a message of the same shape as a chat's commits (see `POST /v1/chats/{id}/end`),
under which is "Made by Moebius when the chat's work was offered.", and the
branch is offered exactly as when the chat ends (see `POST /v1/chats/{id}/end`): a change without a card on
`moebius/chat-<run id>`, published through the repository's forge, an open `ready_to_merge` inbox item with the chat's
title and the event `change_offered`; for a chat in the harness configuration repository, after the same check of the
branch. The chat goes on in the same worktree and takes messages as before.

Offered again after the chat has changed more, the chat's open change is updated rather than a second one made: its
`head` becomes the branch's new commit (on GitHub the branch is pushed to the same pull request), its inbox item is
resolved and a new one added, and `change_offered` is published. Offered again when the branch has not moved since, the
open change is the answer and nothing is made. A chat whose change was merged or closed offers a new change once it has
changed more. Ending the chat later offers what it changed since in the same way.

The offer is recorded as a decision of kind `offer_chat_changes` (decider `person`, or `agent:Moebius` for the Moebius
agent's run token), with the chat's run and the context `change` and `branch`.

Request: `{"reason": "..."}`, or `{}`; `reason` is optional.

Response: the `Change`. Errors: `404` no such run; `409` the chat's agent is working (`starting` or `running`), as its
worktree may be half edited: offer once it has answered; `422` the run is not a chat, the chat has ended, it is a
grilling, it works in no repository, it has nothing to offer (its branch changes nothing beyond the default branch, or
nothing that was not offered in a change since merged or closed), or its harness configuration branch fails the check
(the message says why).

### `POST /v1/chats/{id}/publish`

Stores what a grilling chat has left in its worktree the way its end would, without ending the chat and while the
chat takes messages as before: the same grilling, its inbox item and `grilling_published` in place of
`grilling_ended`. The first publish stores the grilling; a publish of the same chat that follows replaces what was
stored — the grilling row and its open inbox item, kind included, since the chat may have gone from issues to a
report or back — while what was published has not been confirmed or dismissed yet, so the grilling may go on after
its person has the issues. Ending or stopping the chat after a publish publishes nothing more: what was published is
the grilling of the run.

Response: the `Grilling`. Errors: `404` no such run; `422` the run is not a chat, the chat has ended, it is not with
the Grilling partner, what it left is `empty` (nothing usable in the worktree; the reason why comes with the error),
or what was published was settled (`confirmed` or `dismissed`) already.

### `POST /v1/chats/{id}/seen`

A person has read the chat up to the event `seq`: `{"seq": number}`. The chat's `seen` becomes that `seq`, unless it is
already further, since reading never goes back. The web app sends it while the chat is open on its Chat page, and
counts on the sidebar's Chat entry the chats that are unread. Response: `{}`. Errors: `404` no such run; `422` the run
is not a chat.

### `POST /v1/chats/{id}/title`

Names a chat: `{"title": "..."}`, at most 80 characters of it, taken after the spaces at either end. The name replaces
the title taken from the chat's first message everywhere the chat is titled: the list of chats, an inbox item and a
pull request made from the chat's branch. An empty or blank `title` gives that title back. A chat may be named at any
time, including after it ended: the name belongs to the chat, not to its run.

Response: the `Chat` as `GET /v1/chats` shows it, with its new `title`. Errors: `404` no such run; `422` the run is not
a chat.

### `POST /v1/chats/{id}/share`

The person who started a chat shares it with every person of the organisation, or takes it back: `{"shared": boolean}`.
A shared chat is read by others (`GET /v1/chats`, the run, its events and its diff, the stream, a grilling it made) but
only its owner writes to it: its messages, steer and stop, settings, title, offer, publish, end, terminal and delete,
and marking it seen, stay with them. A chat that is not shared is invisible to every other person: its routes answer `403`
and it is missing from their list and from their event stream. Announced as `chat_shared` to everyone, so that every
client reads the list again. Response: the `Chat` as the person sees it. Errors: `403` the chat is another person's, or
an agent's run token; `404` no such run; `422` the run is not a chat, or the chat has no owner and belongs to everyone.

### `DELETE /v1/chats/{id}`

A person deletes a chat, also one of the web app's Moebius agent. A chat that has not ended is stopped first, as `POST
/v1/runs/{id}/stop` stops a run, and the deletion waits for it to end (`409` if it does not within a minute: try
again). Then everything that belongs to the chat is deleted, and nothing else:

- the chat's run, its events and its read marker;
- the changes the chat offered (those on its branch `moebius/chat-<run id>`, also one already published or merged) and
  their inbox items (only the change record goes: a pull request already published on the forge stays open there, for a
  person to close);
- the grillings the chat ended in, with their inbox items (issues a grilling filed stay on the forge and as cards);
- its worktree, or empty directory, and its branch `moebius/chat-<run id>` in the repository's clone, or in the harness
  configuration repository's for a chat there; a branch pushed to a forge stays there.

The chat's `usage` rows and the judgements made in it stay, with `run` null from now on: usage is a history of spend,
so the totals of `GET /v1/usage` do not change, and the judgements are what the decision model and the training export
learn from. Its replays are deleted with it.

The decision is recorded (kind `delete_chat`, decider `person`, context: `chat` the id, `title`, `agent`, `events` the
number deleted, `changes` the ids of the changes, `inbox_items`, `grillings` and `usage_kept`) and announced as
`chat_deleted`, and as `grilling_deleted` for each grilling. Request: `{"reason": string}` (optional). Response: the
chat's `Run` as it was. Errors: `403` for an agent's run token (only a person deletes); `404` no such chat, also a run
that is not a chat; `409` the chat is being started again, or does not end.

### `POST /v1/chats/{id}/end`

The web app ends a grilling chat while it waits (its "End grilling" button); other open chats are stopped by a person
while their agent works, and a chat that waits long frees its Pi by itself; the command line and the Moebius agent end
chats with this route, and a grilling ends with it. A person ends a chat that has not ended: Pi is asked to shut down (its standard input is closed; a Pi that has not exited
after 10 seconds is killed). With a repository, what the agent left in the worktree is committed on the chat's branch,
and then the worktree, or the empty directory, is removed. A chat that changed nothing has still succeeded. Its commit
says what the chat did, never just the chat's first message: the subject is the first line of the agent's last answer
(the chat's title when it left no answer), the body carries the rest of that answer, and the trailer names the chat,
its title and its agent — `Chat <run id>: <title> (agent <agent>)`, under which is "Made by Moebius when the chat
ended."

If the chat's branch then has commits that the repository's default branch does not have, they are offered as a change
that has no card: a `Change` with `card_id` null on the branch `moebius/chat-<run id>`, published through the
repository's forge (on GitHub the branch is pushed and a pull request is opened whose title is the chat's title and
whose body says what the chat did — the agent's last answer under "What the chat did", the files changed under
"Files changed", and that it came from the chat, asked to what its title says; without a forge there is nothing to
publish), an open `ready_to_merge` inbox item
with the chat's title (`card_id` null, `data.change`, `data.branch`) and the event `change_offered`. A person merges it
with `POST /v1/changes/{id}/merge`, which resolves the item; the merge commit's message is `Merge <branch>: <the
branch's last commit's subject>`, and `Merge <branch>` when the branch's last commit has no subject. A chat
whose work was offered with `POST /v1/chats/{id}/offer` updates its open change instead, and offers nothing new when it
changed nothing since. A chat that changed nothing, or has no repository, offers nothing, and a chat that ends as `failed` offers nothing. If the offer
fails, the chat's run gets a `moebius_error` event saying why and the work stays on the branch. A chat that is stopped
offers what it made in the same way.

A chat in the harness configuration repository (`harness` true) is committed the same way and offered as a change of
that repository: a `Change` with `repository_id` null and `harness_repository` true, no card and no pull request, on the
branch `moebius/chat-<run id>`, with the same `ready_to_merge` inbox item and `change_offered` event. Before it is
offered, the branch is checked as a branch of a Retro proposal is: it may change only harness configuration files
(`agents/`, `skills/`, `extensions/`, `rules.md`, `mcp.toml`), and every agent that could be read on the default branch
must still be readable with it. A branch that passes is pushed to `origin`. A branch that fails is not offered: the chat's
run gets a `moebius_error` event saying why, and the branch stays in the clone.

A grilling (a chat with `Grilling-partner`) commits nothing and offers no change: before the worktree is removed Moebius
reads what the agent left in it (see `POST /v1/grillings/{id}/confirm`) and stores a `Grilling`, whose status and id are
the event `grilling_ended`.

The chat ends as `succeeded`, which publishes `run_ended`; the change is offered before that.

Request: `{}`.

Response: the `Run`, as it was when the request was made. Errors: `404` no such run; `422` the run is not a chat or has
ended; `409` the chat's agent is starting after a message woke it (try again in a moment).

### `GET /v1/automations`

The automations, by name: an array of `Automation`. An automation is a workflow with a `schedule` trigger and one `chat`
node (see [workflows](workflows.md#automations)); these routes and the commands keep their old names and arguments as a
shortcut over it. The workflow file is the definition, and the automation's row keeps the id its firings use. A row
whose file has been edited into an ordinary workflow is not listed: the workflow owns its schedule, and the row fires
it no more. Deleting the workflow's file also stops the automation, until the next start of the server writes it
again; then the schedule fires it as before.

### `POST /v1/automations`

Adds an automation: recurring work that runs on its own. It writes the `schedule` workflow named after it, in the harness
configuration repository, with one `chat` node; a name an ordinary workflow or another automation already holds gets
`automation-<id>.toml` instead, so neither loses its file and two automations may share a name. Without a `[harness]`
the automation keeps its row, is fired from it,
and moves to a workflow when a harness configuration repository is configured. Once a minute the workflow schedule reads
its local time; each automation that is on and whose schedule came due since the minute before (or, after a late check,
within the last five minutes) fires once. It starts from the minute the automation was last checked or saved, so an
upgrade or a save cannot fire the minute it happens in again. A repository automation starts a workflow run whose `chat`
node starts a chat as `POST /v1/chats` would, with the automation's agent, place and `prompt` as the first message, and
`origin` `automation`; an automation that works in the harness configuration repository starts that chat directly. A
firing that came due while the server was not running is not made up. A run never waits for a person: once the chat's
agent has answered and the chat waits for a message, the next check ends it as `POST /v1/chats/{id}/end` does, so what
it changed is offered as a change and lands in the inbox. A firing due while the chat of the automation's last started
firing has neither ended nor answered is skipped and recorded as `skipped`. A chat that cannot be started (its agent can
no longer run, say) is recorded as `failed` with the reason. Each firing publishes `automation_fired`, whose `workflow`
names the workflow behind it and whose `workflow_run` names the run that carries it.

Request:

| Field | Type | Meaning |
|---|---|---|
| `name` | string | Not blank. |
| `schedule` | string | A cron expression of five fields, minute (0-59), hour (0-23), day of the month (1-31), month (1-12 or `jan`-`dec`) and day of the week (0-7 or `sun`-`sat`, Sunday 0 or 7), in the server's local time. A field is `*`, a number, a range `a-b`, either with a step (`*/15`, `8-18/2`, `5/10` from 5 on), or a list of these separated by commas. When both the day of the month and the day of the week are restricted (neither starts with `*`), a day that matches either comes due, as in cron. |
| `agent` | string | An agent the server can run, as `GET /v1/agents` lists them. |
| `prompt` | string | The first message of each chat, not blank. Stored with secrets removed. |
| `repository` | number | The id of a repository its chats work in. Optional; without it and without `harness` each chat works in an empty directory. |
| `harness` | boolean | Its chats work in the harness configuration repository (default false). Not together with `repository`. |
| `enabled` | boolean | Whether the schedule fires it (default true). An automation that is off fires only with `POST /v1/automations/{id}/run`. |

Response: the `Automation`. Errors: `404` no such repository; `422` the name or prompt is blank, the schedule does not
parse (the message says which field is wrong), the agent does not exist, `harness` is set together with `repository`, or
`harness` is set and the server has no `[harness]`.

### `GET /v1/automations/{id}`

The `Automation`. Errors: `404` no such automation.

### `PUT /v1/automations/{id}`

Replaces an automation: the same body and checks as `POST /v1/automations`. Its firings stay, it fires next when its new
schedule comes due, and the workflow file changes with it. Response: the `Automation`. Errors: as `POST
/v1/automations`, and `404` no such automation.

### `DELETE /v1/automations/{id}`

Deletes an automation, its workflow file and its firings; the chats it started stay. Response: `{}`. Errors: `404` no
such automation.

### `POST /v1/automations/{id}/run`

Fires an automation now ("Run now"), whether it is on or not, as its schedule would: it is skipped while the chat of its
last started firing still runs, and a repository automation starts a workflow run like the schedule. Request: `{}`.
Response: the `Firing` (`manual` true). Errors: `404` no such automation.

### `GET /v1/automations/{id}/firings`

The automation's firings, newest first, at most 50: an array of `Firing`, each with its workflow run
(`workflow_run_id`), its chat (`run_id`, whose events are `GET /v1/runs/{id}/events`) and that chat's status. Errors:
`404` no such automation.

### `POST /v1/retros`

Runs the Retro of a day now: the day's errors are collected into a report and, when the day had errors and `[harness]`
is configured, the Retro analyst starts (see [harness configuration](harness-configuration.md#the-retro)). The analysis
goes on after the response, which has the status `analysing`; its end is the event `retro_done`. When the analyst cannot be
started at all, the response is still `200`, with the status `failed` and the reason in `warnings`, and `retro_done` is sent. Without `[harness]`, or
without errors, no run is made and the Retro is `reported` at once. The server does this by itself once a day, at
`retro_at` (see [configuration](configuration.md)).

Request: `{}` for today, or `{"date": "YYYY-MM-DD"}` for a local day. Response: the `RetroView`. Errors: `409` the day has
a Retro already; `422` the date is not a date.

`RetroView`: `date`, `status` (`reported`: the report is all there is; `analysing`: the analyst is running; `proposed`: it
ran and its proposals are stored, there may be none; `failed`: its run failed or was stopped, the server stopped
meanwhile, or storing its proposals failed), `run_id` (the analyst's run, or null), `warnings` (array of strings: the branches left out and why, or why it
failed; a run that settled with no answer at all counts as failed too), `at`, `report` and `proposals` (array of `Proposal`).

`report`: `{"date", "errors": [...], "merged": [...], "estimates": [...]}`; each kind of error keeps at most 100 entries, its earliest
ones, and a card sent back from review keeps only the latest entry of each kind per card, so that a busy day's repeated
findings cannot make the prompt unsendable. `merged` are the cards that reached Done that day after one code
run and without being sent back from review, each `{"card", "title", "text"}` (`text` the start of the issue), which the
Retro analyst writes lessons from (see [lessons](#get-v1lessons)). A day with errors or merged cards starts the analyst.
`estimates` are the cards that reached Done that day whose [estimate](#estimates) was furthest off, the furthest first
(by whichever of tokens and time was further off), at most five, each `{"card", "title", "estimate", "tokens", "minutes", "tokens_ratio", "time_ratio"}`. A point is worth
the median tokens (and minutes) per point of every done card that has an estimate and cost something; `tokens_ratio`
is the tokens the card took over what its points are worth (3 is three times as many, 0.2 a fifth), and `time_ratio`
the same for the minutes. A card is listed when it took at least one and a half times or at most two thirds of what its
points are worth in tokens, or at least one and a half times in time. The list is empty while no done card says what a
point is worth, and a card with no tokens is never listed.
`errors` are in the order in time. Each error is `{"kind", "run", "card", "agent", "text"}`;
`run`, `card` and `agent` are null where the error has none, and `text` is at most 500 characters, with secrets removed as
stored. The kinds are `tool_failed` (a tool call that ended in error: the tool and the start of what it said, not a call
Moebius refused), `tool_refused` (the tool and the rule's reason), `run_failed` (the run's last `moebius_error`),
`run_stopped`, `steered` (a person's message to a running run), and `checks_failed`, `conflict` and `review_findings`
(a card sent back from In review to In progress, with its note). The runs of the Retro analyst itself are left out.

`Proposal`: `id`, `kind`, `retro_date`, `branch` (`proposal/<date>-<name>`, also on the harness repository's `origin` for
a `harness` proposal), `title` (the subject of its commit), `body` (the rest of the message, without its `Cards:` line:
the Retro's reason), `agents` (the names of the agents whose files it changes), `cards` (the ids of the cards whose
errors it addresses, from the `Cards: 48, 50` line that ends its commit message; empty without one), `status`, `trials`
(array of `Trial`, oldest first), `diff`, `edits`, `repository`, `change` and `at`.

`kind` of a proposal: `harness` (a change to the harness configuration, on its branch), `settings` (a change to
`moebius.toml`) or `repository` (a change to a repository's own rules or the column scripts of its home project, the files of `repos/<repository name>/` and `projects/<project name>/` in the harness configuration repository); see
[the Retro](harness-configuration.md#the-retro). For `settings` and `repository`, `diff` is a unified diff of what it
changes, `edits` is what it sets and removes (`{"set", "remove"}`, as `PUT /v1/settings` takes it) or the new text of
each file (`{"files": {"repos/<repository name>/rules.md": text, "projects/<project name>/<file>.luau": text}}`), `repository` is the repository's id for `repository` (else null),
and `change` is the id of the change of the harness configuration repository a merged `repository` proposal was offered as (else null). A `harness` proposal has
an empty `diff`, `edits` `{}` and both null.

`status` of a proposal: `open` (not tried yet), `trying` (its trials are going on), `positive` (every trial was positive),
`mixed` (no regression trial was negative, some trial of its own cards was, and at least half of its own trials that
ended were positive), `negative` (a regression trial was negative, or fewer than half of its own trials that ended were
positive; a trial ended when it was `positive` or `negative`, not `skipped` or `failed`), `untried`
(no trial was negative, but some was skipped or failed, or it names no cards, so nobody knows), `merged` or
`dismissed`. Every proposal but a `negative` one has an open inbox item of the kind `proposal`, with the outcome and the
reason of each of its trials; a `mixed` one is a trade-off a person decides about. A `settings` or `repository` proposal
is never tried: it is `untried` and in the inbox from the start.

`Trial`: `id`, `proposal_id`, `card_id` (the card it was tried on; null for the one trial of a proposal that names no
cards), `regression` (true for a card of the regression set, see
[the regression set](harness-configuration.md#the-regression-set); its `original` is then the card's code runs together,
with no `errors`, and its trial must pass where they did), `run_id` (the trial's run, or null when none was started), `outcome` (`positive`, `negative`, `skipped`: there was
nothing to try it on, or `failed`: it could not be tried), `reason` (text), `measurements` (an object, `{}` when
nothing was run) and `at`. `measurements` is `{"trial": {...}, "original": {...}}`: for the trial run `succeeded`,
`errors` (the kinds of the addressed errors that happened in the run), `checks` (true, false, or null when the repository
has no checks or the run made nothing to check), `tokens` and `rounds` (always 1); for the card's first code run `errors`
(the kinds of the card's errors in the report), `checks_failed` (the card was once sent back because the repository's
checks failed), `rounds` (the card's code runs) and `tokens`. See [the trials](harness-configuration.md#the-trials).

### `GET /v1/retros`

The Retros, the latest day first. Response: an array of `{"date", "status", "run_id", "errors" (how many the report
holds), "proposals" (how many), "at"}`.

### `GET /v1/retros/{date}`

One Retro as the `RetroView`. Errors: `404` the day has none.

### `POST /v1/retros/{date}/retry`

Runs the analysis of a day whose Retro `failed` again (its run failed or was stopped, or the server stopped while the
analyst was going): the report stored for the day is kept as it is, the analyst starts again on it, and the `warnings`
that said why it failed and its `run_id` are cleared. The analysis goes on after the response, which has the status
`analysing`; its end is the event `retro_done`, and a run that fails again leaves the day `failed` with the new reason.
As with `POST /v1/retros`, an analyst that cannot be started at all gives a `200` with the status `failed` and the
reason in `warnings`. Of two retries at once only one starts an analyst; the other is refused. Nothing retries by
itself: the nightly schedule only ever looks for a day that has no Retro.

Request: `{}`. Response: the `RetroView`. Errors: `404` the day has no Retro; `422` its Retro is not `failed` (or another retry took it first), or
`[harness]` is no longer configured.

### `GET /v1/lessons`

The lessons the Retro learned from earlier cards, newest first; the analyst writes them at the end of its answer from the
day's failed runs, review rejections and merged cards. Query: `repository` (an id; only that repository's lessons) and
`retired` (`true` to include retired lessons, which are left out otherwise). Response: an array of `Lesson`: `id`,
`repository_id`, `kind` (`failure`, `rejection` or `success`), `situation`, `lesson` (what went wrong or what worked),
`advice` (what to do), `cards` (the cards it was learned from), `retro_date` (null for none), `status` (`active` or
`retired`), `retired_reason` (null while active: `not retrieved for 60 days`, or `harmful: ...` when a trial run the
lesson was given to was negative), `created_at`, `last_retrieved_at` (null if never) and `retrievals`.

At the start of a `code` or `review` run, the up to 5 active lessons of the card's repository that best match the issue's
text are put in the prompt under "Lessons from earlier cards", and the Pi tool `lessons(query)` finds more (see the
[bridge protocol](bridge-protocol.md#post-moebiuslessons)). Each counts as retrieved.

### `GET /v1/lessons/{id}`

One `Lesson`. Errors: `404`.

### `PUT /v1/lessons/{id}`

Rewrites a lesson's text. Request: `{"situation", "lesson", "advice"}`, each non-empty and at most 1000 characters;
secrets are removed. Response: the `Lesson`. Errors: `404`; `422` for an empty or long part.

### `DELETE /v1/lessons/{id}`

Deletes a lesson, retired or not. Response: `{}`. Errors: `404`.

### `GET /v1/proposals`

All proposals, the newest first. Response: an array of `Proposal`, each with its trials.

### `POST /v1/proposals/{id}/trial`

Tries a proposal now, at any hour, as the schedule does inside `[trial_window]`: its status becomes `trying`, the trials
go on after the response, and each trial ends with the event `trial_done` and the proposal with `proposal_status`.
Request: no body. Response: the `Proposal`, with the status `trying`. Errors: `404` no such
proposal; `409` the proposal is not `open`, is not of the kind `harness`, or another proposal is being tried (only one is
at a time); `422` there is no harness configuration repository.

### `POST /v1/proposals/{id}/merge`

A person merges a `positive`, `mixed` or `untried` proposal: its branch is merged into the default branch of the harness
configuration repository's clone with a merge commit, pushed to `origin`, and the agents are read again. Then the decision
is recorded (kind `merge_proposal`, choice `merge_proposal`, decider `person`), the proposal's inbox item is resolved and
its status becomes `merged`. A merge that conflicts, and a push that fails, change nothing. Request: `{"reason": string}`
(optional). Response: the `Proposal`. Errors: `404` no such proposal; `409` the proposal is neither `positive`, `mixed`
nor `untried`, or its branch does not merge cleanly; `422` there is no harness configuration repository.

A `settings` proposal is merged by saving its `edits` to `moebius.toml` as `PUT /v1/settings` saves a change (refused
with `422`, and nothing written, when the result is not a valid configuration), which records `edit_settings` too; when
the file then has a key the running server only uses after a restart, the server restarts shortly after the response,
as after `POST /v1/server/restart`. A `repository` proposal is merged by committing its files on the branch
`moebius/proposal-<id>` of the harness configuration repository's clone, from its default branch, and offering that
branch as a change of that repository, with a `ready_to_merge` inbox item; the change then merges as any change of it
does (`422` when the server has no `[harness]`), and `change` names it. Both record the decision as a `harness` proposal does, with the proposal's
`kind` and `diff` in its context.

### `POST /v1/proposals/{id}/dismiss`

A person dismisses a `positive`, `mixed` or `untried` proposal: the decision is recorded (kind `merge_proposal`, choice `dismiss`,
decider `person`), the inbox item is resolved and the status becomes `dismissed`. The repository is not changed, nor is
`moebius.toml` or any repository for a `settings` or `repository` proposal. Request and errors as for `merge`, without
the conflict. Response: the `Proposal`.

### `GET /v1/decision-model`

What the decision model has measured (see [the decision model](decision-model.md)). Response, a `DecisionModelView`:

```json
{"configured": true, "model": "moebius-decider", "agreement": 0.95, "min_judgements": 100,
 "kinds": [{"kind": "tool_call", "shadows": 120, "agreement": 0.97, "failed": 2, "dropped": 0, "routed": false}],
 "takeovers": [{"id": 1, "kind": "tool_call", "agreement": 0.97, "count": 120, "status": "open", "at": 1790000000}]}
```

`configured` is false without a `[decision_model]` table, and then `model`, `agreement` and `min_judgements` are null and
`kinds` is empty. Otherwise `model`, `agreement` (the share a takeover needs) and `min_judgements` are the configuration's.
`kinds` has an entry, kinds in alphabetical order, for each kind with a shadow, a dropped request or a merged takeover:
`shadows` (held-out judgements the model was asked about), `agreement` (the share of them where it chose the recorded
choice, null before there is a shadow), `failed` (the shadows where it failed, counted as disagreement), `dropped`
(requests dropped because the queue was full, since the server started) and `routed` (the model decides the kind now).
The kind `localize` is measured by the recall of its files and not by agreement (see
[`localize`](decision-model.md#localize)): its entry has a null `agreement` and besides `measured` (the cards whose
change is known), `recall` (the model's mean recall at ten over them) and `baseline_recall` (the recorded hints'), null
before there is one; it is listed whenever a decision model is configured. `takeovers` are all of them, oldest first.

### `GET /v1/takeovers`

All takeovers, oldest first. Response: an array of `Takeover`: `id`, `kind`, `agreement` (between 0 and 1, when it was
proposed), `count` (the shadows it was measured on), `status` (`open`, `merged`, `dismissed` or `released`) and `at`.
An `open` takeover has an open inbox item of the kind `takeover`.

### `POST /v1/takeovers/{id}/merge`

A person lets the decision model take the kind over. The decision is recorded (kind `takeover`, choice `merge`, decider
`person`), the inbox item is resolved, the status becomes `merged` and the kind is routed to the model from now on and
after a restart. Request: `{"reason": string}` (optional). Response: the `Takeover`. Errors: `404` no such takeover; `409`
the takeover is not `open`.

### `POST /v1/takeovers/{id}/dismiss`

A person refuses an `open` takeover: recorded as choice `dismiss`, the inbox item is resolved and the status becomes
`dismissed`. The kind is proposed again only after `min_judgements` more shadows. Request, response and errors as for
`merge`.

### `POST /v1/takeovers/{id}/release`

A person hands a `merged` kind back to the decider it had: recorded as choice `release`, the status becomes `released`.
Request and response as for `merge`. Errors: `404` no such takeover; `409` the takeover is not `merged`.

### `GET /v1/grillings`

All grillings, the newest first. Response: an array of `Grilling`.

### `GET /v1/grillings/{id}`

One `Grilling`. Errors: `404` no such grilling.

### `POST /v1/grillings/{id}/confirm`

A grilling is a chat with the built-in agent `Grilling-partner` in a repository (`POST /v1/chats`). The agent works in the
chat's worktree and ends by writing exactly one of `.moebius/grilling/issues.json` (a JSON array of `{"title": string,
"body": string, "blocked_by": [number]}`, where `blocked_by` are indexes into the array) or `.moebius/grilling/report.md`
(a research report). It never commits or pushes them. When the chat ends or is stopped, Moebius reads the files before
the worktree is removed and stores a `Grilling`:

| Result | `status` | Inbox item |
|---|---|---|
| `issues.json` is a list of issues, each with a title, whose `blocked_by` are in range, not the issue itself and not in a circle | `proposed` | `grilling_issues`, titled "N issues from a grilling" |
| no `issues.json`, and a `report.md` that is not blank | `reported`, with `report` | `grilling_report` |
| `issues.json` exists but is broken or does not hold together (it decides even when a report is there), or neither file exists | `empty`, with `reason` | none |

The person reads the proposal, may edit, drop and reorder the issues, and confirms with the final list. Request:
`{"issues": [{"title": string, "body": string, "blocked_by": [number]}]}`; `body` and `blocked_by` may be left out, and
`blocked_by` indexes this list, not the proposed one. The server

1. checks the list as it checked the agent's (at least one issue, every title not blank, no `blocked_by` out of range, the
   issue itself or in a circle) and that the status is `proposed`, and sets the status to `confirmed`;
2. makes one card per issue in the repository's `Backlog`, each blocked by the cards of its `blocked_by` (their numbers
   are the card's dependencies, as `card.blocked_by` reads them), blockers first. The cards are `confirmed`
   (`card.confirmed`), so `backlog.luau` moves them on to `Not started` without approval, where `ready.luau` holds each until
   its blockers are done. Without a forge the cards are Moebius cards, authored by `person`. On a GitHub repository each
   issue is opened first through the GitHub API, its body ending in a line `Blocked by #3, #4` for the numbers of the
   issues that block it (the line the poll reads), and its card gets the issue's number, so that the poll finds the card
   existing; its `author` is `person`;
3. records the decision (kind `confirm_grilling`, choice `confirm_grilling`, decider `person`, the context holding the
   grilling's id and the final list as JSON) and resolves the inbox item.

If opening an issue fails the request fails with the error of the forge, the status is `proposed` again and the issues
already opened stay on GitHub; the cards made so far stay. Response: the `Grilling`, `confirmed`. Errors: `404` no such
grilling; `422` the grilling is not `proposed`, or the list is not valid (the message says why).

### `POST /v1/grillings/{id}/dismiss`

A person dismisses the proposal of a `proposed` grilling, or reads the report of a `reported` one and dismisses it: the
inbox item is resolved and the status becomes `dismissed`. Nothing is made. Request: `{}`. Response: the `Grilling`. Errors:
`404` no such grilling; `422` it is not `proposed` or `reported`.

### `DELETE /v1/grillings/{id}`

A person deletes a grilling: its record, its inbox item and its chat, with everything `DELETE /v1/chats/{id}` deletes.
The issues it filed stay on the forge and as cards. The decision is recorded (kind `delete_grilling`, decider `person`,
context: `grilling`, `chat`, `status` and `inbox_items`) and announced as `grilling_deleted` and `chat_deleted`. Request:
`{"reason": string}` (optional). Response: the deleted `Grilling`. Errors: `403` for an agent's run token; `404` no such
grilling; `409` as for a chat.

### `POST /v1/research`

Stores a brief for a research as a draft (see [the Autoresearcher](harness-configuration.md#autoresearch)). Nothing runs
until a person confirms it. Request: `{"repository": id, "goal": string, "measure": string, "better": "lower" |
"higher", "budget": {"kind": "experiments" | "tokens", "amount": number}}`. `measure` is a shell command; it runs in a
worktree of the repository with the environment cleared (`PATH` and `HOME` only), and its result is the last number it
prints on standard output; an exit status other than 0, running longer than 10 minutes, or printing no number means the
measurement failed. `better` says which way the number should move. `budget` limits the research, by experiments or by
tokens; `amount` is a whole number of more than 0. With `"kind": "experiments"` at most `amount` experiments are
measured against the baseline (an experiment is one lever branch measured, the event `experiment_done`; the baseline
does not count). No experiment starts once the count is reached, even in the middle of a round: the levers proposed and
not measured are left as branches and are not offered, and the research is `spent`. The Autoresearcher is told how many
are left. With `"kind": "tokens"` the tokens (input plus output) of the research's runs are compared with `amount`
before each round, and a round may run over. `token_budget` (a number) is still accepted instead of `budget`, as the
kind `tokens`; a request that gives both, or neither, is refused. Response: the `ResearchView`. Errors: `404` no such
repository; `422` a blank `goal` or `measure`, another `better`, another `kind`, an `amount` that is not a whole number
of more than 0, or both `budget` and `token_budget` (or neither).

`ResearchView`: `id`, `repository_id`, `goal`, `measure`, `better`, `budget` (`{"kind": "experiments" | "tokens",
"amount": number}`), `used` (`{"experiments": number, "tokens": number}`: the experiments measured and the tokens its
runs have used so far, whichever the kind is), `limit_reached` (for a `spent` research the kind of the limit it reached,
`"experiments"` or `"tokens"`; null for another status), `token_budget` (kept for older clients: the amount when the
kind is `tokens`, otherwise 0), `status`, `baseline` (the measurement of the repository as it is, or null until it is
measured), `reason` (why the research ended; empty before), `at`, `tokens` (the same as `used.tokens`) and `experiments`
(array of `Experiment`, the first first: one for each lever).

`status` of a research: `draft` (stored, not confirmed), `confirmed` (waits for the trial window or for `start`),
`running`, `done` (a round proposed no more levers), `spent` (it used its budget: as many experiments measured, or its
runs used as many tokens), `stopped` (a person stopped it) or `failed` (the baseline could not be measured, or the
Autoresearcher's run failed; `reason` says which).

`Experiment`: `id`, `research_id`, `branch` (`research/<research id>-<name>`, the lever), `title` (the subject of the
lever's commit), `value` (what the measure printed on the branch; null when the measurement failed), `checks` (true or
false: the repository's own checks on the branch, or null when it has none or the measurement failed), `kept`, `reason`
(text), `change_id` (the change offered for a kept lever, or null) and `at`. A lever is kept when `value` is better than
`baseline` in the direction of `better` by more than 1 % of the baseline, and `checks` is not false.

### `GET /v1/research`

All research, the newest first. Response: an array of `ResearchView`.

### `GET /v1/research/{id}`

One research as the `ResearchView`. Errors: `404` no such research.

### `PUT /v1/research/{id}`

A person, or the Moebius agent, writes the brief of a draft anew before it is confirmed. Request: `{"goal": string,
"measure": string, "better": "lower" | "higher", "budget": {"kind": "experiments" | "tokens", "amount": number},
"reason": string}` (or `token_budget` instead of `budget`); `reason` is optional and is
kept with the decision; the brief is checked as for `POST /v1/research`. The decision is recorded (kind
`edit_research`, decider `person` or `agent:Moebius`; its context has `repository`, `goal`, `measure`, `better`,
`budget_kind` and `budget_amount`, and each of the five as before, `previous_goal`, `previous_budget_kind` and so on) and the event `research_status` is sent with
the status `draft`. A brief that changes nothing records nothing. Response: the `ResearchView`. Errors: `404` no such
research; `409` it is not a draft; `422` as for `POST /v1/research`.

### `POST /v1/research/{id}/confirm`

A person confirms the brief of a draft: the decision is recorded (kind `confirm_brief`, choice `confirm`, decider
`person`) and the status becomes `confirmed`. The research then starts by itself inside `[trial_window]` (see
[configuration](configuration.md)), when no proposal is being tried and no research is running; the oldest confirmed one
goes first. Request: `{}`. Response: the `ResearchView`. Errors: `404` no such research; `409` it is not a draft.

### `POST /v1/research/{id}/start`

Starts a confirmed research now, at any hour. Its status becomes `running` and the research goes on after the response:
the baseline is measured in a fresh worktree of the repository at its default branch, then rounds follow. In a round the
Autoresearcher (a run with the purpose `research` and no card) proposes levers, each a branch
`research/<research id>-<name>` with one commit; an experiment measures each new branch and runs the repository's own
checks; each kept lever is offered as a change without a card, like the work of a chat: published through the
repository's forge, with a `ready_to_merge` inbox item (`data.change`, `data.branch`) and the event `change_offered`. A
person merges it with `POST /v1/changes/{id}/merge`. Before each round the tokens of the research's runs are compared with
the budget; the research is `spent` when they have reached it (a round may run over). It is `done` when a round leaves no
new lever branch. Every status change is the event `research_status` and every experiment the event `experiment_done`.
When the server starts, research that was `running` goes back to `confirmed`; started again it goes on with the baseline
and the experiments it has. Request: `{}`. Response: the `ResearchView`, `running`. Errors: `404` no such research; `409`
the research is not `confirmed`, or another research is running (only one is at a time).

### `POST /v1/research/{id}/stop`

A person stops a research that has not ended: the status becomes `stopped` and a research run that is going is stopped
like `POST /v1/runs/{id}/stop` does. Experiments done stay, and so do the lever branches in the repository's clone. Request:
`{}`. Response: the `ResearchView`. Errors: `404` no such research; `409` it has ended already.

### `DELETE /v1/research/{id}`

A person deletes a research brief in any status. A research that is running is stopped first, and the deletion waits for
its runs and its thread to end (`409` if they do not within a minute: try again). Then it deletes the research, its
experiments, its runs with their events, the changes offered for its kept levers with their inbox items, its lever
branches `research/<id>-*` in the repository's clone and the worktrees of its runs. The `usage` rows of its runs stay,
with `run` null from now on, as for a chat. The decision is recorded (kind `delete_research`, decider `person`, context:
`research`, `repository`, `goal`, `status`, `experiments`, `runs`, `changes`, `branches` and `usage_kept`) and announced
as `research_deleted`. Request: `{"reason": string}` (optional). Response: the deleted research (as it was stored).
Errors: `403` for an agent's run token; `404` no such research; `409` as above.

## Events

`GET /v1/events` sends one JSON object per message: `{"kind": "<kind>", "data": ...}`. These are all the kinds.

| Kind | When | `data` |
|---|---|---|
| `repository_added` | A repository was added. | The `Repository`. |
| `repository_updated` | The logins a repository on a forge trusts were changed through the API. | The `Repository`. |
| `repository_removed` | A repository was removed through the API. | The `Repository`. |
| `project_changed` | A project was made, renamed or made private or public through the API; sent only to those who see its record (see [private projects](#private-projects)). A repository's home project, made with it, comes with `repository_added`. | `{"id", "name", "key", "created_at", "repositories": [id], "files": id or null, "private": bool, "members": [person id]}` |
| `wiki_changed` | A person edited the wiki through the API, or a refresh brought commits pushed to its `origin` from elsewhere. | `{"commit": string (the full id the pages are now read at)}` |
| `wiki_error` | The wiki could not follow its `origin` on the server's refresh (every 30 seconds). | `{"message": string}` |
| `intake_changed` | A request to a project was made, drafted, edited, accepted or declined. It carries no text. | `{"project": id, "request": id, "status": string}` |
| `milestone_changed` | A milestone was made or edited through the API. | `{"id", "name", "target_date", "description", "created_at"}` |
| `milestone_removed` | A milestone was deleted through the API; its cards stay. | `{"milestone": id}` |
| `file_changed` | A file of a repository was saved through the API. | `{"repository": id, "path": string}` |
| `script_changed` | A column's script of a project's settings was saved or removed, through the API. | `{"project": id, "column": string, "path": string (from the root of the harness configuration repository)}` |
| `project_settings_changed` | A file of a repository's settings was saved or removed, through the API. | `{"repository": id, "file": string}` |
| `settings_moved` | The default branch of a repository still has a `.moebius/` folder, which is not read any more; said once for each repository while the server runs. | `{"repository": the repository's name, "message": string}` |
| `card_added` | An issue became a card: created through the API or found on GitHub, GitLab or Forgejo. | The `Card`. |
| `card_updated` | The title or body of a card changed: edited through the API, or changed on GitHub, GitLab or Forgejo; or it was held or released (`held`). | The `Card`. |
| `card_parent_changed` | An issue got a parent, was moved to another or lost it, through the API or on the forge. | `{"card": id, "parent": issue number or null}` |
| `card_labels_changed` | The labels of an issue changed, through the API or on the forge. | `{"card": id, "labels": [text]}` |
| `kind_changed` | A kind of issue was made, or its name or fields changed. | `{"id", "name", "fields": [{"id", "name", "type", "options"}]}` |
| `kind_removed` | A kind of issue was deleted. | `{"kind": id}` |
| `project_kinds_changed` | The kinds a project offers changed. | `{"project": id, "kinds": [id]}` |
| `card_kind_changed` | A card got a kind, lost it or had the values of its fields changed (also when its kind's fields were). | `{"card": id, "kind": text or null, "fields": {name: value}}` |
| `card_assignees_changed` | The people a card is given to changed. | `{"card": id, "assignees": [handle]}` |
| `card_milestones_changed` | The milestones a card is in changed. | `{"card": id, "milestones": [id]}` (in the order of their target dates) |
| `inbox_assignees_changed` | The people an inbox item is given to changed. | `{"item": id, "assignees": [handle]}` |
| `notice_added` | A person was told something (assigned, mentioned, a followed card changed). Sent only to that person. | `{"person": handle, "notice": the Notice}` |
| `notice_read` | A person read one of their notices. Sent only to that person. | `{"person": handle, "notice": the Notice}` |
| `card_blocking_changed` | A blocker was added or removed through the API, or the blockers the forge and the body name changed. | `{"card": id, "blocked_by": [number]}` (every blocker, done or not) |
| `card_links_changed` | The duplicate or related links of an issue changed (sent for each issue they touch). | `{"card": id, "duplicate_of": number or null, "duplicates": [number], "related": [number]}` |
| `card_claimed` | An issue was claimed. Not sent for a claim by the claimer again. | `{"card": id, "by": text}` |
| `card_released` | A claim was released. | `{"card": id}` |
| `card_commented` | A comment was written through the API or read from the forge. | `{"card": id, "comment": the Comment}` |
| `card_closed` | An issue was closed: through the API, by a column script (`moebius.close_issue`) or on the forge. Its card moves to `Done` (`card_moved`). | `{"card": id}` |
| `card_reopened` | A closed issue was reopened, through the API or on the forge. A card in `Done` moves to `Backlog` (`card_moved`). | `{"card": id}` |
| `card_deleted` | A card was deleted through the API, with everything stored for it. A client drops it, and closes what shows it. | `{"card": id}` |
| `card_moved` | A card moved to another column. | `{"card": id, "from": column, "to": column}` |
| `card_waiting` | The reason a card waits changed: set, replaced, or cleared because the card moved or its script did not wait. Not sent when it stays the same. | `{"card": id, "reason": string or null}` |
| `chat_renamed` | A chat was given a name, or its name was cleared, through the API. | `{"run": id, "title": string}` (the title the list of chats shows now) |
| `chat_shared` | The owner of a chat shared it with the organisation or took it back. Sent to everyone, so that a client reads the list of chats again. | `{"run": id, "shared": boolean}` |
| `chat_deleted` | A chat was deleted through the API, with its changes and inbox items, or deleted with its grilling. Read the snapshot again. | `{"run": id}` |
| `change_offered` | A card's change, the work of a chat, a kept lever of a research or a workflow run was offered to a person to merge. | The `Change` (`card_id` is null for a chat's, a lever's or a workflow run's; `run_id` is set for a workflow run's). |
| `change_updated` | A change was published or published again, its checks started or ended, or the checks or review of its pull request changed. | The `Change`. |
| `change_merged` | A change was merged, here or on its forge. | `{"change": id, "commit": sha or null}` |
| `change_closed` | A change's pull or merge request was closed on its forge without a merge. | `{"change": id}` |
| `attempt_judged` | An attempt of a card was judged by its tests and checks (`moebius.check_attempt`). | `{"card": id, "run": id, "result": "passed" \| "failed"}` |
| `attempts_settled` | A card's open attempts were settled (`moebius.settle_attempts`): one picked, or none passed. | `{"card": id, "picked": id or null, "runs": [id]}` |
| `run_started` | A harness process for a run started. | `{"run": id, "card": id or null}` (null for a chat or an agent of a workflow run) |
| `run_event` | Something happened in a run. | `{"run": id, "seq": number, "kind": string, "data": object}` (below) |
| `run_ended` | A run ended. | `{"run": id, "status": "succeeded" \| "failed" \| "stopped"}` |
| `workflow_run_queued` | A workflow run was stored, queued for its first node. | `{"run": id, "workflow": string, "repository": id}` |
| `workflow_run_started` | A workflow run began running. | `{"run": id}` |
| `workflow_run_waiting` | A workflow run waits for a person or another run. | `{"run": id}` |
| `workflow_run_ended` | A workflow run ended. | `{"run": id, "status": "succeeded" \| "failed" \| "cancelled", "error": string or null}` |
| `workflow_run_status` | A workflow run changed status. | `{"run": id, "status": "queued" \| "running" \| "waiting" \| "succeeded" \| "failed" \| "cancelled"}` |
| `workflow_node_started` | A node of a workflow run began an attempt. | `{"run": id, "node": string, "attempt": number}` |
| `workflow_node_ended` | A node of a workflow run ended an attempt. | `{"run": id, "node": string, "attempt": number, "status": string, "error": string or null}` |
| `workflow_node_status` | A node of a workflow run changed status. | `{"run": id, "node": string, "attempt": number, "status": string}` |
| `board_error` | A column script failed; the card stays where it is. | `{"card": id, "message": string}` |
| `forge_error` | Polling a repository on GitHub, GitLab or Forgejo, or refreshing the clone of a repository without a forge, failed; it is tried again at the next poll or refresh (every 30 seconds without a forge). | `{"repository": "owner/repo" (GitHub, Forgejo), the project's path (GitLab) or the repository's name, "message": string}` |
| `harness_error` | An agent's files in the harness configuration repository are broken (it is left out), or refreshing the repository failed (every 30 seconds). | `{"agent": name (left out when the refresh failed), "message": string}` |
| `package_installed` | A package was installed or updated (a plugin is in place and started; the files of the others are proposed). | `{"name": string, "version": string, "kind": "plugin" \| "workflow" \| "rules" \| "skills"}` |
| `package_removed` | A package was removed. | `{"name": string, "kind": string}` |
| `plugin_enabled_changed` | A plugin was turned off or on (`PUT /v1/plugins/{plugin}/enabled`, or `enabled` set through `PUT /v1/settings`). | `{"name": string, "enabled": boolean}` |
| `plugin_enable_refused` | A change of `moebius.toml` that turns off the last launcher that is on was not applied, as nobody confirmed that runs would start unwrapped (a hand edit picked up by a later save); the plugin stays on. | `{"name": string, "enabled": false, "message": string}` |
| `package_update_available` | The nightly check found a newer version that works with this Moebius; it is in the inbox. | `{"name": string, "version": string}` |
| `extension_update_available` | The nightly check found a newer version of an installed Pi package; it is in the inbox as an `extension_update` item (once for each version). | `{"name": string, "version": string}` |
| `package_check_error` | The nightly check could not read a registry, or failed. | `{"registry": string (empty when the check itself failed), "message": string}` |
| `retro_done` | A Retro ended: it was `reported`, or the analyst's run ended and its proposals were stored (`proposed`), or it `failed`. | `{"date": "YYYY-MM-DD", "status": "reported" \| "proposed" \| "failed", "proposals": number}` |
| `retro_error` | The schedule could not start the day's Retro, the trials of a proposal, a research or the replays, or the trials of a proposal broke off; or a workflow file could not be read, and its schedule is left out; it is tried again a minute later. | `{"message": string}` |
| `replayed` | A refused tool call was replayed and its judgement has the outcome. | `{"judgement": id, "run": id, "outcome": string}` |
| `trial_done` | One trial of a proposal ended and was stored. | `{"proposal": id, "card": id or null, "outcome": "positive" \| "negative" \| "skipped" \| "failed", "regression": bool}` |
| `proposal_status` | A proposal's status changed: it is being tried, was tried, was merged or dismissed, or a proposal for Moebius's own configuration was stored, `untried`. | `{"proposal": id, "status": string}` (the statuses above) |
| `takeover_proposed` | The decision model earned a takeover proposal for a kind, which is in the inbox. | `{"takeover": id, "kind": string, "agreement": number}` |
| `takeover_status` | A takeover was merged, dismissed or released. | `{"takeover": id, "status": string}` (the statuses above) |
| `research_status` | A research was stored, its brief was edited, or it was confirmed, started, stopped or ended. | `{"research": id, "status": string}` (the statuses above) |
| `research_deleted` | A research was deleted through the API, with its experiments and their changes. Read the snapshot again. | `{"research": id}` |
| `experiment_done` | An experiment of a research ended and was stored. | `{"research": id, "lever": the lever's branch, "value": number or null, "kept": boolean}` |
| `automation_fired` | An automation fired, by its schedule or `POST /v1/automations/{id}/run`, for the workflow behind it. | `{"automation": id, "firing": id, "workflow": string, "status": "started" \| "skipped" \| "failed", "run": the chat's id or null, "workflow_run": the workflow run's id or null}` |
| `grilling_ended` | A grilling chat ended and its result was stored. | `{"grilling": id, "status": "proposed" \| "reported" \| "empty"}` |
| `grilling_published` | A person had what an open grilling chat left stored while the chat goes on; a later publish replaced it. | `{"grilling": id, "status": "proposed" \| "reported"}` |
| `grilling_deleted` | A grilling was deleted through the API, or its chat was. | `{"grilling": id}` |
| `export_deleted` | A training-set export was deleted through the API. | `{"export": id}` |
| `replay_deleted` | A replay was deleted through the API. | `{"replay": id}` |
| `port_opened` | The server noticed a port that a run, chat or terminal listens on (looked for every 3 seconds while a run or terminal exists). | `{"port": integer, "pid": integer, "process": string, "kind": "run" \| "chat" \| "terminal", "run": id or null}` (as `GET /v1/ports`) |
| `port_closed` | A port that was listed is no longer listened on, or its run or terminal is gone. | The same as `port_opened`, for the port as it was. |
| `lagged` | This client fell too far behind (more than 1024 events) and lost the oldest. | `{"missed": number}` |

A `run_event`'s source is told by its inner `kind`:

- `tool_check`: the answer to a tool-call check, `{"tool", "tool_call_id", "decision": "allow" | "refuse", "reason"}`. See
  the [bridge protocol](bridge-protocol.md).
- `mcp_call`: a call of a tool of an MCP server through the MCP gateway, `{"server", "tool", "tool_call_id", "input",
  "decision", "reason", "result"}`, where `result` is what the harness was given (the refusal for a refused call). See
  the [MCP gateway](bridge-protocol.md#the-mcp-gateway).
- `mcp_error`: an MCP server of the run's agent could not be started or reached, `{"server", "message"}`; its tools are
  left out.
- `launcher_note`: what the launcher plugin said about starting the run's process, `{"note"}` (see
  [launchers](plugin-api.md#launchers)), such as that the run is on a runner and is not wrapped.
- Every other kind is a record Pi-Bolt wrote on its RPC channel, with its `type` as the kind (for example
  `agent_settled`, `tool_execution_end`, `response`) and the whole record as `data`. A line that is not JSON is passed on
  as kind `unparsed` with `{"type": "unparsed", "error", "text"}`. A run on Claude Code (`harness` is `claude-code`)
  has the same kinds, made of Claude Code's `stream-json` records: `agent_start` when it is given a message, an
  `assistant` record as `message_start`, a `message_update` (`text_delta` or `thinking_delta`) per text and thinking
  block, a `tool_execution_start` per `tool_use` block and `message_end`; each `tool_result` as a `tool_execution_end`
  (`{"toolCallId", "toolName", "result": {"content"}, "isError"}`); a `result` as `agent_end` (its text the final answer,
  with `subtype`, `is_error`, `num_turns`, `duration_ms`) and `agent_settled` (`{"aborted": false}`). Any other record is
  kept whole as `claude_<type>` (such as `claude_system`), and a line that is not JSON as `claude_unparsed` with
  `{"text"}`.

A provider that refuses the key, has no quota left or answers an HTTP error does not end the run: Pi-Bolt ends its assistant
message with `"stopReason": "error"` and an `"errorMessage"`, in the `message` of a `message_end` record and in the
`messages` of the `agent_end` that follows (a terminal's record has only the `agent_end`); a Claude Code run ends with an
`agent_end` that has `"is_error": true`. A client shows that as the model's error in the conversation. A run that
Moebius ends in failure has a `moebius_error` event instead.

`seq` is the number the event is stored under, the same as in `GET /v1/runs/{id}/events`. Events of a run are published in
the order of their `seq`. Every event of a run is published and stored: Pi's records, `tool_check`, `mcp_call`, `mcp_error` and `launcher_note` events, `steered` events
(`{"message"}`, a person's steer, with secrets removed), `said` events (`{"message"}`, a person's message to a chat, the first
one included, with secrets removed; stored before the agent's records of its answer) `shell_opened` events (`{"who"}`, a person opened a shell in the worktree; see `GET /v1/runs/{id}/terminal`), `tui_opened` events (`{"who"}`, a person opened Pi-Bolt's terminal interface in a chat; see `GET /v1/chats/{id}/tui`), `settings_changed` events (`{"model", "protocol", "thinking"}`, what a chat runs with in place of its agent's; see `POST /v1/chats/{id}/settings`) and `moebius_error` events (`{"message"}`) that explain why a
run failed. The one exception is a run that was still going when the server stopped: it is failed at the next start with
a `moebius_error` that is stored but not published. A chat that had answered and was only waiting for the next message is
the exception to that: nothing was going in it, so it is `stopped` at the next start and nothing is added to it.
