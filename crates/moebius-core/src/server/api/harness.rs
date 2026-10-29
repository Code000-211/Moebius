//! The agents, skills and MCP servers, read from and edited in the harness configuration repository (see
//! [`crate::harness::repo`]), and setting one up.
//!
//! - `GET /v1/agents`: the agents the server can run. `GET`, `PUT` and `DELETE /v1/agents/{name}`: read, write and delete
//!   an agent of the harness configuration repository; a built-in agent is read without one.
//! - `GET /v1/skills` and `GET /v1/mcp-servers`: the skills and MCP servers an agent can name, and `GET`, `PUT` and
//!   `DELETE` of `/v1/skills/{name}` and `/v1/mcp-servers/{name}` to read and edit them, and of
//!   `/v1/skills/{name}/files/{path}` for a skill's other files.
//! - `POST /v1/harness/setup`: moves the agents and rules of `moebius.toml` into a new harness configuration
//!   repository (see [`crate::server::settings`]).

use std::sync::Arc;

use axum::Extension;
use axum::Router;
use axum::extract::{Path as UrlPath, State};
use axum::routing::{get, post};
use serde::Serialize;

use super::{ApiError, ApiState, blocking, harness_repo, person_only};
use crate::app::AppError;
use crate::board::AgentInfo;
use crate::decide::Actor;
use crate::harness::mcp::McpServer;
use crate::harness::pibolt::{BuiltInAgent, built_in_agent};
use crate::harness::repo::{
    AgentDefinition, AgentView, HarnessRepo, McpServerInfo, McpServerView, SkillFileText,
    SkillInfo, SkillText, SkillView,
};
use crate::repos::files::FileView;
use crate::server::settings::Saved;

/// The routes of the agents, skills and MCP servers.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/agents", get(agents))
        .route(
            "/v1/agents/{name}",
            get(get_agent).put(put_agent).delete(delete_agent),
        )
        .route("/v1/harness/setup", post(setup_harness))
        .route("/v1/skills", get(skills))
        .route(
            "/v1/skills/{name}",
            get(get_skill).put(put_skill).delete(delete_skill),
        )
        .route(
            "/v1/skills/{name}/files/{*path}",
            get(get_skill_file)
                .put(put_skill_file)
                .delete(delete_skill_file),
        )
        .route("/v1/mcp-servers", get(mcp_servers))
        .route(
            "/v1/mcp-servers/{name}",
            get(get_mcp_server)
                .put(put_mcp_server)
                .delete(delete_mcp_server),
        )
}

async fn agents(State(state): State<ApiState>) -> axum::Json<Vec<AgentInfo>> {
    axum::Json(state.runs.agents())
}

/// A person moves the agents and rules of `moebius.toml` into a new harness configuration repository (see
/// `Live::set_up_harness`); the server uses it once it restarts.
async fn setup_harness(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
) -> Result<axum::Json<Saved>, ApiError> {
    person_only(&by)?;
    let live = state.config.clone();
    blocking(&state.app, move |app| live.set_up_harness(app, &by)).await
}

async fn skills(State(state): State<ApiState>) -> Result<axum::Json<Vec<SkillInfo>>, ApiError> {
    let Some(repo) = state.harness_repo.clone() else {
        return Ok(axum::Json(vec![]));
    };
    blocking(&state.app, move |_| Ok(repo.skills())).await
}

async fn mcp_servers(
    State(state): State<ApiState>,
) -> Result<axum::Json<Vec<McpServerInfo>>, ApiError> {
    let Some(repo) = state.harness_repo.clone() else {
        return Ok(axum::Json(vec![]));
    };
    blocking(&state.app, move |_| Ok(repo.mcp_servers())).await
}

/// The harness configuration repository, or the 409 that says skills and MCP servers need one.
fn editable_harness_repo(state: &ApiState) -> Result<Arc<HarnessRepo>, AppError> {
    state.harness_repo.clone().ok_or_else(|| {
        AppError::Conflict(
            "skills and MCP servers are edited in a harness configuration repository, and the server has none".into(),
        )
    })
}

async fn get_skill(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<SkillView>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |_| repo.skill(&name)).await
}

async fn put_skill(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(new): axum::Json<SkillText>,
) -> Result<axum::Json<SkillView>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |app| {
        repo.save_skill(app, &by, &name, &new)
    })
    .await
}

async fn delete_skill(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<()>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |app| repo.delete_skill(app, &by, &name)).await
}

async fn get_skill_file(
    State(state): State<ApiState>,
    UrlPath((name, path)): UrlPath<(String, String)>,
) -> Result<axum::Json<FileView>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |_| repo.skill_file(&name, &path)).await
}

async fn put_skill_file(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((name, path)): UrlPath<(String, String)>,
    axum::Json(new): axum::Json<SkillFileText>,
) -> Result<axum::Json<SkillView>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |app| {
        repo.save_skill_file(app, &by, &name, &path, &new)
    })
    .await
}

async fn delete_skill_file(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath((name, path)): UrlPath<(String, String)>,
) -> Result<axum::Json<SkillView>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |app| {
        repo.delete_skill_file(app, &by, &name, &path)
    })
    .await
}

async fn get_mcp_server(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<McpServerView>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |_| repo.mcp_server(&name)).await
}

async fn put_mcp_server(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(server): axum::Json<McpServer>,
) -> Result<axum::Json<McpServerView>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |app| {
        repo.save_mcp_server(app, &by, &name, &server)
    })
    .await
}

async fn delete_mcp_server(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<()>, ApiError> {
    let repo = editable_harness_repo(&state)?;
    blocking(&state.app, move |app| {
        repo.delete_mcp_server(app, &by, &name)
    })
    .await
}

/// What `GET /v1/agents/{name}` answers: the agent's files, when it has them, and what a built-in agent brings itself.
#[derive(Debug, Serialize)]
struct AgentAnswer {
    name: String,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    definition: Option<AgentDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    built_in: Option<BuiltInAgent>,
}

/// A built-in agent is answered without files and without a harness configuration repository; any other agent needs
/// both.
async fn get_agent(
    State(state): State<ApiState>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<AgentAnswer>, ApiError> {
    let built_in = built_in_agent(&name);
    let repo = match (harness_repo(&state), &built_in) {
        (Ok(repo), _) => Some(repo),
        (Err(_), Some(_)) => None,
        (Err(e), None) => return Err(e.into()),
    };
    blocking(&state.app, move |_| {
        let definition = match repo.map(|repo| repo.agent(&name)) {
            Some(Ok(view)) => Some(view.definition),
            Some(Err(AppError::NotFound(_))) if built_in.is_some() => None,
            Some(Err(e)) => return Err(e),
            None => None,
        };
        Ok(AgentAnswer {
            name,
            definition,
            built_in,
        })
    })
    .await
}

async fn put_agent(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
    axum::Json(definition): axum::Json<AgentDefinition>,
) -> Result<axum::Json<AgentView>, ApiError> {
    let repo = harness_repo(&state)?;
    blocking(&state.app, move |app| {
        repo.save_agent(app, &by, &name, &definition)
    })
    .await
}

async fn delete_agent(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(name): UrlPath<String>,
) -> Result<axum::Json<()>, ApiError> {
    let repo = harness_repo(&state)?;
    blocking(&state.app, move |app| repo.delete_agent(app, &by, &name)).await
}

#[cfg(test)]
// Some tests are Unix only; on Windows what only they use is unused.
#[cfg_attr(not(unix), allow(unused))]
mod tests {
    use std::collections::BTreeMap;

    use crate::app::App;
    use crate::config::AgentConfig;
    use crate::router::Protocol;
    use crate::server::api::ApiState;
    use crate::server::api::testing::{agent, get, post, start, start_with, status, until};
    use crate::store::Run;

    /// The arguments Pi was started with for a run.
    fn argv(app: &App, run_id: i64) -> Vec<String> {
        let file = app.data_dir.join(format!("runs/{run_id}/agent/argv.json"));
        serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
    }

    /// Starts a chat without an agent and returns its run once the agent has answered.
    async fn chat_without_agent(state: &ApiState, addr: &str, repository: Option<i64>) -> Run {
        let body = serde_json::json!({ "message": "Hello", "repository": repository });
        let started = post(state, addr, "/v1/chats".into(), body).await;
        assert_eq!(started.status(), 200);
        let run: Run = started.json().await.unwrap();
        until("the agent has answered", || {
            status(&state.app, run.id) == "waiting"
        })
        .await;
        run
    }

    /// Where the Moebius agent's built-in skills are written: the reference library, then the task skills.
    fn built_in_skill_dirs(app: &App) -> Vec<String> {
        std::iter::once(&crate::harness::skill::MOEBIUS)
            .chain(crate::harness::skill::TASKS.iter())
            .map(|skill| {
                crate::harness::skill::dir(&app.data_dir, skill.name)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[tokio::test]
    async fn a_built_in_agent_shows_its_built_in_instructions_and_skills_without_a_harness_repository()
     {
        let (_dir, state, addr) = start().await;
        let moebius: serde_json::Value = get(&state, &addr, "/v1/agents/Moebius").await;
        assert_eq!(moebius["name"], "Moebius");
        // No files: the agent is the Coder's model and the built-in part.
        assert!(moebius.get("settings").is_none(), "{moebius}");
        let instructions = moebius["built_in"]["instructions"].as_str().unwrap();
        assert!(instructions.starts_with("You are the Moebius agent."));
        let skills = moebius["built_in"]["skills"].as_array().unwrap();
        let names: Vec<&str> = skills.iter().map(|s| s["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "moebius",
                "moebius-operate",
                "moebius-configure",
                "moebius-workflows",
                "moebius-extensions",
                "moebius-plugins"
            ]
        );
        assert!(
            skills
                .iter()
                .all(|s| s["description"].as_str().unwrap().contains("Use it"))
        );

        // The Grilling partner keeps the reference library only.
        let grilling: serde_json::Value = get(&state, &addr, "/v1/agents/Grilling-partner").await;
        assert!(
            grilling["built_in"]["instructions"]
                .as_str()
                .unwrap()
                .starts_with("You are the Grilling partner.")
        );
        assert_eq!(grilling["built_in"]["skills"].as_array().unwrap().len(), 1);
        assert_eq!(grilling["built_in"]["skills"][0]["name"], "moebius");
        // Its engineering skills are shown apart; the Moebius agent has none.
        let engineering: Vec<&str> = grilling["built_in"]["engineering_skills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            engineering,
            ["grilling", "to-tickets", "moebius-autonomous"]
        );
        assert!(
            moebius["built_in"]["engineering_skills"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    // Unix only: the fake Pi is a Python script started by its `#!` line.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_chat_without_an_agent_is_with_the_moebius_agent_on_the_coders_model() {
        let (_dir, state, addr) = start().await;
        let run = chat_without_agent(&state, &addr, None).await;
        assert_eq!(run.agent, "Moebius");
        let argv = argv(&state.app, run.id);
        assert!(argv.windows(2).any(|w| w == ["--model", "fake"]));
        // Only the Coder's model and protocol carry over.
        assert!(!argv.iter().any(|a| a == "--thinking"));
        // The reference library, then each task skill.
        let skills: Vec<&str> = argv
            .windows(2)
            .filter(|w| w[0] == "--skill")
            .map(|w| w[1].as_str())
            .collect();
        assert_eq!(skills, built_in_skill_dirs(&state.app));
        assert_eq!(skills.len(), 6);
        let instructions = argv.windows(2).find(|w| w[0] == "--append-system-prompt");
        let instructions = &instructions.unwrap()[1];
        assert!(instructions.starts_with("You are the Moebius agent."));
        for task in &crate::harness::skill::TASKS {
            assert!(instructions.contains(task.name), "{}", task.name);
        }

        // Asking for the agent by name is the same.
        let named = serde_json::json!({ "agent": "Moebius", "message": "Hello" });
        let named: Run = post(&state, &addr, "/v1/chats".into(), named)
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(named.agent, "Moebius");
    }

    #[tokio::test]
    async fn agents_cannot_be_edited_without_a_harness_repository() {
        let (_dir, state, addr) = start().await;
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/v1/agents/Coder");
        let body = serde_json::json!({ "settings": { "model": "m" } });
        let responses = [
            client.get(&url),
            client.put(&url).json(&body),
            client.delete(&url),
        ];
        for request in responses {
            let response = request.bearer_auth(&*state.token).send().await.unwrap();
            assert_eq!(response.status(), 422);
            assert!(response.text().await.unwrap().contains("moebius.toml"));
        }
        for route in ["skills", "mcp-servers"] {
            let response = client
                .get(format!("http://{addr}/v1/{route}"))
                .bearer_auth(&*state.token)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            assert_eq!(response.text().await.unwrap(), "[]");
        }
    }

    #[tokio::test]
    async fn skills_and_mcp_servers_cannot_be_edited_without_a_harness_repository() {
        let (_dir, state, addr) = start().await;
        let client = reqwest::Client::new();
        for (url, body) in [
            ("skills/x", serde_json::json!({ "description": "d" })),
            (
                "skills/x/files/notes.md",
                serde_json::json!({ "text": "t" }),
            ),
            ("mcp-servers/x", serde_json::json!({ "command": "c" })),
        ] {
            let url = format!("http://{addr}/v1/{url}");
            let requests = [
                client.get(&url),
                client.put(&url).json(&body),
                client.delete(&url),
            ];
            for request in requests {
                let response = request.bearer_auth(&*state.token).send().await.unwrap();
                assert_eq!(response.status(), 409);
                assert!(
                    response
                        .text()
                        .await
                        .unwrap()
                        .contains("harness configuration repository")
                );
            }
        }
    }

    // Unix only: the fake Pi is a Python script started by its `#!` line.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_moebius_agent_configured_has_its_own_model_and_adds_to_what_it_is_told() {
        let own = AgentConfig {
            thinking: Some("high".into()),
            instructions: Some("Prefer small scripts.".into()),
            skills: vec!["/skills/house-style".into()],
            ..agent("own-model")
        };
        // There is no Coder: the Moebius agent needs none.
        let (_dir, state, addr) = start_with(BTreeMap::from([("Moebius".to_string(), own)])).await;
        let run = chat_without_agent(&state, &addr, None).await;
        let argv = argv(&state.app, run.id);
        assert!(argv.windows(2).any(|w| w == ["--model", "own-model"]));
        assert!(argv.windows(2).any(|w| w == ["--thinking", "high"]));
        // The built-in skills always come first.
        let skills: Vec<&str> = argv
            .windows(2)
            .filter(|w| w[0] == "--skill")
            .map(|w| w[1].as_str())
            .collect();
        let mut expected = built_in_skill_dirs(&state.app);
        expected.push("/skills/house-style".into());
        assert_eq!(skills, expected);
        let instructions = argv.windows(2).find(|w| w[0] == "--append-system-prompt");
        let instructions = &instructions.unwrap()[1];
        assert!(instructions.starts_with("You are the Moebius agent."));
        assert!(instructions.ends_with("\n\nPrefer small scripts."));
    }

    #[tokio::test]
    async fn the_agents_are_listed_with_the_built_in_agents_first_and_nothing_secret() {
        let mut reviewer = agent("fast");
        reviewer.protocol = Protocol::Anthropic;
        reviewer.instructions = Some("secret sauce".into());
        let agents = BTreeMap::from([
            ("Coder".to_string(), agent("big")),
            ("Reviewer".to_string(), reviewer),
        ]);
        let (_dir, state, addr) = start_with(agents).await;
        let listed: serde_json::Value = get(&state, &addr, "/v1/agents").await;
        assert_eq!(
            listed,
            serde_json::json!([
                { "name": "Moebius", "model": "big", "protocol": "openai", "harness": "pi-bolt", "built_in": true },
                { "name": "Grilling-partner", "model": "big", "protocol": "openai", "harness": "pi-bolt", "built_in": true },
                { "name": "Coder", "model": "big", "protocol": "openai", "harness": "pi-bolt", "built_in": false },
                { "name": "Reviewer", "model": "fast", "protocol": "anthropic", "harness": "pi-bolt", "built_in": false },
            ])
        );

        // Without a model to give it, the Moebius agent cannot run and is not offered.
        let (_dir, state, addr) = start_with(BTreeMap::new()).await;
        let listed: serde_json::Value = get(&state, &addr, "/v1/agents").await;
        assert_eq!(listed, serde_json::json!([]));
    }

    #[tokio::test]
    async fn the_moebius_agent_without_a_model_to_use_cannot_start_a_chat() {
        let (_dir, state, addr) = start_with(BTreeMap::new()).await;
        let body = serde_json::json!({ "message": "Hello" });
        let refused = post(&state, &addr, "/v1/chats".into(), body).await;
        assert_eq!(refused.status(), 422);
        let message = refused.text().await.unwrap();
        assert!(message.contains("[agents.Moebius]"), "{message}");
        assert!(state.app.store.chats().unwrap().is_empty());
    }
}
