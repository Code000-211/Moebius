//! The settings plugins ask of a person, over HTTP: a plugin in its own process declares project and app settings in its
//! manifest, a person reads and saves them (a project's as files of the harness configuration repository, the server's
//! in the store), an agent may read but never save, and the plugin's own page is served as a script that cannot be
//! sniffed into anything else.

// Unix only: the plugin is the Python script of the plugin API's tests, started by its `#!` line.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::router::Grant;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use reqwest::Method;
use serde_json::{Value, json};

struct Setup {
    server: Server,
    api: String,
    agent_token: String,
    client: reqwest::Client,
}

impl Setup {
    /// A server with the plugin `box`, whose manifest declares one project setting, one app setting and a page of its
    /// own; with a harness configuration repository `dir/harness` unless `harness` is false.
    async fn start(dir: &Path, harness: bool) -> Self {
        let plugin = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("moebius-plugin-api")
            .join("testdata")
            .join("plugin.py")
            .canonicalize()
            .unwrap();
        let mut text = String::new();
        if harness {
            let repository = dir.join("harness");
            moebius_core::harness::repo::create(&repository, &Default::default(), None).unwrap();
            text += &format!("[harness]\nurl = {:?}\n", repository.display().to_string());
        }
        text += &format!(
            "[[plugins]]\nname = \"box\"\npath = {:?}\nargs = [\"settings\", \"box\"]\n",
            plugin.display().to_string()
        );
        let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            sessions: Default::default(),
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            config: server.live.clone(),
            token: server.token.clone(),
            router: server.router.clone(),
        };
        tokio::spawn(api::serve(listener, state));
        let run = server
            .app
            .store
            .add_run(None, "Moebius", "pi-bolt", "chat")
            .unwrap();
        let agent_token = server.router.issue_token(Grant {
            run_id: run.id,
            card_id: None,
            budget: None,
        });
        Self {
            server,
            api,
            agent_token,
            client: reqwest::Client::new(),
        }
    }

    fn person(&self) -> String {
        self.server.token.to_string()
    }

    async fn send(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> reqwest::Response {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.api))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        request.send().await.unwrap()
    }

    async fn call(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> (u16, Value) {
        let response = self.send(token, method, path, body).await;
        let status = response.status().as_u16();
        let text = response.text().await.unwrap();
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    fn repository(&self, dir: &Path) -> i64 {
        let source = dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(source.join("README.md"), "hello\n").unwrap();
        git(&source, &["add", "-A"]).unwrap();
        git(&source, &["commit", "--quiet", "-m", "first"]).unwrap();
        self.server
            .app
            .add_repository("demo", source.to_str().unwrap())
            .unwrap()
            .id
    }

    fn clone_path(&self) -> PathBuf {
        self.server
            .harness_repo
            .as_ref()
            .unwrap()
            .path()
            .to_path_buf()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_saves_a_projects_settings_as_files_and_an_agent_only_reads() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), true).await;
    let id = setup.repository(dir.path());
    let person = setup.person();
    let (list, one) = (
        format!("/v1/repositories/{id}/plugin-settings"),
        format!("/v1/repositories/{id}/plugin-settings/box"),
    );

    let (status, view) = setup.call(&person, Method::GET, &list, None).await;
    assert_eq!(status, 200, "{view}");
    assert_eq!(view["repository"], id);
    assert_eq!(view["plugins"][0]["plugin"], "box");
    assert_eq!(view["plugins"][0]["ui"], true);
    assert_eq!(view["plugins"][0]["settings"][0]["key"], "image");
    assert_eq!(view["plugins"][0]["settings"][0]["input"], "code");

    // The Moebius agent reads the settings but a person alone saves them.
    let agent = setup.agent_token.clone();
    assert_eq!(setup.call(&agent, Method::GET, &list, None).await.0, 200);
    let body = Some(json!({ "values": { "image": "FROM debian" }, "reason": "agent" }));
    assert_eq!(
        setup.call(&agent, Method::PUT, &one, body.clone()).await.0,
        403
    );
    let app_body = Some(json!({ "values": { "registry": "r.example" } }));
    assert_eq!(
        setup
            .call(&agent, Method::PUT, "/v1/plugins/box/settings", app_body)
            .await
            .0,
        403
    );
    assert!(
        setup
            .server
            .app
            .store
            .app_settings("box")
            .unwrap()
            .is_empty()
    );
    assert!(
        !setup
            .clone_path()
            .join("repos")
            .join("demo")
            .join("plugins")
            .exists()
    );

    // A key the plugin does not declare is refused and a save is a commit with its event.
    let wrong = Some(json!({ "values": { "nothing": "x" } }));
    assert_eq!(setup.call(&person, Method::PUT, &one, wrong).await.0, 422);
    let mut events = setup.server.app.bus.subscribe();
    let body = Some(json!({ "values": { "image": "FROM debian" }, "reason": "a base" }));
    let (status, saved) = setup.call(&person, Method::PUT, &one, body).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["settings"][0]["value"], "FROM debian");
    let origin = dir.path().join("harness");
    assert_eq!(
        git(&origin, &["show", "main:repos/demo/plugins/box/image"]).unwrap(),
        "FROM debian"
    );
    let event = events.try_recv().unwrap();
    assert_eq!(event.kind, "project_settings_changed");
    assert_eq!(
        event.data,
        json!({ "repository": id, "file": "plugins/box" })
    );
    let (_, again) = setup.call(&person, Method::GET, &list, None).await;
    assert_eq!(again["plugins"][0]["settings"][0]["value"], "FROM debian");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_without_a_harness_repository_keeps_no_project_settings() {
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), false).await;
    let id = setup.repository(dir.path());
    let body = Some(json!({ "values": { "image": "FROM debian" } }));
    let (status, why) = setup
        .call(
            &setup.person(),
            Method::PUT,
            &format!("/v1/repositories/{id}/plugin-settings/box"),
            body,
        )
        .await;
    assert_eq!(status, 409, "{why}");
    assert!(why.as_str().unwrap().contains("[harness]"), "{why}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_servers_settings_are_saved_by_a_person_and_the_page_of_a_plugin_is_served_as_a_script()
{
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::start(dir.path(), true).await;
    let person = setup.person();
    let (status, saved) = setup
        .call(
            &person,
            Method::PUT,
            "/v1/plugins/box/settings",
            Some(json!({ "values": { "registry": "r.example" } })),
        )
        .await;
    assert_eq!(status, 200, "{saved}");
    let (_, view) = setup
        .call(&person, Method::GET, "/v1/plugins/settings", None)
        .await;
    assert_eq!(view["plugins"][0]["plugin"], "box");
    assert_eq!(view["plugins"][0]["settings"][0]["value"], "r.example");
    // The decision records which keys changed, never the values.
    let decisions = setup
        .server
        .app
        .store
        .judgements("edit_app_settings")
        .unwrap();
    assert_eq!(decisions.len(), 1);
    assert!(!format!("{:?}", decisions[0]).contains("r.example"));

    let response = setup
        .send(&person, Method::GET, "/v1/plugins/box/settings-ui", None)
        .await;
    assert_eq!(response.status().as_u16(), 200);
    let header = |name: &str| response.headers()[name].to_str().unwrap().to_string();
    assert_eq!(header("content-type"), "text/javascript; charset=utf-8");
    assert_eq!(header("cache-control"), "no-store");
    assert_eq!(header("x-content-type-options"), "nosniff");
    assert_eq!(
        response.text().await.unwrap(),
        "export default function () {}"
    );
    assert_eq!(
        setup
            .call(&person, Method::GET, "/v1/plugins/nobody/settings-ui", None)
            .await
            .0,
        404
    );
}
