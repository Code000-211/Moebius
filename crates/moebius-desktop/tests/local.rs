//! A server of this machine, set up and started by the app: against a fake release (an axum server on port 0 with the
//! program and its `SHA256SUMS`) and a fake `moebius` (a Python script that writes a configuration on `init`, and on
//! `serve` writes its token and answers `/v1/health`), with a throwaway home directory.
//!
//! The scripts need `sh` and Python, so the tests are for Unix; the Windows side (the same scripts through `wsl.exe`) is
//! the host's command line only.
#![cfg(unix)]

use std::path::Path;

use axum::Router;
use axum::routing::get;
use moebius_desktop::local::{Host, Local, State};
use moebius_desktop::server::VERSION;
use sha2::{Digest, Sha256};

/// The fake program: `init` writes `moebius.toml` listening on the port in `FAKE_PORT` (a port the test found free),
/// `serve` writes the token and answers `/v1/health` with this app's version, `--version` says it.
fn fake_program(version: &str) -> String {
    format!(
        r#"#!/usr/bin/env python3
import http.server, os, sys
if sys.argv[1:] == ["--version"]:
    print("moebius {version}")
elif sys.argv[1:] == ["init"]:
    open("moebius.toml", "w").write('listen = "127.0.0.1:' + os.environ["FAKE_PORT"] + '"\n')
    open("init-ran", "w").write("x")
elif sys.argv[1:] == ["serve"]:
    os.makedirs(".moebius", exist_ok=True)
    open(".moebius/token", "w").write("tok-123")
    class H(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            if self.headers.get("Authorization") != "Bearer tok-123":
                self.send_response(401); self.end_headers(); return
            body = b'{{"version": "{VERSION}"}}'
            self.send_response(200); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
        def log_message(self, *a): pass
    http.server.HTTPServer(("127.0.0.1", int(os.environ["FAKE_PORT"])), H).serve_forever()
"#
    )
}

/// A fake release with `program` and the hash `listed` for it in its `SHA256SUMS`; its base URL.
async fn release(program: String, listed: Option<String>) -> String {
    let hash = listed.unwrap_or_else(|| {
        Sha256::digest(program.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    });
    let sums = format!("{hash}  moebius-x86_64-unknown-linux-gnu\n");
    let app = Router::new()
        .route(
            "/moebius-x86_64-unknown-linux-gnu",
            get(move || async move { program }),
        )
        .route("/SHA256SUMS", get(move || async move { sums }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    base
}

/// A shell whose home is `home`, on a system that is Linux x86_64 whatever the test runs on, and with `FAKE_PORT` set.
fn host(home: &Path, port: u16) -> Host {
    // The host appends `-c <script>`, which this wrapper receives as `$1 $2`.
    let setup = format!(
        "HOME={home}; FAKE_PORT={port}; PATH={home}/nobin:/usr/bin:/bin; export HOME FAKE_PORT PATH; \
         uname() {{ echo Linux x86_64; }}; eval \"$2\"",
        home = home.display()
    );
    Host::new(&["sh", "-c", &setup, "sh"])
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Stops the server the test started, which no longer has the app as its parent.
fn stop_servers(home: &Path) {
    let _ = std::process::Command::new("pkill")
        .args(["-f", &format!("{}/.moebius/bin/moebius", home.display())])
        .status();
}

#[tokio::test]
async fn a_first_start_makes_a_server_and_connects_and_a_restart_finds_it_again() {
    let home = tempfile::tempdir().unwrap();
    let port = free_port();
    let base = release(fake_program(VERSION), None).await;
    let local = Local::new(host(home.path(), port), base);

    // Looking changes nothing: declining leaves nothing installed.
    assert_eq!(local.state().await, State::NotSetUp);
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);

    let server = local.start().await.unwrap();
    assert_eq!(server.url, format!("http://127.0.0.1:{port}"));
    assert_eq!(server.token, "tok-123");
    assert!(home.path().join(".moebius/bin/moebius").is_file());
    assert!(home.path().join(".moebius/server/moebius.toml").is_file());

    // The app starts again: it finds the same server, and does not set up or start another.
    let again = Local::new(host(home.path(), port), "http://127.0.0.1:1".to_string());
    assert_eq!(again.state().await, State::Running);
    let found = again.start().await.unwrap();
    assert_eq!((found.url, found.token), (server.url, server.token));
    stop_servers(home.path());
}

#[tokio::test]
async fn a_server_that_stopped_is_started_again_without_a_new_init() {
    let home = tempfile::tempdir().unwrap();
    let port = free_port();
    let base = release(fake_program(VERSION), None).await;
    let local = Local::new(host(home.path(), port), base);
    local.start().await.unwrap();
    stop_servers(home.path());
    // Wait until it no longer answers, then start it again.
    let mut state = local.state().await;
    for _ in 0..100 {
        if state == State::Stopped {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        state = local.state().await;
    }
    assert_eq!(state, State::Stopped);
    std::fs::remove_file(home.path().join(".moebius/server/init-ran")).unwrap();
    local.start().await.unwrap();
    assert!(!home.path().join(".moebius/server/init-ran").exists());
    stop_servers(home.path());
}

#[tokio::test]
async fn a_download_that_does_not_match_its_checksum_installs_nothing() {
    let home = tempfile::tempdir().unwrap();
    let base = release(fake_program(VERSION), Some("0".repeat(64))).await;
    let local = Local::new(host(home.path(), free_port()), base);
    let error = local.start().await.err().unwrap();
    assert!(error.to_string().contains("SHA256SUMS"), "{error}");
    assert!(!home.path().join(".moebius").exists());
}

#[tokio::test]
async fn a_release_that_is_not_there_is_an_error_naming_it() {
    let home = tempfile::tempdir().unwrap();
    let base = release(fake_program(VERSION), None).await + "/missing";
    let local = Local::new(host(home.path(), free_port()), base);
    let error = local.start().await.err().unwrap();
    assert!(error.to_string().contains("/missing/"), "{error}");
    assert!(!home.path().join(".moebius").exists());
}

#[tokio::test]
async fn a_program_on_the_path_of_this_release_line_is_used_and_nothing_is_downloaded() {
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("nobin");
    std::fs::create_dir(&bin).unwrap();
    let program = bin.join("moebius");
    std::fs::write(&program, fake_program(VERSION)).unwrap();
    std::fs::set_permissions(
        &program,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let local = Local::new(host(home.path(), free_port()), "http://127.0.0.1:1".into());
    local.start().await.unwrap();
    assert!(!home.path().join(".moebius/bin").exists());
    let _ = std::process::Command::new("pkill")
        .args(["-f", &format!("{}/nobin/moebius", home.path().display())])
        .status();
}

#[tokio::test]
async fn another_system_has_no_release_and_nothing_is_offered() {
    let local = Local::new(
        Host::new(&["sh", "-c", "echo 'MOEBIUS-SYSTEM Linux aarch64'", "sh"]),
        "http://127.0.0.1:1".into(),
    );
    match local.state().await {
        State::Unavailable(reason) => assert!(reason.contains("aarch64"), "{reason}"),
        other => panic!("{other:?}"),
    }
}
