//! The terminals, each a WebSocket that only a person opens (see [`crate::harness::terminal`]).
//!
//! - `GET /v1/runs/{id}/terminal`: a shell in the run's worktree.
//! - `GET /v1/chats/{id}/tui`: Pi-Bolt's own terminal interface in a waiting chat's worktree, continuing the chat's
//!   conversation (see [`crate::harness::pibolt`]).
//! - `GET /v1/terminal`: a shell in the directory of `moebius.toml`, recorded as an `open_terminal` decision.
//! - `GET /v1/repositories/{id}/terminal`: a shell in the repository's clone, recorded the same way.

use std::path::PathBuf;

use axum::Extension;
use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path as UrlPath, State};
use axum::response::Response;
use axum::routing::get;

use super::{ApiError, ApiState, blocking, person_only};
use crate::decide::Actor;

/// The routes of the terminals.
pub(super) fn routes() -> Router<ApiState> {
    Router::new()
        .route("/v1/runs/{id}/terminal", get(run_terminal))
        .route("/v1/chats/{id}/tui", get(chat_tui))
        .route("/v1/terminal", get(server_terminal))
        .route("/v1/repositories/{id}/terminal", get(repository_terminal))
}

async fn run_terminal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    person_only(&by)?;
    let axum::Json((worktree, slot)) = blocking(&state.app, move |app| app.open_shell(id)).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        crate::harness::terminal::serve(socket, &worktree, slot).await
    }))
}

async fn chat_tui(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    person_only(&by)?;
    let runs = state.runs.clone();
    let axum::Json((tui, mut slot)) =
        blocking(&state.app, move |app| app.open_tui(runs, id)).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        let command = crate::harness::terminal::on_terminal(&tui.command);
        crate::harness::terminal::serve_command(socket, command, &mut slot).await;
        // The chat's own process is back before the chat takes a message again.
        let _ = tokio::task::spawn_blocking(move || drop(tui)).await;
        drop(slot);
    }))
}

async fn server_terminal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    person_only(&by)?;
    let file = state.config.get().file.clone();
    let dir = match file.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let work = dir.clone();
    let axum::Json(slot) = blocking(&state.app, move |app| app.open_server_shell(&work)).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        crate::harness::terminal::serve(socket, &dir, slot).await
    }))
}

async fn repository_terminal(
    State(state): State<ApiState>,
    Extension(by): Extension<Actor>,
    UrlPath(id): UrlPath<i64>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    person_only(&by)?;
    let axum::Json((dir, slot)) =
        blocking(&state.app, move |app| app.open_repository_shell(id)).await?;
    Ok(upgrade.on_upgrade(move |socket| async move {
        crate::harness::terminal::serve(socket, &dir, slot).await
    }))
}

// Unix only: the shells and the fake Pi on the terminals are Unix programs.
#[cfg(all(test, unix))]
mod tests {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    use crate::app::Chat;
    use crate::server::api::testing::{chat, end, get, post, say, start, status, until};
    use crate::store::Run;

    type Terminal = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn open_terminal(
        addr: &str,
        token: &str,
        run: i64,
    ) -> Result<Terminal, tokio_tungstenite::tungstenite::Error> {
        open_at(addr, token, &format!("/v1/runs/{run}/terminal")).await
    }

    async fn open_at(
        addr: &str,
        token: &str,
        path: &str,
    ) -> Result<Terminal, tokio_tungstenite::tungstenite::Error> {
        let mut request = format!("ws://{addr}{path}").into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        tokio_tungstenite::connect_async(request)
            .await
            .map(|(socket, _)| socket)
    }

    /// Reads what the shell prints until `pattern` matches it, and returns the match's first group.
    async fn read_until(terminal: &mut Terminal, pattern: &str) -> String {
        let pattern = regex::Regex::new(pattern).unwrap();
        let mut printed = String::new();
        let read = async {
            while let Some(message) = terminal.next().await {
                if let tokio_tungstenite::tungstenite::Message::Binary(bytes) = message.unwrap() {
                    printed.push_str(&String::from_utf8_lossy(&bytes));
                    if let Some(found) = pattern.captures(&printed) {
                        return found[1].to_string();
                    }
                }
            }
            panic!("the terminal closed before {pattern} was printed: {printed:?}");
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), read)
            .await
            .expect("the shell printed nothing in a minute")
    }

    async fn type_in(terminal: &mut Terminal, text: &str) {
        use futures_util::SinkExt;
        let bytes = text.as_bytes().to_vec().into();
        terminal
            .send(tokio_tungstenite::tungstenite::Message::Binary(bytes))
            .await
            .unwrap();
    }

    /// The status a refused WebSocket upgrade was answered with.
    fn status_of(e: tokio_tungstenite::tungstenite::Error) -> u16 {
        match e {
            tokio_tungstenite::tungstenite::Error::Http(response) => response.status().as_u16(),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn a_person_opens_a_shell_in_the_worktree_of_a_running_chat() {
        use futures_util::SinkExt;
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let started = post(
            &state,
            &addr,
            "/v1/chats".into(),
            serde_json::json!({ "agent": "Coder", "message": "Work for ever [hang]" }),
        )
        .await;
        let chat: Run = started.json().await.unwrap();
        until("the agent works", || {
            app.store.run(chat.id).unwrap().unwrap().status == "running"
        })
        .await;
        let worktree = app.store.run(chat.id).unwrap().unwrap().worktree.unwrap();

        let mut terminal = open_terminal(&addr, &state.token, chat.id).await.unwrap();
        type_in(&mut terminal, "echo moebius-$((40+2))\n").await;
        assert_eq!(
            read_until(&mut terminal, "(moebius-42)").await,
            "moebius-42"
        );

        // The shell starts in the worktree and takes the size it is sent.
        type_in(&mut terminal, "echo where-$(pwd -P)-end\n").await;
        let printed = read_until(&mut terminal, "where-(/[^\\s]*)-end").await;
        assert_eq!(
            std::path::Path::new(&printed),
            std::fs::canonicalize(&worktree).unwrap()
        );
        let resize = r#"{"type":"resize","cols":100,"rows":30}"#;
        terminal.send(resize.into()).await.unwrap();
        type_in(&mut terminal, "stty size\n").await;
        assert_eq!(read_until(&mut terminal, "(30 100)").await, "30 100");

        // What a person typed is not stored; that a shell was opened is.
        let events = app.store.events(chat.id).unwrap();
        let opened: Vec<_> = events.iter().filter(|e| e.kind == "shell_opened").collect();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].data, serde_json::json!({ "who": "person" }));
        assert!(opened[0].at > 0);
        assert!(!format!("{events:?}").contains("moebius-42"));

        // Closing the socket kills the shell.
        type_in(&mut terminal, "echo pid-$$-end\n").await;
        let pid = read_until(&mut terminal, "pid-(\\d+)-end").await;
        terminal.close(None).await.unwrap();
        until("the shell is gone", || {
            !std::path::Path::new(&format!("/proc/{pid}")).exists()
        })
        .await;

        // A shell that exits closes the socket.
        let mut terminal = open_terminal(&addr, &state.token, chat.id).await.unwrap();
        type_in(&mut terminal, "exit\n").await;
        let closed = async {
            while let Some(message) = terminal.next().await {
                if message.is_err() || message.unwrap().is_close() {
                    break;
                }
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), closed)
            .await
            .expect("the socket stays open after the shell exits");
    }

    #[tokio::test]
    async fn a_shell_is_closed_when_its_run_is_stopped() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let started = post(
            &state,
            &addr,
            "/v1/chats".into(),
            serde_json::json!({ "agent": "Coder", "message": "Work for ever [hang]" }),
        )
        .await;
        let chat: Run = started.json().await.unwrap();
        until("the agent works", || {
            app.store.run(chat.id).unwrap().unwrap().status == "running"
        })
        .await;
        let mut terminal = open_terminal(&addr, &state.token, chat.id).await.unwrap();
        type_in(&mut terminal, "echo pid-$$-end\n").await;
        let pid = read_until(&mut terminal, "pid-(\\d+)-end").await;

        let stopped = post(
            &state,
            &addr,
            format!("/v1/runs/{}/stop", chat.id),
            serde_json::json!({ "reason": "enough" }),
        )
        .await;
        assert_eq!(stopped.status(), 200);
        let closed = async {
            while let Some(message) = terminal.next().await {
                if message.is_err() || message.unwrap().is_close() {
                    break;
                }
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(60), closed)
            .await
            .expect("the terminal stays open after its run ended");
        until("the shell is gone", || {
            !std::path::Path::new(&format!("/proc/{pid}")).exists()
        })
        .await;
    }

    #[tokio::test]
    async fn a_shell_needs_the_token_a_worktree_and_a_free_place() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let run = app.store.add_run(Some(1), "Coder", "test", "code").unwrap();
        let wd = tempfile::tempdir().unwrap();
        app.store
            .set_run_worktree(run.id, &wd.path().to_string_lossy())
            .unwrap();

        // A run that is not over has a worktree; with the wrong token nobody gets in.
        app.store.set_run_status(run.id, "running").unwrap();
        assert_eq!(
            status_of(open_terminal(&addr, "nope", run.id).await.unwrap_err()),
            401
        );
        assert_eq!(
            status_of(
                open_terminal(&addr, &state.token, run.id + 1)
                    .await
                    .unwrap_err()
            ),
            404
        );

        // At most a few at once, and a closed one makes room.
        let mut open = Vec::new();
        for _ in 0..crate::harness::terminal::MAX_PER_RUN {
            open.push(open_terminal(&addr, &state.token, run.id).await.unwrap());
        }
        let refused = open_terminal(&addr, &state.token, run.id).await;
        assert_eq!(status_of(refused.unwrap_err()), 422);
        let events = app.store.events(run.id).unwrap();
        assert_eq!(
            events.iter().filter(|e| e.kind == "shell_opened").count(),
            4
        );
        open.pop().unwrap().close(None).await.unwrap();
        until("a place is free", || app.terminals.take(run.id).is_some()).await;
        drop(open);

        // Once the run is over its worktree is gone.
        app.store.set_run_status(run.id, "succeeded").unwrap();
        let finished = open_terminal(&addr, &state.token, run.id).await;
        assert_eq!(status_of(finished.unwrap_err()), 422);
    }

    #[tokio::test]
    async fn a_person_opens_a_shell_in_the_directory_of_the_configuration_file() {
        use futures_util::SinkExt;
        let (dir, state, addr) = start().await;
        let mut terminal = open_at(&addr, &state.token, "/v1/terminal").await.unwrap();
        type_in(&mut terminal, "echo moebius-$((40+2))\n").await;
        assert_eq!(
            read_until(&mut terminal, "(moebius-42)").await,
            "moebius-42"
        );
        type_in(&mut terminal, "echo where-$(pwd -P)-end\n").await;
        let printed = read_until(&mut terminal, "where-(/[^\\s]*)-end").await;
        assert_eq!(
            std::path::Path::new(&printed),
            std::fs::canonicalize(dir.path()).unwrap()
        );
        let resize = r#"{"type":"resize","cols":100,"rows":30}"#;
        terminal.send(resize.into()).await.unwrap();
        type_in(&mut terminal, "stty size\n").await;
        assert_eq!(read_until(&mut terminal, "(30 100)").await, "30 100");

        // That a shell was opened is a decision; what was typed is not stored.
        let opened = state.app.store.judgements("open_terminal").unwrap();
        assert_eq!(opened.len(), 1);
        assert_eq!(opened[0].decider, "person");
        assert_eq!(opened[0].choice, "open_terminal");

        // Closing the socket kills the shell.
        type_in(&mut terminal, "echo pid-$$-end\n").await;
        let pid = read_until(&mut terminal, "pid-(\\d+)-end").await;
        terminal.close(None).await.unwrap();
        until("the shell is gone", || {
            !std::path::Path::new(&format!("/proc/{pid}")).exists()
        })
        .await;
    }

    #[tokio::test]
    async fn a_waiting_chat_goes_on_in_pis_terminal_as_the_same_conversation() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let run = chat(&state, &addr, "Hello", None).await;
        let run_dir = app.data_dir.join(format!("runs/{}", run.id));
        let read = |file: &str| -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(run_dir.join(file)).unwrap()).unwrap()
        };
        let argv = || -> Vec<String> { serde_json::from_value(read("agent/argv.json")).unwrap() };
        let pid = || std::fs::read_to_string(run_dir.join("agent/pid.txt")).unwrap();
        let rpc_argv = argv();
        let rpc_pid = pid();
        let token = read("agent/env.json")["MOEBIUS_RUN_TOKEN"].clone();
        let seen = app.store.events(run.id).unwrap().len();

        let mut tui = open_at(&addr, &state.token, &format!("/v1/chats/{}/tui", run.id))
            .await
            .unwrap();
        read_until(&mut tui, "(tui ready)").await;

        // The chat's own Pi has ended, and Pi's interactive mode continues the chat's session with the chat's agent
        // directory and token: it is the same command without `--mode rpc`, and with `--continue`.
        assert!(!std::path::Path::new(&format!("/proc/{rpc_pid}")).exists());
        let interactive = argv();
        assert!(
            !interactive.contains(&"--mode".to_string()),
            "{interactive:?}"
        );
        assert!(interactive.contains(&"--continue".to_string()));
        let value_of = |argv: &[String], flag: &str| {
            argv[argv.iter().position(|a| a == flag).unwrap() + 1].clone()
        };
        let sessions = value_of(&interactive, "--session-dir");
        assert_eq!(sessions, value_of(&rpc_argv, "--session-dir"));
        assert!(sessions.ends_with("/sessions"), "{sessions}");
        assert_eq!(value_of(&interactive, "--model"), "fake");
        assert_eq!(
            interactive.last(),
            rpc_argv.last(),
            "the bridge is loaded last"
        );
        assert_eq!(read("agent/env.json")["MOEBIUS_RUN_TOKEN"], token);
        assert_eq!(status(app, run.id), "waiting");

        // What is said in the terminal is recorded as the chat's events, in the shapes of Pi's records.
        type_in(&mut tui, "notes\n").await;
        assert_eq!(read_until(&mut tui, "tui heard: (notes)").await, "notes");
        // The turn's end is recorded after its answer.
        until("the answer is recorded", || {
            app.store.events(run.id).unwrap()[seen..]
                .iter()
                .any(|e| e.kind == "agent_end")
        })
        .await;
        let events = app.store.events(run.id).unwrap();
        let added: Vec<(&str, serde_json::Value)> = events[seen..]
            .iter()
            .map(|e| (e.kind.as_str(), e.data.clone()))
            .collect();
        let delta = |kind: &str, index: usize, text: &str| serde_json::json!({ "assistantMessageEvent": { "type": kind, "contentIndex": index, "delta": text } });
        assert_eq!(
            added[0],
            ("tui_opened", serde_json::json!({ "who": "person" }))
        );
        assert_eq!(
            added[1..8],
            [
                ("said", serde_json::json!({ "message": "notes" })),
                ("agent_start", serde_json::json!({})),
                ("message_update", delta("thinking_delta", 0, "Let me look.")),
                ("message_update", delta("text_delta", 1, "Reading.")),
                (
                    "tool_execution_start",
                    serde_json::json!({ "toolCallId": "tui-1", "toolName": "read", "args": { "path": "notes.txt" } })
                ),
                (
                    "tool_execution_end",
                    serde_json::json!({ "toolCallId": "tui-1", "toolName": "read", "isError": true, "durationMs": 3,
                        "result": { "content": [{ "type": "text", "text": "no such file" }] } })
                ),
                ("message_update", delta("text_delta", 0, "You said: notes")),
            ]
        );
        assert_eq!(added[8].0, "agent_end");
        until("the chat waits", || status(app, run.id) == "waiting").await;
        assert_eq!(
            app.run(run.id).unwrap().summary.as_deref(),
            Some("You said: notes")
        );

        // The chat takes no message meanwhile, nor a second terminal.
        assert_eq!(say(&state, &addr, run.id, "Again").await.status(), 409);
        let second = open_at(&addr, &state.token, &format!("/v1/chats/{}/tui", run.id)).await;
        assert_eq!(status_of(second.unwrap_err()), 409);

        // Closing the terminal starts the chat's Pi again on the same session, and the chat takes a message again.
        let tui_pid = pid();
        tui.close(None).await.unwrap();
        until("the chat takes a message", || {
            !app.terminals
                .is_open(crate::harness::terminal::tui_key(run.id))
        })
        .await;
        assert!(!std::path::Path::new(&format!("/proc/{tui_pid}")).exists());
        until("the chat's Pi has started", || pid() != tui_pid).await;
        let again = argv();
        assert_eq!(value_of(&again, "--mode"), "rpc");
        assert!(again.contains(&"--continue".to_string()), "{again:?}");
        assert_eq!(status(app, run.id), "waiting");
        assert_eq!(say(&state, &addr, run.id, "Again").await.status(), 200);
        until("the agent answers", || {
            app.store.events(run.id).unwrap().iter().any(|e| {
                e.kind == "message_update"
                    && e.data["assistantMessageEvent"]["delta"] == "You said: Again"
            })
        })
        .await;
    }

    #[tokio::test]
    async fn a_chat_ended_in_its_terminal_ends_when_the_terminal_closes() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let run = chat(&state, &addr, "Hello", None).await;
        let mut tui = open_at(&addr, &state.token, &format!("/v1/chats/{}/tui", run.id))
            .await
            .unwrap();
        read_until(&mut tui, "(tui ready)").await;
        let pid =
            std::fs::read_to_string(app.data_dir.join(format!("runs/{}/agent/pid.txt", run.id)))
                .unwrap();

        // Ending the chat closes its terminal, and the chat ends without its Pi starting again.
        assert_eq!(end(&state, &addr, run.id).await.status(), 200);
        until("the chat has ended", || {
            app.store.run(run.id).unwrap().unwrap().ended_at.is_some()
        })
        .await;
        assert_eq!(status(app, run.id), "succeeded");
        assert_eq!(
            std::fs::read_to_string(app.data_dir.join(format!("runs/{}/agent/pid.txt", run.id)))
                .unwrap(),
            pid,
            "no Pi was started after the terminal's"
        );
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }

    #[tokio::test]
    async fn a_chat_started_without_a_message_waits_and_opens_in_its_terminal_at_once() {
        let (_dir, state, addr) = start().await;
        let app = &state.app;
        let body =
            serde_json::json!({ "agent": "Coder", "model": "big-model", "thinking": "high" });
        let started = post(&state, &addr, "/v1/chats".into(), body).await;
        assert_eq!(started.status(), 200);
        let run: Run = started.json().await.unwrap();
        assert_eq!(run.status, "waiting");
        let mut tui = open_at(&addr, &state.token, &format!("/v1/chats/{}/tui", run.id))
            .await
            .unwrap();
        read_until(&mut tui, "(tui ready)").await;
        let argv: Vec<String> = serde_json::from_slice(
            &std::fs::read(
                app.data_dir
                    .join(format!("runs/{}/agent/argv.json", run.id)),
            )
            .unwrap(),
        )
        .unwrap();
        let value_of = |flag: &str| argv[argv.iter().position(|a| a == flag).unwrap() + 1].clone();
        assert_eq!(
            (value_of("--model"), value_of("--thinking")),
            ("big-model".into(), "high".into())
        );

        // While the terminal is open the chat has no Pi of its own: a change is kept for when it starts again.
        let change = serde_json::json!({ "thinking": "low" });
        let changed = post(
            &state,
            &addr,
            format!("/v1/chats/{}/settings", run.id),
            change,
        )
        .await;
        assert_eq!(changed.status(), 200);
        let listed: Vec<Chat> = get(&state, &addr, "/v1/chats").await;
        assert_eq!(listed[0].model.as_deref(), Some("big-model"));
        assert_eq!(listed[0].thinking.as_deref(), Some("low"));
        assert_eq!(listed[0].title, "");
        let pid =
            std::fs::read_to_string(app.data_dir.join(format!("runs/{}/agent/pid.txt", run.id)))
                .unwrap();
        tui.close(None).await.unwrap();
        let argv = || -> Option<Vec<String>> {
            let file = app
                .data_dir
                .join(format!("runs/{}/agent/argv.json", run.id));
            serde_json::from_slice(&std::fs::read(file).ok()?).ok()
        };
        until("the chat's Pi starts with the new level", || {
            std::fs::read_to_string(app.data_dir.join(format!("runs/{}/agent/pid.txt", run.id)))
                .is_ok_and(|now| now != pid)
                && argv().is_some_and(|argv| argv.windows(2).any(|w| w == ["--thinking", "low"]))
        })
        .await;

        // A level Pi does not take is refused, and so is a change while the agent works.
        let wrong = serde_json::json!({ "thinking": "huge" });
        let refused = post(
            &state,
            &addr,
            format!("/v1/chats/{}/settings", run.id),
            wrong,
        )
        .await;
        assert_eq!(refused.status(), 422);
        assert_eq!(say(&state, &addr, run.id, "[hang]").await.status(), 200);
        let change = serde_json::json!({ "model": "other" });
        let refused = post(
            &state,
            &addr,
            format!("/v1/chats/{}/settings", run.id),
            change,
        )
        .await;
        assert_eq!(refused.status(), 409);
        assert_eq!(end(&state, &addr, run.id).await.status(), 200);
    }

    #[tokio::test]
    async fn a_chats_terminal_needs_a_person_and_a_chat_that_is_not_working() {
        let (_dir, state, addr) = start().await;
        let tui = |token: std::sync::Arc<str>, id: i64| {
            let addr = addr.clone();
            async move { open_at(&addr, &token, &format!("/v1/chats/{id}/tui")).await }
        };
        assert_eq!(status_of(tui("nope".into(), 1).await.unwrap_err()), 401);
        assert_eq!(
            status_of(tui(state.token.clone(), 99).await.unwrap_err()),
            404
        );

        // A chat whose agent is working keeps its worktree: 409.
        let started = post(
            &state,
            &addr,
            "/v1/chats".into(),
            serde_json::json!({ "agent": "Coder", "message": "Work for ever [hang]" }),
        )
        .await;
        let working: Run = started.json().await.unwrap();
        until("the agent works", || {
            state.app.store.run(working.id).unwrap().unwrap().status == "running"
        })
        .await;
        assert_eq!(
            status_of(tui(state.token.clone(), working.id).await.unwrap_err()),
            409
        );
        assert!(
            !state
                .app
                .store
                .events(working.id)
                .unwrap()
                .iter()
                .any(|e| e.kind == "tui_opened")
        );

        // The run token of the Moebius agent is refused: the terminal stays with a person.
        let waiting = chat(&state, &addr, "Hello", None).await;
        let moebius = state
            .app
            .store
            .add_run(Some(1), "Moebius", "test", "chat")
            .unwrap();
        let agent_token = state.router.issue_token(crate::router::Grant {
            run_id: moebius.id,
            card_id: None,
            budget: None,
        });
        assert_eq!(
            status_of(tui(agent_token.into(), waiting.id).await.unwrap_err()),
            403
        );

        // An ended chat has none.
        assert_eq!(end(&state, &addr, waiting.id).await.status(), 200);
        until("the chat has ended", || {
            state
                .app
                .store
                .run(waiting.id)
                .unwrap()
                .unwrap()
                .ended_at
                .is_some()
        })
        .await;
        assert_eq!(
            status_of(tui(state.token.clone(), waiting.id).await.unwrap_err()),
            422
        );
    }

    #[tokio::test]
    async fn the_server_shell_needs_the_token_a_person_and_a_free_place() {
        let (_dir, state, addr) = start().await;
        assert_eq!(
            status_of(open_at(&addr, "nope", "/v1/terminal").await.unwrap_err()),
            401
        );

        // The run token of an agent is refused: the shell stays with a person.
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
        assert_eq!(
            status_of(
                open_at(&addr, &agent_token, "/v1/terminal")
                    .await
                    .unwrap_err()
            ),
            403
        );

        let mut open = Vec::new();
        for _ in 0..crate::harness::terminal::MAX_PER_RUN {
            open.push(open_at(&addr, &state.token, "/v1/terminal").await.unwrap());
        }
        let refused = open_at(&addr, &state.token, "/v1/terminal").await;
        assert_eq!(status_of(refused.unwrap_err()), 422);
        open.pop().unwrap().close(None).await.unwrap();
        until("a place is free", || {
            state
                .app
                .terminals
                .take(crate::harness::terminal::SERVER)
                .is_some()
        })
        .await;
    }

    #[tokio::test]
    async fn a_person_opens_a_shell_in_the_clone_of_a_repository() {
        let (dir, state, addr) = start().await;
        let clone = dir.path().join("clone");
        std::fs::create_dir_all(&clone).unwrap();
        let repo = state
            .app
            .store
            .add_repository("widgets", &clone.to_string_lossy(), "none", "main")
            .unwrap();
        let path = format!("/v1/repositories/{}/terminal", repo.id);

        let mut terminal = open_at(&addr, &state.token, &path).await.unwrap();
        type_in(&mut terminal, "echo where-$(pwd -P)-end\n").await;
        let printed = read_until(&mut terminal, "where-(/[^\\s]*)-end").await;
        assert_eq!(
            std::path::Path::new(&printed),
            std::fs::canonicalize(&clone).unwrap()
        );
        assert_eq!(
            state.app.store.judgements("open_terminal").unwrap().len(),
            1
        );

        // Nobody gets in with the wrong token, into a repository that is not there, or with an agent's run token.
        assert_eq!(
            status_of(open_at(&addr, "nope", &path).await.unwrap_err()),
            401
        );
        assert_eq!(
            status_of(
                open_at(&addr, &state.token, "/v1/repositories/999/terminal")
                    .await
                    .unwrap_err()
            ),
            404
        );
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
        assert_eq!(
            status_of(open_at(&addr, &agent_token, &path).await.unwrap_err()),
            403
        );

        // At most a few at once per repository, and another repository has its own.
        let mut open = vec![terminal];
        for _ in 1..crate::harness::terminal::MAX_PER_RUN {
            open.push(open_at(&addr, &state.token, &path).await.unwrap());
        }
        let refused = open_at(&addr, &state.token, &path).await;
        assert_eq!(status_of(refused.unwrap_err()), 422);
        let other = open_at(&addr, &state.token, "/v1/repositories/1/terminal").await;
        assert_eq!(status_of(other.unwrap_err()), 422, "its clone is not there");
    }
}
