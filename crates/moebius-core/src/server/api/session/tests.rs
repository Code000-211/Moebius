//! For the tests only: signing in with each kind of token and code, what a session survives and what ends it.

use super::*;

#[test]
fn a_code_works_once_and_only_until_it_expires() {
    let codes = Codes::default();
    let (code, expires) = codes.new_code_at(7, Some(3), 1000).unwrap();
    assert_eq!(expires, 1000 + CODE_LIFETIME);
    assert_eq!(
        codes.redeem_at(&code, 1000 + CODE_LIFETIME - 1),
        Some((7, Some(3)))
    );
    assert!(codes.redeem_at(&code, 1000).is_none());
    let (late, _) = codes.new_code_at(7, None, 1000).unwrap();
    assert!(codes.redeem_at(&late, 1000 + CODE_LIFETIME).is_none());
    assert!(codes.redeem_at("unknown", 1000).is_none());
}

#[test]
fn the_cookie_is_found_among_others() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_static("a=b; moebius_session=abc; c=d"),
    );
    assert_eq!(cookie_of(&headers), Some("abc"));
    assert_eq!(cookie_of(&HeaderMap::new()), None);
}

use futures_util::StreamExt;
use reqwest::StatusCode as Status;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use crate::server::api::testing::start;

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
}

/// Signs in with `body` and gives the cookie (`moebius_session=<id>`) the answer sets.
async fn sign_in_as(addr: &str, body: serde_json::Value) -> String {
    let response = http()
        .post(format!("http://{addr}/v1/session"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), Status::NO_CONTENT);
    let set = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(set.contains("HttpOnly") && set.contains("SameSite=Strict") && set.contains("Path=/"));
    assert!(set.contains(&format!("Max-Age={SESSION_LIFETIME}")));
    assert!(!set.contains("Secure"));
    set.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn the_token_signs_in_and_the_cookie_acts_as_a_person() {
    let (_dir, state, addr) = start().await;
    let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;

    let health = http()
        .get(format!("http://{addr}/v1/health"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), Status::OK);
    let status = http()
        .get(format!("http://{addr}/v1/session"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    let status: SessionStatus = status.json().await.unwrap();
    assert!(status.signed_in);
    assert_eq!(status.person.unwrap().handle, crate::store::FIRST_OWNER);
    // The bearer token says the same.
    let bearer = http()
        .get(format!("http://{addr}/v1/session"))
        .bearer_auth(&*state.token)
        .send()
        .await
        .unwrap();
    assert_eq!(bearer.status(), Status::OK);

    // A person-only route accepts the cookie.
    let secrets = http()
        .get(format!("http://{addr}/v1/webhooks"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(secrets.status(), Status::OK);

    // Signing out forgets the session.
    let out = http()
        .delete(format!("http://{addr}/v1/session"))
        .header("cookie", &cookie)
        .header("x-moebius", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(out.status(), Status::NO_CONTENT);
    assert!(
        out.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let after = http()
        .get(format!("http://{addr}/v1/health"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), Status::UNAUTHORIZED);
}

#[tokio::test]
async fn a_wrong_token_code_or_cookie_is_refused() {
    let (_dir, state, addr) = start().await;
    for body in [
        serde_json::json!({"token": "nope"}),
        serde_json::json!({"code": "nope"}),
        serde_json::json!({}),
    ] {
        let response = http()
            .post(format!("http://{addr}/v1/session"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), Status::UNAUTHORIZED);
        assert_eq!(response.text().await.unwrap(), "a valid token is needed");
    }
    for cookie in ["moebius_session=nope", "other=1"] {
        let response = http()
            .get(format!("http://{addr}/v1/health"))
            .header("cookie", cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), Status::UNAUTHORIZED);
    }
    let none = http()
        .get(format!("http://{addr}/v1/session"))
        .send()
        .await
        .unwrap();
    assert_eq!(none.status(), Status::UNAUTHORIZED);
    // A valid cookie does not rescue a wrong bearer token.
    let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;
    let both = http()
        .get(format!("http://{addr}/v1/health"))
        .header("cookie", &cookie)
        .bearer_auth("nope")
        .send()
        .await
        .unwrap();
    assert_eq!(both.status(), Status::UNAUTHORIZED);
}

#[tokio::test]
async fn a_run_token_never_gets_a_session() {
    let (_dir, state, addr) = start().await;
    let run = state
        .app
        .store
        .add_run(Some(1), "Moebius", "test", "chat")
        .unwrap();
    let agent_token = state.router.issue_token(crate::router::Grant {
        run_id: run.id,
        card_id: None,
        budget: None,
    });
    let response = http()
        .post(format!("http://{addr}/v1/session"))
        .json(&serde_json::json!({"token": agent_token}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), Status::FORBIDDEN);
    assert_eq!(response.text().await.unwrap(), "only a person can do this");
    // Nor can it mint a code with it ... it can not: a code opens a person's session.
    let code = http()
        .post(format!("http://{addr}/v1/session/codes"))
        .bearer_auth(&agent_token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(code.status(), Status::FORBIDDEN);
}

#[tokio::test]
async fn a_one_time_code_signs_in_once() {
    let (_dir, state, addr) = start().await;
    let minted: LoginCode = http()
        .post(format!("http://{addr}/v1/session/codes"))
        .bearer_auth(&*state.token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(minted.expires_at > crate::store::now());
    let prefix = format!("http://{addr}/?login=");
    let code = minted
        .url
        .strip_prefix(&prefix)
        .expect("the url holds the code")
        .to_string();

    let cookie = sign_in_as(&addr, serde_json::json!({"code": &code})).await;
    let ok = http()
        .get(format!("http://{addr}/v1/health"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), Status::OK);
    let again = http()
        .post(format!("http://{addr}/v1/session"))
        .json(&serde_json::json!({"code": &code}))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), Status::UNAUTHORIZED);

    // The cookie can mint a code too, as a person.
    let by_cookie = http()
        .post(format!("http://{addr}/v1/session/codes"))
        .header("cookie", &cookie)
        .header("x-moebius", "1")
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(by_cookie.status(), Status::OK);
}

#[tokio::test]
async fn a_change_by_cookie_must_come_from_the_web_app() {
    let (_dir, state, addr) = start().await;
    let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;
    let post = |extra: Option<(&'static str, String)>| {
        let mut request = http()
            .post(format!("http://{addr}/v1/session/codes"))
            .header("cookie", &cookie)
            .json(&serde_json::json!({}));
        if let Some((name, value)) = extra {
            request = request.header(name, value);
        }
        request
    };
    let refused = post(None).send().await.unwrap();
    assert_eq!(refused.status(), Status::FORBIDDEN);
    assert_eq!(refused.text().await.unwrap(), "cross-site request refused");

    let foreign = post(Some(("x-moebius", "1".into())))
        .header("origin", "http://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), Status::FORBIDDEN);

    let own = post(Some(("x-moebius", "1".into())))
        .header("origin", format!("http://{addr}"))
        .send()
        .await
        .unwrap();
    assert_eq!(own.status(), Status::OK);

    // The bearer token needs neither.
    let bearer = http()
        .post(format!("http://{addr}/v1/session/codes"))
        .bearer_auth(&*state.token)
        .header("origin", "http://evil.example")
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(bearer.status(), Status::OK);
}

#[tokio::test]
async fn the_cookie_opens_the_event_stream_from_the_apps_own_origin_only() {
    let (_dir, state, addr) = start().await;
    let cookie = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;
    let open = |origin: Option<String>, cookie: String| {
        let mut request = format!("ws://{addr}/v1/events")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
        }
        tokio_tungstenite::connect_async(request)
    };
    let (mut ws, _) = open(Some(format!("http://{addr}")), cookie.clone())
        .await
        .unwrap();
    let event = crate::bus::Event::CardMoved {
        card: 1,
        from: "Backlog".into(),
        to: "Not started".into(),
    };
    let received = loop {
        state.app.bus.publish(event.clone());
        if let Ok(Some(message)) =
            tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await
        {
            break message.unwrap();
        }
    };
    let back: crate::bus::AppEvent = serde_json::from_str(received.to_text().unwrap()).unwrap();
    assert_eq!(back, crate::bus::AppEvent::from(event));

    assert!(
        open(Some("http://evil.example".into()), cookie)
            .await
            .is_err()
    );
    assert!(open(None, "moebius_session=nope".into()).await.is_err());
}

#[tokio::test]
async fn the_web_app_is_served_without_a_token_and_the_api_is_not() {
    let (_dir, _state, addr) = start().await;
    for path in ["/", "/cards/3"] {
        let page = http()
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), Status::OK, "{path}");
        assert!(
            page.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
    }
    let api = http()
        .get(format!("http://{addr}/v1/nothing"))
        .send()
        .await
        .unwrap();
    assert_eq!(api.status(), Status::UNAUTHORIZED);
}

/// Lea, a member, with an access token; gives her and the token.
fn lea(state: &ApiState) -> (Person, String, i64) {
    let store = &state.app.store;
    let lea = store
        .add_person("lea", "Lea", crate::store::Role::Member)
        .unwrap();
    let token = super::super::people::new_token().unwrap();
    let made = store.add_access_token(lea.id, "laptop", &token).unwrap();
    (lea, token, made.id)
}

async fn status_of(request: reqwest::RequestBuilder) -> (Status, Option<Person>) {
    let response = request.send().await.unwrap();
    let status = response.status();
    if status != Status::OK {
        return (status, None);
    }
    let body: SessionStatus = response.json().await.unwrap();
    (status, body.person)
}

#[tokio::test]
async fn an_access_token_signs_in_as_its_person_until_it_is_revoked() {
    let (_dir, state, addr) = start().await;
    let (lea, token, token_id) = lea(&state);
    let session = || http().get(format!("http://{addr}/v1/session"));

    let (status, person) = status_of(session().bearer_auth(&token)).await;
    assert_eq!(status, Status::OK);
    assert_eq!(person.unwrap().id, lea.id);
    let cookie = sign_in_as(&addr, serde_json::json!({"token": &token})).await;
    let (_, person) = status_of(session().header("cookie", &cookie)).await;
    assert_eq!(person.unwrap().handle, "lea");
    // A code Lea mints signs a second browser in as her, and ends with her token too.
    let minted: LoginCode = http()
        .post(format!("http://{addr}/v1/session/codes"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let code = minted.url.rsplit_once("login=").unwrap().1.to_string();
    let second = sign_in_as(&addr, serde_json::json!({"code": code})).await;
    let (_, person) = status_of(session().header("cookie", &second)).await;
    assert_eq!(person.unwrap().handle, "lea");

    assert!(state.app.store.revoke_access_token(token_id).unwrap());
    for request in [
        session().bearer_auth(&token),
        session().header("cookie", &cookie),
        session().header("cookie", &second),
    ] {
        assert_eq!(status_of(request).await.0, Status::UNAUTHORIZED);
    }
    let again = http()
        .post(format!("http://{addr}/v1/session"))
        .json(&serde_json::json!({"token": &token}))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), Status::UNAUTHORIZED);
}

#[tokio::test]
async fn disabling_a_person_refuses_their_token_and_their_sessions() {
    let (_dir, state, addr) = start().await;
    let (lea, token, _) = lea(&state);
    let cookie = sign_in_as(&addr, serde_json::json!({"token": &token})).await;
    state.app.store.set_person_disabled(lea.id, true).unwrap();
    let health =
        |request: reqwest::RequestBuilder| async move { request.send().await.unwrap().status() };
    let url = format!("http://{addr}/v1/health");
    assert_eq!(
        health(http().get(&url).bearer_auth(&token)).await,
        Status::UNAUTHORIZED
    );
    assert_eq!(
        health(http().get(&url).header("cookie", &cookie)).await,
        Status::UNAUTHORIZED
    );
}

#[tokio::test]
async fn the_servers_token_is_refused_while_the_first_owner_is_disabled() {
    let (_dir, state, addr) = start().await;
    let first = state
        .app
        .store
        .person_by_handle(crate::store::FIRST_OWNER)
        .unwrap()
        .unwrap();
    let url = format!("http://{addr}/v1/health");
    let with_server_token = || http().get(&url).bearer_auth(&*state.token).send();
    assert_eq!(with_server_token().await.unwrap().status(), Status::OK);
    state.app.store.set_person_disabled(first.id, true).unwrap();
    assert_eq!(
        with_server_token().await.unwrap().status(),
        Status::UNAUTHORIZED
    );
    // Signing a browser in with it is refused as well.
    let sign_in = http()
        .post(format!("http://{addr}/v1/session"))
        .json(&serde_json::json!({"token": &*state.token}))
        .send()
        .await
        .unwrap();
    assert_eq!(sign_in.status(), Status::UNAUTHORIZED);
    state
        .app
        .store
        .set_person_disabled(first.id, false)
        .unwrap();
    assert_eq!(with_server_token().await.unwrap().status(), Status::OK);
}

#[tokio::test]
async fn a_session_survives_a_restart_and_nothing_secret_is_stored_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("store");
    let store = std::sync::Arc::new(crate::store::Store::open(&data).unwrap());
    let (state, addr) = crate::server::api::testing::start_in(dir.path(), store).await;
    let (_, token, _) = lea(&state);
    let cookie = sign_in_as(&addr, serde_json::json!({"token": &token})).await;
    let owners = sign_in_as(&addr, serde_json::json!({"token": &*state.token})).await;
    let session_id = |cookie: &str| cookie.split_once('=').unwrap().1.to_string();

    // A second server over the same data, as after a restart: its codes are new, its sessions are the store's.
    let reopened = std::sync::Arc::new(crate::store::Store::open(&data).unwrap());
    let (_again, addr) = crate::server::api::testing::start_in(dir.path(), reopened).await;
    let (status, person) = status_of(
        http()
            .get(format!("http://{addr}/v1/session"))
            .header("cookie", &cookie),
    )
    .await;
    assert_eq!(status, Status::OK);
    assert_eq!(person.unwrap().handle, "lea");

    // Neither the tokens nor the session ids are anywhere in the store's files.
    drop(state);
    let mut stored = Vec::new();
    for entry in std::fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            stored.extend(std::fs::read(path).unwrap());
        }
    }
    let stored = String::from_utf8_lossy(&stored);
    assert!(stored.contains(&crate::store::hash_secret(&token)));
    for secret in [token, session_id(&cookie), session_id(&owners)] {
        assert!(!stored.contains(&secret), "{secret} is stored as it is");
    }
}
