//! The repositories: `repos` lists those of the boards, `repo add` clones one into the server, on the GitHub forge
//! when it names one.

use clap::Subcommand;
use serde_json::{Value, json};

use super::{Plan, Request, Show, table};

#[derive(Debug, Subcommand)]
pub enum RepoCommand {
    /// Clone a repository into the server.
    Add {
        name: String,
        /// Anything `git clone` takes, as seen from the server. An absolute path that does not exist, or an empty
        /// folder, becomes a new repository.
        url: String,
        /// `owner/repo` on GitHub; the repository then uses the GitHub forge.
        #[arg(long)]
        github: Option<String>,
        /// A GitHub login whose issues need no approval; give it again for more.
        #[arg(long = "trusted")]
        trusted: Vec<String>,
    },
}

/// `repos`, which reads the snapshot.
pub fn repos() -> Plan {
    Ok((Request::get("/v1/snapshot"), Show::Repositories))
}

/// `repo add`.
pub fn repo(command: &RepoCommand) -> Plan {
    let RepoCommand::Add {
        name,
        url,
        github,
        trusted,
    } = command;
    let mut body = json!({ "name": name, "url": url });
    if let Some(github) = github {
        body["forge"] = "github".into();
        body["github"] = github.clone().into();
    }
    if !trusted.is_empty() {
        body["trusted"] = json!(trusted);
    }
    Ok((Request::post("/v1/repositories", body), Show::Object))
}

/// The repositories of the snapshot's boards, one a line.
pub fn show_repositories(snapshot: &Value) -> String {
    let repositories: Vec<Value> = snapshot["boards"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|board| board["repository"].clone())
        .collect();
    table(
        &Value::Array(repositories),
        &["id", "name", "forge", "default_branch", "github"],
    )
}
