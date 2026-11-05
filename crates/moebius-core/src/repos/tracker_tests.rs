//! Tests of what the forge clients and their polls do for an issue tracker: closing, labels, comments, sub-issues and
//! blocking, written to and read from a fake forge that answers each request from a table and records it.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::IntoResponse;
use serde_json::{Value, json};

use super::forge::Remembered;
use super::forgejo::Forgejo;
use super::github::GitHub;
use super::gitlab::GitLab;
use super::poll::Polled;

/// A request the fake forge saw: its method, its path and query, and its JSON body (null when it had none).
type Seen = (String, String, Value);

/// Serves `answers` (method, path without its query, JSON to answer with) on a port of its own and records every
/// request. A path with no answer is a 404. An answer that is `{"__pages": [...]}` is a list in pages: the request's
/// `page` (the first by default) is answered with that page and a `Link` header to the next.
async fn serve(
    answers: Vec<(&'static str, &'static str, Value)>,
) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
    let recorded = seen.clone();
    let app = Router::new().fallback(
        move |method: Method, uri: Uri, headers: HeaderMap, body: String| {
            let (answers, recorded) = (answers.clone(), recorded.clone());
            async move {
                let sent = serde_json::from_str(&body).unwrap_or(Value::Null);
                recorded
                    .lock()
                    .unwrap()
                    .push((method.to_string(), uri.to_string(), sent));
                match answers
                    .iter()
                    .find(|(m, p, _)| *m == method.as_str() && *p == uri.path())
                {
                    Some((_, _, Value::Null)) => StatusCode::NO_CONTENT.into_response(),
                    Some((_, path, answer)) if answer.get("__pages").is_some() => {
                        let pages = answer["__pages"].as_array().unwrap();
                        let page: usize = uri
                            .query()
                            .and_then(|q| q.split('&').find_map(|p| p.strip_prefix("page=")))
                            .and_then(|p| p.parse().ok())
                            .unwrap_or(1);
                        let host = headers.get("host").unwrap().to_str().unwrap();
                        let mut response = axum::Json(pages[page - 1].clone()).into_response();
                        if page < pages.len() {
                            response.headers_mut().insert(
                                "link",
                                format!("<http://{host}{path}?page={}>; rel=\"next\"", page + 1)
                                    .parse()
                                    .unwrap(),
                            );
                        }
                        response
                    }
                    Some((_, _, answer)) => axum::Json(answer.clone()).into_response(),
                    None => StatusCode::NOT_FOUND.into_response(),
                }
            }
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, seen)
}

/// The requests seen so far, as "METHOD path" with their bodies, leaving none behind.
fn take(seen: &Arc<Mutex<Vec<Seen>>>) -> Vec<(String, Value)> {
    std::mem::take(&mut *seen.lock().unwrap())
        .into_iter()
        .map(|(method, uri, body)| (format!("{method} {uri}"), body))
        .collect()
}

fn github_answers() -> Vec<(&'static str, &'static str, Value)> {
    let expires = time::OffsetDateTime::now_utc() + std::time::Duration::from_secs(3600);
    let expires = expires
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    vec![
        (
            "GET",
            "/repos/acme/widgets/installation",
            json!({ "id": 77 }),
        ),
        (
            "POST",
            "/app/installations/77/access_tokens",
            json!({ "token": "ghs_token", "expires_at": expires }),
        ),
    ]
}

fn github_client(url: &str) -> GitHub {
    GitHub::new(
        url,
        4242,
        include_bytes!("../../testdata/github-app-test-key.pem"),
    )
    .unwrap()
}

/// What the requests were, without the calls that fetch the installation's token.
fn without_token(mut requests: Vec<(String, Value)>) -> Vec<(String, Value)> {
    requests.retain(|(request, _)| !request.contains("installation"));
    requests
}

#[tokio::test]
async fn github_closes_labels_comments_nests_and_blocks_issues() {
    let mut answers = github_answers();
    answers.extend([
        ("PATCH", "/repos/acme/widgets/issues/3", json!({ "number": 3 })),
        ("PUT", "/repos/acme/widgets/issues/3/labels", json!([])),
        ("POST", "/repos/acme/widgets/issues/3/comments", json!({ "id": 9 })),
        (
            "GET",
            "/repos/acme/widgets/issues/3/comments",
            json!([{ "id": 9, "user": { "login": "leon" }, "body": "Hi", "created_at": "2026-01-02T03:04:05Z" }]),
        ),
        ("GET", "/repos/acme/widgets/issues/3", json!({ "id": 103, "number": 3 })),
        ("GET", "/repos/acme/widgets/issues/4", json!({ "id": 104, "number": 4 })),
        ("POST", "/repos/acme/widgets/issues/3/sub_issues", json!({})),
        ("DELETE", "/repos/acme/widgets/issues/3/sub_issue", json!({})),
        ("POST", "/repos/acme/widgets/issues/4/dependencies/blocked_by", json!({})),
        ("DELETE", "/repos/acme/widgets/issues/4/dependencies/blocked_by/103", json!({})),
    ]);
    let (url, seen) = serve(answers).await;
    let github = github_client(&url);
    let name = "acme/widgets";

    github.set_issue_state(name, 3, false).await.unwrap();
    github.set_issue_state(name, 3, true).await.unwrap();
    github
        .set_issue_labels(name, 3, &["bug".into(), "wayfinder:map".into()])
        .await
        .unwrap();
    assert_eq!(github.create_issue_comment(name, 3, "Hi").await.unwrap(), 9);
    let comments = github.issue_comments(name, 3).await.unwrap();
    assert_eq!(
        (comments[0].id, comments[0].user.login.as_str()),
        (9, "leon")
    );
    github
        .set_issue_parent(name, 4, Some(3), None)
        .await
        .unwrap();
    github
        .set_issue_parent(name, 4, None, Some(3))
        .await
        .unwrap();
    github.set_issue_parent(name, 4, None, None).await.unwrap();
    github.add_blocker(name, 4, 3).await.unwrap();
    github.remove_blocker(name, 4, 3).await.unwrap();

    let request = |request: &str, body: Value| (request.to_string(), body);
    let base = "/repos/acme/widgets";
    assert_eq!(
        without_token(take(&seen)),
        [
            request(
                &format!("PATCH {base}/issues/3"),
                json!({ "state": "closed", "state_reason": "completed" })
            ),
            request(
                &format!("PATCH {base}/issues/3"),
                json!({ "state": "open", "state_reason": "reopened" })
            ),
            request(
                &format!("PUT {base}/issues/3/labels"),
                json!({ "labels": ["bug", "wayfinder:map"] })
            ),
            request(
                &format!("POST {base}/issues/3/comments"),
                json!({ "body": "Hi" })
            ),
            request(
                &format!("GET {base}/issues/3/comments?per_page=100"),
                Value::Null
            ),
            request(&format!("GET {base}/issues/4"), Value::Null),
            request(
                &format!("POST {base}/issues/3/sub_issues"),
                json!({ "sub_issue_id": 104, "replace_parent": true })
            ),
            request(&format!("GET {base}/issues/4"), Value::Null),
            request(
                &format!("DELETE {base}/issues/3/sub_issue"),
                json!({ "sub_issue_id": 104, "replace_parent": true })
            ),
            request(&format!("GET {base}/issues/4"), Value::Null),
            request(&format!("GET {base}/issues/3"), Value::Null),
            request(
                &format!("POST {base}/issues/4/dependencies/blocked_by"),
                json!({ "issue_id": 103 })
            ),
            request(&format!("GET {base}/issues/3"), Value::Null),
            request(
                &format!("DELETE {base}/issues/4/dependencies/blocked_by/103"),
                Value::Null
            ),
        ]
    );
}

#[tokio::test]
async fn the_github_poll_reads_labels_parent_comment_counts_and_the_blockers_of_issues_that_have_any()
 {
    let issue = |number: i64, extra: Value| {
        let mut issue = json!({ "number": number, "title": "T", "body": "b", "state": "open",
                                "user": { "login": "leon" }, "comments": 0 });
        issue
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        issue
    };
    let mut answers = github_answers();
    answers.extend([
        (
            "GET",
            "/repos/acme/widgets/issues",
            json!([
                issue(1, json!({ "labels": [{ "name": "wayfinder:map" }], "comments": 2 })),
                issue(2, json!({
                    "parent_issue_url": "https://api.github.com/repos/acme/widgets/issues/1",
                    "issue_dependencies_summary": { "blocked_by": 1, "total_blocked_by": 1 },
                    "updated_at": "2026-02-01T00:00:00Z",
                })),
                issue(3, json!({ "issue_dependencies_summary": { "blocked_by": 0, "total_blocked_by": 0 } })),
                issue(4, json!({ "parent_issue_url": "https://api.github.com/repos/acme/other/issues/1" })),
            ]),
        ),
        (
            "GET",
            "/repos/acme/widgets/issues/2/dependencies/blocked_by",
            json!({ "__pages": [[{ "number": 1, "id": 101 }], [{ "number": 7, "id": 107 }]] }),
        ),
    ]);
    let (url, seen) = serve(answers).await;
    let github = github_client(&url);
    let issues = Polled::issues(&github, "acme/widgets")
        .await
        .unwrap()
        .unwrap();
    let read: Vec<_> = issues
        .iter()
        .map(|i| {
            (
                i.number,
                i.labels.clone(),
                i.parent,
                i.blocked_by.clone(),
                i.comment_count,
            )
        })
        .collect();
    // A parent in another repository is none: its number is that of another issue here.
    assert_eq!(
        read,
        [
            (1, vec!["wayfinder:map".to_string()], None, None, 2),
            (2, vec![], Some(1), None, 0),
            (3, vec![], None, Some(vec![]), 0),
            (4, vec![], None, None, 0),
        ]
    );
    assert_eq!(
        issues.iter().map(|i| i.blockers_unread).collect::<Vec<_>>(),
        [false, true, false, false]
    );
    // Listing the issues cost no request for blockers; reading an issue's costs one per page, and not again while the
    // issue is as it was.
    let requests = |seen| {
        take(seen)
            .into_iter()
            .filter(|(request, _)| request.contains("dependencies"))
            .count()
    };
    assert_eq!(requests(&seen), 0);
    let stamp = &issues[1].updated;
    assert_eq!(
        Polled::blockers(&github, "acme/widgets", 2, stamp).await,
        Some(vec![1, 7])
    );
    assert_eq!(requests(&seen), 2);
    assert_eq!(
        Polled::blockers(&github, "acme/widgets", 2, stamp).await,
        Some(vec![1, 7])
    );
    assert_eq!(requests(&seen), 0);
    Polled::blockers(&github, "acme/widgets", 2, "2026-03-01T00:00:00Z").await;
    assert_eq!(requests(&seen), 2);
}

#[tokio::test]
async fn gitlab_closes_labels_comments_and_links_issues_and_the_poll_reads_links_once_per_change() {
    let base = "/api/v4/projects/acme%2Fwidgets";
    let issue = |iid: i64, state: &str, updated: &str| {
        json!({ "iid": iid, "title": "T", "description": "d", "state": state, "author": { "username": "leon" },
                "updated_at": updated, "labels": ["bug"], "user_notes_count": 1, "project_id": 5 })
    };
    let (url, seen) = serve(vec![
        ("PUT", "/api/v4/projects/acme%2Fwidgets/issues/3", json!({ "iid": 3 })),
        ("POST", "/api/v4/projects/acme%2Fwidgets/issues/3/notes", json!({ "id": 9 })),
        (
            "GET",
            "/api/v4/projects/acme%2Fwidgets/issues/3/notes",
            json!([
                { "id": 1, "body": "changed the description", "author": { "username": "leon" }, "created_at": "2026-01-02T03:04:05Z", "system": true },
                { "id": 2, "body": "Hi", "author": { "username": "leon" }, "created_at": "2026-01-02T03:04:06Z", "system": false },
            ]),
        ),
        (
            "GET",
            "/api/v4/projects/acme%2Fwidgets/issues/3/links",
            json!([
                { "iid": 1, "project_id": 5, "issue_link_id": 55, "link_type": "is_blocked_by" },
                { "iid": 2, "project_id": 5, "issue_link_id": 56, "link_type": "relates_to" },
                { "iid": 9, "project_id": 6, "issue_link_id": 57, "link_type": "is_blocked_by" },
            ]),
        ),
        ("POST", "/api/v4/projects/acme%2Fwidgets/issues/3/links", json!({ "link_type": "is_blocked_by" })),
        // GitLab without Premium links the issues as related whatever is asked.
        ("POST", "/api/v4/projects/acme%2Fwidgets/issues/5/links", json!({ "link_type": "relates_to" })),
        ("GET", "/api/v4/projects/acme%2Fwidgets/issues/3", json!({ "description": "As it is now." })),
        ("DELETE", "/api/v4/projects/acme%2Fwidgets/issues/3/links/55", json!({})),
        (
            "GET",
            "/api/v4/projects/acme%2Fwidgets/issues",
            json!([issue(3, "opened", "2026-02-01T00:00:00Z"), issue(4, "closed", "2026-01-01T00:00:00Z")]),
        ),
    ])
    .await;
    let gitlab = GitLab::new(&url, "token");
    let name = "acme/widgets";

    gitlab.set_issue_state(name, 3, false).await.unwrap();
    gitlab.set_issue_state(name, 3, true).await.unwrap();
    gitlab
        .set_issue_labels(name, 3, &["a".into(), "b".into()])
        .await
        .unwrap();
    assert_eq!(gitlab.create_note(name, 3, "Hi").await.unwrap(), 9);
    let notes = gitlab.notes(name, 3).await.unwrap();
    assert_eq!(notes.iter().map(|n| n.id).collect::<Vec<_>>(), [2]);
    assert!(gitlab.add_blocker(name, 3, 1).await.unwrap());
    assert!(!gitlab.add_blocker(name, 5, 1).await.unwrap());
    assert!(gitlab.remove_blocker(name, 3, 1).await.unwrap());
    assert!(!gitlab.remove_blocker(name, 3, 8).await.unwrap());
    assert_eq!(
        gitlab.issue_description(name, 3).await.unwrap(),
        "As it is now."
    );
    let sent = take(&seen);
    let sent: Vec<(&str, &Value)> = sent.iter().map(|(r, b)| (r.as_str(), b)).collect();
    assert_eq!(
        sent,
        [
            (
                format!("PUT {base}/issues/3").as_str(),
                &json!({ "state_event": "close" })
            ),
            (
                format!("PUT {base}/issues/3").as_str(),
                &json!({ "state_event": "reopen" })
            ),
            (
                format!("PUT {base}/issues/3").as_str(),
                &json!({ "labels": "a,b" })
            ),
            (
                format!("POST {base}/issues/3/notes").as_str(),
                &json!({ "body": "Hi" })
            ),
            (
                format!("GET {base}/issues/3/notes?sort=asc&order_by=created_at&per_page=100")
                    .as_str(),
                &Value::Null
            ),
            (
                format!("POST {base}/issues/3/links").as_str(),
                &json!({ "target_project_id": "acme/widgets", "target_issue_iid": 1, "link_type": "is_blocked_by" })
            ),
            (
                format!("POST {base}/issues/5/links").as_str(),
                &json!({ "target_project_id": "acme/widgets", "target_issue_iid": 1, "link_type": "is_blocked_by" })
            ),
            (format!("GET {base}/issues/3/links").as_str(), &Value::Null),
            (
                format!("DELETE {base}/issues/3/links/55").as_str(),
                &Value::Null
            ),
            (format!("GET {base}/issues/3/links").as_str(), &Value::Null),
            (format!("GET {base}/issues/3").as_str(), &Value::Null),
        ]
    );

    // The poll reads the links of an open issue, of its own project only, and not again until the issue changes.
    let issues = Polled::issues(&gitlab, name).await.unwrap().unwrap();
    assert_eq!(
        issues
            .iter()
            .map(|i| (
                i.number,
                i.labels.clone(),
                i.comment_count,
                i.blocked_by.clone(),
                i.blockers_unread
            ))
            .collect::<Vec<_>>(),
        [
            (3, vec!["bug".to_string()], 1, None, true),
            (4, vec!["bug".to_string()], 1, None, false)
        ]
    );
    take(&seen);
    let stamp = &issues[0].updated;
    for _ in 0..2 {
        assert_eq!(
            Polled::blockers(&gitlab, name, 3, stamp).await,
            Some(vec![1])
        );
    }
    let links = take(&seen)
        .into_iter()
        .filter(|(request, _)| request.starts_with("GET") && request.contains("/links"))
        .count();
    assert_eq!(links, 1);
    assert_eq!(
        Polled::comments(&gitlab, name, 3).await.unwrap()[0].at,
        1_767_323_046
    );
}

#[tokio::test]
async fn forgejo_closes_labels_comments_and_depends_issues_and_the_poll_reads_dependencies_once_per_change()
 {
    let base = "/api/v1/repos/acme/widgets";
    let issue = |number: i64, state: &str, updated: &str| {
        json!({ "number": number, "title": "T", "body": "b", "state": state, "user": { "login": "leon" },
                "updated_at": updated, "labels": [{ "name": "bug" }], "comments": 1 })
    };
    let (url, seen) = serve(vec![
        ("PATCH", "/api/v1/repos/acme/widgets/issues/3", json!({ "number": 3 })),
        ("PUT", "/api/v1/repos/acme/widgets/issues/3/labels", json!([])),
        ("POST", "/api/v1/repos/acme/widgets/issues/3/comments", json!({ "id": 9 })),
        (
            "GET",
            "/api/v1/repos/acme/widgets/issues/3/comments",
            json!([{ "id": 9, "user": { "login": "leon" }, "body": "Hi", "created_at": "2026-01-02T03:04:05Z" }]),
        ),
        ("POST", "/api/v1/repos/acme/widgets/issues/3/dependencies", json!({})),
        ("DELETE", "/api/v1/repos/acme/widgets/issues/3/dependencies", json!({})),
        (
            "GET",
            "/api/v1/repos/acme/widgets/issues/3/dependencies",
            json!({ "__pages": [[{ "number": 1 }], [{ "number": 2 }]] }),
        ),
        ("GET", "/api/v1/repos/acme/widgets/issues/3", json!({ "body": "As it is now." })),
        (
            "GET",
            "/api/v1/repos/acme/widgets/issues",
            json!([issue(3, "open", "2026-02-01T00:00:00Z"), issue(4, "closed", "2026-01-01T00:00:00Z")]),
        ),
    ])
    .await;
    let forgejo = Forgejo::new(&url, "token");
    let name = "acme/widgets";

    forgejo.set_issue_state(name, 3, false).await.unwrap();
    forgejo.set_issue_state(name, 3, true).await.unwrap();
    forgejo
        .set_issue_labels(name, 3, &["a".into()])
        .await
        .unwrap();
    assert_eq!(
        forgejo.create_issue_comment(name, 3, "Hi").await.unwrap(),
        9
    );
    assert_eq!(forgejo.issue_comments(name, 3).await.unwrap()[0].id, 9);
    forgejo.add_blocker(name, 3, 1).await.unwrap();
    forgejo.remove_blocker(name, 3, 1).await.unwrap();
    assert_eq!(forgejo.issue_body(name, 3).await.unwrap(), "As it is now.");
    let dependency = json!({ "index": 1, "owner": "acme", "repo": "widgets" });
    let request = |request: String, body: Value| (request, body);
    assert_eq!(
        take(&seen),
        [
            request(
                format!("PATCH {base}/issues/3"),
                json!({ "state": "closed" })
            ),
            request(format!("PATCH {base}/issues/3"), json!({ "state": "open" })),
            request(
                format!("PUT {base}/issues/3/labels"),
                json!({ "labels": ["a"] })
            ),
            request(
                format!("POST {base}/issues/3/comments"),
                json!({ "body": "Hi" })
            ),
            request(
                format!("GET {base}/issues/3/comments?limit=50"),
                Value::Null
            ),
            request(
                format!("POST {base}/issues/3/dependencies"),
                dependency.clone()
            ),
            request(format!("DELETE {base}/issues/3/dependencies"), dependency),
            request(format!("GET {base}/issues/3"), Value::Null),
        ]
    );

    let issues = Polled::issues(&forgejo, name).await.unwrap().unwrap();
    assert_eq!(
        issues
            .iter()
            .map(|i| (
                i.number,
                i.labels.clone(),
                i.comment_count,
                i.blockers_unread
            ))
            .collect::<Vec<_>>(),
        [
            (3, vec!["bug".to_string()], 1, true),
            (4, vec!["bug".to_string()], 1, false)
        ]
    );
    take(&seen);
    // The dependencies come in pages, all of which are read, and are not read again until the issue changes.
    let stamp = &issues[0].updated;
    for _ in 0..2 {
        assert_eq!(
            Polled::blockers(&forgejo, name, 3, stamp).await,
            Some(vec![1, 2])
        );
    }
    let dependencies = take(&seen)
        .into_iter()
        .filter(|(request, _)| request.starts_with("GET") && request.contains("/dependencies"))
        .count();
    assert_eq!(dependencies, 2);
    assert_eq!(
        Polled::comments(&forgejo, name, 3).await.unwrap()[0].author,
        "leon"
    );
}

#[tokio::test]
async fn a_poll_whose_forge_cannot_list_blockers_goes_on_with_what_the_bodies_name() {
    let issue = |id: i64| {
        json!({ "iid": id, "title": "T", "description": "d", "state": "opened",
                "author": { "username": "leon" }, "updated_at": "2026-02-01T00:00:00Z", "project_id": 5 })
    };
    // No route for the links of GitLab or the dependencies of Forgejo: the forge answers 404, as one without them does.
    let (url, _) = serve(vec![
        ("GET", "/api/v4/projects/acme%2Fwidgets/issues", json!([issue(3)])),
        (
            "GET",
            "/api/v1/repos/acme/widgets/issues",
            json!([{ "number": 3, "title": "T", "body": "", "state": "open", "user": { "login": "leon" },
                     "updated_at": "2026-02-01T00:00:00Z" }]),
        ),
    ])
    .await;
    let gitlab = GitLab::new(&url, "t");
    let issues = Polled::issues(&gitlab, "acme/widgets")
        .await
        .unwrap()
        .unwrap();
    assert_eq!((issues[0].number, issues[0].blocked_by.clone()), (3, None));
    assert_eq!(
        Polled::blockers(&gitlab, "acme/widgets", 3, &issues[0].updated).await,
        None
    );
    let forgejo = Forgejo::new(&url, "t");
    let issues = Polled::issues(&forgejo, "acme/widgets")
        .await
        .unwrap()
        .unwrap();
    assert_eq!((issues[0].number, issues[0].blocked_by.clone()), (3, None));
    assert_eq!(
        Polled::blockers(&forgejo, "acme/widgets", 3, &issues[0].updated).await,
        None
    );
}

#[test]
fn what_a_poll_read_for_an_issue_is_used_only_while_the_issue_is_unchanged() {
    let remembered = Remembered::default();
    assert_eq!(remembered.get("a/b", 1, "t1"), None);
    remembered.put("a/b", 1, "t1", vec![5]);
    assert_eq!(remembered.get("a/b", 1, "t1"), Some(vec![5]));
    assert_eq!(remembered.get("a/b", 1, "t2"), None);
    assert_eq!(remembered.get("a/c", 1, "t1"), None);
}

#[test]
fn what_a_poll_read_is_read_again_once_it_is_old_even_if_the_issue_looks_unchanged() {
    // A forge need not change an issue's time when a link changes, so an answer does not last for ever.
    let remembered = Remembered::lasting(std::time::Duration::ZERO);
    remembered.put("a/b", 1, "t1", vec![5]);
    assert_eq!(remembered.get("a/b", 1, "t1"), None);
}
