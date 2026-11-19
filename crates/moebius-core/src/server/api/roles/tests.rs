//! The role table against the reference and the routes of the code, and every route called as each role.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{Access, ROUTES};
use crate::server::api::testing::{agent_token, start};
use crate::store::Role;

/// What a row of the table says, as the reference writes it: the role (`none` for no token) and whether the run
/// token of the Moebius agent may use the route (`yes`, `no`, or `—` for a route without a token).
type Said = (String, String);

fn said(access: Access) -> Said {
    match access.role {
        None => ("none".into(), "—".into()),
        Some(role) => (
            role.as_str().into(),
            if access.agents { "yes" } else { "no" }.into(),
        ),
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The rows of the table in the section Roles of the desktop API reference, by `METHOD path`.
fn reference_roles() -> BTreeMap<String, Said> {
    let page = std::fs::read_to_string(root().join("docs/reference/desktop-api.md")).unwrap();
    let section = page
        .split("\n## Roles\n")
        .nth(1)
        .expect("the reference has a section Roles");
    let section = section.split("\n## ").next().unwrap();
    let mut rows = BTreeMap::new();
    for line in section.lines().filter(|l| l.starts_with("| `")) {
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        assert_eq!(cells.len(), 3, "a row of the roles table: {line}");
        let route = cells[0].trim_matches('`').to_string();
        let row = (cells[1].to_string(), cells[2].to_string());
        assert!(
            rows.insert(route.clone(), row).is_none(),
            "{route} is in the roles table twice"
        );
    }
    rows
}

/// The source of the API's modules without their tests.
fn api_sources(dir: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            api_sources(&path, out);
        } else if name.ends_with(".rs") && name != "tests.rs" && name != "testing.rs" {
            let text = std::fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
            out.push(code.to_string());
        }
    }
}

/// Every `METHOD path` that a `.route(path, method(handler)...)` of the API's code serves.
fn code_routes() -> Vec<String> {
    let mut sources = Vec::new();
    api_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src/server/api"),
        &mut sources,
    );
    let mut routes = Vec::new();
    for source in sources {
        let code: String = source.chars().filter(|c| !c.is_whitespace()).collect();
        for (start, _) in code.match_indices(".route(\"") {
            let rest = &code[start + ".route(\"".len()..];
            let (path, rest) = rest.split_once("\",").unwrap();
            // The methods are the calls at the outer level of the rest of the arguments: `get(a).post(b)`.
            let mut depth = 0;
            let mut piece = String::new();
            for c in rest.chars() {
                match c {
                    '(' => depth += 1,
                    ')' if depth == 0 => break,
                    ')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 && c == '.' {
                    piece.clear();
                    continue;
                }
                if depth == 1 && c == '(' {
                    let method = piece.rsplit("::").next().unwrap().to_ascii_uppercase();
                    if method == "LAYER" {
                        continue;
                    }
                    assert!(
                        ["GET", "POST", "PUT", "DELETE", "PATCH"].contains(&method.as_str()),
                        "{path} has the method {method}"
                    );
                    routes.push(format!("{method} {path}"));
                }
                if depth == 0 {
                    piece.push(c);
                }
            }
        }
    }
    routes
}

#[test]
fn the_roles_table_is_the_reference_and_names_every_route() {
    let table: BTreeMap<String, Said> = ROUTES
        .iter()
        .map(|(method, path, access)| (format!("{method} {path}"), said(*access)))
        .collect();
    assert_eq!(table.len(), ROUTES.len(), "a route is in ROUTES twice");
    let reference = reference_roles();
    let routes: std::collections::BTreeSet<&String> =
        table.keys().chain(reference.keys()).collect();
    let differ: Vec<String> = routes
        .into_iter()
        .filter(|route| table.get(*route) != reference.get(*route))
        .map(|route| {
            format!(
                "{route}: the code says {:?}, the reference {:?}",
                table.get(route),
                reference.get(route)
            )
        })
        .collect();
    assert!(differ.is_empty(), "{}", differ.join("\n"));

    let routes = code_routes();
    assert!(routes.len() > 150, "found only {} routes", routes.len());
    let unnamed: Vec<&String> = routes.iter().filter(|r| !table.contains_key(*r)).collect();
    assert!(unnamed.is_empty(), "routes without a role: {unnamed:?}");
    let gone: Vec<&String> = table.keys().filter(|r| !routes.contains(r)).collect();
    assert!(
        gone.is_empty(),
        "roles of routes that do not exist: {gone:?}"
    );
}

#[test]
fn a_route_without_a_role_is_refused() {
    let owner = crate::decide::Actor::first_owner();
    let refused = super::check(&owner, None, &axum::http::Method::GET, "/v1/nothing").unwrap_err();
    assert_eq!(refused.0, axum::http::StatusCode::FORBIDDEN);
    assert!(super::check(&owner, None, &axum::http::Method::HEAD, "/v1/snapshot").is_ok());
}

/// `path` with each of its parameters filled in with something that names nothing.
fn filled(path: &str) -> String {
    path.split('/')
        .map(|part| {
            if part.starts_with('{') {
                "999999"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether the role `role` (an agent's run token for `None`) may use a route of which the reference says `said`.
fn may((needed, agents): &Said, role: Option<Role>) -> bool {
    let order = ["viewer", "member", "owner"];
    let rank = |name: &str| order.iter().position(|r| *r == name).unwrap();
    match role {
        None => agents == "yes",
        Some(role) => rank(role.as_str()) >= rank(needed),
    }
}

/// Every route behind the token, as the reference names it, called by a viewer, a member, an owner and the Moebius
/// agent with parameters that name nothing: `403` comes exactly where the reference's row says the caller may not.
#[tokio::test]
async fn every_route_answers_403_exactly_where_the_reference_says() {
    let (_dir, state, addr) = start().await;
    let store = &state.app.store;
    let mut callers = Vec::new();
    for (i, role) in [Role::Viewer, Role::Member, Role::Owner]
        .into_iter()
        .enumerate()
    {
        let person = store.add_person(&format!("p{i}"), "Someone", role).unwrap();
        let token = format!("{}{:064}", super::super::TOKEN_PREFIX, i);
        store.add_access_token(person.id, "test", &token).unwrap();
        callers.push((Some(role), token, person.id));
    }
    callers.push((None, agent_token(&state), 999999));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap();
    let reference = reference_roles();
    let mut wrong = Vec::new();
    for (role, token, id) in &callers {
        for (route, said) in &reference {
            if said.0 == "none" {
                continue;
            }
            let (method, path) = route.split_once(' ').unwrap();
            // A person may read their own tokens only, which is the handler's check and not the role's.
            let url = if path == "/v1/people/{id}/tokens" {
                format!("http://{addr}/v1/people/{id}/tokens")
            } else {
                format!("http://{addr}{}", filled(path))
            };
            let request = client
                .request(method.parse().unwrap(), &url)
                .bearer_auth(token);
            let request = if matches!(method, "POST" | "PUT") {
                request.json(&serde_json::json!({}))
            } else {
                request
            };
            let response = request.send().await.unwrap();
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let refused = status == 403;
            if refused == may(said, *role) {
                wrong.push(format!(
                    "{} {method} {path}: {status} {text}",
                    role.map_or("agent", Role::as_str)
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "answered against the reference:\n{}",
        wrong.join("\n")
    );
}

#[tokio::test]
async fn a_viewers_issue_made_through_the_api_waits_for_approval() {
    let (_dir, state, addr) = start().await;
    let store = &state.app.store;
    let viewer = store.add_person("vera", "Vera", Role::Viewer).unwrap();
    let token = format!("{}{:064}", super::super::TOKEN_PREFIX, 7);
    store.add_access_token(viewer.id, "test", &token).unwrap();
    let repo = store.repositories().unwrap()[0].id;
    let made = reqwest::Client::new()
        .post(format!("http://{addr}/v1/repositories/{repo}/issues"))
        .bearer_auth(&token)
        .json(&serde_json::json!({"title": "Please add dark mode"}))
        .send()
        .await
        .unwrap();
    assert_eq!(made.status(), 200);
    let made: crate::server::api::CreatedIssue = made.json().await.unwrap();
    assert_eq!(made.card.author, "person:vera");
    assert!(!made.card.confirmed);
    assert!(!state.app.is_trusted(&made.card));
}
