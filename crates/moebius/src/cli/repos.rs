//! The repositories: `repos` lists those of the boards, `repo add` clones one into the server, on the GitHub, GitLab
//! or Forgejo forge when it names one, and `repo remove` deletes one with everything stored for it.

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
        #[arg(long, conflicts_with_all = ["gitlab", "forgejo"])]
        github: Option<String>,
        /// The project's path on GitLab, such as `group/project`; the repository then uses the GitLab forge.
        #[arg(long, conflicts_with = "forgejo")]
        gitlab: Option<String>,
        /// `owner/repo` on Forgejo, such as Codeberg; the repository then uses the Forgejo forge.
        #[arg(long)]
        forgejo: Option<String>,
        /// A login on the forge whose issues need no approval; give it again for more.
        #[arg(long = "trusted")]
        trusted: Vec<String>,
    },
    /// Remove a repository from the server, with its cards, changes and chats, and delete its clone. Refused while
    /// one of its runs is still going.
    Remove {
        /// The repository, as `repos` shows it.
        id: i64,
        /// Why; recorded with the decision.
        #[arg(long)]
        reason: Option<String>,
    },
}

/// `repos`, which reads the snapshot.
pub fn repos() -> Plan {
    Ok((Request::get("/v1/snapshot"), Show::Repositories))
}

/// `repo add` and `repo remove`.
pub fn repo(command: &RepoCommand) -> Plan {
    match command {
        RepoCommand::Remove { id, reason } => Ok((
            Request::delete_with(
                format!("/v1/repositories/{id}"),
                reason.clone().map(|reason| json!({ "reason": reason })),
            ),
            Show::Object,
        )),
        RepoCommand::Add {
            name,
            url,
            github,
            gitlab,
            forgejo,
            trusted,
        } => {
            let mut body = json!({ "name": name, "url": url });
            if let Some(github) = github {
                body["forge"] = "github".into();
                body["github"] = github.clone().into();
            }
            if let Some(project) = gitlab {
                body["forge"] = "gitlab".into();
                body["project"] = project.clone().into();
            }
            if let Some(repo) = forgejo {
                body["forge"] = "forgejo".into();
                body["project"] = repo.clone().into();
            }
            if !trusted.is_empty() {
                body["trusted"] = json!(trusted);
            }
            Ok((Request::post("/v1/repositories", body), Show::Object))
        }
    }
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
        &["id", "name", "forge", "default_branch", "github", "project"],
    )
}
