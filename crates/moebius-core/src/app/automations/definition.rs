//! What an automation is as a workflow: the shape that makes a definition an automation, how an automation's row is
//! read from its file and written to one, and which file holds it. The API, the CLI and the schedule all read and write
//! the same definition through here.

use serde_json::{Map, Value};

use super::cron;
use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::board::Runs;
use crate::store::{Automation, NewAutomation};
use crate::workflows::definition::{
    self, DEFAULT_MAX_NODES, DEFAULT_NODE_TIMEOUT_SECONDS, Edge, NodeDefinition, TRIGGER_ID,
    Trigger, WorkflowDefinition, file_for, read,
};
use crate::workflows::nodes::chat;
use crate::workflows::trigger::schedule as schedule_trigger;

/// Whether `definition` is an automation: a schedule trigger and one node, which starts an agent chat.
pub fn is_automation(definition: &WorkflowDefinition) -> bool {
    definition.trigger.len() == 1
        && definition.trigger[0].kind == schedule_trigger::KIND
        && definition.nodes.len() == 1
        && definition.nodes[0].kind == chat::KIND
}

impl App {
    /// Checks what an automation is made of: a name, a schedule that parses, an agent `runs` has, a prompt, and one
    /// place at most, which exists.
    pub(super) fn check_automation(
        &self,
        runs: &dyn Runs,
        new: &NewAutomation,
    ) -> Result<NewAutomation> {
        let new = NewAutomation {
            name: new.name.trim().to_string(),
            schedule: new
                .schedule
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            agent: new.agent.trim().to_string(),
            prompt: redact(new.prompt.trim()),
            ..new.clone()
        };
        if new.name.is_empty() {
            return Err(AppError::Invalid("an automation needs a name".into()));
        }
        cron::parse(&new.schedule).map_err(AppError::Invalid)?;
        if !runs.has_agent(&new.agent) {
            return Err(AppError::Invalid(format!("no agent named {}", new.agent)));
        }
        if new.prompt.is_empty() {
            return Err(AppError::Invalid("an automation needs a prompt".into()));
        }
        if new.harness && new.repository.is_some() {
            return Err(AppError::Invalid(
                "an automation works in a repository or in the harness configuration, not both"
                    .into(),
            ));
        }
        if let Some(id) = new.repository {
            self.repository(id)?;
        }
        if new.harness {
            self.harness_repo()?;
        }
        Ok(new)
    }

    /// The workflow file that holds the automation `id`: the file its row was written to, and otherwise one taken
    /// from its name.
    pub(super) fn workflow_key(&self, id: i64, name: &str) -> Result<String> {
        match self.store.automation_workflow_key(id)? {
            Some(key) => Ok(key),
            None => self.derived_workflow_key(id, name),
        }
    }

    /// The file an automation called `name` may be written to: its name when the file of that name is free or holds
    /// an automation no other row keeps, and otherwise one made from its row, so that an automation beside a workflow
    /// of the same name, or beside an automation of the same name, keeps a file of its own.
    pub(super) fn derived_workflow_key(&self, id: i64, name: &str) -> Result<String> {
        if file_for(name).is_ok() && self.workflow_key_available(id, name)? {
            return Ok(name.to_string());
        }
        let mut key = format!("automation-{id}");
        let mut count = 2;
        while !self.workflow_key_available(id, &key)? {
            key = format!("automation-{id}-{count}");
            count += 1;
        }
        Ok(key)
    }

    /// Whether the workflow file `key` may hold the automation `id`: it is not an ordinary or unreadable workflow,
    /// and no other automation's row already keeps it.
    fn workflow_key_available(&self, id: i64, key: &str) -> Result<bool> {
        if self.workflow_file_taken(key)? {
            return Ok(false);
        }
        Ok(match self.store.automation_with_workflow_key(key)? {
            Some(owner) => owner.id == id,
            None => true,
        })
    }

    /// Whether the file of the workflow `name` is one an automation may not have: an ordinary workflow, or one that
    /// cannot be read. A file that already holds an automation may still belong to another row; its key does.
    fn workflow_file_taken(&self, name: &str) -> Result<bool> {
        let Some(harness) = self.harness_repo.get() else {
            return Ok(false);
        };
        let path = harness
            .path()
            .join(definition::DIRECTORY)
            .join(format!("{name}.toml"));
        if !path.is_file() {
            return Ok(false);
        }
        match read(harness.path(), name) {
            Ok(definition) => Ok(!is_automation(&definition)),
            Err(_) => Ok(true),
        }
    }

    /// The workflow an automation is stored as: a `schedule` trigger and one `chat` node.
    pub(super) fn automation_definition(&self, new: &NewAutomation) -> Result<WorkflowDefinition> {
        let repository = match new.repository {
            Some(repository_id) => self
                .store
                .repositories()?
                .into_iter()
                .find(|repository| repository.id == repository_id)
                .map(|repository| repository.name),
            None => None,
        };
        let mut params = Map::new();
        params.insert("cron".into(), Value::String(new.schedule.clone()));
        params.insert("enabled".into(), Value::Bool(new.enabled));
        if let Some(repository) = repository {
            params.insert("repository".into(), Value::String(repository));
        }
        let mut chat_params = Map::new();
        if !new.agent.is_empty() {
            chat_params.insert("agent".into(), Value::String(new.agent.clone()));
        }
        chat_params.insert("prompt".into(), Value::String(new.prompt.clone()));
        if new.harness {
            chat_params.insert("harness".into(), Value::Bool(true));
        }
        Ok(WorkflowDefinition {
            name: new.name.clone(),
            description: "An automation: starts an agent chat on a cron schedule.".into(),
            node_timeout_seconds: DEFAULT_NODE_TIMEOUT_SECONDS,
            max_nodes: DEFAULT_MAX_NODES,
            trigger: vec![Trigger {
                kind: schedule_trigger::KIND.into(),
                params: Value::Object(params),
            }],
            nodes: vec![NodeDefinition {
                id: "chat".into(),
                kind: chat::KIND.into(),
                params: Value::Object(chat_params),
                position: Default::default(),
            }],
            edges: vec![Edge {
                from: TRIGGER_ID.into(),
                to: "chat".into(),
                port: "out".into(),
            }],
        })
    }

    /// Every automation-shaped workflow in the harness configuration repository, by name.
    pub(super) fn automation_definitions(&self) -> Result<Vec<(String, WorkflowDefinition)>> {
        let Some(harness) = self.harness_repo.get() else {
            return Ok(Vec::new());
        };
        let definitions = definition::list(harness.path())
            .map_err(|error| AppError::Invalid(error.to_string()))?;
        Ok(definitions
            .into_iter()
            .filter(|(_, definition)| is_automation(definition))
            .collect())
    }

    /// Read the workflow that holds `automation`, when the harness configuration repository has one.
    pub(super) fn automation_workflow(
        &self,
        id: i64,
        name: &str,
    ) -> Result<Option<WorkflowDefinition>> {
        let Some(harness) = self.harness_repo.get() else {
            return Ok(None);
        };
        let key = self.workflow_key(id, name)?;
        if !harness
            .path()
            .join(definition::DIRECTORY)
            .join(format!("{key}.toml"))
            .is_file()
        {
            return Ok(None);
        }
        read(harness.path(), &key)
            .map(Some)
            .map_err(|error| AppError::Invalid(error.to_string()))
    }

    /// The automation as its workflow file writes it, with the row's id and firings kept.
    pub(super) fn automation_from_workflow(
        &self,
        mut automation: Automation,
        definition: &WorkflowDefinition,
    ) -> Result<Automation> {
        let Some(trigger) = definition.trigger.first() else {
            return Ok(automation);
        };
        let Value::Object(params) = &trigger.params else {
            return Ok(automation);
        };
        if let Some(Value::String(cron)) = params.get("cron") {
            automation.schedule = cron.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        automation.enabled = params
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        automation.repository_id = match params.get("repository").and_then(Value::as_str) {
            Some(name) => self
                .store
                .repositories()?
                .into_iter()
                .find(|repository| repository.name == name)
                .map(|repository| repository.id),
            None => None,
        };
        if let Some(node) = definition.nodes.iter().find(|node| node.kind == chat::KIND)
            && let Ok(chat) = chat::Chat::from_params(&node.params)
        {
            automation.agent = chat.agent;
            automation.prompt = chat.prompt;
            automation.harness_repository = chat.harness;
        }
        automation.name = definition.name.clone();
        Ok(automation)
    }

    /// The row the automation-shaped workflow in the file `key` is kept against, adding one when the workflow was
    /// written by hand and taking a row of its name that has no file of its own yet.
    pub(super) fn ensure_automation(
        &self,
        key: &str,
        definition: &WorkflowDefinition,
    ) -> Result<Automation> {
        if let Some(automation) = self.store.automation_with_workflow_key(key)? {
            return Ok(automation);
        }
        if let Some(automation) = self
            .store
            .automation_without_workflow_named(&definition.name)?
        {
            self.store.set_automation_workflow_key(automation.id, key)?;
            return Ok(automation);
        }
        let blank = Automation {
            id: 0,
            name: String::new(),
            schedule: String::new(),
            agent: String::new(),
            prompt: String::new(),
            repository_id: None,
            harness_repository: false,
            enabled: true,
            created_at: 0,
        };
        let parsed = self.automation_from_workflow(blank, definition)?;
        let new = NewAutomation {
            name: parsed.name,
            schedule: parsed.schedule,
            agent: parsed.agent,
            prompt: parsed.prompt,
            repository: parsed.repository_id,
            harness: parsed.harness_repository,
            enabled: parsed.enabled,
        };
        let added = self
            .store
            .add_automation(&new, &self.store.local_minute()?)?;
        self.store.set_automation_workflow_key(added.id, key)?;
        Ok(added)
    }

    /// The automation row with its workflow file over it, when the file is an automation.
    pub(super) fn stored_automation(&self, automation: Automation) -> Result<Automation> {
        match self.automation_workflow(automation.id, &automation.name)? {
            Some(definition) if is_automation(&definition) => {
                self.automation_from_workflow(automation, &definition)
            }
            _ => Ok(automation),
        }
    }

    /// The workflow that holds `automation`, built from its row when there is no file yet.
    pub(super) fn definition_of(&self, automation: &Automation) -> Result<WorkflowDefinition> {
        if let Some(definition) = self.automation_workflow(automation.id, &automation.name)?
            && is_automation(&definition)
        {
            return Ok(definition);
        }
        self.automation_definition(&NewAutomation {
            name: automation.name.clone(),
            schedule: automation.schedule.clone(),
            agent: automation.agent.clone(),
            prompt: automation.prompt.clone(),
            repository: automation.repository_id,
            harness: automation.harness_repository,
            enabled: automation.enabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTOMATION: &str = r#"
name = "nightly-triage"
description = "Starts a chat each night."

[[trigger]]
kind = "schedule"
params = { cron = "30 3 * * *", repository = "demo" }

[[nodes]]
id = "chat"
kind = "chat"
params = { prompt = "Read the board." }

[[edges]]
from = "trigger"
to = "chat"
port = "out"
"#;

    #[test]
    fn a_schedule_with_one_chat_node_is_an_automation() {
        assert!(is_automation(
            &definition::parse("nightly-triage", AUTOMATION).unwrap()
        ));
    }

    #[test]
    fn a_schedule_with_any_other_node_is_not_an_automation() {
        let other = AUTOMATION.replace("kind = \"chat\"", "kind = \"test\"");
        assert!(!is_automation(
            &definition::parse("nightly-triage", &other).unwrap()
        ));
    }
}
