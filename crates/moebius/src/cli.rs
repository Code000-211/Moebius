//! The `moebius` binary as a client of a running server: a command for each thing done often through the desktop API,
//! and `api` for every route. A person uses them in a terminal; the Moebius agent uses them in its chats, as
//! `"$MOEBIUS" board`, with the run's own token.
//!
//! Each command is a thin mapping: [`Operate::plan`] makes the one request it stands for (a few look an item up in the
//! snapshot first), [`Client`] sends it, and [`Show`] says how the answer is written as plain text. With `--json` the
//! answer is the API's JSON as it came.

use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Subcommand, ValueEnum};
use reqwest::Method;
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// The server a command talks to and the token that proves who it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub url: String,
    pub token: String,
}

/// What the desktop app saved: `<config dir>/moebius/desktop.toml`.
#[derive(Debug, Deserialize)]
struct Saved {
    address: String,
    token: String,
}

/// The desktop app's settings file of this person, if the platform has a config directory.
pub fn desktop_settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("moebius").join("desktop.toml"))
}

/// The address and token the desktop app saved; none when it saved nothing.
fn desktop_settings(path: Option<&Path>) -> Result<Option<Connection>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let saved: Saved =
        toml::from_str(&text).map_err(|e| format!("{} is not valid: {e}", path.display()))?;
    Ok(Some(Connection {
        url: address_url(&saved.address),
        token: saved.token,
    }))
}

/// The server of a `moebius.toml` that exists: the `listen` address and the content of the token file.
fn config_settings(config: &Path) -> Result<Option<Connection>, String> {
    if !config.is_file() {
        return Ok(None);
    }
    let config = moebius_core::config::Config::load(config).map_err(|e| e.to_string())?;
    let token = std::fs::read_to_string(&config.token_file).map_err(|e| {
        format!(
            "cannot read the token {}: {e} (has the server started once?)",
            config.token_file.display()
        )
    })?;
    Ok(Some(Connection {
        url: listen_url(config.listen),
        token: token.trim().to_string(),
    }))
}

/// `http://` before an address that has no scheme, and no `/` at the end.
fn address_url(address: &str) -> String {
    let address = address.trim().trim_end_matches('/');
    if address.contains("://") {
        address.to_string()
    } else {
        format!("http://{address}")
    }
}

/// The URL for an address a server listens on; a server that listens on all interfaces is reached on the loopback.
fn listen_url(listen: SocketAddr) -> String {
    let ip = match listen.ip() {
        ip if ip.is_unspecified() && ip.is_ipv6() => std::net::Ipv6Addr::LOCALHOST.into(),
        ip if ip.is_unspecified() => std::net::Ipv4Addr::LOCALHOST.into(),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, listen.port()))
}

/// Where the server is, from the first source that says: `--server` and `--token` (or `MOEBIUS_URL` and
/// `MOEBIUS_TOKEN`), which must come together; the desktop app's saved settings; the `moebius.toml` that exists. A
/// `--token` alone replaces the token of the settings or the file.
pub fn connect(
    server: Option<&str>,
    token: Option<&str>,
    desktop: Option<&Path>,
    config: &Path,
) -> Result<Connection, String> {
    if let Some(server) = server {
        let token = token.ok_or("--server needs --token (or MOEBIUS_TOKEN)")?;
        return Ok(Connection {
            url: address_url(server),
            token: token.trim().to_string(),
        });
    }
    let found = match desktop_settings(desktop)? {
        Some(found) => found,
        None => config_settings(config)?.ok_or_else(|| {
            format!(
                "no server to connect to: give --server and --token, set MOEBIUS_URL and MOEBIUS_TOKEN, \
                 save the address in the desktop app, or run it where {} is",
                config.display()
            )
        })?,
    };
    Ok(Connection {
        token: token.map_or(found.token, |token| token.trim().to_string()),
        ..found
    })
}

/// One request to the desktop API.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub body: Option<Value>,
}

impl Request {
    fn get(path: impl Into<String>) -> Self {
        Self {
            method: Method::GET,
            path: path.into(),
            body: None,
        }
    }

    fn post(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: Method::POST,
            path: path.into(),
            body: Some(body),
        }
    }

    fn put(path: impl Into<String>, body: Value) -> Self {
        Self {
            method: Method::PUT,
            path: path.into(),
            body: Some(body),
        }
    }

    fn delete(path: impl Into<String>) -> Self {
        Self {
            method: Method::DELETE,
            path: path.into(),
            body: None,
        }
    }
}

/// The desktop API over HTTP.
pub struct Client {
    http: reqwest::blocking::Client,
    connection: Connection,
}

impl Client {
    pub fn new(connection: Connection) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            // A merge or a chat's start does git work before it answers.
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { http, connection })
    }

    /// Sends the request and returns the answer's body; an error status is an error with the server's message.
    pub fn send(&self, request: &Request) -> Result<String, String> {
        let url = format!("{}{}", self.connection.url, request.path);
        let mut builder = self
            .http
            .request(request.method.clone(), &url)
            .bearer_auth(&self.connection.token);
        if let Some(body) = &request.body {
            builder = builder.json(body);
        }
        let response = builder
            .send()
            .map_err(|e| format!("cannot reach {}: {e}", self.connection.url))?;
        let status = response.status();
        let text = response.text().map_err(|e| e.to_string())?;
        if status.is_success() {
            Ok(text)
        } else {
            Err(format!(
                "the server answered {}: {}",
                status.as_u16(),
                text.trim()
            ))
        }
    }

    /// The JSON a GET answers.
    pub fn get(&self, path: &str) -> Result<Value, String> {
        let text = self.send(&Request::get(path))?;
        serde_json::from_str(&text).map_err(|e| format!("the answer to {path} is not JSON: {e}"))
    }
}

/// The commands that operate a running server.
#[derive(Debug, Subcommand)]
pub enum Operate {
    /// Call any route of the desktop API and print the answer, such as `api GET /v1/snapshot`.
    Api {
        /// GET, POST, PUT or DELETE.
        method: String,
        /// The route, such as `/v1/cards/3/approve`.
        route: String,
        /// The JSON body: the JSON itself, a file that holds it, or `-` for standard input.
        #[arg(long)]
        body: Option<String>,
    },
    /// The boards, with the cards of each column; of one repository (its name or id) if given.
    Board { repository: Option<String> },
    /// A card (its id, as `board` shows it), or with an option its timeline or its runs.
    Card {
        id: i64,
        /// The card's timeline.
        #[arg(long, conflicts_with = "runs")]
        history: bool,
        /// The card's runs.
        #[arg(long)]
        runs: bool,
    },
    /// What waits for a person; `inbox act` does what the app's buttons do.
    #[command(args_conflicts_with_subcommands = true)]
    Inbox {
        #[command(subcommand)]
        command: Option<InboxCommand>,
    },
    /// The chats, newest first.
    Chats,
    /// Start, write to and end a chat.
    Chat {
        #[command(subcommand)]
        command: ChatCommand,
    },
    /// The agents the server can run.
    Agents,
    /// Show, save and delete an agent of the harness configuration repository.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// The rules page; `rules save` replaces the rules in prose.
    #[command(args_conflicts_with_subcommands = true)]
    Rules {
        #[command(subcommand)]
        command: Option<RulesCommand>,
    },
    /// Every key of moebius.toml with its value; `settings set` changes the file.
    #[command(args_conflicts_with_subcommands = true)]
    Settings {
        #[command(subcommand)]
        command: Option<SettingsCommand>,
    },
    /// API keys.
    Secrets {
        #[command(subcommand)]
        command: SecretsCommand,
    },
    /// The repositories.
    Repos,
    /// Add a repository.
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
    /// Create an issue.
    Issue {
        #[command(subcommand)]
        command: IssueCommand,
    },
    /// The Retros, or the Retro of a day; `retro run` runs one now.
    #[command(args_conflicts_with_subcommands = true)]
    Retro {
        /// The day, `YYYY-MM-DD`.
        date: Option<String>,
        #[command(subcommand)]
        command: Option<RetroCommand>,
    },
    /// The proposals of the Retros.
    Proposals,
    /// Merge or dismiss a proposal.
    Proposal {
        #[command(subcommand)]
        command: ProposalCommand,
    },
    /// The research, or one research; `research new`, `confirm`, `start` and `stop` act on it.
    #[command(args_conflicts_with_subcommands = true)]
    Research {
        id: Option<i64>,
        #[command(subcommand)]
        command: Option<ResearchCommand>,
    },
    /// What grilling chats ended in.
    Grillings,
    /// Show, confirm and dismiss a grilling.
    Grilling {
        #[command(subcommand)]
        command: GrillingCommand,
    },
    /// The tokens, calls and model time of the last days.
    Usage {
        /// The window in days, 1 to 365; 1 (today) without it.
        #[arg(long)]
        days: Option<i64>,
    },
    /// What the decision model has measured.
    DecisionModel,
    /// Check that the server is ready for a first card: one line for each step, and exit status 1 while a step is
    /// missing.
    Doctor,
    /// The takeovers the decision model earned.
    Takeovers,
    /// Merge, dismiss or release a takeover.
    Takeover {
        #[command(subcommand)]
        command: TakeoverCommand,
    },
    /// Operate the server itself.
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum InboxCommand {
    /// Do to an item what the app does with a button: approve, reject, merge, retry, confirm or dismiss, as its kind
    /// allows.
    Act {
        item: i64,
        action: Verb,
        /// Why; sent with the decision.
        #[arg(long, default_value = "")]
        reason: String,
    },
}

/// What a person can do to an inbox item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Verb {
    Approve,
    Reject,
    Merge,
    Retry,
    Confirm,
    Dismiss,
}

#[derive(Debug, Subcommand)]
pub enum ChatCommand {
    /// Start a chat; the agent answers the message.
    Start {
        /// The first message, or `-` for standard input.
        message: String,
        /// The agent to talk to; the Moebius agent without it.
        #[arg(long)]
        agent: Option<String>,
        /// The id of the repository to work in.
        #[arg(long)]
        repository: Option<i64>,
        /// The id of an ended chat to go on from.
        #[arg(long)]
        continues: Option<i64>,
    },
    /// Write to a chat that has not ended.
    Say {
        id: i64,
        /// The message, or `-` for standard input.
        message: String,
    },
    /// End a chat.
    End { id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// An agent as its files say.
    Show { name: String },
    /// Add an agent or replace it, committed to the harness configuration repository.
    Save {
        name: String,
        #[arg(long)]
        model: String,
        /// `openai` or `anthropic`.
        #[arg(long)]
        protocol: Option<String>,
        #[arg(long)]
        thinking: Option<String>,
        #[arg(long)]
        token_budget: Option<i64>,
        /// A skill of the repository; give it again for more.
        #[arg(long = "skill")]
        skills: Vec<String>,
        /// An extension file of the repository; give it again for more.
        #[arg(long = "extension")]
        extensions: Vec<String>,
        /// A server of mcp.toml; give it again for more.
        #[arg(long = "mcp")]
        mcp: Vec<String>,
        /// A file with the agent's instructions, or `-` for standard input.
        #[arg(long)]
        instructions: Option<String>,
    },
    /// Delete an agent.
    Delete { name: String },
}

#[derive(Debug, Subcommand)]
pub enum RulesCommand {
    /// Replace the rules in prose with the content of a file, or `-` for standard input.
    Save { file: String },
}

#[derive(Debug, Subcommand)]
pub enum SettingsCommand {
    /// Set keys and remove others. A value is JSON when it parses as JSON (`5`, `true`, `["a"]`, `'{"model":"m"}'`)
    /// and text otherwise.
    Set {
        #[arg(value_name = "KEY=VALUE", value_parser = assignment)]
        assignments: Vec<(String, Value)>,
        /// A key or table to remove; give it again for more.
        #[arg(long = "remove")]
        remove: Vec<String>,
    },
}

/// `key=value`: the value is JSON if it parses as JSON, else the text.
fn assignment(text: &str) -> Result<(String, Value), String> {
    let (key, value) = text
        .split_once('=')
        .ok_or("write it as KEY=VALUE".to_string())?;
    if key.is_empty() {
        return Err("a key is needed before the =".into());
    }
    let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
    Ok((key.to_string(), value))
}

#[derive(Debug, Subcommand)]
pub enum SecretsCommand {
    /// Store an API key under a name. The key is read from standard input, never from an argument.
    Set { name: String },
}

#[derive(Debug, Subcommand)]
pub enum RepoCommand {
    /// Clone a repository into the server.
    Add {
        name: String,
        /// Anything `git clone` takes, as seen from the server.
        url: String,
        /// `owner/repo` on GitHub; the repository then uses the GitHub forge.
        #[arg(long)]
        github: Option<String>,
        /// A GitHub login whose issues need no approval; give it again for more.
        #[arg(long = "trusted")]
        trusted: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum IssueCommand {
    /// Create an issue, which becomes a card in the backlog.
    Create {
        /// The id of the repository.
        repository: i64,
        title: String,
        #[arg(long, default_value = "")]
        body: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum RetroCommand {
    /// Run the Retro of a day now.
    Run {
        /// The day, `YYYY-MM-DD`; today without it.
        #[arg(long)]
        date: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProposalCommand {
    Merge {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    Dismiss {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ResearchCommand {
    /// Store a brief as a draft.
    New {
        /// The id of the repository.
        #[arg(long)]
        repository: i64,
        #[arg(long)]
        goal: String,
        /// The shell command that measures the goal.
        #[arg(long)]
        measure: String,
        /// `lower` or `higher`.
        #[arg(long)]
        better: String,
        #[arg(long)]
        token_budget: i64,
    },
    /// Confirm a draft.
    Confirm { id: i64 },
    /// Start a confirmed research now.
    Start { id: i64 },
    /// Stop a research.
    Stop { id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum GrillingCommand {
    Show {
        id: i64,
    },
    /// Make cards of the issues: the proposed ones, or the list in a JSON file (or `-`).
    Confirm {
        id: i64,
        #[arg(long)]
        issues: Option<String>,
    },
    Dismiss {
        id: i64,
    },
}

#[derive(Debug, Subcommand)]
pub enum TakeoverCommand {
    Merge {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    Dismiss {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
    Release {
        id: i64,
        #[arg(long, default_value = "")]
        reason: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// Restart the server.
    Restart,
}

/// How an answer is written as plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
    /// The answer as it came: pretty JSON, or the text of what is not JSON.
    Raw,
    Board(Option<String>),
    Card(i64),
    Inbox,
    /// An array of objects, one line each, with these fields.
    Table(&'static [&'static str]),
    /// An object, one line for each field.
    Object,
    Rules,
    Settings,
    Retro,
    Usage,
    DecisionModel,
    Doctor,
    /// A short sentence, for an answer with nothing in it to show.
    Done(String),
}

impl Operate {
    /// The request this command stands for and how to show its answer. `get` reads a route; the commands that act on
    /// an item they have to look up use it.
    pub fn plan(
        &self,
        get: &mut dyn FnMut(&str) -> Result<Value, String>,
    ) -> Result<(Request, Show), String> {
        use Operate::*;
        let reason = |reason: &str| json!({ "reason": reason });
        Ok(match self {
            Api {
                method,
                route,
                body,
            } => {
                let method = Method::from_bytes(method.to_ascii_uppercase().as_bytes())
                    .ok()
                    .filter(|m| {
                        [Method::GET, Method::POST, Method::PUT, Method::DELETE].contains(m)
                    })
                    .ok_or_else(|| format!("{method} is not GET, POST, PUT or DELETE"))?;
                let route = if route.starts_with('/') {
                    route.clone()
                } else {
                    format!("/{route}")
                };
                let body = match body {
                    Some(body) => Some(api_body(body)?),
                    // A request that changes something has a body, `{}` at least.
                    None if method == Method::POST || method == Method::PUT => Some(json!({})),
                    None => None,
                };
                (
                    Request {
                        method,
                        path: route,
                        body,
                    },
                    Show::Raw,
                )
            }
            Board { repository } => (
                Request::get("/v1/snapshot"),
                Show::Board(repository.clone()),
            ),
            Card {
                id, history: true, ..
            } => (
                Request::get(format!("/v1/cards/{id}/history")),
                Show::Table(&["at", "kind", "data"]),
            ),
            Card { id, runs: true, .. } => (
                Request::get(format!("/v1/cards/{id}/runs")),
                Show::Table(&["id", "agent", "purpose", "status", "summary"]),
            ),
            Card { id, .. } => (Request::get("/v1/snapshot"), Show::Card(*id)),
            Inbox { command: None } => (Request::get("/v1/snapshot"), Show::Inbox),
            Inbox {
                command:
                    Some(InboxCommand::Act {
                        item,
                        action,
                        reason: why,
                    }),
            } => {
                let snapshot = get("/v1/snapshot")?;
                let found = snapshot["inbox"]
                    .as_array()
                    .and_then(|items| items.iter().find(|i| i["id"] == *item))
                    .ok_or_else(|| format!("no item {item} in the inbox"))?;
                (inbox_request(found, *action, why)?, Show::Object)
            }
            Chats => (
                Request::get("/v1/chats"),
                Show::Table(&["id", "agent", "status", "repository_id", "title"]),
            ),
            Chat {
                command:
                    ChatCommand::Start {
                        message,
                        agent,
                        repository,
                        continues,
                    },
            } => {
                let mut body = Map::new();
                body.insert("message".into(), text_argument(message)?.into());
                for (key, value) in [
                    ("agent", json!(agent)),
                    ("repository", json!(repository)),
                    ("continues", json!(continues)),
                ] {
                    if !value.is_null() {
                        body.insert(key.into(), value);
                    }
                }
                (Request::post("/v1/chats", body.into()), Show::Object)
            }
            Chat {
                command: ChatCommand::Say { id, message },
            } => (
                Request::post(
                    format!("/v1/chats/{id}/messages"),
                    json!({ "message": text_argument(message)? }),
                ),
                Show::Object,
            ),
            Chat {
                command: ChatCommand::End { id },
            } => (
                Request::post(format!("/v1/chats/{id}/end"), json!({})),
                Show::Object,
            ),
            Agents => (
                Request::get("/v1/agents"),
                Show::Table(&["name", "model", "protocol", "built_in", "error"]),
            ),
            Agent {
                command: AgentCommand::Show { name },
            } => (Request::get(format!("/v1/agents/{name}")), Show::Object),
            Agent {
                command:
                    AgentCommand::Save {
                        name,
                        model,
                        protocol,
                        thinking,
                        token_budget,
                        skills,
                        extensions,
                        mcp,
                        instructions,
                    },
            } => {
                let mut settings = json!({
                    "model": model,
                    "skills": skills,
                    "extensions": extensions,
                    "mcp": mcp,
                });
                for (key, value) in [
                    ("protocol", json!(protocol)),
                    ("thinking", json!(thinking)),
                    ("token_budget", json!(token_budget)),
                ] {
                    if !value.is_null() {
                        settings[key] = value;
                    }
                }
                let instructions = match instructions {
                    Some(source) => read_input(source)?,
                    None => String::new(),
                };
                (
                    Request::put(
                        format!("/v1/agents/{name}"),
                        json!({ "settings": settings, "instructions": instructions }),
                    ),
                    Show::Object,
                )
            }
            Agent {
                command: AgentCommand::Delete { name },
            } => (
                Request::delete(format!("/v1/agents/{name}")),
                Show::Done(format!("deleted the agent {name}")),
            ),
            Rules { command: None } => (Request::get("/v1/rules"), Show::Rules),
            Rules {
                command: Some(RulesCommand::Save { file }),
            } => (
                Request::put("/v1/rules", json!({ "text": read_input(file)? })),
                Show::Rules,
            ),
            Settings { command: None } => (Request::get("/v1/settings"), Show::Settings),
            Settings {
                command:
                    Some(SettingsCommand::Set {
                        assignments,
                        remove,
                    }),
            } => {
                let set: Map<String, Value> = assignments.iter().cloned().collect();
                (
                    Request::put("/v1/settings", json!({ "set": set, "remove": remove })),
                    Show::Object,
                )
            }
            Secrets {
                command: SecretsCommand::Set { name },
            } => {
                let value = read_input("-")?;
                (
                    Request::put(
                        format!("/v1/secrets/{name}"),
                        json!({ "value": value.trim_end_matches(['\n', '\r']) }),
                    ),
                    Show::Done(format!("stored the key {name}")),
                )
            }
            Repos => (Request::get("/v1/snapshot"), Show::Raw),
            Repo {
                command:
                    RepoCommand::Add {
                        name,
                        url,
                        github,
                        trusted,
                    },
            } => {
                let mut body = json!({ "name": name, "url": url });
                if let Some(github) = github {
                    body["forge"] = "github".into();
                    body["github"] = github.clone().into();
                }
                if !trusted.is_empty() {
                    body["trusted"] = json!(trusted);
                }
                (Request::post("/v1/repositories", body), Show::Object)
            }
            Issue {
                command:
                    IssueCommand::Create {
                        repository,
                        title,
                        body,
                    },
            } => (
                Request::post(
                    format!("/v1/repositories/{repository}/issues"),
                    json!({ "title": title, "body": body }),
                ),
                Show::Object,
            ),
            Retro {
                date: None,
                command: None,
            } => (
                Request::get("/v1/retros"),
                Show::Table(&["date", "status", "errors", "proposals"]),
            ),
            Retro {
                date: Some(date),
                command: None,
            } => (Request::get(format!("/v1/retros/{date}")), Show::Retro),
            Retro {
                command: Some(RetroCommand::Run { date }),
                ..
            } => (
                Request::post(
                    "/v1/retros",
                    date.as_ref().map_or(json!({}), |d| json!({ "date": d })),
                ),
                Show::Retro,
            ),
            Proposals => (
                Request::get("/v1/proposals"),
                Show::Table(&["id", "status", "branch", "title"]),
            ),
            Proposal {
                command: ProposalCommand::Merge { id, reason: why },
            } => (
                Request::post(format!("/v1/proposals/{id}/merge"), reason(why)),
                Show::Object,
            ),
            Proposal {
                command: ProposalCommand::Dismiss { id, reason: why },
            } => (
                Request::post(format!("/v1/proposals/{id}/dismiss"), reason(why)),
                Show::Object,
            ),
            Research {
                id: None,
                command: None,
            } => (
                Request::get("/v1/research"),
                Show::Table(&["id", "status", "repository_id", "goal"]),
            ),
            Research {
                id: Some(id),
                command: None,
            } => (Request::get(format!("/v1/research/{id}")), Show::Object),
            Research {
                command:
                    Some(ResearchCommand::New {
                        repository,
                        goal,
                        measure,
                        better,
                        token_budget,
                    }),
                ..
            } => (
                Request::post(
                    "/v1/research",
                    json!({
                        "repository": repository,
                        "goal": goal,
                        "measure": measure,
                        "better": better,
                        "token_budget": token_budget,
                    }),
                ),
                Show::Object,
            ),
            Research {
                command: Some(ResearchCommand::Confirm { id }),
                ..
            } => (
                Request::post(format!("/v1/research/{id}/confirm"), json!({})),
                Show::Object,
            ),
            Research {
                command: Some(ResearchCommand::Start { id }),
                ..
            } => (
                Request::post(format!("/v1/research/{id}/start"), json!({})),
                Show::Object,
            ),
            Research {
                command: Some(ResearchCommand::Stop { id }),
                ..
            } => (
                Request::post(format!("/v1/research/{id}/stop"), json!({})),
                Show::Object,
            ),
            Grillings => (
                Request::get("/v1/grillings"),
                Show::Table(&["id", "status", "repository_id", "reason"]),
            ),
            Grilling {
                command: GrillingCommand::Show { id },
            } => (Request::get(format!("/v1/grillings/{id}")), Show::Object),
            Grilling {
                command: GrillingCommand::Confirm { id, issues },
            } => {
                let issues = match issues {
                    Some(source) => api_body(source)?,
                    None => get(&format!("/v1/grillings/{id}"))?["issues"].clone(),
                };
                (
                    Request::post(
                        format!("/v1/grillings/{id}/confirm"),
                        json!({ "issues": issues }),
                    ),
                    Show::Object,
                )
            }
            Grilling {
                command: GrillingCommand::Dismiss { id },
            } => (
                Request::post(format!("/v1/grillings/{id}/dismiss"), json!({})),
                Show::Object,
            ),
            Usage { days } => (
                Request::get(match days {
                    Some(days) => format!("/v1/usage?days={days}"),
                    None => "/v1/usage".into(),
                }),
                Show::Usage,
            ),
            DecisionModel => (Request::get("/v1/decision-model"), Show::DecisionModel),
            Doctor => (Request::get("/v1/start"), Show::Doctor),
            Takeovers => (
                Request::get("/v1/takeovers"),
                Show::Table(&["id", "kind", "status", "agreement", "count"]),
            ),
            Takeover { command } => {
                let (action, id, why) = match command {
                    TakeoverCommand::Merge { id, reason } => ("merge", id, reason),
                    TakeoverCommand::Dismiss { id, reason } => ("dismiss", id, reason),
                    TakeoverCommand::Release { id, reason } => ("release", id, reason),
                };
                (
                    Request::post(format!("/v1/takeovers/{id}/{action}"), reason(why)),
                    Show::Object,
                )
            }
            Server {
                command: ServerCommand::Restart,
            } => (
                Request::post("/v1/server/restart", json!({})),
                Show::Done("the server is restarting".into()),
            ),
        })
    }
}

/// The request that does `verb` to the inbox item `item`, as the app's button of that kind does.
fn inbox_request(item: &Value, verb: Verb, reason: &str) -> Result<Request, String> {
    let kind = item["kind"].as_str().unwrap_or_default();
    let id = |value: &Value| {
        value
            .as_i64()
            .ok_or_else(|| format!("item {} has no id to act on", item["id"]))
    };
    let (card, data) = (&item["card_id"], &item["data"]);
    let because = json!({ "reason": reason });
    Ok(match (kind, verb) {
        ("approve_issue", Verb::Approve) => {
            Request::post(format!("/v1/cards/{}/approve", id(card)?), because)
        }
        ("stuck" | "change_closed", Verb::Retry) => {
            Request::post(format!("/v1/cards/{}/retry", id(card)?), because)
        }
        ("ready_to_merge", Verb::Merge) => Request::post(
            format!("/v1/changes/{}/merge", id(&data["change"])?),
            because,
        ),
        ("ready_to_merge", Verb::Reject) => Request::post(
            format!("/v1/changes/{}/reject", id(&data["change"])?),
            because,
        ),
        ("proposal", Verb::Merge) => Request::post(
            format!("/v1/proposals/{}/merge", id(&data["proposal"])?),
            because,
        ),
        ("proposal", Verb::Dismiss) => Request::post(
            format!("/v1/proposals/{}/dismiss", id(&data["proposal"])?),
            because,
        ),
        ("takeover", Verb::Merge) => Request::post(
            format!("/v1/takeovers/{}/merge", id(&data["takeover"])?),
            because,
        ),
        ("takeover", Verb::Dismiss) => Request::post(
            format!("/v1/takeovers/{}/dismiss", id(&data["takeover"])?),
            because,
        ),
        ("grilling_issues", Verb::Confirm) => Request::post(
            format!("/v1/grillings/{}/confirm", id(&data["grilling"])?),
            json!({ "issues": data["issues"] }),
        ),
        ("grilling_issues" | "grilling_report", Verb::Dismiss) => Request::post(
            format!("/v1/grillings/{}/dismiss", id(&data["grilling"])?),
            json!({}),
        ),
        _ => {
            let allowed = match kind {
                "approve_issue" => "approve",
                "stuck" | "change_closed" => "retry",
                "ready_to_merge" => "merge or reject",
                "proposal" | "takeover" => "merge or dismiss",
                "grilling_issues" => "confirm or dismiss",
                "grilling_report" => "dismiss",
                _ => "nothing here; use `moebius api`",
            };
            return Err(format!(
                "item {} is a {kind} item: it takes {allowed}",
                item["id"]
            ));
        }
    })
}

/// The JSON of `--body`: the JSON itself, a file, or standard input.
fn api_body(source: &str) -> Result<Value, String> {
    let text = if source.trim_start().starts_with(['{', '[']) {
        source.to_string()
    } else {
        read_input(source)?
    };
    serde_json::from_str(&text).map_err(|e| format!("the body is not valid JSON: {e}"))
}

/// A file's content, or standard input for `-`.
fn read_input(source: &str) -> Result<String, String> {
    if source == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read standard input: {e}"))?;
        Ok(text)
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("cannot read {source}: {e}"))
    }
}

/// A message given as an argument, or `-` for standard input.
fn text_argument(text: &str) -> Result<String, String> {
    if text == "-" {
        read_input("-")
    } else {
        Ok(text.to_string())
    }
}

/// Runs `command` and says what it printed and whether everything was in order: false only for `doctor` with a step
/// missing.
pub fn operate(client: &Client, command: &Operate, json: bool) -> Result<(String, bool), String> {
    let (request, show) = command.plan(&mut |path| client.get(path))?;
    let text = client.send(&request)?;
    let ready = match show {
        Show::Doctor => {
            serde_json::from_str::<Value>(&text)
                .map_err(|e| format!("the answer is not JSON: {e}"))?["done"]
                == true
        }
        _ => true,
    };
    if json {
        return Ok((pretty(&text), ready));
    }
    // `repos` and `api` show the answer as it came, except that the repositories are the boards'.
    if matches!(command, Operate::Repos) {
        let snapshot: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let repositories: Vec<Value> = snapshot["boards"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|board| board["repository"].clone())
            .collect();
        return Ok((
            table(
                &Value::Array(repositories),
                &["id", "name", "forge", "default_branch", "github"],
            ),
            ready,
        ));
    }
    Ok((render(&show, &text)?, ready))
}

/// JSON pretty printed; what is not JSON as it is.
fn pretty(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.to_string()),
        Err(_) => text.trim_end().to_string(),
    }
}

/// An answer written as plain text.
pub fn render(show: &Show, text: &str) -> Result<String, String> {
    if let Show::Raw = show {
        return Ok(pretty(text));
    }
    let value: Value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(text).map_err(|e| format!("the answer is not JSON: {e}"))?
    };
    Ok(match show {
        Show::Raw => unreachable!("handled above"),
        Show::Done(sentence) => sentence.clone(),
        _ if value.is_null() => "done".into(),
        Show::Board(filter) => board(&value, filter.as_deref())?,
        Show::Card(id) => card(&value, *id)?,
        Show::Inbox => inbox(&value),
        Show::Table(columns) => table(&value, columns),
        Show::Object => object(&value),
        Show::Rules => rules(&value),
        Show::Settings => settings(&value),
        Show::Retro => retro(&value),
        Show::Usage => usage(&value),
        Show::DecisionModel => decision_model(&value),
        Show::Doctor => doctor(&value),
    })
}

/// A value as one line: text as it is, `-` for null, anything else as JSON.
fn cell(value: &Value) -> String {
    match value {
        Value::Null => "-".into(),
        Value::String(text) => text.replace('\n', " "),
        other => other.to_string(),
    }
}

/// Rows of objects as aligned lines with `columns`; `none` for no rows.
fn table(rows: &Value, columns: &[&str]) -> String {
    let rows: Vec<Vec<String>> = rows
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| columns.iter().map(|c| cell(&row[*c])).collect())
        .collect();
    if rows.is_empty() {
        return "none".into();
    }
    let widths: Vec<usize> = (0..columns.len())
        .map(|i| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0))
        .collect();
    rows.iter()
        .map(|row| {
            let last = row.len() - 1;
            row.iter()
                .enumerate()
                .map(|(i, text)| {
                    if i == last {
                        text.clone()
                    } else {
                        format!("{text:<width$}", width = widths[i])
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// An object as `key: value` lines; text of several lines is indented below its key, other nested values are JSON.
fn object(value: &Value) -> String {
    let Some(fields) = value.as_object() else {
        return cell(value);
    };
    fields
        .iter()
        .map(|(key, value)| match value {
            Value::String(text) if text.contains('\n') => {
                let indented: Vec<String> = text.lines().map(|l| format!("  {l}")).collect();
                format!("{key}:\n{}", indented.join("\n"))
            }
            Value::String(text) => format!("{key}: {text}"),
            Value::Null => format!("{key}: -"),
            other => format!("{key}: {other}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The columns in the order of the board, then any other.
const COLUMNS: [&str; 6] = [
    "Backlog",
    "Ready",
    "In progress",
    "In review",
    "Ready to merge",
    "Done",
];

fn board(snapshot: &Value, filter: Option<&str>) -> Result<String, String> {
    let boards: Vec<&Value> = snapshot["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|board| {
            filter.is_none_or(|f| {
                board["repository"]["name"] == f
                    || board["repository"]["id"].to_string().as_str() == f
            })
        })
        .collect();
    if boards.is_empty() {
        return Err(match filter {
            Some(f) => format!("no repository {f}"),
            None => "no repository yet".into(),
        });
    }
    let mut out = Vec::new();
    for board in boards {
        let repository = &board["repository"];
        out.push(format!(
            "{} (repository {})",
            cell(&repository["name"]),
            cell(&repository["id"])
        ));
        let cards = board["cards"].as_array().cloned().unwrap_or_default();
        let mut columns: Vec<String> = COLUMNS.iter().map(|c| c.to_string()).collect();
        for card in &cards {
            let column = cell(&card["column"]);
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
        for column in columns {
            let here: Vec<&Value> = cards
                .iter()
                .filter(|c| cell(&c["column"]) == column)
                .collect();
            out.push(format!("  {column} ({})", here.len()));
            for card in here {
                let mut line = format!(
                    "    {}  #{}  {}",
                    cell(&card["id"]),
                    cell(&card["number"]),
                    cell(&card["title"])
                );
                if let Some(run) = card["run"].as_object() {
                    line += &format!(
                        " [{} {}]",
                        cell(&run["agent"].clone()),
                        cell(&run["status"].clone())
                    );
                }
                if let Some(why) = board["waiting"][card["id"].to_string()].as_str() {
                    line += &format!(" (waiting: {})", why.lines().next().unwrap_or_default());
                }
                out.push(line);
            }
        }
    }
    Ok(out.join("\n"))
}

fn card(snapshot: &Value, id: i64) -> Result<String, String> {
    for board in snapshot["boards"].as_array().into_iter().flatten() {
        let Some(card) = board["cards"]
            .as_array()
            .and_then(|cards| cards.iter().find(|c| c["id"] == id))
        else {
            continue;
        };
        let mut lines = vec![
            format!(
                "card {id} #{} {}",
                cell(&card["number"]),
                cell(&card["title"])
            ),
            format!(
                "repository: {} ({})",
                cell(&board["repository"]["name"]),
                cell(&card["repository_id"])
            ),
            format!("column: {}", cell(&card["column"])),
            format!("author: {}", cell(&card["author"])),
            format!(
                "rounds: {}, tokens: {}",
                cell(&card["rounds"]),
                cell(&card["tokens"])
            ),
        ];
        if let Some(run) = card["run"].as_object() {
            lines.push(format!(
                "run: {} {} {} ({})",
                cell(&run["id"]),
                cell(&run["agent"]),
                cell(&run["status"]),
                cell(&run["purpose"])
            ));
        }
        if let Some(change) = card["change"].as_object() {
            lines.push(format!(
                "change: {} checks {} review {}",
                cell(&change["id"]),
                cell(&change["checks"]),
                cell(&change["review_verdict"])
            ));
        }
        if let Some(why) = board["waiting"][id.to_string()].as_str() {
            lines.push(format!("waiting: {why}"));
        }
        if let Some(note) = card["note"].as_str() {
            lines.push(format!("note: {note}"));
        }
        if card["blocked_by"].as_array().is_some_and(|b| !b.is_empty()) {
            lines.push(format!("blocked by: {}", card["blocked_by"]));
        }
        if let Some(body) = card["body"].as_str().filter(|b| !b.trim().is_empty()) {
            lines.push(String::new());
            lines.push(body.trim_end().to_string());
        }
        return Ok(lines.join("\n"));
    }
    Err(format!("no card {id}"))
}

fn inbox(snapshot: &Value) -> String {
    let items = Value::Array(
        snapshot["inbox"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| {
                let mut item = item.clone();
                if item["card_id"].is_null() {
                    item["card_id"] = "-".into();
                }
                item
            })
            .collect(),
    );
    table(&items, &["id", "kind", "card_id", "title"])
}

fn rules(view: &Value) -> String {
    let mut out = vec!["built in:".to_string()];
    for pattern in view["patterns"].as_array().into_iter().flatten() {
        out.push(format!(
            "  {}  {}",
            cell(&pattern["name"]),
            cell(&pattern["description"])
        ));
    }
    let prose = &view["prose"];
    out.push(format!(
        "in prose (from {}, {}):",
        cell(&prose["source"]),
        if prose["editable"] == true {
            "editable"
        } else {
            "read only"
        }
    ));
    if let Some(error) = prose["error"].as_str() {
        out.push(format!("  error: {error}"));
    }
    out.extend(
        prose["text"]
            .as_str()
            .unwrap_or("")
            .lines()
            .map(|l| format!("  {l}")),
    );
    for repository in view["repositories"].as_array().into_iter().flatten() {
        out.push(format!("in {}:", cell(&repository["repository"])));
        out.extend(
            repository["text"]
                .as_str()
                .unwrap_or("")
                .lines()
                .map(|l| format!("  {l}")),
        );
    }
    if let Some(refusals) = view["refusals"].as_object().filter(|r| !r.is_empty()) {
        out.push("refusals in the last 7 days:".into());
        out.extend(refusals.iter().map(|(rule, n)| format!("  {rule}: {n}")));
    }
    if let Some(commit) = view["commit"].as_str() {
        out.push(format!("committed {commit}"));
    }
    out.join("\n")
}

fn settings(view: &Value) -> String {
    let mut out = vec![format!("file: {}", cell(&view["file"]))];
    for (key, entry) in view["values"].as_object().into_iter().flatten() {
        let note = if entry["default"] == true {
            "  (default)"
        } else {
            ""
        };
        out.push(format!("{key} = {}{note}", entry["value"]));
    }
    let names: Vec<String> = view["secrets"]
        .as_array()
        .into_iter()
        .flatten()
        .map(cell)
        .collect();
    out.push(format!(
        "stored keys: {}",
        if names.is_empty() {
            "none".into()
        } else {
            names.join(", ")
        }
    ));
    if view["restart_needed"] == true {
        out.push(format!("restart needed for: {}", view["restart_keys"]));
    }
    out.join("\n")
}

fn retro(view: &Value) -> String {
    let mut out = vec![format!(
        "retro {} {}",
        cell(&view["date"]),
        cell(&view["status"])
    )];
    for warning in view["warnings"].as_array().into_iter().flatten() {
        out.push(format!("warning: {}", cell(warning)));
    }
    let errors = &view["report"]["errors"];
    out.push(format!(
        "errors ({}):",
        errors.as_array().map_or(0, Vec::len)
    ));
    for error in errors.as_array().into_iter().flatten() {
        out.push(format!(
            "  {}  {}",
            cell(&error["kind"]),
            cell(&error["text"])
        ));
    }
    let proposals = &view["proposals"];
    if proposals.as_array().is_some_and(|p| !p.is_empty()) {
        out.push("proposals:".into());
        out.push(
            table(proposals, &["id", "status", "branch", "title"])
                .lines()
                .map(|l| format!("  {l}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    out.join("\n")
}

fn usage(view: &Value) -> String {
    let totals = &view["totals"];
    let mut out = vec![
        format!(
            "{} day(s) up to {}: {} tokens, {} calls, {}% cached, day budget {}",
            cell(&view["days_in_window"]),
            cell(&view["today"]),
            cell(&totals["tokens"]),
            cell(&totals["calls"]),
            (totals["cached_share"].as_f64().unwrap_or(0.0) * 100.0).round(),
            cell(&totals["day_budget"]),
        ),
        "agents:".into(),
    ];
    out.push(table(&view["agents"], &["agent", "tokens", "calls"]));
    out.push("models:".into());
    out.push(table(&view["models"], &["model", "tokens", "calls"]));
    out.join("\n")
}

/// One line for each step of `GET /v1/start`.
fn doctor(view: &Value) -> String {
    let steps = view["steps"].as_array().into_iter().flatten();
    steps
        .map(|step| {
            let state = if step["done"] == true {
                "ok"
            } else {
                "missing"
            };
            format!(
                "{state:<8} {}: {}",
                cell(&step["key"]),
                cell(&step["detail"])
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn decision_model(view: &Value) -> String {
    if view["configured"] != true {
        return "no decision model is configured".into();
    }
    let mut out = vec![format!(
        "model {}: takes a kind over at {} agreement with {} judgements",
        cell(&view["model"]),
        cell(&view["agreement"]),
        cell(&view["min_judgements"])
    )];
    out.push("kinds:".into());
    out.push(table(
        &view["kinds"],
        &[
            "kind",
            "shadows",
            "agreement",
            "failed",
            "dropped",
            "routed",
        ],
    ));
    out.push("takeovers:".into());
    out.push(table(
        &view["takeovers"],
        &["id", "kind", "status", "agreement", "count"],
    ));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct Probe {
        #[command(subcommand)]
        command: Operate,
    }

    fn plan(args: &[&str]) -> Result<(Request, Show), String> {
        let probe = Probe::try_parse_from(std::iter::once("moebius").chain(args.iter().copied()))
            .map_err(|e| e.to_string())?;
        probe
            .command
            .plan(&mut |_| Err("no lookup expected".into()))
    }

    #[test]
    fn an_api_call_takes_a_method_a_route_and_a_body() {
        let (request, _) = plan(&["api", "post", "v1/cards/3/approve"]).unwrap();
        assert_eq!(request.method, Method::POST);
        assert_eq!(request.path, "/v1/cards/3/approve");
        assert_eq!(request.body, Some(json!({})));

        let (request, _) = plan(&["api", "GET", "/v1/snapshot"]).unwrap();
        assert_eq!((request.method, request.body), (Method::GET, None));

        let (request, _) = plan(&["api", "PUT", "/v1/rules", "--body", r#"{"text":"x"}"#]).unwrap();
        assert_eq!(request.body, Some(json!({"text": "x"})));

        assert!(
            plan(&["api", "PATCH", "/v1/rules"])
                .unwrap_err()
                .contains("PATCH")
        );
        assert!(
            plan(&["api", "POST", "/v1/rules", "--body", "{nope"])
                .unwrap_err()
                .contains("not valid JSON")
        );
    }

    #[test]
    fn a_body_can_come_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("body.json");
        std::fs::write(&file, r#"{"reason": "fine"}"#).unwrap();
        let (request, _) =
            plan(&["api", "POST", "/v1/x", "--body", file.to_str().unwrap()]).unwrap();
        assert_eq!(request.body, Some(json!({"reason": "fine"})));
        assert!(
            plan(&["api", "POST", "/v1/x", "--body", "/no/such/file"])
                .unwrap_err()
                .contains("cannot read")
        );
    }

    #[test]
    fn commands_map_to_their_routes() {
        let route = |args: &[&str]| {
            let (request, _) = plan(args).unwrap();
            (request.method.to_string(), request.path, request.body)
        };
        assert_eq!(route(&["board"]).1, "/v1/snapshot");
        assert_eq!(route(&["card", "4", "--history"]).1, "/v1/cards/4/history");
        assert_eq!(route(&["card", "4", "--runs"]).1, "/v1/cards/4/runs");
        assert_eq!(
            route(&[
                "chat",
                "start",
                "hello",
                "--repository",
                "2",
                "--agent",
                "Coder"
            ]),
            (
                "POST".into(),
                "/v1/chats".into(),
                Some(json!({"message": "hello", "repository": 2, "agent": "Coder"}))
            )
        );
        assert_eq!(route(&["chat", "end", "7"]).1, "/v1/chats/7/end");
        assert_eq!(
            route(&["issue", "create", "1", "Fix it", "--body", "now"]),
            (
                "POST".into(),
                "/v1/repositories/1/issues".into(),
                Some(json!({"title": "Fix it", "body": "now"}))
            )
        );
        assert_eq!(
            route(&[
                "repo",
                "add",
                "w",
                "/srv/w",
                "--github",
                "acme/w",
                "--trusted",
                "leon"
            ])
            .2,
            Some(
                json!({"name": "w", "url": "/srv/w", "forge": "github", "github": "acme/w", "trusted": ["leon"]})
            )
        );
        assert_eq!(route(&["usage", "--days", "7"]).1, "/v1/usage?days=7");
        assert_eq!(
            route(&["takeover", "release", "3", "--reason", "back"]),
            (
                "POST".into(),
                "/v1/takeovers/3/release".into(),
                Some(json!({"reason": "back"}))
            )
        );
        assert_eq!(route(&["retro"]).1, "/v1/retros");
        assert_eq!(route(&["retro", "2026-10-28"]).1, "/v1/retros/2026-10-28");
        assert_eq!(
            route(&["retro", "run", "--date", "2026-10-27"]).2,
            Some(json!({"date": "2026-10-27"}))
        );
        assert_eq!(route(&["research"]).1, "/v1/research");
        assert_eq!(route(&["research", "2"]).1, "/v1/research/2");
        assert_eq!(route(&["research", "stop", "2"]).1, "/v1/research/2/stop");
        assert_eq!(route(&["server", "restart"]).1, "/v1/server/restart");
        assert_eq!(route(&["agent", "delete", "Old"]).0, "DELETE");
    }

    #[test]
    fn settings_values_are_json_when_they_parse_as_json() {
        let (request, _) = plan(&[
            "settings",
            "set",
            "board.round_limit=7",
            "teacher.model=qwen",
            "budgets.day_tokens=null",
            "harness.mcp_env=[\"A\"]",
            "--remove",
            "agents.Coder",
        ])
        .unwrap();
        assert_eq!(request.path, "/v1/settings");
        assert_eq!(
            request.body,
            Some(json!({
                "set": {
                    "board.round_limit": 7,
                    "teacher.model": "qwen",
                    "budgets.day_tokens": null,
                    "harness.mcp_env": ["A"],
                },
                "remove": ["agents.Coder"],
            }))
        );
        assert!(
            plan(&["settings", "set", "nokey"])
                .unwrap_err()
                .contains("KEY=VALUE")
        );
    }

    #[test]
    fn a_key_is_never_an_argument() {
        // `secrets set` takes the name only; the key is read from standard input.
        assert!(plan(&["secrets", "set", "anthropic", "sk-123"]).is_err());
    }

    #[test]
    fn an_agent_is_saved_from_flags() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("be-brief.md");
        std::fs::write(&file, "Be brief.\n").unwrap();
        let (request, _) = plan(&[
            "agent",
            "save",
            "Fast",
            "--model",
            "m",
            "--thinking",
            "low",
            "--skill",
            "a",
            "--skill",
            "b",
            "--instructions",
            file.to_str().unwrap(),
        ])
        .unwrap();
        assert_eq!(
            (request.method, request.path.as_str()),
            (Method::PUT, "/v1/agents/Fast")
        );
        assert_eq!(
            request.body,
            Some(json!({
                "settings": {"model": "m", "thinking": "low", "skills": ["a", "b"], "extensions": [], "mcp": []},
                "instructions": "Be brief.\n",
            }))
        );
    }

    #[test]
    fn an_inbox_item_is_acted_on_as_its_kind_allows() {
        let item = |kind: &str, card: Value, data: Value| json!({"id": 5, "kind": kind, "card_id": card, "title": "t", "data": data});
        let path = |item: &Value, verb| inbox_request(item, verb, "ok").map(|r| (r.path, r.body));
        assert_eq!(
            path(&item("approve_issue", json!(3), json!({})), Verb::Approve).unwrap(),
            ("/v1/cards/3/approve".into(), Some(json!({"reason": "ok"})))
        );
        assert_eq!(
            path(&item("stuck", json!(3), json!({})), Verb::Retry)
                .unwrap()
                .0,
            "/v1/cards/3/retry"
        );
        assert_eq!(
            path(
                &item("ready_to_merge", json!(3), json!({"change": 9})),
                Verb::Reject
            )
            .unwrap()
            .0,
            "/v1/changes/9/reject"
        );
        assert_eq!(
            path(
                &item("proposal", Value::Null, json!({"proposal": 2})),
                Verb::Dismiss
            )
            .unwrap()
            .0,
            "/v1/proposals/2/dismiss"
        );
        assert_eq!(
            path(
                &item("takeover", Value::Null, json!({"takeover": 4})),
                Verb::Merge
            )
            .unwrap()
            .0,
            "/v1/takeovers/4/merge"
        );
        assert_eq!(
            path(
                &item(
                    "grilling_issues",
                    Value::Null,
                    json!({"grilling": 6, "issues": [{"title": "a"}]})
                ),
                Verb::Confirm
            )
            .unwrap(),
            (
                "/v1/grillings/6/confirm".into(),
                Some(json!({"issues": [{"title": "a"}]}))
            )
        );
        let refused = path(&item("stuck", json!(3), json!({})), Verb::Merge).unwrap_err();
        assert!(
            refused.contains("stuck") && refused.contains("retry"),
            "{refused}"
        );
        assert!(path(&item("custom", Value::Null, json!({})), Verb::Merge).is_err());
    }

    #[test]
    fn acting_on_an_item_looks_it_up_in_the_inbox() {
        let probe =
            Probe::try_parse_from(["moebius", "inbox", "act", "5", "merge", "--reason", "ok"])
                .unwrap();
        let snapshot = json!({"inbox": [{"id": 5, "kind": "ready_to_merge", "card_id": 1, "data": {"change": 8}}]});
        let (request, _) = probe.command.plan(&mut |_| Ok(snapshot.clone())).unwrap();
        assert_eq!(request.path, "/v1/changes/8/merge");
        let missing = probe
            .command
            .plan(&mut |_| Ok(json!({"inbox": []})))
            .unwrap_err();
        assert_eq!(missing, "no item 5 in the inbox");
    }

    #[test]
    fn the_board_is_written_by_column() {
        let snapshot = json!({"boards": [{
            "repository": {"id": 1, "name": "demo"},
            "cards": [
                {"id": 11, "number": 1, "title": "First", "column": "Ready", "run": null},
                {"id": 12,
                    "repository_id": 1, "number": 2, "title": "Second", "column": "In progress",
                 "run": {"id": 3, "agent": "Coder", "purpose": "code", "status": "running"}},
                {"id": 13, "number": 3, "title": "Third", "column": "Ready"},
            ],
            "waiting": {"13": "blocked by #2"},
        }]});
        let text = snapshot.to_string();
        let all = render(&Show::Board(None), &text).unwrap();
        assert_eq!(
            all,
            "demo (repository 1)\n  Backlog (0)\n  Ready (2)\n    11  #1  First\n    13  #3  Third (waiting: blocked by #2)\n  \
             In progress (1)\n    12  #2  Second [Coder running]\n  In review (0)\n  Ready to merge (0)\n  Done (0)"
        );
        assert_eq!(render(&Show::Board(Some("1".into())), &text).unwrap(), all);
        assert_eq!(
            render(&Show::Board(Some("demo".into())), &text).unwrap(),
            all
        );
        assert_eq!(
            render(&Show::Board(Some("other".into())), &text).unwrap_err(),
            "no repository other"
        );
        assert!(
            render(&Show::Card(12), &text)
                .unwrap()
                .starts_with("card 12 #2 Second\nrepository: demo (1)")
        );
        assert_eq!(render(&Show::Card(99), &text).unwrap_err(), "no card 99");
    }

    #[test]
    fn tables_align_and_objects_list_their_fields() {
        let rows = json!([{"id": 1, "name": "demo", "x": null}, {"id": 20, "name": "widgets", "x": "a\nb"}]);
        assert_eq!(
            table(&rows, &["id", "name", "x"]),
            "1   demo     -\n20  widgets  a b"
        );
        assert_eq!(table(&json!([]), &["id"]), "none");
        assert_eq!(
            object(&json!({"a": 1, "b": "text", "c": null, "d": "x\ny", "e": [1]})),
            "a: 1\nb: text\nc: -\nd:\n  x\n  y\ne: [1]"
        );
        assert_eq!(render(&Show::Object, "").unwrap(), "done");
        assert_eq!(render(&Show::Done("ok".into()), "").unwrap(), "ok");
        assert_eq!(render(&Show::Raw, "{\"a\":1}").unwrap(), "{\n  \"a\": 1\n}");
        assert_eq!(render(&Show::Raw, "line\n").unwrap(), "line");
    }

    #[test]
    fn the_server_comes_from_the_flags_then_the_desktop_app_then_the_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let desktop = dir.path().join("desktop.toml");
        let config = dir.path().join("moebius.toml");
        let nothing = dir.path().join("none.toml");
        let go = |server, token, desktop: &Path, config: &Path| {
            connect(server, token, Some(desktop), config)
        };

        // Nothing at all.
        let error = go(None, None, &nothing, &nothing).unwrap_err();
        assert!(error.contains("no server to connect to"), "{error}");

        // The configuration in the current directory: its listen address and its token file.
        std::fs::write(&config, "listen = \"0.0.0.0:9001\"\n").unwrap();
        let error = go(None, None, &nothing, &config).unwrap_err();
        assert!(error.contains("cannot read the token"), "{error}");
        std::fs::create_dir_all(dir.path().join(".moebius")).unwrap();
        std::fs::write(dir.path().join(".moebius/token"), "from-file\n").unwrap();
        let found = go(None, None, &nothing, &config).unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("http://127.0.0.1:9001", "from-file")
        );

        // The desktop app's settings come before the file; an address without a scheme gets one.
        std::fs::write(
            &desktop,
            "address = \"10.0.0.2:7171/\"\ntoken = \"from-app\"\n",
        )
        .unwrap();
        let found = go(None, None, &desktop, &config).unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("http://10.0.0.2:7171", "from-app")
        );

        // A token alone replaces the token only.
        let found = go(None, Some("mine"), &desktop, &config).unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("http://10.0.0.2:7171", "mine")
        );

        // A server given must come with its token, so that one server's token is never sent to another.
        let found = go(
            Some("https://moebius.example/"),
            Some("t"),
            &desktop,
            &config,
        )
        .unwrap();
        assert_eq!(
            (found.url.as_str(), found.token.as_str()),
            ("https://moebius.example", "t")
        );
        let error = go(Some("http://x:1"), None, &desktop, &config).unwrap_err();
        assert!(error.contains("--server needs --token"), "{error}");

        // Settings that are not valid are an error, not skipped.
        std::fs::write(&desktop, "address = 1\n").unwrap();
        assert!(
            go(None, None, &desktop, &config)
                .unwrap_err()
                .contains("not valid")
        );
    }

    #[test]
    fn the_flags_read_the_environment() {
        use clap::CommandFactory;
        let command = crate::Cli::command();
        let arg = |name: &str| {
            command
                .get_arguments()
                .find(|a| a.get_id() == name)
                .unwrap()
                .get_env()
                .map(|e| e.to_string_lossy().into_owned())
        };
        assert_eq!(arg("server").as_deref(), Some("MOEBIUS_URL"));
        assert_eq!(arg("token").as_deref(), Some("MOEBIUS_TOKEN"));
    }
}
