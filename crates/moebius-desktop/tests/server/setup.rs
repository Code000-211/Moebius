//! The Setup pages against a real server: models, plugins and settings read, saved to `moebius.toml` and read back,
//! the models the upstreams serve, and the steps to a first card.

use std::time::Duration;

use moebius_desktop::app::{App, Message, Page};
use moebius_desktop::client::{Client, Snapshot};
use moebius_desktop::screens::agents;
use moebius_desktop::screens::setup;
use serde_json::json;

use crate::fixtures::{settle, shown, start_with};

#[tokio::test]
async fn the_client_reads_the_models_plugins_and_settings_of_a_real_server() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _server) = start_with(
        dir.path(),
        "[board]\nround_limit = 4\n\
         [[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"http://127.0.0.1:8080\"\n\
         [teacher]\nmodel = \"teacher-model\"\n\
         [agents.Coder]\nmodel = \"coder-model\"\nthinking = \"low\"\ntoken_budget = 1000\n",
    )
    .await;
    let client = Client::new(settings);

    let models = client.models().await.unwrap();
    assert_eq!(models.upstreams.len(), 1);
    assert_eq!(models.upstreams[0].base_url, "http://127.0.0.1:8080");
    assert!(!models.upstreams[0].api_key_set);
    let coder = models.agents.iter().find(|a| a.name == "Coder").unwrap();
    assert_eq!(
        (
            coder.model.as_str(),
            coder.thinking.as_deref(),
            coder.token_budget
        ),
        ("coder-model", Some("low"), Some(1000))
    );
    assert_eq!(models.teacher.unwrap().model, "teacher-model");
    assert!(models.decision_model.is_none());

    let plugins = client.plugins().await.unwrap();
    assert_eq!(plugins.time_limit_seconds, 30);
    let guard = plugins.plugins.iter().find(|p| p.name == "guard").unwrap();
    assert_eq!(guard.decides, ["tool_call"]);
    assert_eq!(guard.process, None);

    let server = client.server_settings().await.unwrap();
    assert_eq!(server.value("board.round_limit"), &json!(4));
    assert!(server.is_set("board.round_limit"));
    assert_eq!(server.value("listen"), &json!("127.0.0.1:7171"));
    assert!(!server.is_set("listen"));
    assert!(server.value("github.app_id").is_null());
    assert!(server.value("harness.url").is_null());
    assert!(!server.restart_needed);
}

/// Saving in the Settings and Models pages writes `moebius.toml` of a real server and the pages read it back; a key
/// that needs a restart shows the banner, and an agent of the file is edited in the Agents page.
#[tokio::test(flavor = "multi_thread")]
async fn the_apps_settings_models_and_agents_are_saved_to_the_servers_file_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with(
        dir.path(),
        "# kept\n[board]\nround_limit = 4\n[agents.Coder]\nmodel = \"coder-model\"\n",
    )
    .await;
    let client = Client::new(settings.clone());
    let mut app = App::with_snapshot(Snapshot::default());
    let _ = app.update(Message::Checked(Ok((
        settings.clone(),
        client.snapshot().await.unwrap(),
    ))));
    let file = dir.path().join("moebius.toml");

    let task = app.update(Message::Select(Page::Settings));
    settle(&mut app, task).await;
    assert!(shown(&app, "Coding rounds a card"));
    for (key, text) in [
        ("board.round_limit", "9"),
        ("budgets.day_tokens", "5000"),
        ("listen", "127.0.0.1:9"),
    ] {
        let _ = app.update(Message::Setup(setup::Message::Edit(key, text.into())));
    }
    let task = app.update(Message::Setup(setup::Message::SaveSettings));
    settle(&mut app, task).await;
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with("[router]") || text.contains("# kept"),
        "{text}"
    );
    assert!(
        text.contains("round_limit = 9") && text.contains("day_tokens = 5000"),
        "{text}"
    );
    assert!(text.contains("listen = \"127.0.0.1:9\""), "{text}");
    let read = client.server_settings().await.unwrap();
    assert_eq!(read.value("board.round_limit"), &json!(9));
    assert_eq!(read.value("budgets.day_tokens"), &json!(5000));
    assert_eq!(read.restart_keys, ["listen"]);
    assert!(shown(&app, "Saved"));
    assert!(shown(&app, "Restart the server to use: listen"));
    assert_eq!(server.live.get().round_limit, 9);

    // A refused value is the server's message, and the file is as it was.
    let _ = app.update(Message::Setup(setup::Message::Edit(
        "run_limit",
        "0".into(),
    )));
    let task = app.update(Message::Setup(setup::Message::SaveSettings));
    settle(&mut app, task).await;
    assert!(shown(&app, "run_limit must be at least 1"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);

    // An upstream with a typed key: the key is stored by the server, the file names it.
    let task = app.update(Message::Select(Page::Models));
    settle(&mut app, task).await;
    let _ = app.update(Message::Setup(setup::Message::AddUpstream));
    let _ = app.update(Message::Setup(setup::Message::UpstreamUrl(
        0,
        "http://127.0.0.1:1".into(),
    )));
    let owner = setup::Owner::Upstream(0);
    let _ = app.update(Message::Setup(setup::Message::KeyMode(
        owner,
        setup::KeyMode::Typed,
    )));
    let _ = app.update(Message::Setup(setup::Message::KeyTyped(
        owner,
        "sk-typed-by-hand-0123456789".into(),
    )));
    let task = app.update(Message::Setup(setup::Message::SaveModels));
    settle(&mut app, task).await;
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("api_key_secret = \"upstream-1\""), "{text}");
    assert!(!text.contains("sk-typed-by-hand"));
    let read = client.server_settings().await.unwrap();
    assert_eq!(read.secrets, ["upstream-1"]);
    assert_eq!(
        read.value("router.upstreams")[0]["base_url"],
        "http://127.0.0.1:1"
    );
    let stored = std::fs::read_to_string(dir.path().join(".moebius/secrets.toml")).unwrap();
    assert!(stored.contains("sk-typed-by-hand-0123456789"));

    // The agent of the file is opened, changed and saved from the Agents page.
    let task = app.update(Message::Select(Page::Agents));
    settle(&mut app, task).await;
    let task = app.update(Message::Agents(agents::Message::Select("Coder".into())));
    settle(&mut app, task).await;
    let _ = app.update(Message::Agents(agents::Message::ModelEdited(
        "better-model".into(),
    )));
    let _ = app.update(Message::Agents(agents::Message::BudgetEdited("777".into())));
    let task = app.update(Message::Agents(agents::Message::Save));
    settle(&mut app, task).await;
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("model = \"better-model\"") && text.contains("token_budget = 777"),
        "{text}"
    );
    let agents = client.agents().await.unwrap();
    let coder = agents.iter().find(|a| a.name == "Coder").unwrap();
    assert_eq!(
        (coder.model.as_str(), coder.token_budget),
        ("better-model", Some(777))
    );
    assert!(shown(&app, "Saved in moebius.toml."));

    // A restart is asked for with the button; the app then waits for the server (which this test does not start again).
    let task = app.update(Message::Select(Page::Settings));
    settle(&mut app, task).await;
    let task = app.update(Message::Setup(setup::Message::Restart));
    settle(&mut app, task).await;
    tokio::time::timeout(Duration::from_secs(60), server.live.restart_asked())
        .await
        .expect("the server is asked to restart");
    assert!(shown(
        &app,
        "Restarting the server. The app connects again by itself."
    ));
}

/// An upstream that answers every request with a model list, as plain HTTP.
async fn fake_upstream() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buffer = [0u8; 4096];
                let _ = stream.read(&mut buffer).await;
                let body = r#"{"data": [{"id": "fake-small"}, {"id": "fake-large"}]}"#;
                let answer = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(answer.as_bytes()).await;
            });
        }
    });
    url
}

#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_models_the_upstreams_serve_from_a_real_server() {
    let dir = tempfile::tempdir().unwrap();
    let upstream = fake_upstream().await;
    let (settings, _server) = start_with(
        dir.path(),
        &format!("[[router.upstreams]]\nprotocol = \"openai\"\nbase_url = \"{upstream}\"\n"),
    )
    .await;
    let client = Client::new(settings);
    for refresh in [false, true] {
        let available = client.available_models(refresh).await.unwrap();
        let ids: Vec<&str> = available.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["fake-large", "fake-small"]);
        assert_eq!(available.models[0].protocol, "openai");
        assert_eq!(available.models[0].upstream, upstream);
        assert!(available.errors.is_empty());
    }
}

#[tokio::test]
async fn the_client_reads_the_steps_to_a_first_card_of_a_real_server() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _server) = start_with(
        dir.path(),
        "[pi_bolt]\nprogram = \"/no/such/pi-bolt\"\n[agents.Coder]\nmodel = \"coder-model\"\n",
    )
    .await;
    let client = Client::new(settings);
    let start = client.start().await.unwrap();
    assert!(!start.done);
    let keys: Vec<(&str, bool)> = start.steps.iter().map(|s| (&*s.key, s.done)).collect();
    assert_eq!(
        keys,
        [
            ("upstream", false),
            ("pi_bolt", false),
            ("harness", false),
            ("coder", true),
            ("repository", false)
        ]
    );
    assert_eq!(start.steps[0].detail, "Add an upstream on the Models page.");
}
