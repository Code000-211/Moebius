//! Templates for issues (one per kind) and for wiki pages, kept in the harness configuration repository (see
//! `harness::repo`; an issue template's file is named by its kind's id): listing and editing them, which is a commit there, starting an issue from the template of its kind
//! and making a wiki page from a page template. An issue template is checked against its kind when it is saved; its
//! values are checked against the fields again when an issue is made from it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{App, AppError, Result};
use crate::decide::Actor;
use crate::harness::repo::IssueTemplate;
use crate::wiki::{Author, PageView};

/// An issue template with the kind it is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueTemplateView {
    pub kind: String,
    #[serde(flatten)]
    pub template: IssueTemplate,
    /// The short id of the commit a `PUT` made; left out by `GET` and when nothing changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// A page template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageTemplateView {
    pub name: String,
    pub text: String,
    /// The short id of the commit a `PUT` made; left out by `GET` and when nothing changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// Every template, as `GET /v1/templates` answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Templates {
    /// Whether they can be changed: the server has a harness configuration repository.
    pub editable: bool,
    pub issues: Vec<IssueTemplateView>,
    pub pages: Vec<PageTemplateView>,
}

impl App {
    /// The templates the harness configuration repository has; none without one.
    pub fn templates(&self) -> Templates {
        let Ok(repo) = self.harness_repo() else {
            return Templates {
                editable: false,
                issues: Vec::new(),
                pages: Vec::new(),
            };
        };
        let issues = repo
            .issue_template_names()
            .into_iter()
            .filter_map(|name| {
                let template = repo.issue_template(&name)?;
                // A template whose kind is gone is not listed.
                let kind = self.store.kind(name.parse().ok()?).ok().flatten()?;
                Some(IssueTemplateView {
                    kind: kind.name,
                    template,
                    commit: None,
                })
            })
            .collect();
        let pages = repo
            .page_template_names()
            .into_iter()
            .filter_map(|name| {
                let text = repo.page_template(&name)?;
                Some(PageTemplateView {
                    name,
                    text,
                    commit: None,
                })
            })
            .collect();
        Templates {
            editable: true,
            issues,
            pages,
        }
    }

    /// Sets the issue template of the kind `kind`, a commit in the harness configuration repository. The kind has to
    /// exist, and every field the template gives a value for has to be one of the kind's.
    pub fn save_issue_template(
        &self,
        by: &Actor,
        kind: &str,
        template: &IssueTemplate,
    ) -> Result<IssueTemplateView> {
        let kind = self
            .store
            .kind_by_name(kind)?
            .ok_or_else(|| AppError::NotFound(format!("kind {kind}")))?;
        for name in template.fields.keys() {
            if !kind
                .fields
                .iter()
                .any(|f| f.name.eq_ignore_ascii_case(name))
            {
                return Err(AppError::Invalid(format!(
                    "the kind {} has no field {name}",
                    kind.name
                )));
            }
        }
        let commit =
            self.harness_repo()?
                .save_issue_template(self, by, &kind.id.to_string(), template)?;
        Ok(IssueTemplateView {
            kind: kind.name,
            template: template.clone(),
            commit,
        })
    }

    /// Deletes the issue template of the kind `kind`; `NotFound` when it has none.
    pub fn delete_issue_template(&self, by: &Actor, kind: &str) -> Result<Option<String>> {
        let kind = self.kind_named(kind)?;
        self.harness_repo()?
            .delete_template(self, by, "issue", &kind.id.to_string())
    }

    /// Sets the text of the page template `name`, a commit in the harness configuration repository.
    pub fn save_page_template(
        &self,
        by: &Actor,
        name: &str,
        text: &str,
    ) -> Result<PageTemplateView> {
        let commit = self
            .harness_repo()?
            .save_page_template(self, by, name, text)?;
        Ok(PageTemplateView {
            name: name.into(),
            text: text.into(),
            commit,
        })
    }

    /// Deletes the page template `name`; `NotFound` when there is none.
    pub fn delete_page_template(&self, by: &Actor, name: &str) -> Result<Option<String>> {
        self.harness_repo()?.delete_template(self, by, "page", name)
    }

    /// The title, body and field values a new issue starts with: what it was given, and where `use_template` is set and
    /// the kind has a template, the template's title and body for an empty one and its values for a field not given.
    /// A title that stays empty is refused.
    pub fn issue_start(
        &self,
        kind: Option<&str>,
        use_template: bool,
        title: &str,
        body: &str,
        fields: &BTreeMap<String, Value>,
    ) -> Result<(String, String, BTreeMap<String, Value>)> {
        if kind.is_none() && !fields.is_empty() {
            return Err(AppError::Invalid(
                "fields are the kind's: give the kind".into(),
            ));
        }
        let (mut title, mut body, mut fields) =
            (title.to_string(), body.to_string(), fields.clone());
        if use_template {
            let kind = kind.ok_or_else(|| {
                AppError::Invalid("a template is the kind's: give the kind".into())
            })?;
            let kind = self.kind_named(kind)?;
            let template = self
                .harness_repo()?
                .issue_template(&kind.id.to_string())
                .ok_or_else(|| AppError::NotFound(format!("template of the kind {}", kind.name)))?;
            if title.trim().is_empty() {
                title = template.title;
            }
            if body.trim().is_empty() {
                body = template.body;
            }
            for (name, value) in template.fields {
                if !fields.keys().any(|given| given.eq_ignore_ascii_case(&name)) {
                    fields.insert(name, value);
                }
            }
        }
        if title.trim().is_empty() {
            return Err(AppError::Invalid("an issue needs a title".into()));
        }
        Ok((title, body, fields))
    }

    /// Makes the wiki page `path` from the page template `template` (a commit in the wiki). Refused with `Exists` when
    /// the page is there.
    pub fn page_from_template(&self, by: &Actor, template: &str, path: &str) -> Result<PageView> {
        let text = self
            .harness_repo()?
            .page_template(template)
            .ok_or_else(|| AppError::NotFound(format!("page template {template}")))?;
        let wiki = self.wiki()?;
        match wiki.page(path, None) {
            Ok(_) => return Err(AppError::Exists(format!("page {path}"))),
            Err(AppError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
        let message = format!("Add {path} from the template {template}");
        wiki.write_page(self, &Author::of(self, by)?, path, &text, Some(&message))
    }
}
