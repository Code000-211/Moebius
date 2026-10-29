//! What the tests share: a harness configuration repository with agents, skills, an MCP server and rules, a source
//! repository for cards, a server that clones the first, the desktop API served on port 0, and waiting for a run.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use moebius_core::app::PERSON;
use moebius_core::board::Runs;
use moebius_core::config::Config;
use moebius_core::repos::git;
use moebius_core::server::Server;
use moebius_core::server::api::{self, ApiState};
use moebius_core::store::Card;
use serde_json::Value;

/// The Coder as the repository defines it.
const CODER: &str = "model = \"coder-model\"\nthinking = \"low\"\nskills = [\"house-style\"]\nextensions = [\"extensions/log.ts\"]\nmcp = [\"docs\"]\n";

/// Writes `text` to `file` under `dir`, making its directories.
pub fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Commits everything in `dir`.
pub fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]).unwrap();
    git(dir, &["commit", "--quiet", "-m", message]).unwrap();
}

/// A harness configuration repository with two agents, a skill, an extension, an MCP server and rules, and two broken
/// agents. Returns the bare repository the server clones.
pub fn harness_origin(dir: &Path) -> PathBuf {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&work, "agents/Coder.toml", CODER);
    write(&work, "agents/Coder.md", "Keep changes small.\n");
    write(
        &work,
        "agents/Reviewer.toml",
        "model = \"reviewer-model\"\nprotocol = \"anthropic\"\ntoken_budget = 500\n",
    );
    write(&work, "agents/Typo.toml", "modle = \"x\"\n");
    write(
        &work,
        "agents/NoSkill.toml",
        "model = \"x\"\nskills = [\"missing\"]\n",
    );
    write(
        &work,
        "skills/house-style/SKILL.md",
        "---\nname: house-style\ndescription: \"Write plainly.\"\n---\nBody.\n",
    );
    write(&work, "skills/house-style/notes/extra.md", "More.\n");
    write(&work, "extensions/log.ts", "export default () => {};\n");
    write(
        &work,
        "agents/Token.toml",
        "model = \"x\"\nmcp = [\"token\"]\n",
    );
    write(
        &work,
        "agents/Secret.toml",
        "model = \"x\"\nmcp = [\"leak\"]\n",
    );
    write(
        &work,
        "mcp.toml",
        "# The servers.\n[docs]\ncommand = \"docs-server\"\n\
         [token]\ncommand = \"x\"\nenv = { TOKEN = \"${DOCS_TOKEN}\" }\n\
         [leak]\ncommand = \"x\"\nenv = { KEY = \"${OPENAI_API_KEY}\" }\n",
    );
    write(&work, "rules.md", "## no-vendoring\nDo not vendor.\n");
    commit_all(&work, "Start");
    let bare = dir.join("harness.git");
    git(
        dir,
        &["init", "--quiet", "--bare", "-b", "main", "harness.git"],
    )
    .unwrap();
    git(&work, &["push", "--quiet", bare.to_str().unwrap(), "main"]).unwrap();
    bare
}

/// A source repository for the cards of runs.
pub fn source(dir: &Path) -> PathBuf {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    git(&source, &["init", "--quiet", "-b", "main"]).unwrap();
    write(&source, "README.md", "hello\n");
    commit_all(&source, "first");
    source
}

/// A server with the fake Pi that clones the harness configuration repository `harness`.
pub fn server(dir: &Path, harness: &Path) -> Server {
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-pi");
    let text = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\nmcp_env = [\"DOCS_TOKEN\"]\n",
        fake_pi.display().to_string(),
        harness.display().to_string()
    );
    let config = Config::parse(&text, dir, &dir.join("moebius.toml")).unwrap();
    Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap()
}

/// The model of the agent `name` as the server read it, if it has the agent.
pub fn agent_model(server: &Server, name: &str) -> Option<String> {
    let set = server.harness.agents.read().unwrap();
    set.agents.get(name).map(|agent| agent.model.clone())
}

/// The desktop API of a server, served on port 0, and a client with its token.
pub struct Api {
    base: String,
    token: String,
    client: reqwest::Client,
}

impl Api {
    /// Serves the API of `server`.
    pub async fn start(server: &Server) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = ApiState {
            app: server.app.clone(),
            runs: server.harness.clone(),
            harness_repo: server.harness_repo.clone(),
            token: server.token.clone(),
            router: server.router.clone(),
            config: server.live.clone(),
        };
        tokio::spawn(api::serve(listener, state));
        Self {
            base,
            token: server.token.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Sends a request and gives the status and the body.
    pub async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> (u16, String) {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.unwrap();
        (response.status().as_u16(), response.text().await.unwrap())
    }
}

/// The commit `HEAD` of `dir`.
pub fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).unwrap()
}

/// A card on a new repository `demo`.
pub fn new_card(server: &Server, dir: &Path) -> Card {
    let repo = server
        .app
        .add_repository("demo", source(dir).to_str().unwrap())
        .unwrap();
    server
        .app
        .create_issue(repo.id, "Write done.txt", "", PERSON)
        .unwrap()
}

/// The arguments Pi-Bolt was started with for the run `id`.
pub fn argv(dir: &Path, id: i64) -> Vec<String> {
    let argv = dir.join(format!(".moebius/runs/{id}/agent/argv.json"));
    serde_json::from_slice(&std::fs::read(argv).unwrap()).unwrap()
}

/// Runs `agent` on `card` and gives the arguments Pi-Bolt was started with.
pub fn argv_of_a_run(server: &Server, dir: &Path, card: &Card, agent: &str) -> Vec<String> {
    let run = server
        .harness
        .start(&server.app, card, agent, "code", None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while server
        .app
        .store
        .run(run.id)
        .unwrap()
        .unwrap()
        .ended_at
        .is_none()
    {
        assert!(Instant::now() < deadline, "the run did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
    argv(dir, run.id)
}

/// Makes `origin` refuse every push, and returns the hook that does it.
pub fn refuse_pushes(origin: &Path) -> PathBuf {
    let hook = origin.join("hooks/pre-receive");
    std::fs::write(&hook, "#!/bin/sh\necho 'closed for edits' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    hook
}
