//! Adding a repository, by cloning it from a path, a URL or GitHub, editing whom a GitHub repository trusts, and whether
//! a card's issue is trusted to move on without a person's approval.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;

use super::{App, AppError, PERSON, Result};
use crate::bus::AppEvent;
use crate::decide::Actor;
use crate::store::{Card, Repository};

/// Whether `name` is made of letters, digits and `-_.` only.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

/// Whether `path` is a git repository itself: one with a working tree, or a bare one.
fn is_repository(path: &Path) -> bool {
    path.join(".git").exists() || (path.join("HEAD").is_file() && path.join("objects").is_dir())
}

impl App {
    /// Clones `url` and adds it as a repository without a forge. An absolute path that does not exist, or an empty
    /// folder, is first made a new repository; a folder with files that is not a repository is refused.
    pub fn add_repository(&self, name: &str, url: &str) -> Result<Repository> {
        let local = Path::new(url);
        if local.is_absolute() && !is_repository(local) {
            let empty = std::fs::read_dir(local)
                .map_or(!local.exists(), |mut entries| entries.next().is_none());
            if !empty {
                return Err(AppError::Invalid(format!(
                    "{url} has files but is not a git repository; run git init there, or give an empty folder"
                )));
            }
            self.check_new_name(name)?;
            self.repos.init(local)?;
        }
        let (path, branch) = self.clone_repository(name, url, None)?;
        let repository = self.store.add_repository(name, &path, "none", &branch)?;
        self.bus
            .publish(AppEvent::new("repository_added", json!(repository)));
        Ok(repository)
    }

    /// Clones `url` and adds it as a repository whose issues are polled from GitHub as `github` (`owner/repo`).
    /// Issues by the logins in `trusted` need no approval. With the GitHub App set up, the clone authenticates as
    /// its installation, so private repositories work too.
    pub fn add_github_repository(
        &self,
        name: &str,
        url: &str,
        github: &str,
        trusted: &[String],
    ) -> Result<Repository> {
        if !github
            .split_once('/')
            .is_some_and(|(owner, repo)| is_plain_name(owner) && is_plain_name(repo))
        {
            return Err(AppError::Invalid(format!(
                "{github:?} is not a GitHub repository as owner/repo"
            )));
        }
        let auth = match self.forges.get("github") {
            Some(forge) => forge.auth(url, github)?,
            None => None,
        };
        let (path, branch) = self.clone_repository(name, url, auth.as_deref())?;
        let repository = self
            .store
            .add_github_repository(name, &path, &branch, github, trusted)?;
        self.bus
            .publish(AppEvent::new("repository_added", json!(repository)));
        Ok(repository)
    }

    /// Clones `url` as the repository `name` and returns the clone's path and default branch.
    fn clone_repository(
        &self,
        name: &str,
        url: &str,
        auth: Option<&str>,
    ) -> Result<(String, String)> {
        self.check_new_name(name)?;
        let clone = self.repos.clone_or_fetch(name, url, auth)?;
        let branch = self.repos.default_branch(&clone)?;
        Ok((clone.to_string_lossy().into_owned(), branch))
    }

    /// Refuses `name` for a new repository when it is not a plain name or is taken.
    fn check_new_name(&self, name: &str) -> Result<()> {
        if !is_plain_name(name) || name.starts_with('.') {
            return Err(AppError::Invalid(format!(
                "{name:?} is not a repository name"
            )));
        }
        if self.store.repositories()?.iter().any(|r| r.name == name) {
            return Err(AppError::Invalid(format!(
                "the repository name {name:?} is taken"
            )));
        }
        Ok(())
    }

    /// Whether the issue behind `card` may move on without a person's approval: a person created it in the desktop
    /// app, or its author is one of the repository's trusted GitHub logins. Anything unknown is not trusted.
    pub fn is_trusted(&self, card: &Card) -> bool {
        let Ok(Some(repository)) = self.store.repository(card.repository_id) else {
            return false;
        };
        if repository.forge == "none" {
            return card.author == PERSON;
        }
        repository
            .trusted
            .iter()
            .any(|login| login.eq_ignore_ascii_case(&card.author))
    }

    /// A person, or the Moebius agent, replaces the GitHub logins whose issues on a GitHub repository need no
    /// approval. Each login is trimmed and kept once; the edit is recorded as an `edit_repository` decision and
    /// announced as `repository_updated`. A list that changes nothing does neither.
    pub fn edit_repository(
        &self,
        by: &Actor,
        id: i64,
        trusted: &[String],
        reason: &str,
    ) -> Result<Repository> {
        let repository = self.repository(id)?;
        if repository.forge != "github" {
            return Err(AppError::Invalid(format!(
                "{} is not on GitHub, so it trusts no GitHub logins",
                repository.name
            )));
        }
        let mut logins: Vec<String> = Vec::new();
        for login in trusted.iter().map(|login| login.trim()) {
            if !login
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.[]".contains(c))
                || login.is_empty()
            {
                return Err(AppError::Invalid(format!(
                    "{login:?} is not a GitHub login"
                )));
            }
            if !logins.iter().any(|known| known.eq_ignore_ascii_case(login)) {
                logins.push(login.to_string());
            }
        }
        if logins == repository.trusted {
            return Ok(repository);
        }
        let context = BTreeMap::from([
            ("repository".into(), repository.name.clone()),
            ("trusted".into(), logins.join(", ")),
            ("previous_trusted".into(), repository.trusted.join(", ")),
        ]);
        self.record_action(by, "edit_repository", None, None, context, reason)?;
        self.store.set_repository_trusted(id, &logins)?;
        let repository = self.repository(id)?;
        self.bus.publish(AppEvent::new(
            "repository_updated",
            json!(repository.clone()),
        ));
        Ok(repository)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::app::columns;
    use crate::repos::git;
    use crate::store::Store;
    use moebius_plugin_api::Host;
    use std::sync::Arc;

    #[test]
    fn a_path_that_is_not_a_repository_yet_becomes_one() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = App::new(
            store,
            Arc::new(Host::new(std::time::Duration::from_secs(1))),
            &dir.path().join("data"),
        );
        let path = |name: &str| dir.path().join(name).to_string_lossy().into_owned();

        // A path that does not exist, and an empty folder, are made repositories on `main` and cloned.
        let fresh = app.add_repository("fresh", &path("new/fresh")).unwrap();
        assert_eq!(fresh.default_branch, "main");
        let log = git(Path::new(&fresh.path), &["log", "--format=%s"]).unwrap();
        assert_eq!(log, "Start the repository");
        std::fs::create_dir(dir.path().join("empty")).unwrap();
        assert!(app.add_repository("empty", &path("empty")).is_ok());

        // A folder with files is left alone.
        std::fs::create_dir(dir.path().join("files")).unwrap();
        std::fs::write(dir.path().join("files/notes.txt"), "mine").unwrap();
        let refused = app.add_repository("files", &path("files")).unwrap_err();
        assert!(
            refused.to_string().contains("is not a git repository"),
            "{refused}"
        );
        assert!(!dir.path().join("files/.git").exists());

        // A taken name creates nothing.
        assert!(app.add_repository("fresh", &path("other")).is_err());
        assert!(!dir.path().join("other").exists());
    }

    #[test]
    fn who_is_trusted_depends_on_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let app = App::new(
            store.clone(),
            Arc::new(Host::new(std::time::Duration::from_secs(1))),
            dir.path(),
        );
        let local = store
            .add_repository("local", "/srv/local", "none", "main")
            .unwrap();
        let remote = store
            .add_github_repository(
                "remote",
                "/srv/remote",
                "main",
                "acme/remote",
                &["Leon".into()],
            )
            .unwrap();
        let card = |repository: &Repository, author| {
            store
                .add_card(repository.id, "Card", "", author, columns::BACKLOG)
                .unwrap()
        };
        assert!(app.is_trusted(&card(&local, PERSON)));
        assert!(!app.is_trusted(&card(&local, "leon")));
        assert!(app.is_trusted(&card(&remote, "leon")), "logins ignore case");
        assert!(!app.is_trusted(&card(&remote, "mallory")));
        assert!(
            !app.is_trusted(&card(&remote, PERSON)),
            "a GitHub user named like the desktop's person is an outsider"
        );
    }
}
