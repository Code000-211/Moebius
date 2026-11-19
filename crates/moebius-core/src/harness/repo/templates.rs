//! Templates in the clone: `templates/issues/<kind id>.toml` is what a new issue of that kind starts from (a title, a
//! body and values for the kind's fields; the kind's id, not its name, so any name a kind has works and a rename keeps
//! its template), and `templates/pages/<name>.md` is what a new wiki page starts from. They are
//! read from the clone's default branch; a person's or an agent's edit is a commit on that branch pushed to `origin`,
//! like the other edits of the harness configuration repository (see `edits`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::HarnessRepo;
use super::edits::{Edit, remove_if_there};
use crate::app::{App, AppError, Result};
use crate::decide::Actor;

/// The directory of the clone that holds the templates.
pub const DIR: &str = "templates";

/// What a new issue of a kind starts from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IssueTemplate {
    /// The title the issue starts with; empty when the person gives one.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// Values for the kind's fields, by field name.
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
}

/// Whether `name` can name a template: letters, digits and `-_`, so it is one file name on every system.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn check_name(what: &str, name: &str) -> Result<()> {
    if valid_name(name) {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "{name:?} is not a name for a {what} template (letters, digits, - and _, up to 64)"
        )))
    }
}

impl HarnessRepo {
    fn template_file(&self, folder: &str, name: &str, extension: &str) -> PathBuf {
        self.path
            .join(DIR)
            .join(folder)
            .join(format!("{name}.{extension}"))
    }

    /// The names of the templates in `templates/<folder>/` with `extension`, sorted; a link or a name that is not a
    /// template's is left out.
    fn template_names(&self, folder: &str, extension: &str) -> Vec<String> {
        let dir = self.path.join(DIR).join(folder);
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|entry| {
                let file = entry.file_name().to_string_lossy().into_owned();
                let name = file.strip_suffix(&format!(".{extension}"))?.to_string();
                valid_name(&name).then_some(name)
            })
            .collect();
        names.sort();
        names
    }

    /// The names of the issue templates, which are the names of kinds.
    pub fn issue_template_names(&self) -> Vec<String> {
        self.template_names("issues", "toml")
    }

    /// The names of the page templates.
    pub fn page_template_names(&self) -> Vec<String> {
        self.template_names("pages", "md")
    }

    /// The issue template with the file name `name` (a kind's id); none when there is none or it cannot be read.
    pub fn issue_template(&self, name: &str) -> Option<IssueTemplate> {
        if !valid_name(name) {
            return None;
        }
        let path = self.template_file("issues", name, "toml");
        if path.symlink_metadata().ok()?.file_type().is_symlink() {
            return None;
        }
        toml::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    /// The text of the page template `name`; none when there is none.
    pub fn page_template(&self, name: &str) -> Option<String> {
        if !valid_name(name) {
            return None;
        }
        let path = self.template_file("pages", name, "md");
        if path.symlink_metadata().ok()?.file_type().is_symlink() {
            return None;
        }
        std::fs::read_to_string(path).ok()
    }

    /// A person sets the issue template that has the file name `name` (a kind's id): committed on the
    /// default branch and pushed to `origin`. The short id of the commit is given; a template that changes nothing makes
    /// none.
    pub fn save_issue_template(
        &self,
        app: &App,
        by: &Actor,
        name: &str,
        template: &IssueTemplate,
    ) -> Result<Option<String>> {
        check_name("issue", name)?;
        let text = toml::to_string(template)
            .map_err(|e| AppError::Invalid(format!("cannot write the template: {e}")))?;
        let path = self.template_file("issues", name, "toml");
        self.save_template(app, by, ("issue", name), &path, &text)
    }

    /// A person sets the text of the page template `name`, the same way.
    pub fn save_page_template(
        &self,
        app: &App,
        by: &Actor,
        name: &str,
        text: &str,
    ) -> Result<Option<String>> {
        check_name("page", name)?;
        let path = self.template_file("pages", name, "md");
        self.save_template(app, by, ("page", name), &path, text)
    }

    fn save_template(
        &self,
        app: &App,
        by: &Actor,
        (what, name): (&str, &str),
        path: &std::path::Path,
        text: &str,
    ) -> Result<Option<String>> {
        let mut read_at = self.lock();
        let verb = if path.exists() { "Update" } else { "Add" };
        let message = format!("{verb} the {what} template {name}");
        let files = format!("{DIR}/{what}s/{name}.*");
        let context = BTreeMap::from([
            ("template".to_string(), format!("{what}/{name}")),
            ("text".to_string(), text.to_string()),
        ]);
        let write = || {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Ok(std::fs::write(path, text)?)
        };
        let edit = Edit {
            kind: "edit_template",
            files: &files,
            message: &message,
        };
        self.commit(app, by, &mut read_at, &edit, context, write)
    }

    /// A person deletes a template (`what` is `issue` or `page`), the same way; `NotFound` when there is none.
    pub fn delete_template(
        &self,
        app: &App,
        by: &Actor,
        what: &str,
        name: &str,
    ) -> Result<Option<String>> {
        check_name(what, name)?;
        let extension = if what == "issue" { "toml" } else { "md" };
        let path = self.template_file(&format!("{what}s"), name, extension);
        let mut read_at = self.lock();
        if !path.is_file() {
            return Err(AppError::NotFound(format!("{what} template {name}")));
        }
        let message = format!("Delete the {what} template {name}");
        let files = format!("{DIR}/{what}s/{name}.*");
        let context = BTreeMap::from([("template".to_string(), format!("{what}/{name}"))]);
        let edit = Edit {
            kind: "edit_template",
            files: &files,
            message: &message,
        };
        self.commit(app, by, &mut read_at, &edit, context, || {
            remove_if_there(&path)
        })
    }
}
