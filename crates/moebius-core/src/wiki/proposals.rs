//! An agent's edit of the wiki: a branch of the clone with one commit that waits in the Inbox (kind `wiki_proposal`),
//! as the Retro's proposals do. Nothing reaches the pages until a member merges it, which is a merge commit on the
//! clone's branch pushed to `origin`; dismissing it deletes the branch. A branch is never pushed before a merge.

use std::collections::BTreeMap;

use serde_json::json;

use super::edits::{check_project, message_or};
use super::{Author, Wiki, file_of, literal, project_key};
use crate::app::{App, AppError, Result, UNQUOTED};
use crate::bus::Event;
use crate::decide::{Actor, Question};
use crate::repos::{git, kept};
use crate::store::InboxItem;

/// The kind of the inbox item that carries an agent's edit of the wiki.
pub const WIKI_PROPOSAL: &str = "wiki_proposal";

/// The longest diff kept in an inbox item, in bytes.
const DIFF_LIMIT: usize = 40_000;

impl Wiki {
    /// The agent or person `by` proposes the page `path` with `text`: a branch off the clone's branch that holds the
    /// change as one commit, offered in the inbox. Nothing is pushed and no page changes until a person merges it.
    /// Text that changes nothing is refused.
    pub fn propose(
        &self,
        app: &App,
        by: &Actor,
        path: &str,
        text: &str,
        message: Option<&str>,
    ) -> Result<InboxItem> {
        let file = file_of(path)?;
        check_project(app, path)?;
        let author = proposer(app, by)?;
        let mut text = text.to_string();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let _turn = self.turn();
        // A wiki that cannot reach `origin` is still proposed to; the merge follows it again.
        let _ = self.follow(app);
        let base = kept::head(&self.path).ok_or_else(|| {
            AppError::Invalid("the wiki has no page yet: a person writes the first".into())
        })?;
        let target = self.inside(&file)?;
        let verb = if target.is_file() { "Update" } else { "Add" };
        let message = message_or(message, || format!("{verb} {path}"));
        let branch = self.free_branch(by)?;
        let name = branch.replace('/', "-");
        let worktree = app
            .repos
            .add_detached_worktree("wiki", &self.path, &name, &base)?;
        let committed = (|| -> Result<(String, String)> {
            let file_in = self.worktree_file(&worktree, &file)?;
            if let Some(parent) = file_in.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&file_in, &text)?;
            git(&worktree, &["add", "--all"])?;
            if git(&worktree, &["status", "--porcelain"])?.is_empty() {
                return Err(AppError::Invalid(format!(
                    "page {path} has this text already"
                )));
            }
            let who = format!("--author={}", author.git());
            git(&worktree, &["commit", "--quiet", &who, "-m", &message])?;
            let head = git(&worktree, &["rev-parse", "HEAD"])?;
            let diff = git(&worktree, &["diff", &base, &head, "--", &literal(&file)])?;
            Ok((head, diff))
        })();
        let _ = app.repos.remove_worktree(&self.path, &worktree);
        let (head, mut diff) = committed?;
        if diff.len() > DIFF_LIMIT {
            let mut cut = DIFF_LIMIT;
            while !diff.is_char_boundary(cut) {
                cut -= 1;
            }
            diff.truncate(cut);
            diff.push_str("\n… (cut)\n");
        }
        app.repos.point_branch(&self.path, &branch, &head)?;
        let item = app.store.add_inbox_item(
            WIKI_PROPOSAL,
            None,
            &format!("Wiki: {message}"),
            &json!({
                "branch": branch,
                "page": path,
                "message": message,
                "by": author.name,
                "commit": head,
                "diff": diff,
            }),
        );
        if item.is_err() {
            let _ = git(&self.path, &["branch", "-D", "--quiet", &branch]);
        }
        Ok(item?)
    }

    /// A person merges the proposal in `item`: the branch is merged into the clone's branch after the clone has
    /// followed `origin`, and pushed. A merge that conflicts and a push that fails leave the clone as it was. On
    /// success the item is resolved, the branch deleted and the decision recorded; the merge commit is given.
    pub fn merge_proposal(
        &self,
        app: &App,
        by: &Actor,
        item: &InboxItem,
        reason: &str,
    ) -> Result<String> {
        let branch = branch_of(item)?;
        if !by.is_person() {
            return Err(AppError::Forbidden(
                "only a person merges a proposal for the wiki".into(),
            ));
        }
        let merged = {
            let _turn = self.turn();
            self.follow(app)?;
            let before = kept::head(&self.path);
            let tip = git(
                &self.path,
                &["rev-parse", "--verify", &format!("{branch}^{{commit}}")],
            )
            .map_err(|_| AppError::NotFound(format!("the branch {branch} of the proposal")))?;
            let header = app.git_auth(&self.url)?;
            let title = item.title.trim_start_matches("Wiki: ");
            let default = self.repos.default_branch(&self.path)?;
            self.repos
                .merge(&self.path, &tip, &format!("Merge the proposal: {title}"))?;
            if let Err(e) = self
                .repos
                .push(&self.path, &default, header.as_deref(), false)
            {
                if let Some(before) = &before {
                    let _ = git(&self.path, &["reset", "--hard", "--quiet", before]);
                }
                return Err(AppError::Conflict(format!(
                    "the merge could not be pushed: {}",
                    crate::blobs::redact(&e.to_string())
                )));
            }
            let _ = git(&self.path, &["branch", "-D", "--quiet", &branch]);
            // The pages changed, so the links between them are made again before anyone is told.
            if let Err(e) = self.reindex(app) {
                app.bus.publish(Event::WikiError {
                    message: format!("cannot index the wiki's links: {e}"),
                });
            }
            kept::head(&self.path)
        };
        app.store.resolve_inbox_item(item.id)?;
        record(app, by, item, "merge_proposal", reason)?;
        let commit = merged.unwrap_or_default();
        app.bus.publish(Event::WikiChanged {
            commit: commit.clone(),
        });
        Ok(commit)
    }

    /// A person dismisses the proposal in `item`: its branch is deleted and the wiki is as it was.
    pub fn dismiss_proposal(
        &self,
        app: &App,
        by: &Actor,
        item: &InboxItem,
        reason: &str,
    ) -> Result<()> {
        let branch = branch_of(item)?;
        if !by.is_person() {
            return Err(AppError::Forbidden(
                "only a person dismisses a proposal for the wiki".into(),
            ));
        }
        {
            let _turn = self.turn();
            let _ = git(&self.path, &["branch", "-D", "--quiet", &branch]);
        }
        app.store.resolve_inbox_item(item.id)?;
        record(app, by, item, "dismiss", reason)
    }

    /// The first branch `proposal/<who>-<n>` the clone does not have. The turn is held.
    fn free_branch(&self, by: &Actor) -> Result<String> {
        let who = match by {
            Actor::Agent { run, .. } => format!("run-{run}"),
            Actor::Member { handle, .. } => handle.clone(),
            Actor::Script { .. } => "script".to_string(),
        };
        for n in 1.. {
            let branch = format!("proposal/{who}-{n}");
            let there = git(
                &self.path,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{branch}"),
                ],
            );
            if there.is_err() {
                return Ok(branch);
            }
        }
        unreachable!("a free branch name is found")
    }

    /// The file `file` inside `worktree`, refused when a folder above it is a link.
    fn worktree_file(&self, worktree: &std::path::Path, file: &str) -> Result<std::path::PathBuf> {
        let mut at = worktree.to_path_buf();
        for part in file.split('/') {
            at.push(part);
            if std::fs::symlink_metadata(&at).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err(AppError::Invalid(format!(
                    "{file} goes through a link, which the wiki does not follow"
                )));
            }
        }
        Ok(at)
    }
}

/// Who a proposal is by, as the commit's author: a person by name, an agent by its name.
fn proposer(app: &App, by: &Actor) -> Result<Author> {
    match by {
        Actor::Agent { name, .. } => Ok(Author {
            name: format!("{name} (agent)"),
            handle: "agent".into(),
        }),
        Actor::Member { .. } => Author::of(app, by),
        Actor::Script { .. } => Err(AppError::Forbidden(
            "a column script does not edit the wiki".into(),
        )),
    }
}

/// The branch of an open proposal's item; anything else is not one.
fn branch_of(item: &InboxItem) -> Result<String> {
    if item.kind != WIKI_PROPOSAL || item.resolved_at.is_some() {
        return Err(AppError::NotFound(format!(
            "an open proposal for the wiki {}",
            item.id
        )));
    }
    item.data["branch"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| AppError::Invalid(format!("proposal {} has no branch", item.id)))
}

/// What the decision about the proposal in `item` records of it. The decisions are read by everyone and exported, so
/// the title and diff of an edit of a private project's page are left out of them.
fn recorded(app: &App, item: &InboxItem) -> BTreeMap<String, String> {
    let private = item.data["page"]
        .as_str()
        .and_then(project_key)
        .is_some_and(|key| {
            app.store
                .project_by_key(key)
                .ok()
                .flatten()
                .is_some_and(|project| project.private)
        });
    let (title, diff) = if private {
        (UNQUOTED.to_string(), String::new())
    } else {
        (
            item.title.clone(),
            item.data["diff"].as_str().unwrap_or_default().to_string(),
        )
    };
    BTreeMap::from([
        (
            "branch".to_string(),
            item.data["branch"].as_str().unwrap_or_default().to_string(),
        ),
        ("proposal".to_string(), title),
        ("kind".to_string(), WIKI_PROPOSAL.to_string()),
        ("diff".to_string(), diff),
    ])
}

fn record(app: &App, by: &Actor, item: &InboxItem, choice: &str, reason: &str) -> Result<()> {
    app.decisions.record_by(
        by,
        &Question {
            kind: "merge_proposal".into(),
            context: recorded(app, item),
            options: vec!["merge_proposal".into(), "dismiss".into()],
            fail_closed: "dismiss".into(),
            run_id: None,
            card_id: None,
        },
        choice,
        reason,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::super::testing::{bare_origin, push_from_elsewhere};
    use super::*;
    use crate::store::Store;

    fn opened(dir: &Path) -> (App, Wiki, std::path::PathBuf) {
        let store = Arc::new(Store::in_memory(dir).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(5)));
        let app = App::new(store, host, &dir.join("data"));
        let origin = bare_origin(dir);
        push_from_elsewhere(dir, &origin, "Setup.md", "# Setup\n");
        let wiki = Wiki::open(&app, &origin.to_string_lossy(), &dir.join("data/wiki")).unwrap();
        (app, wiki, origin)
    }

    fn agent() -> Actor {
        Actor::Agent {
            name: "Moebius".into(),
            run: 7,
        }
    }

    fn person(app: &App) -> Actor {
        let lea = app
            .store
            .add_person("lea", "Lea Roth", crate::store::Role::Member)
            .unwrap();
        Actor::person(lea.id, "lea")
    }

    fn origin_log(origin: &Path) -> String {
        git(origin, &["log", "--format=%an|%s", "main"]).unwrap_or_default()
    }

    #[test]
    fn an_agents_edit_waits_on_a_branch_and_changes_nothing_until_merged() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        let before = wiki.head();
        let item = wiki
            .propose(&app, &agent(), "Runbook", "# Runbook\n\nRestart it.", None)
            .unwrap();
        assert_eq!(item.kind, WIKI_PROPOSAL);
        assert_eq!(item.data["branch"], "proposal/run-7-1");
        assert!(item.data["diff"].as_str().unwrap().contains("+Restart it."));
        assert_eq!(app.store.inbox().unwrap().len(), 1);

        // The pages, the clone's branch and `origin` are as they were.
        assert_eq!(wiki.head(), before);
        assert!(wiki.page("Runbook", None).is_err());
        assert_eq!(wiki.pages().unwrap().len(), 1);
        assert_eq!(origin_log(&origin).lines().count(), 1);
        assert!(git(&origin, &["rev-parse", "--verify", "proposal/run-7-1"]).is_err());

        // A second proposal of the run takes the next name.
        let second = wiki
            .propose(&app, &agent(), "Other", "Text", Some("Add Other\nmore"))
            .unwrap();
        assert_eq!(second.data["branch"], "proposal/run-7-2");
        assert_eq!(second.title, "Wiki: Add Other");

        let lea = person(&app);
        wiki.merge_proposal(&app, &lea, &item, "good").unwrap();
        assert_eq!(
            wiki.page("Runbook", None).unwrap().text,
            "# Runbook\n\nRestart it.\n"
        );
        assert!(origin_log(&origin).contains("Merge the proposal: Add Runbook"));
        assert!(origin_log(&origin).contains("Moebius (agent)|Add Runbook"));
        assert!(wiki.page("Other", None).is_err());
        assert_eq!(app.store.inbox().unwrap().len(), 1);
        assert!(git(&wiki.path, &["rev-parse", "--verify", "proposal/run-7-1"]).is_err());
    }

    #[test]
    fn the_decision_about_a_private_projects_page_records_none_of_its_text() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, _origin) = opened(dir.path());
        let project = app.store.add_project("Web", "WEB").unwrap();
        let open = |path: &str| {
            wiki.propose(&app, &agent(), path, "# Secret\n\nThe launch.", None)
                .unwrap()
        };
        let public = open("projects/WEB/Plans");
        assert!(recorded(&app, &public)["diff"].contains("+The launch."));
        app.store.set_project_access(project.id, true, &[]).unwrap();
        let private = open("projects/WEB/More");
        let seen = recorded(&app, &private);
        assert_eq!(seen["diff"], "");
        assert_eq!(seen["proposal"], UNQUOTED);
        assert!(!format!("{seen:?}").contains("launch"));
    }

    #[test]
    fn a_merged_proposal_makes_its_links_count() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, _origin) = opened(dir.path());
        let lea = person(&app);
        let item = wiki
            .propose(&app, &agent(), "Runbook", "See [[Setup]].", None)
            .unwrap();
        assert!(
            wiki.links(&app, &lea, "Setup")
                .unwrap()
                .backlinks
                .pages
                .is_empty()
        );
        wiki.merge_proposal(&app, &lea, &item, "good").unwrap();
        let backlinks = wiki.links(&app, &lea, "Setup").unwrap().backlinks.pages;
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].path, "Runbook");
    }

    #[test]
    fn a_dismissed_proposal_changes_nothing_and_leaves_the_inbox() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        let item = wiki
            .propose(&app, &agent(), "Setup", "# New\n", None)
            .unwrap();
        let lea = person(&app);
        wiki.dismiss_proposal(&app, &lea, &item, "no").unwrap();
        assert_eq!(wiki.page("Setup", None).unwrap().text, "# Setup\n");
        assert!(app.store.inbox().unwrap().is_empty());
        assert_eq!(origin_log(&origin).lines().count(), 1);
        assert!(
            git(
                &wiki.path,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/heads/{}", item.data["branch"].as_str().unwrap())
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn only_a_person_merges_and_a_resolved_proposal_cannot_be_merged_again() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, _origin) = opened(dir.path());
        let item = wiki.propose(&app, &agent(), "Runbook", "x", None).unwrap();
        assert!(matches!(
            wiki.merge_proposal(&app, &agent(), &item, ""),
            Err(AppError::Forbidden(_))
        ));
        assert!(wiki.page("Runbook", None).is_err());
        let lea = person(&app);
        wiki.merge_proposal(&app, &lea, &item, "").unwrap();
        let resolved = InboxItem {
            resolved_at: Some(1),
            ..item.clone()
        };
        assert!(wiki.merge_proposal(&app, &lea, &resolved, "").is_err());
    }

    #[test]
    fn a_proposal_that_changes_nothing_or_names_no_page_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, _origin) = opened(dir.path());
        assert!(
            wiki.propose(&app, &agent(), "Setup", "# Setup", None)
                .is_err()
        );
        assert!(wiki.propose(&app, &agent(), "../x", "x", None).is_err());
        assert!(
            wiki.propose(&app, &agent(), "projects/NOPE/x", "x", None)
                .is_err()
        );
        let script = Actor::Script {
            column: "ready".into(),
        };
        assert!(matches!(
            wiki.propose(&app, &script, "A", "x", None),
            Err(AppError::Forbidden(_))
        ));
        assert!(app.store.inbox().unwrap().is_empty());
    }

    // A hook that refuses pushes is a shell script, so this test is for Unix.
    #[cfg(unix)]
    #[test]
    fn a_merge_refused_by_origin_leaves_the_clone_and_the_item_as_they_were() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        let item = wiki.propose(&app, &agent(), "Runbook", "x", None).unwrap();
        let before = wiki.head();
        super::super::testing::refuse_pushes(&origin);
        let lea = person(&app);
        assert!(wiki.merge_proposal(&app, &lea, &item, "").is_err());
        assert_eq!(wiki.head(), before);
        assert!(wiki.page("Runbook", None).is_err());
        assert_eq!(app.store.inbox().unwrap().len(), 1);
    }

    #[test]
    fn a_merge_that_conflicts_with_a_later_push_leaves_the_clone_and_the_item_as_they_were() {
        let dir = tempfile::tempdir().unwrap();
        let (app, wiki, origin) = opened(dir.path());
        let item = wiki
            .propose(&app, &agent(), "Setup", "# Setup\n\nMine.\n", None)
            .unwrap();
        push_from_elsewhere(dir.path(), &origin, "Setup.md", "# Setup\n\nTheirs.\n");
        let lea = person(&app);
        let error = wiki.merge_proposal(&app, &lea, &item, "").unwrap_err();
        assert!(
            matches!(
                error,
                AppError::Git(crate::repos::GitError::Conflict { .. })
            ),
            "{error:?}"
        );
        assert_eq!(
            wiki.page("Setup", None).unwrap().text,
            "# Setup\n\nTheirs.\n"
        );
        assert_eq!(app.store.inbox().unwrap().len(), 1);
        assert!(
            git(&wiki.path, &["status", "--porcelain"])
                .unwrap()
                .is_empty()
        );
        assert!(
            git(
                &wiki.path,
                &["rev-parse", "--verify", "refs/heads/proposal/run-7-1"]
            )
            .is_ok()
        );
    }
}
