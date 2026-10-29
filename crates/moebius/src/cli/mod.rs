//! The `moebius` binary as a client of a running server: a command for each thing done often through the desktop API,
//! and `api` for every route. A person uses them in a terminal; the Moebius agent uses them in its chats, as
//! `"$MOEBIUS" board`, with the run's own token.
//!
//! Each command is a thin mapping: [`Operate::plan`] makes the one request it stands for (a few look an item up in the
//! snapshot first), [`Client`] sends it, and [`Show`] says how the answer is written as plain text. With `--json` the
//! answer is the API's JSON as it came.
//!
//! This module holds the commands as clap sees them, the dispatch to the command groups and the helpers they share
//! for reading input and writing answers. `client` finds the server and sends the requests; each other module is one
//! group of commands, with its requests and how its answers are written: `api` (any route), `board` (the board,
//! cards, the inbox and issues), `chats` (chats and grillings), `agents`, `rules`, `repos` (repositories), `setup`
//! (`doctor`, settings, API keys, usage and the server itself) and `improve` (Retros, proposals, research, the
//! decision model and its takeovers).

mod agents;
mod api;
mod board;
mod chats;
mod client;
mod improve;
mod repos;
mod rules;
mod setup;

use std::io::Read;

use clap::Subcommand;
use serde_json::Value;

use agents::AgentCommand;
use board::{InboxCommand, IssueCommand};
use chats::{ChatCommand, GrillingCommand};
use client::Request;
pub use client::{Client, connect, desktop_settings_path};
use improve::{ProposalCommand, ResearchCommand, RetroCommand, TakeoverCommand};
use repos::RepoCommand;
use rules::RulesCommand;
use setup::{SecretsCommand, ServerCommand, SettingsCommand};

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
    /// Start, write to, offer the changes of and end a chat.
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

/// The request a command stands for and how to show its answer, or why it cannot be made.
type Plan = Result<(Request, Show), String>;

impl Operate {
    /// The request this command stands for and how to show its answer. `get` reads a route; the commands that act on
    /// an item they have to look up use it.
    pub fn plan(&self, get: &mut dyn FnMut(&str) -> Result<Value, String>) -> Plan {
        use Operate::*;
        match self {
            Api {
                method,
                route,
                body,
            } => api::plan(method, route, body.as_deref()),
            Board { repository } => board::board(repository),
            Card { id, history, runs } => board::card(*id, *history, *runs),
            Inbox { command } => board::inbox(command.as_ref(), get),
            Issue { command } => board::issue(command),
            Chats => chats::chats(),
            Chat { command } => chats::chat(command),
            Grillings => chats::grillings(),
            Grilling { command } => chats::grilling(command, get),
            Agents => agents::agents(),
            Agent { command } => agents::agent(command),
            Rules { command } => rules::rules(command.as_ref()),
            Repos => repos::repos(),
            Repo { command } => repos::repo(command),
            Settings { command } => setup::settings(command.as_ref()),
            Secrets { command } => setup::secrets(command),
            Usage { days } => setup::usage(*days),
            Doctor => setup::doctor(),
            Server { command } => setup::server(command),
            Retro { date, command } => improve::retro(date.as_deref(), command.as_ref()),
            Proposals => improve::proposals(),
            Proposal { command } => improve::proposal(command),
            Research { id, command } => improve::research(*id, command.as_ref()),
            DecisionModel => improve::decision_model(),
            Takeovers => improve::takeovers(),
            Takeover { command } => improve::takeover(command),
        }
    }
}

/// How an answer is written as plain text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
    /// The answer as it came: pretty JSON, or the text of what is not JSON.
    Raw,
    Board(Option<String>),
    Card(i64),
    Inbox,
    /// The repositories of the snapshot's boards.
    Repositories,
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
    Ok((render(&show, &text)?, ready))
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
        Show::Board(filter) => board::show_board(&value, filter.as_deref())?,
        Show::Card(id) => board::show_card(&value, *id)?,
        Show::Inbox => board::show_inbox(&value),
        Show::Repositories => repos::show_repositories(&value),
        Show::Table(columns) => table(&value, columns),
        Show::Object => object(&value),
        Show::Rules => rules::show_rules(&value),
        Show::Settings => setup::show_settings(&value),
        Show::Retro => improve::show_retro(&value),
        Show::Usage => setup::show_usage(&value),
        Show::DecisionModel => improve::show_decision_model(&value),
        Show::Doctor => setup::show_doctor(&value),
    })
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

/// JSON given as an argument: the JSON itself, a file that holds it, or standard input for `-`.
fn json_input(source: &str) -> Result<Value, String> {
    let text = if source.trim_start().starts_with(['{', '[']) {
        source.to_string()
    } else {
        read_input(source)?
    };
    serde_json::from_str(&text).map_err(|e| format!("the body is not valid JSON: {e}"))
}

/// JSON pretty printed; what is not JSON as it is.
fn pretty(text: &str) -> String {
    match serde_json::from_str::<Value>(text) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| text.to_string()),
        Err(_) => text.trim_end().to_string(),
    }
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};
    use serde_json::json;

    /// The commands alone, to parse a command line of them in a test.
    #[derive(Debug, Parser)]
    pub struct Probe {
        #[command(subcommand)]
        pub command: Operate,
    }

    /// The plan of the command line `args`, which must not need to look anything up.
    pub fn plan(args: &[&str]) -> Result<(Request, Show), String> {
        let probe = Probe::try_parse_from(std::iter::once("moebius").chain(args.iter().copied()))
            .map_err(|e| e.to_string())?;
        probe
            .command
            .plan(&mut |_| Err("no lookup expected".into()))
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
            route(&["chat", "offer", "7", "--reason", "ready"]),
            (
                "POST".into(),
                "/v1/chats/7/offer".into(),
                Some(json!({"reason": "ready"}))
            )
        );
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

    /// Each `"$MOEBIUS" ...` command of a page, as its words: up to the end of the inline code, the line, a comment or
    /// a pipe. A line that ends with `\` goes on.
    fn commands(page: &str) -> Vec<Vec<String>> {
        let page = page.replace("\\\n", " ");
        page.split("\"$MOEBIUS\" ")
            .skip(1)
            .map(|rest| {
                let end = rest.find(['`', '\n', '|', '#', ';']).unwrap_or(rest.len());
                rest[..end].split_whitespace().map(String::from).collect()
            })
            .collect()
    }

    /// Whether the command line `words` names commands and options that exist: every word that can only be a
    /// subcommand is one, and every `--option` belongs to the command or is global.
    fn check(words: &[String]) -> Result<(), String> {
        let root = crate::Cli::command();
        let global = |long: &str| root.get_arguments().any(|a| a.get_long() == Some(long));
        let mut command = &root;
        let mut named = false;
        for word in words {
            if let Some(option) = word.strip_prefix("--") {
                let option = option.split('=').next().unwrap_or_default();
                if !command
                    .get_arguments()
                    .any(|a| a.get_long() == Some(option))
                    && !global(option)
                {
                    return Err(format!("{} has no option --{option}", command.get_name()));
                }
                continue;
            }
            let a_name =
                !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase() || c == '-');
            if command.has_subcommands() && a_name {
                match command.find_subcommand(word) {
                    Some(sub) => {
                        command = sub;
                        named = true;
                        continue;
                    }
                    None if !named || command.get_positionals().next().is_none() => {
                        return Err(format!("{} has no command {word}", command.get_name()));
                    }
                    None => {}
                }
            }
            if !named {
                return Err(format!("{word} is not a command"));
            }
        }
        if !named {
            return Err("no command".into());
        }
        Ok(())
    }

    /// Whether the desktop API reference has a route `method route`; `{x}` in the reference and `<x>` in the route
    /// stand for any segment, and `{*x}` at the end of the reference for the rest of the route. Quotes around the route
    /// are the shell's.
    fn documented(reference: &str, method: &str, route: &str) -> bool {
        let route = route.trim_matches(['"', '\'']);
        let route = route.split('?').next().unwrap_or_default();
        let segments: Vec<&str> = route.split('/').collect();
        reference
            .lines()
            .filter_map(|line| line.strip_prefix(&format!("### `{method} ")))
            .any(|heading| {
                let mut pattern: Vec<&str> = heading.trim_end_matches('`').split('/').collect();
                let rest = pattern.last().is_some_and(|p| p.starts_with("{*"));
                if rest {
                    pattern.pop();
                }
                let fits = if rest {
                    segments.len() > pattern.len()
                        && segments[pattern.len()..].iter().all(|s| !s.is_empty())
                } else {
                    pattern.len() == segments.len()
                };
                fits && pattern.iter().zip(&segments).all(|(p, s)| {
                    p == s
                        || (p.starts_with('{') && !s.is_empty())
                        || (s.starts_with('<') && p.starts_with('{'))
                })
            })
    }

    #[test]
    fn every_command_and_route_the_built_in_skills_name_exists() {
        let reference = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/reference/desktop-api.md"),
        )
        .unwrap();
        let mut checked = 0;
        for skill in moebius_core::harness::skill::all() {
            for words in commands(skill.page()) {
                check(&words).unwrap_or_else(|e| {
                    panic!("{}: \"$MOEBIUS\" {}: {e}", skill.name, words.join(" "))
                });
                if let [api, method, route, ..] = &words[..]
                    && api == "api"
                    && !method.starts_with('<')
                    && !route.starts_with('<')
                {
                    assert!(
                        documented(&reference, method, route),
                        "{}: {method} {route} is not in the desktop API reference",
                        skill.name
                    );
                }
                checked += 1;
            }
        }
        assert!(checked > 40, "{checked}");

        // The check itself finds what does not exist.
        let words = |line: &str| {
            line.split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>()
        };
        assert!(check(&words("agent show Coder --json")).is_ok());
        assert!(check(&words("research 3")).is_ok());
        assert!(check(&words("agent rename Coder")).is_err());
        assert!(check(&words("boards")).is_err());
        assert!(check(&words("card 3 --everything")).is_err());
        assert!(documented(
            &reference,
            "PUT",
            "/v1/repositories/1/scripts/Ready"
        ));
        assert!(!documented(&reference, "PUT", "/v1/runs/1"));
        assert!(documented(
            &reference,
            "PUT",
            "/v1/repositories/1/files/src/main.rs"
        ));
        assert!(!documented(&reference, "PUT", "/v1/repositories/1/files/"));
    }
}
