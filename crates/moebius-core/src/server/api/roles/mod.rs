//! The role each route of the desktop API needs (ADR 0018), and the check of it.
//!
//! Every route behind the token has a row in [`ROUTES`]: the least role of the organisation (`viewer`, `member` or
//! `owner`) a person needs for it, and whether the run token of the Moebius agent is let in as well. The routes that
//! need no token (signing in, a forge's deliveries, a workflow's hooks) have a row without a role. [`check`] runs in
//! `authenticate` once the request is known to be someone; a route without a row is refused, so a route nobody named
//! a role for fails closed. The same table is in `docs/reference/desktop-api.md` (section Roles), and the tests hold
//! the two and the routes of the code to each other and call every route as each role.

use axum::http::{Method, StatusCode};

use super::ApiError;
use crate::decide::Actor;
use crate::store::Role;

/// Who may use a route: the least role a person needs, `None` for a route that needs no token, and whether the run
/// token of the Moebius agent may use it too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Access {
    pub role: Option<Role>,
    pub agents: bool,
}

const fn access(role: Role, agents: bool) -> Access {
    Access {
        role: Some(role),
        agents,
    }
}

/// Reads: any person and the Moebius agent.
const VIEWER: Access = access(Role::Viewer, true);
/// What a person of any role does for themselves, never an agent: their tokens, signing in, their app's report.
const VIEWER_PERSON: Access = access(Role::Viewer, false);
/// Issues and cards, chats, runs and merging a change.
const MEMBER: Access = access(Role::Member, true);
/// What a member may do and the Moebius agent may not: deleting a chat or a card, the terminal of a chat, a tunnel.
const MEMBER_PERSON: Access = access(Role::Member, false);
/// The configuration, plugins, rules, agents, repositories and the Retro's work.
const OWNER: Access = access(Role::Owner, true);
/// What only an owner and never an agent may do: secrets, logins, shells, people.
const OWNER_PERSON: Access = access(Role::Owner, false);
/// A route that needs no token and checks its own credential.
const PUBLIC: Access = Access {
    role: None,
    agents: false,
};

/// Every route: its method, its path as the router names it, and who may use it.
pub(super) const ROUTES: &[(&str, &str, Access)] = &[
    ("GET", "/v1/health", VIEWER),
    // The session and people.
    ("POST", "/v1/session", PUBLIC),
    ("GET", "/v1/session", PUBLIC),
    ("DELETE", "/v1/session", PUBLIC),
    ("POST", "/v1/session/codes", VIEWER_PERSON),
    ("GET", "/v1/people", VIEWER_PERSON),
    ("POST", "/v1/people", OWNER_PERSON),
    ("PATCH", "/v1/people/{id}", OWNER_PERSON),
    ("GET", "/v1/people/{id}/tokens", VIEWER_PERSON),
    ("GET", "/v1/tokens", VIEWER_PERSON),
    ("POST", "/v1/tokens", VIEWER_PERSON),
    ("DELETE", "/v1/tokens/{id}", VIEWER_PERSON),
    ("GET", "/v1/places", VIEWER_PERSON),
    ("POST", "/v1/favourites", VIEWER_PERSON),
    ("PUT", "/v1/favourites/order", VIEWER_PERSON),
    ("DELETE", "/v1/favourites/{id}", VIEWER_PERSON),
    ("POST", "/v1/recent", VIEWER_PERSON),
    ("DELETE", "/v1/recent/{id}", VIEWER_PERSON),
    ("GET", "/v1/logins", VIEWER_PERSON),
    ("PUT", "/v1/logins", VIEWER_PERSON),
    ("GET", "/v1/preferences", VIEWER_PERSON),
    ("PUT", "/v1/preferences", VIEWER_PERSON),
    ("GET", "/v1/server", VIEWER),
    ("GET", "/v1/client", VIEWER),
    ("PUT", "/v1/client", VIEWER_PERSON),
    // Boards, issues and cards.
    ("GET", "/v1/snapshot", VIEWER),
    ("GET", "/v1/events", VIEWER),
    ("GET", "/v1/projects", VIEWER),
    ("POST", "/v1/projects", OWNER),
    ("GET", "/v1/projects/{id}", VIEWER),
    ("PATCH", "/v1/projects/{id}", OWNER),
    // The wiki: everyone reads it, a member writes it, and an agent's edit is a proposal, a branch that waits in the inbox for a person.
    ("GET", "/v1/wiki/pages", VIEWER),
    ("GET", "/v1/wiki/pages/{*path}", VIEWER),
    ("PUT", "/v1/wiki/pages/{*path}", MEMBER_PERSON),
    ("DELETE", "/v1/wiki/pages/{*path}", MEMBER_PERSON),
    ("POST", "/v1/wiki/moves", MEMBER_PERSON),
    ("PUT", "/v1/wiki/labels/{*path}", MEMBER_PERSON),
    ("GET", "/v1/wiki/history/{*path}", VIEWER),
    ("GET", "/v1/wiki/search", VIEWER),
    ("GET", "/v1/wiki/links/{*path}", VIEWER),
    ("GET", "/v1/wiki/cards/{key}", VIEWER),
    ("GET", "/v1/wiki/status", VIEWER),
    ("PUT", "/v1/wiki/repository", OWNER_PERSON),
    ("POST", "/v1/wiki/refresh", MEMBER),
    ("POST", "/v1/wiki/proposals", MEMBER),
    ("POST", "/v1/wiki/proposals/{id}/merge", MEMBER_PERSON),
    ("POST", "/v1/wiki/proposals/{id}/dismiss", MEMBER_PERSON),
    ("PUT", "/v1/projects/{id}/intake", OWNER_PERSON),
    ("GET", "/v1/projects/{id}/requests", VIEWER_PERSON),
    ("POST", "/v1/projects/{id}/requests", VIEWER_PERSON),
    ("GET", "/v1/requests/{id}", VIEWER_PERSON),
    ("PUT", "/v1/requests/{id}", MEMBER_PERSON),
    ("POST", "/v1/requests/{id}/accept", MEMBER_PERSON),
    ("POST", "/v1/requests/{id}/decline", MEMBER_PERSON),
    ("POST", "/v1/wiki/from-template", MEMBER_PERSON),
    // Templates for issues and pages: everyone reads them, a member edits them.
    ("GET", "/v1/templates", VIEWER),
    ("PUT", "/v1/templates/issues/{kind}", MEMBER_PERSON),
    ("DELETE", "/v1/templates/issues/{kind}", MEMBER_PERSON),
    ("PUT", "/v1/templates/pages/{name}", MEMBER_PERSON),
    ("DELETE", "/v1/templates/pages/{name}", MEMBER_PERSON),
    ("GET", "/v1/projects/{id}/cycles", VIEWER),
    ("POST", "/v1/projects/{id}/cycles", MEMBER_PERSON),
    ("GET", "/v1/cycles/{id}", VIEWER),
    ("PATCH", "/v1/cycles/{id}", MEMBER_PERSON),
    ("DELETE", "/v1/cycles/{id}", MEMBER_PERSON),
    ("POST", "/v1/cycles/{id}/close", MEMBER_PERSON),
    ("POST", "/v1/projects/{id}/import/jira", OWNER_PERSON),
    ("GET", "/v1/cards/{id}/cycle", VIEWER),
    ("PUT", "/v1/cards/{id}/cycle", MEMBER_PERSON),
    ("GET", "/v1/milestones", VIEWER),
    ("POST", "/v1/milestones", MEMBER_PERSON),
    ("GET", "/v1/milestones/{id}", VIEWER),
    ("PATCH", "/v1/milestones/{id}", MEMBER_PERSON),
    ("DELETE", "/v1/milestones/{id}", MEMBER_PERSON),
    ("GET", "/v1/cards/{id}/milestones", VIEWER),
    ("PUT", "/v1/cards/{id}/milestones", MEMBER_PERSON),
    ("GET", "/v1/views", VIEWER_PERSON),
    ("POST", "/v1/views", MEMBER_PERSON),
    ("GET", "/v1/views/{id}", VIEWER_PERSON),
    ("PATCH", "/v1/views/{id}", MEMBER_PERSON),
    ("DELETE", "/v1/views/{id}", MEMBER_PERSON),
    ("GET", "/v1/views/{id}/cards", VIEWER_PERSON),
    ("GET", "/v1/projects/{id}/scripts", VIEWER),
    ("GET", "/v1/projects/{id}/scripts/{column}", VIEWER),
    ("PUT", "/v1/projects/{id}/scripts/{column}", OWNER),
    ("DELETE", "/v1/projects/{id}/scripts/{column}", OWNER),
    ("GET", "/v1/projects/{id}/columns", VIEWER),
    ("PUT", "/v1/projects/{id}/columns", OWNER),
    ("DELETE", "/v1/projects/{id}/columns", OWNER),
    ("PUT", "/v1/projects/{id}/access", OWNER_PERSON),
    ("POST", "/v1/projects/{id}/repositories", OWNER),
    (
        "DELETE",
        "/v1/projects/{id}/repositories/{repository}",
        OWNER,
    ),
    ("POST", "/v1/repositories", OWNER),
    ("PUT", "/v1/repositories/{id}", OWNER),
    ("GET", "/v1/repositories/{id}/upkeep", VIEWER),
    ("PUT", "/v1/repositories/{id}/upkeep", OWNER),
    ("GET", "/v1/repositories/{id}/flow", VIEWER),
    ("PUT", "/v1/repositories/{id}/flow", OWNER_PERSON),
    ("GET", "/v1/projects/{id}/backlog", VIEWER),
    ("PUT", "/v1/projects/{id}/backlog", OWNER),
    ("DELETE", "/v1/projects/{id}/backlog", OWNER),
    ("POST", "/v1/projects/{id}/backlog/sync", MEMBER),
    ("GET", "/v1/repositories/{id}/lane", VIEWER),
    ("PUT", "/v1/repositories/{id}/lane", OWNER_PERSON),
    ("GET", "/v1/lanes", VIEWER),
    ("POST", "/v1/lanes/{name}/resume", OWNER_PERSON),
    ("GET", "/v1/cards/{id}/lane", VIEWER),
    ("POST", "/v1/cards/{id}/lane", MEMBER),
    ("POST", "/v1/digest", OWNER),
    ("POST", "/v1/digest/{id}/dismiss", MEMBER),
    ("DELETE", "/v1/repositories/{id}", OWNER),
    ("POST", "/v1/repositories/{id}/freeze", OWNER),
    ("POST", "/v1/repositories/{id}/unfreeze", OWNER_PERSON),
    ("POST", "/v1/repositories/{id}/issues", VIEWER),
    ("GET", "/v1/repositories/{id}/issues", VIEWER),
    ("GET", "/v1/repositories/{id}/settings", VIEWER),
    ("PUT", "/v1/repositories/{id}/settings/{file}", OWNER),
    ("DELETE", "/v1/repositories/{id}/settings/{file}", OWNER),
    ("GET", "/v1/repositories/{id}/plugin-settings", VIEWER),
    (
        "PUT",
        "/v1/repositories/{id}/plugin-settings/{plugin}",
        OWNER,
    ),
    ("GET", "/v1/repositories/{id}/tree", VIEWER),
    ("GET", "/v1/repositories/{id}/files", VIEWER),
    ("GET", "/v1/repositories/{id}/search", VIEWER),
    ("GET", "/v1/repositories/{id}/files/{*path}", VIEWER),
    ("PUT", "/v1/repositories/{id}/files/{*path}", MEMBER),
    ("GET", "/v1/cards/by-key/{key}", VIEWER),
    ("GET", "/v1/cards/{id}", VIEWER),
    ("PUT", "/v1/cards/{id}", MEMBER),
    ("DELETE", "/v1/cards/{id}", MEMBER_PERSON),
    ("PUT", "/v1/cards/{id}/parent", MEMBER),
    ("PUT", "/v1/cards/{id}/labels", MEMBER),
    ("PUT", "/v1/cards/{id}/repositories", MEMBER),
    ("PUT", "/v1/cards/{id}/assignees", MEMBER_PERSON),
    ("PUT", "/v1/inbox/{id}/assignees", MEMBER_PERSON),
    ("PUT", "/v1/cards/{id}/priority", MEMBER_PERSON),
    ("PUT", "/v1/cards/{id}/due", MEMBER_PERSON),
    ("PUT", "/v1/cards/{id}/estimate", MEMBER_PERSON),
    ("GET", "/v1/cards/{id}/estimate/proposal", VIEWER),
    ("PUT", "/v1/cards/{id}/assignees/{handle}", MEMBER_PERSON),
    ("DELETE", "/v1/cards/{id}/assignees/{handle}", MEMBER_PERSON),
    ("PUT", "/v1/inbox/{id}/assignees/{handle}", MEMBER_PERSON),
    ("DELETE", "/v1/inbox/{id}/assignees/{handle}", MEMBER_PERSON),
    ("GET", "/v1/notices", VIEWER_PERSON),
    ("POST", "/v1/notices/{id}/read", VIEWER_PERSON),
    ("GET", "/v1/cards/{id}/followers", VIEWER),
    ("PUT", "/v1/cards/{id}/follow", VIEWER_PERSON),
    ("GET", "/v1/kinds", VIEWER),
    ("POST", "/v1/kinds", OWNER_PERSON),
    ("PUT", "/v1/kinds/{id}", OWNER_PERSON),
    ("DELETE", "/v1/kinds/{id}", OWNER_PERSON),
    ("PUT", "/v1/projects/{id}/kinds", OWNER),
    ("PUT", "/v1/cards/{id}/kind", MEMBER),
    ("PUT", "/v1/cards/{id}/fields", MEMBER),
    ("GET", "/v1/issues", VIEWER),
    ("POST", "/v1/cards/{id}/blockers", MEMBER),
    ("DELETE", "/v1/cards/{id}/blockers/{number}", MEMBER),
    ("PUT", "/v1/cards/{id}/duplicate", MEMBER),
    ("DELETE", "/v1/cards/{id}/duplicate", MEMBER),
    ("POST", "/v1/cards/{id}/related", MEMBER),
    ("DELETE", "/v1/cards/{id}/related/{number}", MEMBER),
    ("GET", "/v1/cards/{id}/similar", VIEWER),
    ("POST", "/v1/cards/{id}/claim", MEMBER),
    ("DELETE", "/v1/cards/{id}/claim", MEMBER),
    ("POST", "/v1/cards/{id}/close", MEMBER),
    ("POST", "/v1/cards/{id}/reopen", MEMBER),
    ("GET", "/v1/cards/{id}/comments", VIEWER),
    ("POST", "/v1/cards/{id}/comments", VIEWER),
    ("POST", "/v1/cards/{id}/approve", MEMBER),
    ("POST", "/v1/cards/{id}/block", MEMBER),
    ("POST", "/v1/cards/{id}/unblock", MEMBER),
    ("POST", "/v1/cards/{id}/retry", MEMBER),
    ("GET", "/v1/cards/{id}/runs", VIEWER),
    ("GET", "/v1/cards/{id}/history", VIEWER),
    ("POST", "/v1/changes/{id}/merge", MEMBER),
    ("POST", "/v1/changes/{id}/measurement", MEMBER_PERSON),
    ("POST", "/v1/changes/{id}/reject", MEMBER),
    ("POST", "/v1/changes/{id}/review", MEMBER_PERSON),
    ("POST", "/v1/changes/{id}/review/end", MEMBER_PERSON),
    ("POST", "/v1/cards/{id}/done", MEMBER_PERSON),
    ("POST", "/v1/cards/{id}/release-taken", MEMBER_PERSON),
    ("GET", "/v1/changes/{id}/diff", VIEWER),
    // Runs, chats and terminals.
    ("GET", "/v1/runs/finished", VIEWER),
    ("GET", "/v1/runs/{id}", VIEWER),
    ("GET", "/v1/runs/{id}/events", VIEWER),
    ("GET", "/v1/runs/{id}/diff", VIEWER),
    ("POST", "/v1/runs/pause", MEMBER_PERSON),
    ("POST", "/v1/runs/resume", MEMBER_PERSON),
    ("POST", "/v1/runs/{id}/steer", MEMBER),
    ("POST", "/v1/runs/{id}/stop", MEMBER),
    ("GET", "/v1/runs/{id}/terminal", OWNER_PERSON),
    ("GET", "/v1/chats", VIEWER),
    ("POST", "/v1/chats", MEMBER),
    ("DELETE", "/v1/chats/{id}", MEMBER_PERSON),
    ("POST", "/v1/chats/{id}/messages", MEMBER),
    ("POST", "/v1/chats/{id}/offer", MEMBER),
    ("POST", "/v1/chats/{id}/publish", MEMBER),
    ("POST", "/v1/chats/{id}/end", MEMBER),
    ("POST", "/v1/chats/{id}/seen", MEMBER),
    ("POST", "/v1/chats/{id}/settings", MEMBER),
    ("POST", "/v1/chats/{id}/title", MEMBER),
    ("POST", "/v1/chats/{id}/share", MEMBER_PERSON),
    ("GET", "/v1/chats/{id}/tui", MEMBER_PERSON),
    ("GET", "/v1/grillings", VIEWER),
    ("GET", "/v1/grillings/{id}", VIEWER),
    ("DELETE", "/v1/grillings/{id}", MEMBER_PERSON),
    ("POST", "/v1/grillings/{id}/confirm", MEMBER),
    ("POST", "/v1/grillings/{id}/dismiss", MEMBER),
    ("GET", "/v1/terminal", OWNER_PERSON),
    ("GET", "/v1/repositories/{id}/terminal", OWNER_PERSON),
    ("GET", "/v1/ports", VIEWER),
    ("GET", "/v1/ports/{port}/tunnel", MEMBER_PERSON),
    ("POST", "/v1/uploads", MEMBER),
    // The harness configuration repository.
    ("GET", "/v1/runners", VIEWER),
    ("GET", "/v1/agents", VIEWER),
    ("GET", "/v1/agents/{name}", VIEWER),
    ("PUT", "/v1/agents/{name}", OWNER),
    ("DELETE", "/v1/agents/{name}", OWNER),
    ("POST", "/v1/harness/setup", OWNER_PERSON),
    ("GET", "/v1/skills", VIEWER),
    ("GET", "/v1/skills/{name}", VIEWER),
    ("PUT", "/v1/skills/{name}", OWNER),
    ("DELETE", "/v1/skills/{name}", OWNER),
    ("GET", "/v1/skills/{name}/files/{*path}", VIEWER),
    ("PUT", "/v1/skills/{name}/files/{*path}", OWNER),
    ("DELETE", "/v1/skills/{name}/files/{*path}", OWNER),
    ("GET", "/v1/mcp-servers", VIEWER),
    ("GET", "/v1/mcp-servers/{name}", VIEWER),
    ("PUT", "/v1/mcp-servers/{name}", OWNER),
    ("DELETE", "/v1/mcp-servers/{name}", OWNER),
    ("GET", "/v1/rules", VIEWER),
    ("PUT", "/v1/rules", OWNER),
    ("GET", "/v1/projects/{id}/workflows", VIEWER),
    ("GET", "/v1/projects/{id}/workflows/{name}", VIEWER),
    ("PUT", "/v1/projects/{id}/workflows/{name}", OWNER),
    ("DELETE", "/v1/projects/{id}/workflows/{name}", OWNER),
    (
        "GET",
        "/v1/projects/{id}/workflows/{name}/run-inputs",
        VIEWER,
    ),
    ("POST", "/v1/projects/{id}/workflows/{name}/runs", MEMBER),
    ("GET", "/v1/workflows", VIEWER),
    ("GET", "/v1/workflows/{name}", VIEWER),
    ("PUT", "/v1/workflows/{name}", OWNER),
    ("DELETE", "/v1/workflows/{name}", OWNER),
    ("GET", "/v1/workflows/{name}/run-inputs", VIEWER),
    ("GET", "/v1/workflow-runs", VIEWER),
    ("GET", "/v1/workflow-runs/{id}", VIEWER),
    ("POST", "/v1/workflows/{name}/runs", MEMBER),
    ("POST", "/v1/workflows/runs/{id}/cancel", MEMBER),
    ("POST", "/v1/workflows/runs/{run_id}/resume", MEMBER),
    ("GET", "/v1/automations", VIEWER),
    ("POST", "/v1/automations", OWNER),
    ("GET", "/v1/automations/{id}", VIEWER),
    ("PUT", "/v1/automations/{id}", OWNER),
    ("DELETE", "/v1/automations/{id}", OWNER),
    ("POST", "/v1/automations/{id}/run", OWNER),
    ("GET", "/v1/automations/{id}/firings", VIEWER),
    // Extensions and packages.
    ("GET", "/v1/extensions/search", VIEWER),
    ("GET", "/v1/extensions/info", VIEWER),
    ("GET", "/v1/extensions/packages", VIEWER),
    ("POST", "/v1/extensions/packages", OWNER),
    ("POST", "/v1/extensions/packages/{name}/update", OWNER),
    ("DELETE", "/v1/extensions/packages/{name}", OWNER),
    ("POST", "/v1/extensions/update", OWNER),
    ("GET", "/v1/packages/search", VIEWER),
    ("GET", "/v1/packages", VIEWER),
    ("POST", "/v1/packages", OWNER),
    ("GET", "/v1/packages/{name}", VIEWER),
    ("DELETE", "/v1/packages/{name}", OWNER_PERSON),
    ("POST", "/v1/packages/{name}/update", OWNER_PERSON),
    // Improving Moebius.
    ("GET", "/v1/usage", VIEWER),
    ("GET", "/v1/status", VIEWER_PERSON),
    ("GET", "/v1/analytics", VIEWER),
    ("GET", "/v1/judgements", VIEWER),
    ("GET", "/v1/judgements/kinds", VIEWER),
    ("POST", "/v1/judgements/{id}/replay", OWNER),
    ("GET", "/v1/replays", VIEWER),
    ("DELETE", "/v1/replays/{id}", OWNER_PERSON),
    ("GET", "/v1/exports", VIEWER),
    ("POST", "/v1/exports", OWNER),
    ("DELETE", "/v1/exports/{id}", OWNER_PERSON),
    ("GET", "/v1/exports/{id}/{file}", VIEWER),
    ("GET", "/v1/retros", VIEWER),
    ("POST", "/v1/retros", OWNER),
    ("GET", "/v1/retros/{date}", VIEWER),
    ("POST", "/v1/retros/{date}/retry", OWNER),
    ("GET", "/v1/proposals", VIEWER),
    ("POST", "/v1/proposals/{id}/trial", OWNER),
    ("POST", "/v1/proposals/{id}/merge", OWNER),
    ("POST", "/v1/proposals/{id}/dismiss", OWNER),
    ("GET", "/v1/decision-model", VIEWER),
    ("GET", "/v1/takeovers", VIEWER),
    ("POST", "/v1/takeovers/{id}/merge", OWNER),
    ("POST", "/v1/takeovers/{id}/dismiss", OWNER),
    ("POST", "/v1/takeovers/{id}/release", OWNER),
    ("GET", "/v1/research", VIEWER),
    ("POST", "/v1/research", OWNER),
    ("GET", "/v1/research/{id}", VIEWER),
    ("PUT", "/v1/research/{id}", OWNER),
    ("DELETE", "/v1/research/{id}", OWNER_PERSON),
    ("POST", "/v1/research/{id}/confirm", OWNER),
    ("POST", "/v1/research/{id}/start", OWNER),
    ("POST", "/v1/research/{id}/stop", OWNER),
    ("GET", "/v1/lessons", VIEWER),
    ("GET", "/v1/lessons/{id}", VIEWER),
    ("PUT", "/v1/lessons/{id}", OWNER),
    ("DELETE", "/v1/lessons/{id}", OWNER),
    // The server's settings.
    ("GET", "/v1/models", VIEWER),
    ("GET", "/v1/models/available", VIEWER),
    ("GET", "/v1/models/providers", VIEWER),
    ("GET", "/v1/models/upstreams/{place}/login", VIEWER),
    ("POST", "/v1/models/upstreams/{place}/login", OWNER_PERSON),
    (
        "POST",
        "/v1/models/upstreams/{place}/login/code",
        OWNER_PERSON,
    ),
    ("GET", "/v1/start", VIEWER),
    ("GET", "/v1/plugins", VIEWER),
    ("GET", "/v1/plugins/settings", VIEWER),
    ("PUT", "/v1/plugins/{plugin}/enabled", OWNER_PERSON),
    ("PUT", "/v1/plugins/{plugin}/settings", OWNER),
    ("GET", "/v1/plugins/{plugin}/settings-ui", VIEWER),
    ("GET", "/v1/settings", VIEWER),
    ("PUT", "/v1/settings", OWNER),
    ("PUT", "/v1/secrets/{name}", OWNER_PERSON),
    ("DELETE", "/v1/secrets/{name}", OWNER),
    ("POST", "/v1/server/restart", OWNER),
    // The outgoing webhooks, the forges' webhooks and a workflow's hooks.
    ("GET", "/v1/outgoing-webhooks", OWNER_PERSON),
    ("POST", "/v1/outgoing-webhooks", OWNER_PERSON),
    ("DELETE", "/v1/outgoing-webhooks/{id}", OWNER_PERSON),
    ("PUT", "/v1/repositories/{id}/private", OWNER_PERSON),
    ("POST", "/v1/webhooks/{forge}", PUBLIC),
    ("GET", "/v1/webhooks", OWNER_PERSON),
    ("POST", "/v1/webhooks/{forge}/secret", OWNER_PERSON),
    ("POST", "/api/hooks/projects/{project}/{path}", PUBLIC),
    (
        "POST",
        "/api/hooks/projects/{project}/{repository}/{path}",
        PUBLIC,
    ),
    ("POST", "/api/hooks/{path}", PUBLIC),
    ("POST", "/api/hooks/{repository}/{path}", PUBLIC),
];

/// Who the route `path` (as the router names it) needs for `method`, if it is in [`ROUTES`]. `HEAD` is a `GET`.
pub(super) fn access_of(method: &Method, path: &str) -> Option<Access> {
    let method = if method == Method::HEAD {
        "GET"
    } else {
        method.as_str()
    };
    ROUTES
        .iter()
        .find(|(m, p, _)| *m == method && *p == path)
        .map(|(_, _, access)| *access)
}

/// The refusal of what a role may not do, saying which role it needs.
fn needs(role: Role) -> ApiError {
    let message = match role {
        Role::Owner => "only an owner can do this",
        Role::Member => "only a member or an owner can do this",
        Role::Viewer => "only a person can do this",
    };
    ApiError(StatusCode::FORBIDDEN, message.into())
}

/// Lets `actor`, a person with the role `role` or an agent (whose `role` is `None`), use the route `path` with
/// `method`, or refuses it with a `403` that says why. A route the table does not name is refused.
pub(super) fn check(
    actor: &Actor,
    role: Option<Role>,
    method: &Method,
    path: &str,
) -> Result<(), ApiError> {
    let Some(access) = access_of(method, path) else {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "no role is named for this route".into(),
        ));
    };
    let Some(needed) = access.role else {
        return Ok(());
    };
    if !actor.is_person() {
        return if access.agents {
            Ok(())
        } else {
            Err(needs(Role::Viewer))
        };
    }
    // The server token acts as the first owner and is signed in as nobody.
    let role = role.unwrap_or(Role::Owner);
    if rank(role) >= rank(needed) {
        Ok(())
    } else {
        Err(needs(needed))
    }
}

/// How much a role may do: a viewer least, an owner most.
fn rank(role: Role) -> u8 {
    match role {
        Role::Viewer => 0,
        Role::Member => 1,
        Role::Owner => 2,
    }
}

#[cfg(test)]
mod tests;
