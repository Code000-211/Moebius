//! Pi packages: `ext search` finds a package in the catalog of pi.dev, `ext info` shows what a source is, `ext add`,
//! `ext update` and `ext remove` propose installing, updating and removing one as a change that a person merges, and
//! `ext list` lists what is installed.

use clap::Subcommand;
use serde_json::{Value, json};

use super::{Plan, Request, Show, cell};

#[derive(Debug, Subcommand)]
pub enum ExtCommand {
    /// Find packages in the catalog (the npm packages with the keyword pi-package) that match the words.
    Search {
        text: Vec<String>,
        /// Only packages of this type: extension, skill, theme or prompt.
        #[arg(long = "type")]
        kind: Option<String>,
    },
    /// What a package is before it is installed: `npm:<name>[@<version>]` or `git:<host>/<owner>/<repo>[@<ref>]`.
    Info { source: String },
    /// Propose installing a package; a person merges the change. With --agent the agent is given its extensions and skills.
    Add {
        source: String,
        /// The agent that names the package, as `pkg:<name>`.
        #[arg(long)]
        agent: Option<String>,
    },
    /// Propose updating a package, or all of them, to the newest version their source allows.
    Update { name: Option<String> },
    /// Propose removing a package, and its names out of the agents.
    Remove { name: String },
    /// The installed packages.
    List,
}

/// A text as one part of a URL.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The request a `ext` command stands for.
pub fn ext(command: &ExtCommand) -> Plan {
    Ok(match command {
        ExtCommand::Search { text, kind } => {
            let mut path = format!("/v1/extensions/search?q={}", encode(&text.join(" ")));
            if let Some(kind) = kind {
                path.push_str(&format!("&type={}", encode(kind)));
            }
            (
                Request::get(path),
                Show::Table(&[
                    "name",
                    "version",
                    "kinds",
                    "weekly_downloads",
                    "description",
                ]),
            )
        }
        ExtCommand::Info { source } => (
            Request::get(format!("/v1/extensions/info?source={}", encode(source))),
            Show::Object,
        ),
        ExtCommand::Add { source, agent } => {
            let mut body = json!({ "source": source });
            if let Some(agent) = agent {
                body["agent"] = json!(agent);
            }
            (
                Request::post("/v1/extensions/packages", body),
                Show::Offered,
            )
        }
        ExtCommand::Update { name: Some(name) } => (
            Request::post(
                format!("/v1/extensions/packages/{}/update", encode(name)),
                json!({}),
            ),
            Show::Offered,
        ),
        ExtCommand::Update { name: None } => (
            Request::post("/v1/extensions/update", json!({})),
            Show::Offered,
        ),
        ExtCommand::Remove { name } => (
            Request::delete(format!("/v1/extensions/packages/{}", encode(name))),
            Show::Offered,
        ),
        ExtCommand::List => (
            Request::get("/v1/extensions/packages"),
            Show::Table(&["name", "version", "source", "agents"]),
        ),
    })
}

/// A change that was offered, or the changes of an update, as plain text.
pub fn show_offered(value: &Value) -> String {
    let Some(offered) = value["offered"].as_array() else {
        return one(value);
    };
    let mut out: Vec<String> = offered.iter().map(one).collect();
    for package in value["up_to_date"].as_array().into_iter().flatten() {
        out.push(format!("{} is up to date", cell(package)));
    }
    for failure in value["failed"].as_array().into_iter().flatten() {
        out.push(format!(
            "{} could not be updated: {}",
            cell(&failure["package"]),
            cell(&failure["error"])
        ));
    }
    if out.is_empty() {
        out.push("no packages".into());
    }
    out.join("\n\n")
}

fn one(offered: &Value) -> String {
    let versions = match (offered["from"].as_str(), offered["to"].as_str()) {
        (Some(from), Some(to)) => format!(" {from} -> {to}"),
        (None, Some(to)) => format!(" {to}"),
        (Some(from), None) => format!(" {from}"),
        (None, None) => String::new(),
    };
    let mut out = vec![format!(
        "{} {}{versions}: change {} ({}) waits in the inbox for a person to merge",
        cell(&offered["action"]),
        cell(&offered["package"]),
        cell(&offered["change"]["id"]),
        cell(&offered["change"]["branch"]),
    )];
    out.push(format!("  source: {}", cell(&offered["source"])));
    for key in ["resolved", "integrity"] {
        if let Some(text) = offered[key].as_str() {
            out.push(format!("  {key}: {text}"));
        }
    }
    for (label, key) in [("extensions", "extensions"), ("skills", "skills")] {
        let names = offered["resources"][key].as_array().into_iter().flatten();
        let names: Vec<String> = names.map(cell).collect();
        if !names.is_empty() {
            out.push(format!("  {label}: {}", names.join(", ")));
        }
    }
    let agents: Vec<String> = offered["agents"]
        .as_array()
        .into_iter()
        .flatten()
        .map(cell)
        .collect();
    if !agents.is_empty() {
        out.push(format!("  agents: {}", agents.join(", ")));
    }
    let files: Vec<String> = offered["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(cell)
        .collect();
    if !files.is_empty() {
        out.push(format!(
            "  files ({} more below node_modules):",
            cell(&offered["dependency_files"])
        ));
        out.extend(files.iter().map(|file| format!("    {file}")));
    }
    if let Some(changelog) = offered["changelog"].as_str().filter(|c| !c.is_empty()) {
        out.push("  changelog:".into());
        out.extend(changelog.lines().map(|line| format!("    {line}")));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_map_to_their_routes() {
        let route = |args: &[&str]| {
            let (request, _) = super::super::tests::plan(args).unwrap();
            (request.method.to_string(), request.path, request.body)
        };
        assert_eq!(
            route(&["ext", "search", "web", "access", "--type", "extension"]).1,
            "/v1/extensions/search?q=web%20access&type=extension"
        );
        assert_eq!(
            route(&["ext", "info", "npm:@a/b@^1.0.0"]).1,
            "/v1/extensions/info?source=npm%3A%40a%2Fb%40%5E1.0.0"
        );
        assert_eq!(
            route(&["ext", "add", "npm:pi-web-access", "--agent", "Coder"]),
            (
                "POST".into(),
                "/v1/extensions/packages".into(),
                Some(json!({"source": "npm:pi-web-access", "agent": "Coder"}))
            )
        );
        assert_eq!(
            route(&["ext", "update", "@a/b"]).1,
            "/v1/extensions/packages/%40a%2Fb/update"
        );
        assert_eq!(route(&["ext", "update"]).1, "/v1/extensions/update");
        assert_eq!(
            route(&["ext", "remove", "pi-web-access"]),
            (
                "DELETE".into(),
                "/v1/extensions/packages/pi-web-access".into(),
                None
            )
        );
        assert_eq!(route(&["ext", "list"]).1, "/v1/extensions/packages");
    }

    #[test]
    fn an_offered_change_is_told_with_what_a_reviewer_looks_at() {
        let offered = json!({
            "change": {"id": 7, "branch": "moebius/ext-add-x-1"}, "package": "x", "action": "update",
            "source": "npm:x", "from": "1.0.0", "to": "1.1.0", "integrity": "sha512-abc",
            "resources": {"extensions": ["extensions/a.ts"], "skills": []}, "agents": ["Coder"],
            "files": ["M packages/x/package.json"], "dependency_files": 3, "changelog": "## 1.1.0\n- fixed"
        });
        let text = show_offered(&offered);
        assert!(
            text.starts_with("update x 1.0.0 -> 1.1.0: change 7"),
            "{text}"
        );
        assert!(
            text.contains("integrity: sha512-abc") && text.contains("M packages/x/package.json")
        );
        assert!(text.contains("    - fixed"), "{text}");
        let updates = json!({"offered": [], "up_to_date": ["y"], "failed": [{"package": "z", "error": "no"}]});
        assert_eq!(
            show_offered(&updates),
            "y is up to date\n\nz could not be updated: no"
        );
    }
}
