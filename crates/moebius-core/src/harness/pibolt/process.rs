//! Starting Pi for a run: its agent directory (`models.json` with the router as the only provider, and `mcp.json` with
//! the agent's MCP servers), the command line that gives it the agent's model, instructions, skills, extensions and the
//! bridge, and the process itself with the thread that reads it (see `reader`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{Value, json};

use super::reader::Reader;
use super::runs::{RunningRun, fail};
use super::{BRIDGE, Live, MOEBIUS_AGENT, PiBolt, lock, run_dir, send};
use crate::app::App;
use crate::bus::AppEvent;
use crate::config::AgentConfig;
use crate::harness::mcp;
use crate::router::Protocol;
use crate::store::{Card, Run};

/// Where a Pi starts and what it is given: the worktree it works in, its agent directory and sessions, whether it
/// continues the latest session there, the bridge extension, the token of the router and whether it operates Moebius
/// with the desktop API.
pub(super) struct Place<'a> {
    pub(super) worktree: &'a Path,
    pub(super) agent_dir: &'a Path,
    pub(super) sessions: &'a Path,
    pub(super) continues: bool,
    pub(super) bridge: &'a Path,
    pub(super) token: &'a str,
    pub(super) operates: bool,
}

impl PiBolt {
    /// The command that starts Pi for a run: `--mode rpc` for the run's own process, or, for the terminal interface of
    /// a chat, Pi's interactive mode. Both get the same provider, model, instructions, skills, MCP servers, extensions
    /// and bridge, so the checks see the same tool calls.
    pub(super) fn command(&self, agent: &AgentConfig, place: &Place, interactive: bool) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(place.worktree)
            .env("PI_CODING_AGENT_DIR", place.agent_dir)
            .env("MOEBIUS", &self.moebius)
            .env("MOEBIUS_CHECK_URL", &self.router_url)
            .env("MOEBIUS_RUN_TOKEN", place.token);
        if let (true, Some(url)) = (place.operates, self.api_url.get()) {
            // The Moebius agent operates Moebius with `$MOEBIUS`, as the run itself: its token is accepted by the
            // desktop API.
            command
                .env("MOEBIUS_URL", url)
                .env("MOEBIUS_TOKEN", place.token);
        }
        if !interactive {
            command.args(["--mode", "rpc"]);
        }
        command
            .args(["--offline", "--no-approve"])
            .args(["--provider", "moebius", "--model", &agent.model])
            .args(["--no-extensions", "--no-skills"])
            .arg("--session-dir")
            .arg(place.sessions);
        if place.continues {
            command.arg("--continue");
        }
        if let Some(thinking) = &agent.thinking {
            command.args(["--thinking", thinking]);
        }
        if let Some(instructions) = &agent.instructions {
            command.arg("--append-system-prompt").arg(instructions);
        }
        for skill in &agent.skills {
            command.arg("--skill").arg(skill);
        }
        // Pi's MCP support is an extension that `--no-extensions` turns off; it reads the `mcp.json` of the agent
        // directory, which holds the agent's servers.
        if !agent.mcp.is_empty() {
            command.args(["--extension", "builtin:mcp"]);
        }
        for extension in &agent.extensions {
            command.arg("--extension").arg(extension);
        }
        // Last, so that it checks the input and redacts the content the other extensions left.
        command.arg("--extension").arg(place.bridge);
        command
    }

    /// Writes the agent directory `agent_dir`: `models.json` with the router as the `moebius` provider, and `mcp.json` with
    /// the agent's MCP servers, if it has any (without it Pi connects to none). Both hold secrets (the run's token, the
    /// servers' environment), so only the server's user can read them. The model is marked as one that reasons when the
    /// agent has a reasoning level other than `off`, without which Pi would turn the level off.
    pub(super) fn write_agent_dir(
        &self,
        agent_dir: PathBuf,
        agent: &AgentConfig,
        token: &str,
    ) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&agent_dir).map_err(|e| e.to_string())?;
        if !agent.mcp.is_empty() {
            let servers = mcp::pi_config(&agent.mcp, &|name| std::env::var(name).ok())?;
            write_private(&agent_dir.join("mcp.json"), &servers)?;
        }
        let (api, base_url) = match agent.protocol {
            Protocol::OpenAi => ("openai-completions", format!("{}/v1", self.router_url)),
            Protocol::Anthropic => ("anthropic-messages", self.router_url.clone()),
        };
        let models = json!({
            "providers": {
                "moebius": {
                    "baseUrl": base_url,
                    "api": api,
                    "apiKey": token,
                    "models": [{ "id": agent.model, "reasoning": reasons(agent) }]
                }
            }
        });
        write_private(&agent_dir.join("models.json"), &models)?;
        Ok(agent_dir)
    }

    /// Starts Pi for a run whose worktree and token exist, and the thread that reads it. Pi is given `prompt`; without
    /// one, a chat's Pi continues the chat's session and the chat waits for the person's next message.
    pub(super) fn launch(
        &self,
        app: &Arc<App>,
        card: Option<&Card>,
        agent: &AgentConfig,
        run: RunningRun,
        prompt: Option<&str>,
    ) -> Result<(), String> {
        app.store
            .set_run_worktree(run.id, &run.worktree.to_string_lossy())
            .map_err(|e| e.to_string())?;
        let run_dir = run_dir(app, run.id);
        let agent_dir = self
            .write_agent_dir(run_dir.join("agent"), agent, &run.token)
            .map_err(|e| format!("cannot write the agent directory: {e}"))?;
        let bridge = run_dir.join("moebius-bridge.ts");
        std::fs::write(&bridge, BRIDGE)
            .map_err(|e| format!("cannot write the bridge extension: {e}"))?;
        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(run_dir.join("stderr.log"))
            .map_err(|e| e.to_string())?;
        let operates = app
            .store
            .run(run.id)
            .map_err(|e| e.to_string())?
            .is_some_and(|stored| stored.agent == MOEBIUS_AGENT);
        let sessions = run_dir.join("sessions");
        let place = Place {
            worktree: &run.worktree,
            agent_dir: &agent_dir,
            sessions: &sessions,
            continues: prompt.is_none(),
            bridge: &bridge,
            token: &run.token,
            operates,
        };
        let mut command = self.command(agent, &place, false);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|e| format!("cannot start {}: {e}", self.program.display()))?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let live = Live {
            stdin: Arc::new(Mutex::new(child.stdin.take())),
            child: Arc::new(Mutex::new(child)),
            chat: run.purpose == "chat",
            closing: Arc::new(Mutex::new(None)),
            waiting_since: Arc::new(Mutex::new(prompt.is_none().then(Instant::now))),
            agent: agent.clone(),
            run: run.clone(),
        };
        lock(&self.live).insert(run.id, live.clone());
        match prompt {
            Some(prompt) => {
                app.store
                    .set_run_status(run.id, "running")
                    .map_err(|e| e.to_string())?;
                app.bus.publish(AppEvent::new(
                    "run_started",
                    json!({ "run": run.id, "card": card.map(|card| card.id) }),
                ));
                send(
                    &live.stdin,
                    &json!({ "id": "moebius-prompt", "type": "prompt", "message": prompt }),
                );
            }
            None => app
                .store
                .set_run_status(run.id, "waiting")
                .map_err(|e| e.to_string())?,
        }
        let reader = Reader {
            app: app.clone(),
            run,
            live,
            router: self.router.clone(),
            lives: self.live.clone(),
        };
        let name = format!("pi-bolt run {}", reader.run.id);
        std::thread::Builder::new()
            .name(name)
            .spawn(move || reader.read(stdout))
            .map(drop)
            .map_err(|e| e.to_string())
    }

    /// Launches a run that has its worktree and token and returns it; when Pi cannot be started, what was set up for
    /// the run is undone and the run has failed.
    pub(super) fn launch_or_undo(
        &self,
        app: &Arc<App>,
        card: Option<&Card>,
        config: &AgentConfig,
        running: RunningRun,
        prompt: Option<&str>,
    ) -> Result<Run, String> {
        if let Err(e) = self.launch(app, card, config, running.clone(), prompt) {
            // Nothing reads this run: undo what was set up for it.
            self.router.revoke_token(&running.token);
            running.remove_worktree(app);
            return Err(fail(app, running.id, e));
        }
        Ok(app
            .store
            .run(running.id)
            .map_err(|e| e.to_string())?
            .expect("the run exists"))
    }
}

/// Whether `agent` reasons: it has a reasoning level other than `off`.
pub(super) fn reasons(agent: &AgentConfig) -> bool {
    agent
        .thinking
        .as_deref()
        .is_some_and(|level| level != "off")
}

/// Writes `value` as JSON to a new `file` that only the server's user can read.
fn write_private(file: &Path, value: &Value) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let text = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(file)
        .and_then(|mut file| file.write_all(&text))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Runs;
    use crate::harness::mcp::McpServer;
    use crate::harness::pibolt::testing::{setup, wait_until_ended};
    use crate::repos::git;

    #[test]
    fn a_run_is_stored_event_by_event_and_its_work_committed() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Write done.txt", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        assert_eq!(run.status, "running");
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "succeeded");
        // The summary is the text of the last assistant message that agent_end reports.
        assert_eq!(run.summary.as_deref(), Some("line one line two"));

        let events = app.store.events(run.id).unwrap();
        let kinds: Vec<_> = events.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "response",
                "agent_start",
                "message_update",
                "tool_execution_start",
                "tool_execution_end",
                "agent_end",
                "agent_settled"
            ]
        );
        // U+2028 inside a string is part of the record, not a line break.
        assert_eq!(
            events[2].data["assistantMessageEvent"]["delta"],
            "line one\u{2028}line two"
        );

        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        let branch = format!("moebius/run-{}", run.id);
        assert_eq!(
            git(&clone, &["show", &format!("{branch}:done.txt")]).unwrap(),
            "done"
        );
        assert!(!PathBuf::from(run.worktree.unwrap()).exists());

        let run_dir = app.data_dir.join("runs").join(run.id.to_string());
        let argv: Vec<String> =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/argv.json")).unwrap())
                .unwrap();
        assert!(argv.windows(2).any(|w| w == ["--model", "qwen-local"]));
        assert!(argv.iter().any(|a| a == "--no-approve") && argv.iter().any(|a| a == "--offline"));
        assert!(argv.windows(2).any(|w| w == ["--thinking", "low"]));
        // Only the configured extensions and skills load, and the bridge is the last extension.
        let bridge = run_dir.join("moebius-bridge.ts");
        assert!(
            argv.iter().any(|a| a == "--no-extensions") && argv.iter().any(|a| a == "--no-skills")
        );
        let extensions: Vec<_> = argv.windows(2).filter(|w| w[0] == "--extension").collect();
        assert_eq!(extensions[0][1], "/agent/own.ts");
        assert_eq!(extensions.last().unwrap()[1], bridge.to_str().unwrap());
        assert_eq!(std::fs::read_to_string(&bridge).unwrap(), BRIDGE);
        let env: Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/env.json")).unwrap())
                .unwrap();
        assert_eq!(env["MOEBIUS"], "/usr/local/bin/moebius");
        assert_eq!(env["MOEBIUS_CHECK_URL"], "http://127.0.0.1:9");
        assert!(
            env["MOEBIUS_RUN_TOKEN"]
                .as_str()
                .unwrap()
                .starts_with("moebius-run-")
        );
        assert!(
            argv.windows(2)
                .any(|w| w == ["--append-system-prompt", "Be brief."])
        );
        let models: Value =
            serde_json::from_slice(&std::fs::read(run_dir.join("agent/models.json")).unwrap())
                .unwrap();
        // The run's token is in the file, so only the server's user can read it.
        let mode = std::fs::metadata(run_dir.join("agent/models.json"))
            .unwrap()
            .permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        let provider = &models["providers"]["moebius"];
        assert_eq!(provider["baseUrl"], "http://127.0.0.1:9/v1");
        assert!(
            provider["apiKey"]
                .as_str()
                .unwrap()
                .starts_with("moebius-run-")
        );
    }

    #[test]
    fn a_run_gets_exactly_the_skills_and_mcp_servers_of_its_agent() {
        let (_dir, app, pi, repo) = setup();
        let server = |text: &str| {
            let servers = mcp::parse_file(&format!("[s]\n{text}"), &["CARGO_PKG_NAME".to_string()]);
            servers.unwrap().remove("s").unwrap().unwrap()
        };
        let agent = |skill: &str, mcp: &[(&str, McpServer)]| AgentConfig {
            skills: vec![format!("/skills/{skill}").into()],
            extensions: vec![],
            mcp: mcp
                .iter()
                .map(|(name, server)| (name.to_string(), server.clone()))
                .collect(),
            ..pi.agent_set().agents["Coder"].clone()
        };
        let docs = server("command = \"docs\"\nargs = [\"--name\", \"${CARGO_PKG_NAME}\"]\n");
        let search = server(
            "url = \"https://search.test/mcp\"\nheaders = { Name = \"${CARGO_PKG_NAME}\" }\n",
        );
        let agents = [
            ("A".to_string(), agent("a", &[("docs", docs)])),
            ("B".to_string(), agent("b", &[("search", search)])),
            ("C".to_string(), agent("c", &[])),
        ];
        pi.agents.write().unwrap().agents.extend(agents);
        let run_agent = |name: &str| {
            let card = app
                .create_issue(repo, "Write done.txt", "", "person")
                .unwrap();
            let run = pi.start(&app, &card, name, "code", None).unwrap();
            let run = wait_until_ended(&app, run.id);
            let agent_dir = app
                .data_dir
                .join("runs")
                .join(run.id.to_string())
                .join("agent");
            let argv: Vec<String> =
                serde_json::from_slice(&std::fs::read(agent_dir.join("argv.json")).unwrap())
                    .unwrap();
            (argv, agent_dir.join("mcp.json"))
        };
        let values = |argv: &[String], flag: &str| -> Vec<String> {
            argv.windows(2)
                .filter(|w| w[0] == flag)
                .map(|w| w[1].clone())
                .collect()
        };

        let (argv, mcp_json) = run_agent("A");
        assert_eq!(values(&argv, "--skill"), ["/skills/a"]);
        let mode = std::fs::metadata(&mcp_json).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        let config: Value = serde_json::from_slice(&std::fs::read(mcp_json).unwrap()).unwrap();
        assert_eq!(
            config,
            json!({ "mcpServers": { "docs": {
                "command": "docs", "args": ["--name", "moebius-core"], "env": {}, "exposure": "direct"
            } } })
        );
        // Pi's MCP support is switched on explicitly, before the bridge, which is still last.
        let extensions = values(&argv, "--extension");
        assert_eq!(extensions[0], "builtin:mcp");
        assert!(extensions.last().unwrap().ends_with("moebius-bridge.ts"));

        let (argv, mcp_json) = run_agent("B");
        assert_eq!(values(&argv, "--skill"), ["/skills/b"]);
        let config: Value = serde_json::from_slice(&std::fs::read(mcp_json).unwrap()).unwrap();
        assert_eq!(
            config,
            json!({ "mcpServers": { "search": {
                "url": "https://search.test/mcp", "headers": { "Name": "moebius-core" }, "exposure": "direct"
            } } })
        );

        // An agent without servers gets no mcp.json and no MCP support.
        let (argv, mcp_json) = run_agent("C");
        assert_eq!(values(&argv, "--skill"), ["/skills/c"]);
        assert!(!mcp_json.exists());
        assert_eq!(values(&argv, "--extension").len(), 1);
    }
}
