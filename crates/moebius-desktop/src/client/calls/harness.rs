//! The calls about the harness configuration: agents, rules, skills, MCP servers and uploads.

use super::board::encoded;
use crate::client::{
    Agent, AgentDetail, AgentSettings, Client, ClientError, FileDetail, McpDefinition, McpServer,
    McpServerDetail, RulesView, Runner, Skill, SkillDetail, Upload,
};

/// The route of one of a skill's other files.
fn skill_file_path(name: &str, path: &str) -> String {
    format!("/v1/skills/{name}/files/{}", encoded(path, true))
}

impl Client {
    /// The agents a chat can be with, the Moebius agent first.
    pub async fn agents(&self) -> Result<Vec<Agent>, ClientError> {
        self.get("/v1/agents").await
    }

    /// The runners an agent can name.
    pub async fn runners(&self) -> Result<Vec<Runner>, ClientError> {
        self.get("/v1/runners").await
    }

    /// An agent of the harness configuration repository, with its instructions.
    pub async fn agent(&self, name: &str) -> Result<AgentDetail, ClientError> {
        self.get(&format!("/v1/agents/{name}")).await
    }

    /// Adds or changes an agent; the server commits it to the harness configuration repository.
    pub async fn save_agent(
        &self,
        name: &str,
        settings: &AgentSettings,
        instructions: &str,
    ) -> Result<AgentDetail, ClientError> {
        let body = serde_json::json!({ "settings": settings, "instructions": instructions });
        self.put(&format!("/v1/agents/{name}"), body).await
    }

    /// Deletes an agent of the harness configuration repository.
    pub async fn delete_agent(&self, name: &str) -> Result<(), ClientError> {
        self.delete(&format!("/v1/agents/{name}")).await
    }

    /// The rules page: the built-in patterns, the rules in prose and those of each repository.
    pub async fn rules(&self) -> Result<RulesView, ClientError> {
        self.get("/v1/rules").await
    }

    /// Replaces the rules in prose; the server commits them to the harness configuration repository.
    pub async fn save_rules(&self, text: &str) -> Result<RulesView, ClientError> {
        self.put("/v1/rules", serde_json::json!({ "text": text }))
            .await
    }

    /// The skills an agent can name; none without a harness configuration repository.
    pub async fn skills(&self) -> Result<Vec<Skill>, ClientError> {
        self.get("/v1/skills").await
    }

    /// A skill with its text.
    pub async fn skill(&self, name: &str) -> Result<SkillDetail, ClientError> {
        self.get(&format!("/v1/skills/{name}")).await
    }

    /// Adds or changes a skill; the server commits it to the harness configuration repository.
    pub async fn save_skill(
        &self,
        name: &str,
        description: &str,
        text: &str,
    ) -> Result<SkillDetail, ClientError> {
        let body = serde_json::json!({ "description": description, "text": text });
        self.put(&format!("/v1/skills/{name}"), body).await
    }

    /// Deletes a skill of the harness configuration repository.
    pub async fn delete_skill(&self, name: &str) -> Result<(), ClientError> {
        self.delete(&format!("/v1/skills/{name}")).await
    }

    /// One of a skill's other files: its text, or for a binary or large file only its size.
    pub async fn skill_file(&self, name: &str, path: &str) -> Result<FileDetail, ClientError> {
        self.get(&skill_file_path(name, path)).await
    }

    /// Writes one of a skill's other files; the server commits it and answers the skill with its files.
    pub async fn save_skill_file(
        &self,
        name: &str,
        path: &str,
        text: &str,
    ) -> Result<SkillDetail, ClientError> {
        let body = serde_json::json!({ "text": text });
        self.put(&skill_file_path(name, path), body).await
    }

    /// Deletes one of a skill's other files; the server commits it and answers the skill with its files.
    pub async fn delete_skill_file(
        &self,
        name: &str,
        path: &str,
    ) -> Result<SkillDetail, ClientError> {
        self.delete(&skill_file_path(name, path)).await
    }

    /// Sends a zip file to the server, which keeps it where the Moebius agent can read it.
    pub async fn upload(&self, zip: Vec<u8>) -> Result<Upload, ClientError> {
        let request = self
            .http
            .post(format!("{}/v1/uploads", self.settings.address))
            .header(reqwest::header::CONTENT_TYPE, "application/zip")
            .body(zip);
        self.send(request).await
    }

    /// The MCP servers an agent can name.
    pub async fn mcp_servers(&self) -> Result<Vec<McpServer>, ClientError> {
        self.get("/v1/mcp-servers").await
    }

    /// An MCP server as `mcp.toml` has it.
    pub async fn mcp_server(&self, name: &str) -> Result<McpServerDetail, ClientError> {
        self.get(&format!("/v1/mcp-servers/{name}")).await
    }

    /// Adds or changes an MCP server; the server commits it to the harness configuration repository.
    pub async fn save_mcp_server(
        &self,
        name: &str,
        definition: &McpDefinition,
    ) -> Result<McpServerDetail, ClientError> {
        let body = serde_json::to_value(definition).expect("a definition is JSON");
        self.put(&format!("/v1/mcp-servers/{name}"), body).await
    }

    /// Deletes an MCP server of the harness configuration repository.
    pub async fn delete_mcp_server(&self, name: &str) -> Result<(), ClientError> {
        self.delete(&format!("/v1/mcp-servers/{name}")).await
    }
}
