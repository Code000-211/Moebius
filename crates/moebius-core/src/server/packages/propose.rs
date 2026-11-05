//! Workflow, rules and skills packages: nothing is installed at once, the files are proposed as a change that a person
//! merges, always as a change of the harness configuration repository. Workflows go to a project's settings,
//! `repos/<repository name>/` (and rules are added to its `rules.md`), skills to `skills/`. An update proposes the new
//! files and the deletion of those the new version no longer has; a removal proposes deleting what the lock says was
//! proposed.

use std::path::Path;

use super::lock::LockedFile;
use super::manifest::PackageFile;
use super::{Chosen, Locked, Outcome, PackageError, Packages, Result, download};
use crate::app::AppError;
use crate::bus::Event;
use crate::decide::{Actor, rules};
use crate::store::{Change, Repository};

/// Where the files of a package go in the harness configuration repository: skills at its root, the others in the
/// settings of a project.
struct Target {
    project: Option<Repository>,
}

impl Target {
    /// Where `to` of a package file lands, from the root of the harness configuration repository.
    fn path(&self, to: &str) -> String {
        match &self.project {
            Some(project) => format!("repos/{}/{to}", project.name),
            None => format!("skills/{to}"),
        }
    }
}

impl Packages<'_> {
    /// The repository a workflow or rules package goes to: the one named, or the only one.
    fn repository_for(&self, name: Option<&str>) -> Result<Repository> {
        let mut repositories = self.app.store.repositories().map_err(AppError::from)?;
        match name {
            Some(name) => repositories
                .into_iter()
                .find(|r| r.name == name)
                .ok_or_else(|| PackageError::Refused(format!("there is no repository {name}"))),
            None if repositories.len() == 1 => Ok(repositories.remove(0)),
            None => Err(PackageError::Refused(
                "name the repository the files are proposed to (--repository)".into(),
            )),
        }
    }

    fn target(&self, kind: &str, repository: Option<&str>) -> Result<Target> {
        if self.harness.is_none() {
            return Err(PackageError::Refused(
                "the files of a package go to the harness configuration repository, and this server has none"
                    .into(),
            ));
        }
        let project = match kind {
            "skills" => None,
            _ => Some(self.repository_for(repository)?),
        };
        Ok(Target { project })
    }

    /// Offers `write`, a change to a worktree of the harness configuration repository's default branch, as the change
    /// `title`.
    fn offer(
        &self,
        branch: &str,
        title: &str,
        body: &str,
        write: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<Change> {
        let message = format!("{title}\n\n{body}");
        let harness = self.harness.expect("a target needs a harness repository");
        Ok(
            harness.offer_files(self.app, branch, message.trim(), |worktree| {
                write(worktree).map_err(into_app)
            })?,
        )
    }

    /// Proposes the files of `chosen`, replacing those of `previous` when it is an update.
    pub(super) fn propose(
        &self,
        by: &Actor,
        chosen: &Chosen,
        repository: Option<&str>,
        previous: Option<&Locked>,
    ) -> Result<Outcome> {
        let package = &chosen.package;
        let target = self.target(&package.kind, repository)?;
        let mut files = Vec::new();
        for file in &package.files {
            let bytes = download::fetch(&file.url, &file.sha256)?;
            let text = String::from_utf8(bytes).map_err(|_| {
                PackageError::Refused(format!(
                    "{} is not text, and only text is proposed",
                    file.to
                ))
            })?;
            files.push((file, text));
        }
        let mut names = Vec::new();
        let write = |worktree: &Path| -> Result<()> {
            if package.kind == "rules" {
                let file = worktree.join(target.path("rules.md"));
                let current = std::fs::read_to_string(&file).unwrap_or_default();
                let old: &[String] = previous.map_or(&[], |p| &p.rules);
                let (text, added) = merge_rules(&current, old, &files[0].1)?;
                names = added;
                return write_file(&file, &text);
            }
            for (file, text) in &files {
                write_file(&worktree.join(target.path(&file.to)), text)?;
            }
            for old in previous.iter().flat_map(|p| &p.files) {
                if !package.files.iter().any(|f| f.to == old.to) {
                    remove_file(&worktree.join(target.path(&old.to)))?;
                }
            }
            Ok(())
        };
        let verb = if previous.is_some() { "Update" } else { "Add" };
        let title = format!("{verb} the package {} {}", package.name, package.version);
        let listing: Vec<String> = package
            .files
            .iter()
            .map(|f| format!("- {}", target.path(&f.to)))
            .collect();
        let body = format!(
            "{} {} ({}) from the registry {}:\n\n{}",
            package.name,
            package.version,
            package.kind,
            chosen.registry,
            listing.join("\n")
        );
        let branch = format!("moebius/package-{}", package.name);
        let change = self.offer(&branch, &title, &body, write)?;
        let first = package
            .files
            .first()
            .map(|f| (f.url.clone(), f.sha256.to_lowercase()));
        let (url, sha256) = first.unwrap_or_default();
        let locked = Locked {
            name: package.name.clone(),
            version: package.version.clone(),
            kind: package.kind.clone(),
            registry: chosen.registry.clone(),
            url,
            sha256,
            program: None,
            repository: target.project.as_ref().map(|r| r.name.clone()),
            change: Some(change.id),
            files: package.files.iter().map(locked_file).collect(),
            rules: names,
        };
        let done = if previous.is_some() {
            "updated"
        } else {
            "installed"
        };
        let action = if previous.is_some() {
            "update_package"
        } else {
            "install_package"
        };
        self.finish(by, action, locked, done, false)
    }

    /// Proposes deleting what the lock says was proposed for `locked`.
    pub(super) fn propose_removal(&self, by: &Actor, locked: &Locked) -> Result<Outcome> {
        let target = self.target(&locked.kind, locked.repository.as_deref())?;
        let write = |worktree: &Path| -> Result<()> {
            if locked.kind == "rules" {
                let file = worktree.join(target.path("rules.md"));
                let current = std::fs::read_to_string(&file).unwrap_or_default();
                return write_file(&file, &without_rules(&current, &locked.rules));
            }
            for old in &locked.files {
                remove_file(&worktree.join(target.path(&old.to)))?;
            }
            Ok(())
        };
        let title = format!("Remove the package {} {}", locked.name, locked.version);
        let body = format!(
            "Removes what the package {} {} added.",
            locked.name, locked.version
        );
        let branch = format!("moebius/package-{}", locked.name);
        let change = self.offer(&branch, &title, &body, write)?;
        let mut lock = self.read_lock()?;
        lock.remove(&locked.name);
        lock.write(&self.data_dir())?;
        self.record(by, "remove_package", locked)?;
        self.app.bus.publish(Event::PackageRemoved {
            name: locked.name.clone(),
            kind: locked.kind.clone(),
        });
        self.resolve_updates(&locked.name)?;
        Ok(Outcome {
            name: locked.name.clone(),
            version: locked.version.clone(),
            kind: locked.kind.clone(),
            action: "removed",
            restart_needed: false,
            change: Some(change.id),
            plugin: None,
            leftover: None,
        })
    }
}

fn into_app(e: PackageError) -> AppError {
    match e {
        PackageError::App(e) => e,
        other => AppError::Invalid(other.to_string()),
    }
}

fn locked_file(file: &PackageFile) -> LockedFile {
    LockedFile {
        to: file.to.clone(),
        url: file.url.clone(),
        sha256: file.sha256.to_lowercase(),
    }
}

fn write_file(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(std::fs::write(path, text)?)
}

fn remove_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// The name of the rule a line starts, if it starts one.
fn rule_name(line: &str) -> Option<&str> {
    line.strip_prefix("## ").map(str::trim)
}

/// The names of the rules in `text`.
fn rule_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(rule_name)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// `text` without the rules named in `names`, each with its lines up to the next rule.
fn without_rules(text: &str, names: &[String]) -> String {
    let mut kept = String::new();
    let mut skipping = false;
    for line in text.lines() {
        if let Some(name) = rule_name(line) {
            skipping = names.iter().any(|n| n == name);
        }
        if !skipping {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    let kept = kept.trim_end();
    if kept.is_empty() {
        String::new()
    } else {
        format!("{kept}\n")
    }
}

/// `current` without the rules `old` added, with the rules of `fragment` added at the end, and the names of those. A
/// rule of the fragment named like one that stays is refused, since the first of a name would silently win.
fn merge_rules(current: &str, old: &[String], fragment: &str) -> Result<(String, Vec<String>)> {
    let names = rule_names(fragment);
    if names.is_empty() {
        return Err(PackageError::Refused(
            "the rules file of the package has no rule (a line \"## name\")".into(),
        ));
    }
    let kept = without_rules(current, old);
    if let Some(name) = names.iter().find(|n| rule_names(&kept).contains(n)) {
        return Err(PackageError::Refused(format!(
            "a rule named {name} is in the project's rules.md already"
        )));
    }
    // What comes before the first rule of the fragment is its introduction, which is not added.
    let start = fragment
        .lines()
        .position(|line| rule_name(line).is_some())
        .unwrap_or(0);
    let rules: Vec<&str> = fragment.lines().skip(start).collect();
    let mut merged = kept.trim_end().to_string();
    if !merged.is_empty() {
        merged.push_str("\n\n");
    }
    merged.push_str(rules.join("\n").trim_end());
    merged.push('\n');
    rules::check(&merged).map_err(PackageError::Refused)?;
    Ok((merged, names))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_added_replaced_and_removed_by_name_and_never_shadow_another() {
        let current = "Our rules.\n\n## no-ci\nLeave CI alone.\n";
        let fragment =
            "Intro.\n\n## no-secrets\nNever print secrets.\n\n## small-diffs\nKeep diffs small.\n";
        let (merged, names) = merge_rules(current, &[], fragment).unwrap();
        assert_eq!(names, ["no-secrets", "small-diffs"]);
        assert_eq!(
            merged,
            "Our rules.\n\n## no-ci\nLeave CI alone.\n\n## no-secrets\nNever print secrets.\n\n## small-diffs\nKeep diffs small.\n"
        );
        // An update replaces the rules the package added before, whatever they were called.
        let newer = "## no-secrets\nNever print or log secrets.\n";
        let (updated, names) =
            merge_rules(&merged, &["no-secrets".into(), "small-diffs".into()], newer).unwrap();
        assert_eq!(names, ["no-secrets"]);
        assert_eq!(
            updated,
            "Our rules.\n\n## no-ci\nLeave CI alone.\n\n## no-secrets\nNever print or log secrets.\n"
        );
        // Removing takes only the named rules.
        assert_eq!(
            without_rules(&merged, &["no-secrets".into(), "small-diffs".into()]),
            "Our rules.\n\n## no-ci\nLeave CI alone.\n"
        );
        assert_eq!(without_rules("## a\nx\n", &["a".into()]), "");
        // A name that is there already is not shadowed.
        let error = merge_rules(current, &[], "## no-ci\nOther.\n").unwrap_err();
        assert!(error.to_string().contains("no-ci"), "{error}");
        assert!(merge_rules(current, &[], "only prose\n").is_err());
    }
}
