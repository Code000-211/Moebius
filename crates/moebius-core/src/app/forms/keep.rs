//! Which templates are in effect for a repository, and keeping the repository's own copies of them current (ADR 0034).
//!
//! The shipped defaults are replaced file by file by the project's, in the harness configuration repository under
//! `projects/<project>/templates/` (`pull-request.md` and one file per comment kind, see [`Kind::file`]). The copies a
//! forge and its people see are `.github/PULL_REQUEST_TEMPLATE.md` and `.github/moebius/<kind>.md`; [`App::sync_templates`]
//! opens a pull request that brings them up to date. `.github/` is a locked path, so that pull request waits for the
//! owner's click in the Inbox and is never merged by the server.

use std::path::Path;

use serde::Serialize;

use super::comments::Kind;
use super::pull::{DEFAULT_ISSUE_TEMPLATE, PullTemplate};
use crate::app::{App, AppError, Result};
use crate::decide::Actor;
use crate::repos::{commit_all, git};
use crate::store::{Change, Repository};

/// The path of the pull request template in a repository.
pub const PULL_REQUEST_PATH: &str = ".github/PULL_REQUEST_TEMPLATE.md";
/// The folder of the comment templates in a repository.
pub const COMMENT_DIR: &str = ".github/moebius";
/// The branch the templates are offered on.
pub const SYNC_BRANCH: &str = "moebius/templates";
/// The issue form in a repository.
pub const ISSUE_FORM_PATH: &str = ".github/ISSUE_TEMPLATE/moebius.yml";
/// The file of the issue template in a project's folder of templates.
pub const ISSUE_FILE: &str = "issue.md";
/// The file of the pull request template in a project's folder of templates.
pub const PULL_REQUEST_FILE: &str = "pull-request.md";

/// One template in effect.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateInfo {
    /// `pull_request` or a comment kind such as `done`.
    pub name: String,
    /// `default` or `project`.
    pub source: &'static str,
    /// Where a repository keeps its copy.
    pub path: String,
    /// The first line a comment of the kind starts with; empty for the pull request template.
    pub first_line: String,
}

/// What `GET /v1/repositories/{id}/templates` says.
#[derive(Debug, Clone, Serialize)]
pub struct TemplateOverview {
    pub templates: Vec<TemplateInfo>,
    /// The headings of the pull request template, in order.
    pub sections: Vec<super::pull::Section>,
    /// What is wrong with a project's template, when it fell back to the default.
    pub problem: Option<String>,
}

impl App {
    /// The text of the project's template file `file` for `repository`, when there is one.
    fn project_template(&self, repository: &Repository, file: &str) -> Option<String> {
        let project = self.store.project(repository.home_project_id).ok()??;
        let text = self.project_file(&project, &format!("templates/{file}"))?;
        (!text.trim().is_empty()).then_some(text)
    }

    /// The pull request template of the repository, and what is wrong with the project's when that was not used.
    pub fn pull_template_checked(&self, repository: &Repository) -> (PullTemplate, Option<String>) {
        match self.project_template(repository, PULL_REQUEST_FILE) {
            None => (PullTemplate::standard(), None),
            Some(text) => match PullTemplate::read(&text) {
                Ok(template) => (template, None),
                Err(why) => (
                    PullTemplate::standard(),
                    Some(format!("templates/{PULL_REQUEST_FILE} is not used: {why}")),
                ),
            },
        }
    }

    /// The pull request template of the repository: the project's, else the one Moebius ships.
    pub fn pull_template(&self, repository: &Repository) -> PullTemplate {
        self.pull_template_checked(repository).0
    }

    /// The issue template of the repository: the project's `issue.md`, else the one Moebius ships.
    pub fn issue_template(&self, repository: &Repository) -> PullTemplate {
        self.project_template(repository, ISSUE_FILE)
            .and_then(|text| PullTemplate::read(&text).ok())
            .unwrap_or_else(|| {
                PullTemplate::read(DEFAULT_ISSUE_TEMPLATE)
                    .expect("the shipped template has sections")
            })
    }

    /// What the card's issue says, read by the issue template (`card.issue`); none for an issue made by a backlog sync,
    /// which has a format of its own.
    pub fn issue_fields(&self, card: &crate::store::Card) -> Option<super::pull::Pull> {
        let repository = self.repository(card.repository_id).ok()?;
        if self.store.backlog_item_of(card.id).ok()?.is_some() {
            return None;
        }
        Some(super::pull::parse_with(
            &card.body,
            &self.issue_template(&repository),
            false,
        ))
    }

    /// The text of the template of a comment kind: the project's, else the one Moebius ships.
    pub fn comment_template(&self, repository: &Repository, kind: Kind) -> String {
        self.project_template(repository, &format!("{}.md", kind.file()))
            .unwrap_or_else(|| kind.template())
    }

    /// The templates in effect for the repository.
    pub fn template_overview(&self, repository: &Repository) -> TemplateOverview {
        let (template, problem) = self.pull_template_checked(repository);
        let source = |file: &str| {
            if self.project_template(repository, file).is_some() {
                "project"
            } else {
                "default"
            }
        };
        let mut templates = vec![TemplateInfo {
            name: "pull_request".into(),
            source: source(PULL_REQUEST_FILE),
            path: PULL_REQUEST_PATH.into(),
            first_line: String::new(),
        }];
        templates.push(TemplateInfo {
            name: "issue".into(),
            source: source(ISSUE_FILE),
            path: ISSUE_FORM_PATH.into(),
            first_line: String::new(),
        });
        for kind in Kind::ALL {
            templates.push(TemplateInfo {
                name: kind.file().into(),
                source: source(&format!("{}.md", kind.file())),
                path: format!("{COMMENT_DIR}/{}.md", kind.file()),
                first_line: kind.first_line().into(),
            });
        }
        TemplateOverview {
            templates,
            sections: template.sections,
            problem,
        }
    }

    /// The files a repository keeps for the templates, as `(path, content)`.
    fn template_files(&self, repository: &Repository) -> Vec<(String, String)> {
        let mut files = vec![(
            PULL_REQUEST_PATH.to_string(),
            self.pull_template(repository).file(),
        )];
        files.push((
            ISSUE_FORM_PATH.to_string(),
            self.issue_template(repository).issue_form(),
        ));
        for kind in Kind::ALL {
            files.push((
                format!("{COMMENT_DIR}/{}.md", kind.file()),
                self.comment_template(repository, kind),
            ));
        }
        files
    }

    /// Opens a pull request that brings the repository's copies of the templates up to date (ADR 0034): the pull request
    /// template and the comment templates, as the project has them. Answers none when the default branch has them
    /// already. An open pull request of an earlier sync is moved on to the new files instead of another being opened.
    /// `.github/` is a locked path, so the pull request waits in the Inbox for the owner's click.
    pub fn sync_templates(
        &self,
        by: &Actor,
        repository_id: i64,
        reason: &str,
    ) -> Result<Option<Change>> {
        let repository = self.repository(repository_id)?;
        let clone = Path::new(&repository.path);
        let base = repository.base_ref();
        let wanted = self.template_files(&repository);
        let stale: Vec<&(String, String)> = wanted
            .iter()
            .filter(|(path, content)| {
                git(clone, &["show", &format!("{base}:{path}")])
                    .map_or(true, |have| have.trim_end() != content.trim_end())
            })
            .collect();
        if stale.is_empty() {
            return Ok(None);
        }
        let name = format!("templates-{repository_id}");
        let worktree = self
            .repos
            .add_detached_worktree(&repository.name, clone, &name, &base)?;
        let made = (|| -> Result<()> {
            git(&worktree, &["checkout", "--quiet", "-B", SYNC_BRANCH])?;
            for (path, content) in &wanted {
                let target = worktree.join(path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(AppError::from)?;
                }
                std::fs::write(&target, content).map_err(AppError::from)?;
            }
            commit_all(
                &worktree,
                "Keep the pull request and comment templates current",
            )?;
            Ok(())
        })();
        let _ = self.repos.remove_worktree(clone, &worktree);
        made?;
        let context = std::collections::BTreeMap::from([
            ("repository".to_string(), repository.name.clone()),
            (
                "files".to_string(),
                stale
                    .iter()
                    .map(|(path, _)| path.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ]);
        self.record_action(by, "sync_templates", None, None, context, reason)?;
        let list: String = stale
            .iter()
            .map(|(path, _)| format!("- `{path}`\n"))
            .collect();
        let body = format!(
            "Brings the templates of this repository up to date, as its project has them:\n\n{list}\n\
             The pull request template is what the forge shows to whoever opens a pull request; the files in \
             `{COMMENT_DIR}/` are the forms of the comments Moebius reads (`/done`, `## Review answer`, `## Measurement`, \
             `/take`, `/release`).\n\n`.github/` is a locked path, so this waits for the owner's click and is never merged \
             by the server.\n\nMade by Moebius.\n"
        );
        let note = format!(
            "`.github/` is a locked path of the repository, so this pull request waits for your click: Moebius never merges \
             it by itself. It keeps {} of the repository's template files current.",
            stale.len()
        );
        if let Some(open) = self
            .store
            .latest_change_on_branch(SYNC_BRANCH)?
            .filter(|c| c.state == "open" && c.repository_id == Some(repository.id))
        {
            let head = git(clone, &["rev-parse", SYNC_BRANCH])?;
            let forge = self.forge(&repository)?;
            forge.publish(
                &repository,
                "Keep the templates current",
                &body,
                SYNC_BRANCH,
                open.pull_request,
            )?;
            return Ok(Some(self.offer_again(
                open.id,
                &head,
                "Keep the templates current",
            )?));
        }
        let change = self.offer_branch_change_noted(
            &repository,
            SYNC_BRANCH,
            "Keep the templates current",
            &body,
            Some(&note),
        )?;
        Ok(Some(change))
    }
}
