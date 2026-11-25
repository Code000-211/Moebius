//! The check of a private project (ADR 0018) on every route that names one thing by its id: a card, an inbox item, a
//! change, a run or chat, a repository, a project, a lesson, a judgement, a grilling, a proposal, an export, an
//! automation, a research, a project's request or a workflow run. A thing of a private project that the person asking is no member of is
//! answered `404`, as if it did not exist, before the route runs, so that no route of these can show or change it; an
//! id that is not a plain number is refused the same way, since the router would decode `%31` to `1`. [`check`] runs in
//! `authenticate` after the role, and a test fails for a route with a parameter that is neither guarded here nor named
//! as belonging to no project. The lists (the snapshot, the event stream, the lessons, judgements, replays, proposals,
//! Retros, exports, automations, research and ports) leave such things out in their own routes; what is seen is
//! decided in `app::projects::sight`.

use axum::http::StatusCode;

use super::ApiError;
use crate::app::{App, Result, Sight};
use crate::decide::Actor;
use crate::improve::export;
use crate::store::InboxItem;

/// The kinds of thing a route names by the id after `/v1/<kind>/`, which this check guards.
pub(super) const GUARDED: [&str; 15] = [
    "cards",
    "inbox",
    "changes",
    "runs",
    "chats",
    "repositories",
    "projects",
    "lessons",
    "judgements",
    "grillings",
    "proposals",
    "exports",
    "automations",
    "research",
    "requests",
];

/// The kind of an agent's proposal for the wiki, which a route names after `/v1/wiki/proposals/`.
pub(super) const WIKI_PROPOSALS: &str = "wiki_proposals";

/// The kind of a workflow run, which a route names after `/v1/workflows/runs/`.
pub(super) const WORKFLOW_RUNS: &str = "workflow_runs";

/// The routes of a project that show only its record, which an owner who is no member may use to name its members.
const RECORD_ROUTES: [&str; 2] = ["/v1/projects/{id}", "/v1/projects/{id}/access"];

/// The kind of thing `route` names by id and where the id is among the parts of the path, if it names one this check
/// guards.
pub(super) fn guarded(route: &str) -> Option<(&str, usize)> {
    let parts: Vec<&str> = route.split('/').collect();
    match parts.as_slice() {
        ["", "v1", "wiki", "proposals", param, ..] if param.starts_with('{') => {
            Some((WIKI_PROPOSALS, 4))
        }
        ["", "v1", "workflows", "runs", param, ..] if param.starts_with('{') => {
            Some((WORKFLOW_RUNS, 4))
        }
        ["", "v1", kind, param, ..] if param.starts_with('{') && GUARDED.contains(kind) => {
            Some((kind, 3))
        }
        _ => None,
    }
}

/// Refuses with `404` the request for `path`, served by `route`, when it names a thing that `by` does not see. An id
/// that is not a plain number (such as `%31`, which the router would decode to `1`) is refused too, so that no spelling
/// of an id gets past.
pub(super) fn check(
    app: &App,
    by: &Actor,
    route: &str,
    path: &str,
) -> std::result::Result<(), ApiError> {
    let Some((kind, at)) = guarded(route) else {
        return Ok(());
    };
    let segment = path.split('/').nth(at).unwrap_or_default();
    let hidden = || {
        ApiError(
            StatusCode::NOT_FOUND,
            format!("no {} {segment}", noun(kind)),
        )
    };
    let plain = !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit() || b == b'-');
    let Some(id) = segment.parse::<i64>().ok().filter(|_| plain) else {
        return Err(hidden());
    };
    let sight = app.sight(by)?;
    if sight.hides_nothing() || sees(app, &sight, kind, route, id)? {
        Ok(())
    } else {
        Err(hidden())
    }
}

/// The word for one thing of `kind`, as a 404 says it.
fn noun(kind: &str) -> &str {
    match kind {
        "inbox" => "inbox item",
        "chats" => "chat",
        "repositories" => "repository",
        WORKFLOW_RUNS => "workflow run",
        WIKI_PROPOSALS => "proposal for the wiki",
        other => other.strip_suffix('s').unwrap_or(other),
    }
}

/// Whether `sight` sees the thing of `kind` with `id`. A thing that does not exist is seen, so that its route says so.
pub(super) fn sees(app: &App, sight: &Sight, kind: &str, route: &str, id: i64) -> Result<bool> {
    let store = &app.store;
    match kind {
        "cards" => app.sees_card(sight, id),
        "inbox" => match store.inbox()?.into_iter().find(|item| item.id == id) {
            Some(item) => sees_inbox_item(app, sight, &item),
            None => Ok(true),
        },
        "changes" => sees_change(app, sight, id),
        "runs" | "chats" => app.sees_run_id(sight, id),
        "repositories" => Ok(sight.repository(id)),
        "projects" if RECORD_ROUTES.contains(&route) => Ok(sight.project_record(id)),
        "projects" => Ok(sight.project(id)),
        "lessons" => match store.lesson(id)? {
            Some(lesson) => app.sees_lesson(sight, &lesson),
            None => Ok(true),
        },
        "judgements" => match store.judgement(id)? {
            Some(j) => app.sees_judgement(sight, j.card_id, j.run_id),
            None => Ok(true),
        },
        "grillings" => match store.grilling(id)? {
            Some(g) => Ok(sight.repository(g.repository_id) && app.sees_run_id(sight, g.run_id)?),
            None => Ok(true),
        },
        "proposals" => match store.proposal(id)? {
            Some(p) => Ok(p.repository.is_none_or(|r| sight.repository(r))),
            None => Ok(true),
        },
        "exports" => Ok(export::list(&app.data_dir)
            .into_iter()
            .find(|e| e.id == id)
            .is_none_or(|e| export::seen(&e, sight))),
        "automations" => match store.automation(id)? {
            Some(a) => Ok(a.repository_id.is_none_or(|r| sight.repository(r))
                && a.project_id.is_none_or(|p| sight.project(p))),
            None => Ok(true),
        },
        "research" => match store.research(id)? {
            Some(r) => Ok(sight.repository(r.repository_id)),
            None => Ok(true),
        },
        "requests" => match store.request(id)? {
            Some(request) => Ok(sight.project(request.project_id)),
            None => Ok(true),
        },
        WIKI_PROPOSALS => match store.inbox()?.into_iter().find(|item| item.id == id) {
            Some(item) => sees_inbox_item(app, sight, &item),
            None => Ok(true),
        },
        WORKFLOW_RUNS => match store.workflow_run(id)? {
            Some(run) => Ok(super::workflow_runs::sees_run(sight, &run)),
            None => Ok(true),
        },
        _ => Ok(false),
    }
}

/// Whether `sight` sees the change `id`: by its card, else by its repository.
fn sees_change(app: &App, sight: &Sight, id: i64) -> Result<bool> {
    let Some(change) = app.store.change(id)? else {
        return Ok(true);
    };
    if let Some(card) = change.card_id {
        return app.sees_card(sight, card);
    }
    Ok(change.repository_id.is_none_or(|r| sight.repository(r)))
}

/// Whether `sight` sees an inbox item: by its card, or by the change, grilling, proposal or run it is about.
pub(super) fn sees_inbox_item(app: &App, sight: &Sight, item: &InboxItem) -> Result<bool> {
    if let Some(card) = item.card_id {
        return app.sees_card(sight, card);
    }
    let about = |field: &str| item.data[field].as_i64();
    // An agent's edit of a page of the wiki is seen with the page.
    if item.kind == crate::wiki::WIKI_PROPOSAL
        && let Some(page) = item.data["page"].as_str()
    {
        return Ok(crate::wiki::Seen::from_sight(sight.clone()).page(app, page));
    }
    // A workflow's item names a workflow run as `run`, not a run of an agent.
    if item.kind.starts_with("workflow")
        && let Some(run) = about("run")
    {
        return sees(app, sight, WORKFLOW_RUNS, "", run);
    }
    if let Some(change) = about("change") {
        return sees_change(app, sight, change);
    }
    if let Some(grilling) = about("grilling") {
        return sees(app, sight, "grillings", "", grilling);
    }
    if let Some(proposal) = about("proposal") {
        return sees(app, sight, "proposals", "", proposal);
    }
    if let Some(run) = about("run") {
        return app.sees_run_id(sight, run);
    }
    if let Some(repository) = about("repository") {
        return Ok(sight.repository(repository));
    }
    Ok(true)
}

/// The lists of a digest's section that name a card.
const DIGEST_LISTS: [&str; 8] = [
    "merged", "refused", "reverts", "rounds", "taken", "released", "upkeep", "frozen",
];

/// A digest as `sight` may read it (ADR 0032): the sections of the repositories it sees, in them only the lines about
/// cards it sees, and of what needs a person only the items it sees. One digest is made for the organisation, so what
/// it holds of a private project is cut away for anyone who is no member, and the text is told again from what is left.
pub(crate) fn seen_digest(app: &App, sight: &Sight, mut item: InboxItem) -> Result<InboxItem> {
    if item.kind != crate::app::DIGEST_ITEM || sight.hides_nothing() {
        return Ok(item);
    }
    let open: std::collections::HashMap<i64, InboxItem> = app
        .store
        .inbox()?
        .into_iter()
        .map(|each| (each.id, each))
        .collect();
    if let Some(sections) = item.data["repositories"].as_array_mut() {
        sections.retain(|s| s["id"].as_i64().is_some_and(|id| sight.repository(id)));
        for section in sections {
            for key in DIGEST_LISTS {
                if let Some(lines) = section[key].as_array_mut() {
                    lines.retain(|line| match line["card"].as_i64() {
                        Some(card) => app.sees_card(sight, card).unwrap_or(false),
                        None => true,
                    });
                }
            }
        }
    }
    if let Some(groups) = item.data["needs_a_person"].as_array_mut() {
        for group in groups.iter_mut() {
            if let Some(items) = group["items"].as_array_mut() {
                items.retain(|each| {
                    each["id"]
                        .as_i64()
                        .and_then(|id| open.get(&id))
                        .is_some_and(|found| sees_inbox_item(app, sight, found).unwrap_or(false))
                });
            }
        }
        groups.retain(|group| {
            group["items"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        });
    }
    let text = crate::app::digest_text(&item.data);
    item.data["text"] = serde_json::Value::String(text);
    Ok(item)
}

#[cfg(test)]
mod tests;
