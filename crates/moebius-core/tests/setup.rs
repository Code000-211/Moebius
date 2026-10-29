//! The Models, Plugins and Settings answers of the desktop API tell how the server is set up and never show a secret:
//! not the value of an environment variable a key is read from, not the GitHub App's key, not the password in a URL.

use std::path::Path;

use moebius_core::config::Config;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use serde_json::{Value, json};

const HARNESS_PASSWORD: &str = "harness-pass-0451";
const UPSTREAM_PASSWORD: &str = "upstream-pass-7788";
const MODEL_PASSWORD: &str = "model-pass-3320";

async fn get(base: &str, token: &str, path: &str) -> (Value, String) {
    let response = reqwest::Client::new()
        .get(format!("{base}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "{path}");
    let text = response.text().await.unwrap();
    (serde_json::from_str(&text).unwrap(), text)
}

#[tokio::test]
async fn the_answers_tell_how_the_server_is_set_up_and_hold_no_secret() {
    let dir = tempfile::tempdir().unwrap();
    // The value of a variable that is set in every environment stands for a key: it is long and has no other reason
    // to be in an answer.
    let key = std::env::var("PATH").unwrap();
    let pem = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/github-app-test-key.pem");
    let text = format!(
        r#"
data_dir = "data"
listen = "127.0.0.1:7999"
run_limit = 2
retro_at = "21:05"

[trial_window]
start = "01:30"
end = "05:00"

[board]
tick_ms = 500
round_limit = 4

[budgets]
day_tokens = 5000000

[[router.upstreams]]
protocol = "openai"
base_url = "http://user:{UPSTREAM_PASSWORD}@127.0.0.1:8080"

[[router.upstreams]]
protocol = "anthropic"
base_url = "https://api.example.com"
api_key_env = "PATH"

[teacher]
model = "teacher-model"
protocol = "anthropic"
timeout_seconds = 12

[decision_model]
url = "http://decider:{MODEL_PASSWORD}@127.0.0.1:8081/v1"
model = "decider-model"
api_key_env = "PATH"
timeout_seconds = 9

[github]
app_id = 424242
private_key_file = "{}"

[agents.Coder]
model = "coder-model"
thinking = "medium"
token_budget = 2000000

[agents.Reviewer]
model = "reviewer-model"
protocol = "anthropic"
"#,
        pem.display()
    );
    let file = dir.path().join("moebius.toml");
    std::fs::write(&file, &text).unwrap();
    let config = Config::parse(&text, dir.path(), &file).unwrap();
    let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    // The harness repository is only added to the file afterwards, since cloning one needs a server to clone from: a
    // restart would use it.
    std::fs::write(
        &file,
        format!(
            "{text}\n[harness]\nurl = \"https://puller:{HARNESS_PASSWORD}@example.com/acme/harness.git\"\nmcp_env = [\"DOCS_TOKEN\"]\n"
        ),
    )
    .unwrap();
    let state = ApiState {
        app: server.app.clone(),
        runs: server.harness.clone(),
        harness_repo: None,
        token: server.token.clone(),
        router: server.router.clone(),
        config: server.live.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(api::serve(listener, state));
    let token = server.token.to_string();

    let (models, models_text) = get(&base, &token, "/v1/models").await;
    assert_eq!(
        models["upstreams"],
        json!([
            {"protocol": "openai", "base_url": "http://127.0.0.1:8080", "api_key_set": false, "oauth": null},
            {"protocol": "anthropic", "base_url": "https://api.example.com", "api_key_set": true, "oauth": null},
        ])
    );
    let names: Vec<&str> = models["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|agent| agent["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Moebius", "Grilling-partner", "Coder", "Reviewer"]);
    let coder = &models["agents"][2];
    assert_eq!(coder["model"], "coder-model");
    assert_eq!(coder["thinking"], "medium");
    assert_eq!(coder["token_budget"], 2_000_000);
    assert_eq!(models["agents"][3]["protocol"], "anthropic");
    assert!(models["agents"][3].get("thinking").is_none());
    assert_eq!(
        models["teacher"],
        json!({"model": "teacher-model", "protocol": "anthropic", "timeout_seconds": 12})
    );
    assert_eq!(
        models["decision_model"],
        json!({"url": "http://127.0.0.1:8081/v1", "model": "decider-model", "timeout_seconds": 9, "api_key_set": true})
    );

    let (plugins, plugins_text) = get(&base, &token, "/v1/plugins").await;
    assert_eq!(plugins["time_limit_seconds"], 30);
    let plugin = |name: &str| {
        plugins["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|plugin| plugin["name"] == name)
            .unwrap_or_else(|| panic!("no plugin {name}: {plugins}"))
            .clone()
    };
    assert_eq!(plugin("guard")["decides"], json!(["tool_call"]));
    assert_eq!(plugin("guard")["kinds"], json!(["decider"]));
    assert_eq!(plugin("decision-model")["decides"], json!([]));

    let (settings, settings_text) = get(&base, &token, "/v1/settings").await;
    assert_eq!(settings["file"], file.display().to_string());
    let value = |key: &str| settings["values"][key].clone();
    assert_eq!(
        value("data_dir"),
        json!({"value": "data", "default": false})
    );
    assert_eq!(
        value("listen"),
        json!({"value": "127.0.0.1:7999", "default": false})
    );
    assert_eq!(value("round_limit"), Value::Null);
    assert_eq!(
        value("board.round_limit"),
        json!({"value": 4, "default": false})
    );
    assert_eq!(value("run_limit"), json!({"value": 2, "default": false}));
    assert_eq!(
        value("budgets.day_tokens"),
        json!({"value": 5_000_000, "default": false})
    );
    assert_eq!(
        value("retro_at"),
        json!({"value": "21:05", "default": false})
    );
    assert_eq!(value("trial_window.start")["value"], "01:30");
    assert_eq!(value("trial_window.end")["value"], "05:00");
    assert_eq!(value("board.tick_ms")["value"], 500);
    assert_eq!(
        value("checks.timeout_seconds"),
        json!({"value": 1800, "default": true})
    );
    assert_eq!(
        value("harness.url")["value"],
        "https://example.com/acme/harness.git"
    );
    assert_eq!(value("harness.mcp_env")["value"], json!(["DOCS_TOKEN"]));
    assert_eq!(value("github.app_id")["value"], 424242);
    assert_eq!(
        value("github.private_key_file")["value"],
        pem.display().to_string()
    );
    assert_eq!(value("teacher.model")["value"], "teacher-model");
    assert_eq!(value("decision_model.api_key_env")["value"], "PATH");
    assert_eq!(
        value("decision_model.url")["value"],
        "http://127.0.0.1:8081/v1"
    );
    assert_eq!(value("router.upstreams")["value"][1]["api_key_env"], "PATH");
    assert_eq!(value("agents")["value"]["Coder"]["model"], "coder-model");
    // The harness repository is in the file only, so a restart is needed for it.
    assert_eq!(settings["restart_needed"], true);
    assert_eq!(
        settings["restart_keys"],
        json!(["harness.mcp_env", "harness.url"])
    );
    assert_eq!(settings["secrets"], json!([]));

    for answer in [&models_text, &plugins_text, &settings_text] {
        for secret in [
            key.as_str(),
            "BEGIN",
            HARNESS_PASSWORD,
            UPSTREAM_PASSWORD,
            MODEL_PASSWORD,
        ] {
            assert!(!answer.contains(secret), "{secret:?} is in {answer}");
        }
    }
}

#[tokio::test]
async fn a_server_without_a_teacher_a_decision_model_or_github_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
    let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
    let state = ApiState {
        app: server.app.clone(),
        runs: server.harness.clone(),
        harness_repo: None,
        token: server.token.clone(),
        router: server.router.clone(),
        config: server.live.clone(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(api::serve(listener, state));
    let token = server.token.to_string();

    let (models, _) = get(&base, &token, "/v1/models").await;
    assert_eq!(models["upstreams"], json!([]));
    assert_eq!(models["teacher"], Value::Null);
    assert_eq!(models["decision_model"], Value::Null);
    let (settings, _) = get(&base, &token, "/v1/settings").await;
    assert_eq!(
        settings["values"]["github.app_id"],
        json!({"value": null, "default": true})
    );
    assert_eq!(
        settings["values"]["harness.url"],
        json!({"value": null, "default": true})
    );
    assert_eq!(
        settings["values"]["budgets.day_tokens"],
        json!({"value": null, "default": true})
    );
    assert_eq!(
        settings["values"]["harness.mcp_env"],
        json!({"value": [], "default": true})
    );
    assert_eq!(settings["restart_needed"], false);
}
