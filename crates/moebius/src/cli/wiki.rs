//! The wiki from a terminal: `wiki` lists its pages, `wiki read` writes one, and `wiki propose` offers a new text of
//! a page as a branch that waits in the inbox for a member to merge (the Moebius agent edits the wiki no other way).

use clap::Subcommand;
use serde_json::{Value, json};

use super::{Plan, Request, Show, read_input};

#[derive(Debug, Subcommand)]
pub enum WikiCommand {
    /// List the pages, one path each.
    List,
    /// Write a page's text; `--at` reads an older version.
    Read {
        /// The page's path without `.md`, such as `Team/On call`.
        path: String,
        /// A commit of the page's history.
        #[arg(long)]
        at: Option<String>,
    },
    /// Propose the text in a file (`-` for standard input) as the page's new text. It waits in the inbox and changes
    /// the wiki only once a member merges it.
    Propose {
        path: String,
        file: String,
        /// The commit's message; without one it says which page is added or updated.
        #[arg(long)]
        message: Option<String>,
    },
}

/// `wiki ...`.
pub fn wiki(command: &WikiCommand) -> Plan {
    Ok(match command {
        WikiCommand::List => (Request::get("/v1/wiki/pages"), Show::Table(&["path"])),
        WikiCommand::Read { path, at } => {
            let mut route = format!("/v1/wiki/pages/{}", encoded_path(path));
            if let Some(at) = at {
                route.push_str(&format!("?at={}", encoded_part(at)));
            }
            (Request::get(route), Show::Wiki)
        }
        WikiCommand::Propose {
            path,
            file,
            message,
        } => {
            let mut body = json!({ "path": path, "text": read_input(file)? });
            if let Some(message) = message {
                body["message"] = json!(message);
            }
            (Request::post("/v1/wiki/proposals", body), Show::Object)
        }
    })
}

/// A page read, as its text.
pub fn show_page(page: &Value) -> String {
    page["text"].as_str().unwrap_or_default().to_string()
}

/// A page's path as the parts of a URL path.
fn encoded_path(path: &str) -> String {
    path.split('/')
        .map(encoded_part)
        .collect::<Vec<_>>()
        .join("/")
}

fn encoded_part(part: &str) -> String {
    part.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_read_by_its_encoded_path() {
        let (request, _) = wiki(&WikiCommand::Read {
            path: "Team/On call".into(),
            at: Some("abc1234".into()),
        })
        .unwrap();
        assert_eq!(request.path, "/v1/wiki/pages/Team/On%20call?at=abc1234");
    }

    #[test]
    fn a_proposal_is_posted_with_the_text_of_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.md");
        std::fs::write(&file, "# New\n").unwrap();
        let (request, _) = wiki(&WikiCommand::Propose {
            path: "Runbook".into(),
            file: file.to_string_lossy().into_owned(),
            message: Some("Add it".into()),
        })
        .unwrap();
        assert_eq!(request.path, "/v1/wiki/proposals");
        assert_eq!(
            request.body.unwrap(),
            json!({"path": "Runbook", "text": "# New\n", "message": "Add it"})
        );
    }
}
