//! The calls about boards: the snapshot, repositories, cards and issues, what a person decides in the inbox, and a
//! repository's files and column scripts.

use crate::client::{
    Card, Client, ClientError, FileDetail, GrillingIssue, HistoryEntry, NewRepository, Repository,
    ScriptDetail, Snapshot, Tree,
};

/// The route of a column's script; a column's name may have spaces.
fn script_path(repository: i64, column: &str) -> String {
    format!(
        "/v1/repositories/{repository}/scripts/{}",
        column.replace(' ', "%20")
    )
}

/// `text` with every byte but letters, digits and `-._~` percent-encoded, and `/` kept when `slashes` says so, for a
/// path or a query value of a route.
pub(super) fn encoded(text: &str, slashes: bool) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            b'/' if slashes => "/".to_string(),
            b => format!("%{b:02X}"),
        })
        .collect()
}

/// The route of a file of a repository.
fn file_path(repository: i64, path: &str) -> String {
    format!(
        "/v1/repositories/{repository}/files/{}",
        encoded(path, true)
    )
}

impl Client {
    pub async fn snapshot(&self) -> Result<Snapshot, ClientError> {
        self.get("/v1/snapshot").await
    }

    /// Adds a repository; the server clones it.
    pub async fn add_repository(
        &self,
        repository: &NewRepository,
    ) -> Result<Repository, ClientError> {
        let body =
            serde_json::to_value(repository).map_err(|e| ClientError::Decode(e.to_string()))?;
        self.post("/v1/repositories", body).await
    }

    /// Replaces the GitHub logins whose issues on a repository need no approval.
    pub async fn edit_repository(
        &self,
        repository: i64,
        trusted: &[String],
    ) -> Result<Repository, ClientError> {
        let path = format!("/v1/repositories/{repository}");
        self.put(&path, serde_json::json!({ "trusted": trusted }))
            .await
    }

    /// Changes a card's title and body; on a forge the server changes the issue there too.
    pub async fn edit_card(&self, card: i64, title: &str, body: &str) -> Result<Card, ClientError> {
        let path = format!("/v1/cards/{card}");
        self.put(&path, serde_json::json!({ "title": title, "body": body }))
            .await
    }

    /// Creates an issue in a repository.
    pub async fn create_issue(
        &self,
        repository: i64,
        title: &str,
        body: &str,
    ) -> Result<Card, ClientError> {
        let path = format!("/v1/repositories/{repository}/issues");
        self.post(&path, serde_json::json!({ "title": title, "body": body }))
            .await
    }

    pub async fn history(&self, card: i64) -> Result<Vec<HistoryEntry>, ClientError> {
        self.get(&format!("/v1/cards/{card}/history")).await
    }

    /// What a person does with an inbox item. The answers are not read: the snapshot shows what changed.
    async fn decide(&self, path: String, body: serde_json::Value) -> Result<(), ClientError> {
        self.post::<serde_json::Value>(&path, body).await.map(drop)
    }

    /// Merges an open change.
    pub async fn merge(&self, change: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/changes/{change}/merge"), serde_json::json!({}))
            .await
    }

    /// Sends a change back to coding with the reason as the card's note.
    pub async fn reject(&self, change: i64, reason: &str) -> Result<(), ClientError> {
        let body = serde_json::json!({ "reason": reason });
        self.decide(format!("/v1/changes/{change}/reject"), body)
            .await
    }

    /// Approves an outside issue that waits in the backlog.
    pub async fn approve(&self, card: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/cards/{card}/approve"), serde_json::json!({}))
            .await
    }

    /// Gives a stuck card, or one whose pull request was closed, a fresh start.
    pub async fn retry(&self, card: i64) -> Result<(), ClientError> {
        self.decide(format!("/v1/cards/{card}/retry"), serde_json::json!({}))
            .await
    }

    /// Merges a proposal of the Retro into the harness configuration.
    pub async fn merge_proposal(&self, proposal: i64) -> Result<(), ClientError> {
        let path = format!("/v1/proposals/{proposal}/merge");
        self.decide(path, serde_json::json!({})).await
    }

    /// Dismisses a proposal of the Retro.
    pub async fn dismiss_proposal(&self, proposal: i64) -> Result<(), ClientError> {
        let path = format!("/v1/proposals/{proposal}/dismiss");
        self.decide(path, serde_json::json!({})).await
    }

    /// Lets the decision model take a kind over.
    pub async fn merge_takeover(&self, takeover: i64) -> Result<(), ClientError> {
        let path = format!("/v1/takeovers/{takeover}/merge");
        self.decide(path, serde_json::json!({})).await
    }

    /// Refuses a takeover proposal.
    pub async fn dismiss_takeover(&self, takeover: i64) -> Result<(), ClientError> {
        let path = format!("/v1/takeovers/{takeover}/dismiss");
        self.decide(path, serde_json::json!({})).await
    }

    /// Hands a kind that the decision model took over back to its decider.
    pub async fn release_takeover(&self, takeover: i64) -> Result<(), ClientError> {
        let path = format!("/v1/takeovers/{takeover}/release");
        self.decide(path, serde_json::json!({})).await
    }

    /// Confirms a grilling with the issues as the person left them: a card is made of each.
    pub async fn confirm_grilling(
        &self,
        grilling: i64,
        issues: &[GrillingIssue],
    ) -> Result<(), ClientError> {
        let path = format!("/v1/grillings/{grilling}/confirm");
        self.decide(path, serde_json::json!({ "issues": issues }))
            .await
    }

    /// Dismisses the issues or the report of a grilling.
    pub async fn dismiss_grilling(&self, grilling: i64) -> Result<(), ClientError> {
        let path = format!("/v1/grillings/{grilling}/dismiss");
        self.decide(path, serde_json::json!({})).await
    }

    /// The directory `dir` of a repository's default branch; `""` is the root.
    pub async fn tree(&self, repository: i64, dir: &str) -> Result<Tree, ClientError> {
        self.get(&format!(
            "/v1/repositories/{repository}/tree?path={}",
            encoded(dir, false)
        ))
        .await
    }

    /// A file of a repository's default branch.
    pub async fn file(&self, repository: i64, path: &str) -> Result<FileDetail, ClientError> {
        self.get(&file_path(repository, path)).await
    }

    /// Writes a file of a repository; the server commits it on the default branch.
    pub async fn save_file(
        &self,
        repository: i64,
        path: &str,
        text: &str,
    ) -> Result<FileDetail, ClientError> {
        let body = serde_json::json!({ "text": text });
        self.put(&file_path(repository, path), body).await
    }

    /// The script a column runs for a repository.
    pub async fn script(&self, repository: i64, column: &str) -> Result<ScriptDetail, ClientError> {
        self.get(&script_path(repository, column)).await
    }

    /// Saves the repository's own script for a column; the server commits it.
    pub async fn save_script(
        &self,
        repository: i64,
        column: &str,
        text: &str,
    ) -> Result<ScriptDetail, ClientError> {
        let body = serde_json::json!({ "text": text });
        self.put(&script_path(repository, column), body).await
    }

    /// Removes the repository's own script for a column, so that the built-in one runs.
    pub async fn delete_script(
        &self,
        repository: i64,
        column: &str,
    ) -> Result<ScriptDetail, ClientError> {
        self.delete(&script_path(repository, column)).await
    }
}
