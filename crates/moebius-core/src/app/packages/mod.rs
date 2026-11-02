//! Pi packages (issue 171) as the desktop API and the CLI offer them: searching the catalog, looking at a package,
//! listing what is installed, and installing, updating and removing, each made as a branch of the harness
//! configuration repository and offered as a change that a person merges (see `harness::packages`). Also, in `updates`, the
//! nightly check that puts a `extension_update` item in the inbox for each package that has a newer version.

mod updates;

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{App, AppError, Result};
use crate::harness::packages::{
    self, Locked, PACKAGES_DIR, Registry, Resources, Scratch, SearchHit, Source,
};
use crate::harness::repo::{PackageDiff, add_to_agent, remove_from_agents};
use crate::store::Change;

/// The most packages a search returns.
const SEARCH_LIMIT: usize = 40;

/// The inbox item of a package that has a newer version.
pub const UPDATE_ITEM: &str = "extension_update";

/// An installed package, as `GET /v1/extensions/packages` lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageView {
    pub name: String,
    pub source: String,
    pub version: String,
    pub resolved: String,
    pub integrity: Option<String>,
    pub sha256: Option<String>,
    pub resources: Resources,
    /// The scripts of its `package.json` that npm would run on an install, which Moebius never runs.
    pub install_scripts: Vec<String>,
    /// The agents that name one of its resources.
    pub agents: Vec<String>,
    /// Why its files cannot be read, if they cannot.
    pub error: Option<String>,
}

/// A package a source names, as `GET /v1/extensions/info` shows it before it is installed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    pub source: String,
    pub version: String,
    pub resolved: String,
    pub integrity: Option<String>,
    pub description: String,
    pub resources: Resources,
    /// The names of its production dependencies, which an install adds to the repository.
    pub dependencies: Vec<String>,
    pub install_scripts: Vec<String>,
    /// How many files it has, without its dependencies.
    pub files: usize,
    /// The version installed now, if the package is.
    pub installed: Option<String>,
}

/// A change that installs, updates or removes a package, offered and waiting for a person.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Offered {
    pub change: Change,
    pub package: String,
    /// `add`, `update` or `remove`.
    pub action: String,
    pub source: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub resolved: Option<String>,
    pub integrity: Option<String>,
    pub resources: Resources,
    /// The agents that name the package after the change (for an install with an agent) or lose its names (for a removal).
    pub agents: Vec<String>,
    /// The sections of the changelog between the two versions; empty for an install, a removal or a package without one.
    pub changelog: String,
    /// The package's files the change adds, changes and deletes, and the diff of them.
    pub files: Vec<String>,
    pub dependency_files: usize,
    pub diff: String,
}

/// What an update of one or all packages did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Updates {
    pub offered: Vec<Offered>,
    pub up_to_date: Vec<String>,
    /// The packages whose newer version could not be fetched, and why.
    pub failed: Vec<UpdateFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateFailure {
    pub package: String,
    pub error: String,
}

/// A package with a newer version, as the nightly check finds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Available {
    pub package: String,
    pub from: String,
    pub to: String,
}

impl App {
    fn registry(&self) -> Result<Registry> {
        Ok(Registry::new(&self.packages.npm_registry)?)
    }

    /// The packages of the catalog that match `text` and are of the type `kind` (`extension`, `skill`, `theme` or
    /// `prompt`) if given.
    pub fn search_packages(&self, text: &str, kind: Option<&str>) -> Result<Vec<SearchHit>> {
        Ok(self.registry()?.search(text, kind, SEARCH_LIMIT)?)
    }

    /// What the package `source` is, fetched into a throwaway directory without its dependencies.
    pub fn package_info(&self, source: &str) -> Result<PackageInfo> {
        let source = Source::parse(source)?;
        let scratch = Scratch::new(&self.data_dir)?;
        let stage = scratch.path().join("package");
        let fetched = packages::fetch(&self.registry()?, &source, &stage, &scratch, false)?;
        let manifest = fetched.manifest.clone().unwrap_or_default();
        let installed = match self.harness_repo.get() {
            Some(repo) => repo
                .package_lock()?
                .get(&fetched.locked.name)
                .map(|locked| locked.version.clone()),
            None => None,
        };
        Ok(PackageInfo {
            resources: packages::read_resources(&stage)?,
            dependencies: manifest["dependencies"]
                .as_object()
                .map(|d| d.keys().cloned().collect())
                .unwrap_or_default(),
            install_scripts: packages::install_scripts(&manifest),
            files: count_files(&stage),
            description: manifest["description"].as_str().unwrap_or_default().into(),
            name: fetched.locked.name,
            source: fetched.locked.source,
            version: fetched.locked.version,
            resolved: fetched.locked.resolved,
            integrity: fetched.locked.integrity,
            installed,
        })
    }

    /// The packages the harness configuration repository vendors, by name.
    pub fn packages(&self) -> Result<Vec<PackageView>> {
        let repo = self.harness_repo()?;
        let lock = repo.package_lock()?;
        Ok(lock
            .packages
            .into_iter()
            .map(|locked| {
                let dir = repo.path().join(PACKAGES_DIR).join(&locked.name);
                let (resources, scripts, error) = match packages::read_resources(&dir) {
                    _ if !dir.is_dir() => (
                        Default::default(),
                        vec![],
                        Some("packages/ has no directory for it".to_string()),
                    ),
                    Ok(resources) => {
                        let scripts = packages::package_json(&dir)
                            .ok()
                            .flatten()
                            .map(|json| packages::install_scripts(&json))
                            .unwrap_or_default();
                        (resources, scripts, None)
                    }
                    Err(e) => (Default::default(), vec![], Some(e.to_string())),
                };
                PackageView {
                    agents: repo.agents_naming(&locked.name),
                    name: locked.name,
                    source: locked.source,
                    version: locked.version,
                    resolved: locked.resolved,
                    integrity: locked.integrity,
                    sha256: locked.sha256,
                    resources,
                    install_scripts: scripts,
                    error,
                }
            })
            .collect())
    }

    /// Proposes installing the package `source`, and, with `agent`, naming it in that agent's extensions and skills: a
    /// branch of the harness configuration repository with the package in `packages/<name>/` and its entry in
    /// `packages.lock`, offered as a change that a person merges. Nothing runs from the package, and an npm download whose
    /// integrity does not match what the registry said is refused.
    pub fn add_package(&self, source: &str, agent: Option<&str>) -> Result<Offered> {
        let repo = self.harness_repo()?;
        if let Some(agent) = agent
            && !crate::config::valid_agent_name(agent)
        {
            return Err(AppError::Invalid(format!("{agent} is not an agent name")));
        }
        let source = Source::parse(source)?;
        let scratch = Scratch::new(&self.data_dir)?;
        let stage = scratch.path().join("package");
        let fetched = packages::fetch(&self.registry()?, &source, &stage, &scratch, true)?;
        let locked = fetched.locked;
        if let Some(installed) = repo.package_lock()?.get(&locked.name) {
            return Err(AppError::Exists(format!(
                "the package {} ({}); update it or remove it first",
                locked.name, installed.source
            )));
        }
        let resources = packages::read_resources(&stage)?;
        let uses = (
            !resources.extensions.is_empty(),
            !resources.skills.is_empty(),
        );
        if agent.is_some() && uses == (false, false) {
            return Err(AppError::Invalid(format!(
                "{} has no extensions or skills to give {}",
                locked.name,
                agent.unwrap_or_default()
            )));
        }
        let message = commit_message("Add", &locked, None, &resources, &fetched.manifest);
        let branch = repo.branch_with(
            self,
            &format!("add-{}", slug(&locked.name)),
            &message,
            |root| {
                packages::place(&stage, &root.join(PACKAGES_DIR).join(&locked.name))?;
                let mut lock = repo.package_lock_at(root)?;
                lock.put(locked.clone());
                lock.write(root)?;
                if let Some(agent) = agent {
                    add_to_agent(root, agent, &locked.name, uses)?;
                }
                Ok(())
            },
        )?;
        self.offer_package_branch(
            &branch,
            Offer {
                action: "add",
                package: &locked.name,
                source: locked.source.clone(),
                from: None,
                to: Some(locked.version.clone()),
                resolved: Some(locked.resolved.clone()),
                integrity: locked.integrity.clone(),
                resources,
                agents: agent.map(String::from).into_iter().collect(),
                changelog: String::new(),
            },
        )
    }

    /// Proposes updating the package `name`, or every package, to the newest version its source allows (the range that
    /// was chosen for npm, the newest commit of the ref for git). Each package with a newer version is a branch offered
    /// as a change that shows the version change, the changelog sections between the two versions and the file diff.
    pub fn update_packages(&self, name: Option<&str>) -> Result<Updates> {
        let repo = self.harness_repo()?;
        let lock = repo.package_lock()?;
        let targets: Vec<Locked> = match name {
            Some(name) => vec![
                lock.get(name)
                    .cloned()
                    .ok_or_else(|| AppError::NotFound(format!("package {name}")))?,
            ],
            None => lock.packages.clone(),
        };
        let registry = self.registry()?;
        let mut updates = Updates::default();
        for locked in targets {
            match self.update_package(repo, &registry, &locked) {
                Ok(Some(offered)) => updates.offered.push(offered),
                Ok(None) => updates.up_to_date.push(locked.name),
                // One package that cannot be fetched does not hold back the others; asked for alone, it is the error.
                Err(e) if name.is_none() => updates.failed.push(UpdateFailure {
                    package: locked.name,
                    error: e.to_string(),
                }),
                Err(e) => return Err(e),
            }
        }
        Ok(updates)
    }

    fn update_package(
        &self,
        repo: &crate::harness::repo::HarnessRepo,
        registry: &Registry,
        locked: &Locked,
    ) -> Result<Option<Offered>> {
        let scratch = Scratch::new(&self.data_dir)?;
        if packages::latest(registry, locked, &scratch)?.is_none() {
            self.resolve_update_items(&locked.name)?;
            return Ok(None);
        }
        let source = Source::parse(&locked.source)?;
        let stage = scratch.path().join("package");
        let fetched = packages::fetch(registry, &source, &stage, &scratch, true)?;
        let new = fetched.locked;
        if new.name != locked.name {
            return Err(AppError::Invalid(format!(
                "{} is now the package {}",
                locked.name, new.name
            )));
        }
        if new.resolved == locked.resolved {
            return Ok(None);
        }
        let resources = packages::read_resources(&stage)?;
        let changelog = match (
            semver::Version::parse(&locked.version),
            semver::Version::parse(&new.version),
            packages::read_changelog(&stage),
        ) {
            (Ok(from), Ok(to), Some(text)) => packages::sections_between(&text, &from, &to),
            _ => String::new(),
        };
        let message = commit_message(
            "Update",
            &new,
            Some(&locked.version),
            &resources,
            &fetched.manifest,
        );
        let branch = repo.branch_with(
            self,
            &format!("update-{}", slug(&new.name)),
            &message,
            |root| {
                let dir = root.join(PACKAGES_DIR).join(&new.name);
                packages::remove_all(&dir);
                packages::place(&stage, &dir)?;
                let mut lock = repo.package_lock_at(root)?;
                lock.put(new.clone());
                lock.write(root)?;
                Ok(())
            },
        )?;
        let offered = self.offer_package_branch(
            &branch,
            Offer {
                action: "update",
                package: &new.name,
                source: new.source.clone(),
                from: Some(locked.version.clone()),
                to: Some(new.version.clone()),
                resolved: Some(new.resolved.clone()),
                integrity: new.integrity.clone(),
                resources,
                agents: repo.agents_naming(&new.name),
                changelog,
            },
        )?;
        self.resolve_update_items(&new.name)?;
        Ok(Some(offered))
    }

    /// Proposes removing the package `name`: its directory and its entry of `packages.lock`, and its names out of the
    /// agents that have them, offered as a change that a person merges.
    pub fn remove_package(&self, name: &str) -> Result<Offered> {
        let repo = self.harness_repo()?;
        let locked = repo
            .package_lock()?
            .get(name)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("package {name}")))?;
        let resources = packages::read_resources(&repo.path().join(PACKAGES_DIR).join(name))
            .unwrap_or_default();
        let message = format!(
            "Remove the package {name}\n\nSource: {}\nVersion: {}\n",
            locked.source, locked.version
        );
        let mut agents = vec![];
        let branch =
            repo.branch_with(self, &format!("remove-{}", slug(name)), &message, |root| {
                let dir = root.join(PACKAGES_DIR).join(name);
                packages::remove_all(&dir);
                if let Some(scope) = dir.parent().filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with('@'))
                }) {
                    // The folder of a scope goes with its last package.
                    let _ = std::fs::remove_dir(scope);
                }
                agents = remove_from_agents(root, name)?;
                let mut lock = repo.package_lock_at(root)?;
                lock.remove(name);
                lock.write(root)?;
                Ok(())
            })?;
        let offered = self.offer_package_branch(
            &branch,
            Offer {
                action: "remove",
                package: name,
                source: locked.source,
                from: Some(locked.version),
                to: None,
                resolved: None,
                integrity: None,
                resources,
                agents,
                changelog: String::new(),
            },
        )?;
        self.resolve_update_items(name)?;
        Ok(offered)
    }

    /// Offers the branch of a package change, and describes it; a branch that is not fit is deleted.
    fn offer_package_branch(&self, branch: &str, offer: Offer) -> Result<Offered> {
        let repo = self.harness_repo()?;
        let title = match offer.action {
            "add" => format!("Add the package {}", offer.package),
            "update" => format!(
                "Update the package {} to {}",
                offer.package,
                offer.to.as_deref().unwrap_or_default()
            ),
            _ => format!("Remove the package {}", offer.package),
        };
        let change = match self.offer_harness_branch(branch, &title) {
            Ok(change) => change,
            Err(e) => {
                repo.delete_branch(branch);
                return Err(e);
            }
        };
        let PackageDiff {
            files,
            dependency_files,
            diff,
        } = repo.package_diff(branch, offer.package)?;
        Ok(Offered {
            change,
            package: offer.package.to_string(),
            action: offer.action.to_string(),
            source: offer.source,
            from: offer.from,
            to: offer.to,
            resolved: offer.resolved,
            integrity: offer.integrity,
            resources: offer.resources,
            agents: offer.agents,
            changelog: offer.changelog,
            files,
            dependency_files,
            diff,
        })
    }

    /// Whether the open change `change_id` of the harness configuration repository installs, updates or removes a
    /// package: such a change is merged by a person only.
    pub fn change_touches_packages(&self, change_id: i64) -> Result<bool> {
        let change = self.change(change_id)?;
        if change.repository_id.is_some() || change.state != "open" {
            return Ok(false);
        }
        self.harness_repo()?.changes_packages(&change.branch)
    }
}

/// What is told of a change that was offered.
struct Offer<'a> {
    action: &'static str,
    package: &'a str,
    source: String,
    from: Option<String>,
    to: Option<String>,
    resolved: Option<String>,
    integrity: Option<String>,
    resources: Resources,
    agents: Vec<String>,
    changelog: String,
}

/// A name usable in a branch name.
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// The commit message of an install or update: what a reviewer needs to see, the source, version, download and
/// integrity, and what the package gives.
fn commit_message(
    verb: &str,
    locked: &Locked,
    from: Option<&str>,
    resources: &Resources,
    manifest: &Option<serde_json::Value>,
) -> String {
    let mut text = format!("{verb} the package {} {}\n\n", locked.name, locked.version);
    text.push_str(&format!("Source: {}\n", locked.source));
    match from {
        Some(from) => text.push_str(&format!("Version: {from} -> {}\n", locked.version)),
        None => text.push_str(&format!("Version: {}\n", locked.version)),
    }
    text.push_str(&format!("Resolved: {}\n", locked.resolved));
    if let Some(integrity) = &locked.integrity {
        text.push_str(&format!("Integrity: {integrity}\n"));
    }
    for (label, list) in [
        ("Extensions", &resources.extensions),
        ("Skills", &resources.skills),
        ("Prompt templates (not loaded)", &resources.prompts),
        ("Themes (not loaded)", &resources.themes),
    ] {
        if !list.is_empty() {
            text.push_str(&format!("{label}: {}\n", list.join(", ")));
        }
    }
    let scripts = manifest
        .as_ref()
        .map(packages::install_scripts)
        .unwrap_or_default();
    if !scripts.is_empty() {
        text.push_str(&format!(
            "Install scripts (not run): {}\n",
            scripts.join(", ")
        ));
    }
    text
}

fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => count_files(&entry.path()),
            Ok(kind) if kind.is_file() => 1,
            _ => 0,
        })
        .sum()
}
