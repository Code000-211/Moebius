//! The rail against fake servers: two switched between with each keeping its page, one that cannot be reached, a token
//! that is in the keychain and in no file, and a removed server that takes its token with it.

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use moebius_desktop::rail::secrets::{Memory, Secrets};
use moebius_desktop::rail::{self, Options, Rail, RailError};
use moebius_desktop::server::VERSION;
use serde_json::json;

fn files(name: &str) -> Option<Cow<'static, [u8]>> {
    (name == "index.html").then_some(Cow::Borrowed(b"<html>the web app</html>".as_slice()))
}

fn options() -> Options {
    Options {
        files,
        ..Options::standard()
    }
}

#[derive(Clone)]
struct Fake {
    token: &'static str,
    inbox: usize,
    unread: usize,
}

impl Fake {
    fn allowed(&self, headers: &HeaderMap) -> bool {
        headers.get("authorization").and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {}", self.token))
    }
}

/// A fake Moebius server on port 0 that knows `token`; its address, and the task to abort to take it away.
async fn fake(
    token: &'static str,
    inbox: usize,
    unread: usize,
) -> (String, tokio::task::JoinHandle<()>) {
    async fn health(State(f): State<Fake>, headers: HeaderMap) -> impl IntoResponse {
        if f.allowed(&headers) {
            (StatusCode::OK, axum::Json(json!({ "version": VERSION }))).into_response()
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        }
    }
    async fn snapshot(State(f): State<Fake>, headers: HeaderMap) -> impl IntoResponse {
        if !f.allowed(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        axum::Json(json!({ "inbox": vec![json!({}); f.inbox] })).into_response()
    }
    async fn notices(State(f): State<Fake>, headers: HeaderMap) -> impl IntoResponse {
        if !f.allowed(&headers) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        let mut all = vec![json!({ "read": true })];
        all.extend(vec![json!({ "read": false }); f.unread]);
        axum::Json(all).into_response()
    }
    let app = Router::new()
        .route("/v1/health", get(health))
        .route("/v1/snapshot", get(snapshot))
        .route("/v1/notices", get(notices))
        .with_state(Fake {
            token,
            inbox,
            unread,
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (address, task)
}

fn rail(dir: &Path, secrets: &Arc<Memory>) -> Rail {
    Rail::new(dir, secrets.clone(), options())
}

/// Where the window is led to by `url` (the proxy's entry), as the browser would be: not followed further.
async fn leads_to(url: &str) -> String {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client.get(url).send().await.unwrap();
    assert_eq!(response.status(), 303);
    response.headers()["location"].to_str().unwrap().to_string()
}

#[tokio::test]
async fn two_servers_are_switched_between_and_each_keeps_its_page() {
    let (one, _a) = fake("token-one", 2, 1).await;
    let (two, _b) = fake("token-two", 0, 0).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let mut rail = rail(dir.path(), &secrets);

    let first = rail.add(&one, "token-one").await.unwrap();
    rail.note_page("/fleet?tab=2");
    let second = rail.add(&two, "token-two").await.unwrap();
    assert_ne!(first, second);
    assert_eq!(rail.open_id(), Some(second.as_str()));
    rail.note_page("/chat");

    let url = rail.open(&first).await.unwrap();
    assert_eq!(leads_to(&url).await, "/fleet?tab=2");
    assert_eq!(rail.open_id(), Some(first.as_str()));
    let url = rail.open(&second).await.unwrap();
    assert_eq!(leads_to(&url).await, "/chat");

    // Each server has a proxy (an origin) of its own.
    let origin_two = rail.open_origin().unwrap();
    rail.open(&first).await.unwrap();
    assert_ne!(rail.open_origin().unwrap(), origin_two);

    let entries = rail::poll(rail.targets()).await;
    let shown: Vec<_> = entries
        .iter()
        .map(|e| (e.open, e.reachable, e.inbox))
        .collect();
    assert_eq!(shown, [(true, true, 3), (false, true, 0)]);
}

#[tokio::test]
async fn a_server_that_cannot_be_reached_has_the_grey_dot() {
    let (up, _a) = fake("t1", 1, 0).await;
    let (down, gone) = fake("t2", 4, 0).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let mut rail = rail(dir.path(), &secrets);
    rail.add(&down, "t2").await.unwrap();
    rail.add(&up, "t1").await.unwrap();
    assert_eq!(
        rail::poll(rail.targets())
            .await
            .iter()
            .map(|e| e.reachable)
            .collect::<Vec<_>>(),
        [true, true]
    );

    gone.abort();
    let _ = gone.await;
    let entries = rail::poll(rail.targets()).await;
    assert!(!entries[0].reachable);
    assert_eq!(entries[0].inbox, 0);
    assert!(entries[1].reachable);
    assert_eq!(entries[1].inbox, 1);
}

#[tokio::test]
async fn a_token_is_in_the_keychain_and_in_no_file_and_removing_the_server_removes_it() {
    let (address, _a) = fake("secret-token-value", 0, 0).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let mut rail = rail(dir.path(), &secrets);
    let id = rail.add(&address, "secret-token-value").await.unwrap();

    assert_eq!(
        secrets.get(&id).unwrap().as_deref(),
        Some("secret-token-value")
    );
    let mut files = 0;
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        files += 1;
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.contains("secret-token-value"), "{text}");
        assert!(text.contains(&address), "{text}");
    }
    assert!(files > 0);

    // A new start finds the server again, and opens it with the token from the keychain.
    let mut again = Rail::new(dir.path(), secrets.clone(), options());
    assert_eq!(again.startup_order(), std::slice::from_ref(&id));
    again.open(&id).await.unwrap();

    assert!(again.remove(&id).unwrap());
    assert_eq!(secrets.get(&id).unwrap(), None);
    assert!(again.startup_order().is_empty());
    assert!(
        Rail::new(dir.path(), secrets, options())
            .startup_order()
            .is_empty()
    );
    assert!(matches!(again.open(&id).await, Err(RailError::Unknown)));
}

#[tokio::test]
async fn a_server_that_refuses_the_token_is_not_added_and_a_lost_token_is_said() {
    let (address, _a) = fake("right", 0, 0).await;
    let dir = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Memory::default());
    let mut rail = rail(dir.path(), &secrets);
    let refused = rail.add(&address, "wrong").await.unwrap_err();
    assert!(refused.to_string().contains("does not accept"), "{refused}");
    assert!(rail.targets().is_empty());

    let id = rail.add(&address, "right").await.unwrap();
    // The same address again updates the server; it is not a second square.
    assert_eq!(rail.add(&address, "right").await.unwrap(), id);
    assert_eq!(rail.targets().len(), 1);

    secrets.delete(&id).unwrap();
    let mut restarted = Rail::new(dir.path(), secrets, options());
    let lost = restarted.open(&id).await.unwrap_err();
    assert!(matches!(lost, RailError::NoToken(_)), "{lost}");
}

/// A fake `ssh` is a Python script run by its `#!` line, so these need Unix.
#[cfg(unix)]
mod ssh {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    /// A fake `ssh` that, when the file `ready` is beside it, prints the token and forwards the `-L` port to the fake
    /// server whose port is in `target`; without `ready` it ends as the remote command does without a token.
    fn fake_ssh(dir: &Path) -> String {
        let path = dir.join("ssh");
        std::fs::write(
            &path,
            r#"#!/usr/bin/env python3
import os, socket, sys, threading
here = os.path.dirname(os.path.abspath(__file__))
args = sys.argv[1:]
local = int(args[args.index("-L") + 1].split(":")[1])
if not os.path.exists(os.path.join(here, "ready")):
    sys.exit(1)
target = int(open(os.path.join(here, "target")).read())
print("a banner", flush=True)
print("MOEBIUS-TOKEN " + open(os.path.join(here, "token")).read().strip(), flush=True)
server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", local))
server.listen()
def pipe(a, b):
    try:
        while (data := a.recv(65536)):
            b.sendall(data)
    except OSError:
        pass
    for s in (a, b):
        try: s.shutdown(socket.SHUT_RDWR)
        except OSError: pass
while True:
    client, _ = server.accept()
    upstream = socket.create_connection(("127.0.0.1", target))
    threading.Thread(target=pipe, args=(client, upstream), daemon=True).start()
    threading.Thread(target=pipe, args=(upstream, client), daemon=True).start()
"#,
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    /// A fake `moebius` that makes the host's server appear, as the setup does, and then keeps running.
    fn fake_moebius(dir: &Path) -> String {
        let path = dir.join("moebius");
        std::fs::write(
            &path,
            format!("#!/bin/sh\ntouch '{}/ready'\nsleep 60\n", dir.display()),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    #[tokio::test]
    async fn a_server_over_ssh_is_reached_through_its_tunnel_and_set_up_after_a_yes() {
        let (address, _a) = fake("ssh-token", 5, 0).await;
        let tools = tempfile::tempdir().unwrap();
        std::fs::write(tools.path().join("token"), "ssh-token\n").unwrap();
        std::fs::write(
            tools.path().join("target"),
            address.rsplit(':').next().unwrap(),
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(Memory::default());
        let mut rail = Rail::new(
            dir.path(),
            secrets.clone(),
            Options {
                ssh: fake_ssh(tools.path()),
                moebius: fake_moebius(tools.path()),
                files,
            },
        );

        // No server on the host yet: the rail says so, and changes nothing until it is told to set one up.
        let none = rail.add_ssh("devbox", false).await.unwrap_err();
        assert!(
            matches!(none, RailError::NoServer(ref host) if host == "devbox"),
            "{none}"
        );
        assert!(!tools.path().join("ready").exists());

        let id = rail.add_ssh("devbox", true).await.unwrap();
        assert!(tools.path().join("ready").exists());
        assert_eq!(secrets.get(&id).unwrap().as_deref(), Some("ssh-token"));
        let entries = rail::poll(rail.targets()).await;
        assert_eq!((entries[0].reachable, entries[0].inbox), (true, 5));

        // After a restart the tunnel is not up: grey until the server is opened, which opens the tunnel again.
        let mut restarted = Rail::new(
            dir.path(),
            secrets,
            Options {
                ssh: fake_ssh(tools.path()),
                moebius: "moebius".into(),
                files,
            },
        );
        assert!(!rail::poll(restarted.targets()).await[0].reachable);
        restarted.open(&id).await.unwrap();
        assert!(rail::poll(restarted.targets()).await[0].reachable);
    }
}
