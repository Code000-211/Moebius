//! The `webhook` trigger: a path on the server that starts a run of the workflow whose trigger names it.
//!
//! A trigger gives the workflow its `path`, the one safe URL segment the hook answers on, and may name a `secret` and
//! a `repository`. The `secret` is the *name* of a key kept in the server's own secret store (`<data_dir>/secrets.toml`,
//! what `PUT /v1/secrets/{name}` writes), never the key itself: a workflow file is committed to a git repository and
//! pushed to a forge, so a key written in it would leave the server. A delivery then carries that key in
//! `Authorization: Bearer <key>`, and a hook whose named key the server does not have answers exactly as a path with
//! no hook on it does, so a secret nobody has set cannot be probed. The `repository` is the name of the repository a
//! run belongs to when the URL does not name one; `POST /api/hooks/{repository}/{path}` names it per delivery.
//!
//! The door that answers is `server/api/workflow_hooks.rs`; the contract people read is
//! `docs/reference/workflows/webhook-trigger.md`.

use serde_json::Value;

use crate::workflows::definition::WorkflowDefinition;
use crate::workflows::nodes::Ports;

/// The trigger's kind name in a workflow file.
pub const KIND: &str = "webhook";

/// How many runs of one workflow its hook lets be queued or running at once. Past this a delivery is refused with a
/// clear status rather than dropped, so the sender knows to try again later.
pub const MAX_ACTIVE_RUNS: i64 = 8;

/// The longest hook path or secret name a trigger may give: a URL segment a person can read off a forge's settings
/// page.
const MAX_SEGMENT: usize = 64;

/// What a `webhook` trigger says about the hook its workflow answers on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookTrigger {
    /// The one URL segment `POST /api/hooks/{path}` answers on.
    pub path: String,
    /// The name of the key in the server's secret store whose value a delivery must carry, when the hook is not open
    /// to everyone who knows its path.
    pub secret: Option<String>,
    /// The repository a run belongs to when the URL does not name one.
    pub repository: Option<String>,
}

impl WebhookTrigger {
    /// The trigger these parameters describe, or what is wrong with them.
    pub fn parse(params: &Value) -> Result<Self, String> {
        let table = params
            .as_object()
            .ok_or("the webhook trigger's settings are a table")?;
        for key in table.keys() {
            match key.as_str() {
                "path" | "secret" | "repository" => {}
                other => return Err(format!("unknown webhook trigger setting {other:?}")),
            }
        }
        let path =
            segment(table.get("path"), "path")?.ok_or("the webhook trigger needs a `path`")?;
        let secret = segment(table.get("secret"), "secret")?;
        let repository = match table.get("repository") {
            None => None,
            Some(Value::String(name)) if !name.is_empty() && !name.contains('/') => {
                Some(name.clone())
            }
            Some(_) => {
                return Err("the webhook trigger's `repository` is a repository name".to_string());
            }
        };
        Ok(Self {
            path,
            secret,
            repository,
        })
    }

    /// The trigger has one output, the value its run starts with, and no input.
    pub fn ports(&self) -> Ports {
        Ports::new(std::iter::empty::<String>(), ["out"])
    }
}

/// Where a hook path leads.
#[derive(Debug)]
pub struct Hook {
    /// The workflow whose trigger answers on the path, named as its file is.
    pub workflow: String,
    /// That workflow's webhook trigger.
    pub trigger: WebhookTrigger,
}

/// Every workflow whose `webhook` trigger answers on `path`, in the order the workflows are read.
pub fn matching(workflows: &[(String, WorkflowDefinition)], path: &str) -> Vec<Hook> {
    let mut found = Vec::new();
    for (name, definition) in workflows {
        let Some(trigger) = trigger_of(definition) else {
            continue;
        };
        if trigger.path != path {
            continue;
        }
        found.push(Hook {
            workflow: name.clone(),
            trigger,
        });
    }
    found
}

/// The one hook among `hooks`, or what is wrong when two workflows claim one path: a configuration the server will
/// not guess at, so the answer names both.
pub fn only(mut hooks: Vec<Hook>) -> Result<Option<Hook>, String> {
    if hooks.len() > 1 {
        return Err(format!(
            "workflows {:?} and {:?} both have a webhook trigger on {:?}",
            hooks[0].workflow, hooks[1].workflow, hooks[0].trigger.path
        ));
    }
    Ok(hooks.pop())
}

/// The webhook trigger of a workflow that has one, and none for a workflow triggered otherwise.
fn trigger_of(definition: &WorkflowDefinition) -> Option<WebhookTrigger> {
    let trigger = definition
        .trigger
        .first()
        .filter(|trigger| trigger.kind == KIND)?;
    WebhookTrigger::parse(&trigger.params).ok()
}

/// A setting that is one URL segment: letters, digits, `-` and `_`, at most [`MAX_SEGMENT`] of them.
fn segment(value: Option<&Value>, key: &str) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        Value::String(text) if is_segment(text) => Ok(Some(text.clone())),
        _ => Err(format!(
            "the webhook trigger's {key:?} is not one URL segment of letters, digits, \"-\" and \"_\""
        )),
    }
}

fn is_segment(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_SEGMENT
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::definition;
    use serde_json::json;

    /// A workflow file whose trigger is a webhook trigger, with one node behind it.
    const FILE: &str = r#"
name = "w"
description = "One."

[[trigger]]
kind = "webhook"
[trigger.params]
path = "build"
secret = "build-hook"
repository = "radiance"

[[nodes]]
id = "build"
kind = "test"

[[edges]]
from = "trigger"
to = "build"
port = "out"
"#;

    #[test]
    fn a_workflow_file_with_a_webhook_trigger_is_valid() {
        let definition = definition::parse("w", FILE).unwrap();
        let trigger = trigger_of(&definition).unwrap();
        assert_eq!(trigger.path, "build");
        assert_eq!(trigger.secret.as_deref(), Some("build-hook"));
        assert_eq!(trigger.repository.as_deref(), Some("radiance"));
    }

    #[test]
    fn a_webhook_trigger_without_a_path_is_refused_in_its_file() {
        let error = definition::parse(
            "w",
            "name = \"w\"\ndescription = \"One.\"\n[[trigger]]\nkind = \"webhook\"\n",
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("trigger: the webhook trigger needs a `path`"),
            "{error}"
        );
    }

    #[test]
    fn an_edge_leaves_the_webhook_trigger_by_its_only_port() {
        let error = definition::parse("w", &FILE.replace("port = \"out\"", "port = \"in\""))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                r#"edge to "build" leaves by port "in", which is not one of its outputs"#
            ),
            "{error}"
        );
    }

    #[test]
    fn a_webhook_trigger_is_not_a_node_kind() {
        let error = definition::parse(
            "w",
            "name = \"w\"\ndescription = \"One.\"\n[[trigger]]\nkind = \"test\"\n\n[[nodes]]\nid = \"n\"\nkind = \"webhook\"\n",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("unknown node kind"), "{error}");
    }

    fn definition(trigger: &str) -> WorkflowDefinition {
        WorkflowDefinition::parse(
            &format!("name = \"w\"\ndescription = \"One.\"\n[[trigger]]\nkind = {trigger}\n"),
            "workflows/w.toml",
        )
        .unwrap()
    }

    #[test]
    fn a_trigger_names_its_path() {
        let trigger = WebhookTrigger::parse(&json!({"path": "nightly-build"})).unwrap();
        assert_eq!(trigger.path, "nightly-build");
        assert_eq!(trigger.secret, None);
        assert_eq!(trigger.repository, None);
        assert_eq!(trigger.ports().outputs, ["out"]);
        assert!(trigger.ports().inputs.is_empty());
    }

    #[test]
    fn a_trigger_names_a_secret_and_a_repository() {
        let trigger = WebhookTrigger::parse(&json!({
            "path": "build",
            "secret": "build-hook",
            "repository": "radiance",
        }))
        .unwrap();
        assert_eq!(trigger.secret.as_deref(), Some("build-hook"));
        assert_eq!(trigger.repository.as_deref(), Some("radiance"));
    }

    #[test]
    fn a_trigger_without_a_path_is_refused() {
        assert_eq!(
            WebhookTrigger::parse(&json!({})).unwrap_err(),
            "the webhook trigger needs a `path`"
        );
    }

    #[test]
    fn a_path_is_one_safe_segment() {
        for path in [
            "",
            "two/segments",
            "with space",
            "dotted.path",
            &"x".repeat(65),
        ] {
            let error = WebhookTrigger::parse(&json!({"path": path})).unwrap_err();
            assert!(
                error.contains("is not one URL segment"),
                "{path:?} taken for a path: {error}"
            );
        }
    }

    #[test]
    fn a_secret_is_a_name_and_not_a_key() {
        assert_eq!(
            WebhookTrigger::parse(&json!({"path": "build", "secret": "s ecr et"})).unwrap_err(),
            "the webhook trigger's \"secret\" is not one URL segment of letters, digits, \"-\" and \"_\""
        );
    }

    #[test]
    fn a_setting_nobody_named_is_refused() {
        assert_eq!(
            WebhookTrigger::parse(&json!({"path": "build", "token": "x"})).unwrap_err(),
            "unknown webhook trigger setting \"token\""
        );
    }

    #[test]
    fn a_repository_is_a_name() {
        assert_eq!(
            WebhookTrigger::parse(&json!({"path": "build", "repository": ""})).unwrap_err(),
            "the webhook trigger's `repository` is a repository name"
        );
    }

    fn hooked(name: &str, path: &str) -> (String, WorkflowDefinition) {
        (
            name.to_string(),
            WorkflowDefinition::parse(
                &format!(
                    "name = \"{name}\"\ndescription = \"One.\"\n[[trigger]]\nkind = \"webhook\"\n[trigger.params]\npath = \"{path}\"\n"
                ),
                &format!("workflows/{name}.toml"),
            )
            .unwrap(),
        )
    }

    #[test]
    fn a_hook_is_the_workflow_whose_trigger_names_its_path() {
        let workflows = vec![
            ("a".to_string(), definition("\"test\"")),
            hooked("b", "build"),
            hooked("c", "release"),
        ];

        assert_eq!(
            only(matching(&workflows, "build"))
                .unwrap()
                .unwrap()
                .workflow,
            "b",
            "the hook on \"build\" is workflow b"
        );
        assert_eq!(
            only(matching(&workflows, "release"))
                .unwrap()
                .unwrap()
                .workflow,
            "c"
        );
        assert!(matching(&workflows, "other").is_empty());
    }

    #[test]
    fn two_workflows_on_one_path_are_refused_together() {
        let workflows = vec![hooked("a", "build"), hooked("b", "build")];
        assert_eq!(
            only(matching(&workflows, "build")).unwrap_err(),
            "workflows \"a\" and \"b\" both have a webhook trigger on \"build\""
        );
    }

    #[test]
    fn only_a_webhook_trigger_on_a_path_is_a_hook() {
        assert!(trigger_of(&definition("\"test\"")).is_none());
        assert!(
            trigger_of(&definition("\"webhook\"")).is_none(),
            "a webhook trigger with no path answers on no path"
        );
    }
}
