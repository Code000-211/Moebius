//! The package manager (`docs/reference/packages.md`): finds, installs, updates and removes what extends Moebius
//! itself, from registries, which are git repositories with an `index.toml`.
//!
//! A package is a `plugin` (a program speaking the process plugin protocol, installed to
//! `<data_dir>/packages/<name>/<version>/` after its sha256 is checked, with a `[[plugins]]` entry written through the
//! live settings and the plugin started), or a `workflow`, `rules` or `skills` package (files proposed as a change that a
//! person merges: to a repository's `.moebius/`, or to the harness configuration repository). `<data_dir>/packages.lock`
//! records what is installed. Nothing of a package runs while it is installed, and a download whose sha256 differs
//! from the index, or a package that does not work with the running Moebius, is refused before anything changes.
//!
//! This module holds what the parts share: the errors, the registries' catalog and the choice of a version, and the
//! answers of search, info and list. `version` and `manifest` are the formats, `registry` reads a registry, `lock`
//! keeps the lock, `plugin` installs, updates and removes programs, `propose` the files that are proposed as changes,
//! `download` fetches and checks what is installed, and `check` is the nightly look for newer versions.

mod check;
mod download;
mod lock;
mod manifest;
mod plugin;
mod propose;
mod registry;
mod version;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;

use crate::app::{App, AppError};
use crate::config::RegistryConfig;
use crate::decide::Actor;
use crate::harness::repo::HarnessRepo;
use crate::server::settings::Live;

pub use check::check_if_due;
pub use lock::{Lock, Locked};
pub use manifest::Package;
pub use plugin::current_target;
use version::Version;

/// Held for the whole of every operation that changes what is installed (install, update, remove and the nightly
/// check), so that two of them never read the lock and the configuration at the same time and write over each other.
static OPERATIONS: Mutex<()> = Mutex::new(());

/// Waits for the other operations that change what is installed, and holds the right to be the only one.
fn exclusive() -> MutexGuard<'static, ()> {
    OPERATIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Why a package operation was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("no package {0} in the registries")]
    NotFound(String),
    #[error("{0} is not installed")]
    NotInstalled(String),
    #[error("{0} is installed already; update it instead")]
    Installed(String),
    #[error("{name} {version} has no build for {target}")]
    NoBuild {
        name: String,
        version: String,
        target: String,
    },
    #[error("{name} has no version {version}")]
    NoVersion { name: String, version: String },
    #[error("{name} {version} works with Moebius {needs}, and this is Moebius {running}")]
    Incompatible {
        name: String,
        version: String,
        needs: String,
        running: String,
    },
    #[error("the download of {url} has the sha256 {actual}, not {expected}; nothing was installed")]
    ShaMismatch {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("cannot download {url}: {message}")]
    Download { url: String, message: String },
    #[error("the registry {registry}: {message}")]
    Registry { registry: String, message: String },
    #[error("{0}")]
    Index(String),
    #[error("{0}")]
    Lock(String),
    #[error("only a person can install, update or remove a plugin")]
    PersonOnly,
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    App(#[from] AppError),
    #[error("cannot read or write files: {0}")]
    Io(#[from] std::io::Error),
}

impl From<crate::repos::GitError> for PackageError {
    fn from(e: crate::repos::GitError) -> Self {
        Self::App(e.into())
    }
}

pub type Result<T> = std::result::Result<T, PackageError>;

/// What the package manager works on: the app, the live settings (a plugin's entry is written through them), the
/// harness configuration repository if there is one, and the Moebius version packages must work with.
pub struct Packages<'a> {
    pub app: &'a App,
    pub live: &'a Live,
    pub harness: Option<&'a HarnessRepo>,
    pub moebius: &'a str,
}

/// A package found by `search`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub name: String,
    pub kind: String,
    /// The newest version that works with this Moebius, or the newest of all when none does.
    pub version: String,
    pub description: String,
    pub registry: String,
    /// Whether `version` works with this Moebius.
    pub compatible: bool,
    /// The installed version, if any.
    pub installed: Option<String>,
}

/// A registry that could not be read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegistryProblem {
    pub registry: String,
    pub message: String,
}

/// The answer of `search`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Search {
    pub packages: Vec<Summary>,
    pub problems: Vec<RegistryProblem>,
}

/// One version of a package in `info`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VersionInfo {
    pub compatible: bool,
    #[serde(flatten)]
    pub package: Package,
}

/// The answer of `info`: the versions of the package in the first registry that has it, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Info {
    pub name: String,
    pub registry: String,
    pub installed: Option<Locked>,
    pub versions: Vec<VersionInfo>,
}

/// An installed package, with the newer version the nightly check found, if any.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Installed {
    #[serde(flatten)]
    pub locked: Locked,
    pub update_available: Option<String>,
}

/// What an install, update or removal did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outcome {
    pub name: String,
    pub version: String,
    pub kind: String,
    /// `installed`, `updated`, `removed` or `unchanged` (an update with nothing newer).
    pub action: &'static str,
    /// Whether the server must restart for the change to take effect, such as for a launcher.
    pub restart_needed: bool,
    /// The change that proposes the files of a workflow, rules or skills package.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<i64>,
    /// How the plugin runs after an install or update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<moebius_plugin_api::ProcessStatus>,
    /// After removing a plugin: the folder of its files, when it could not be deleted (a program that has not ended
    /// yet can keep it on Windows); everything else is removed already, and the folder can be deleted by hand.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leftover: Option<String>,
}

/// What `install` takes.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub name: String,
    /// An exact version; the newest that works with this Moebius without one.
    #[serde(default)]
    pub version: Option<String>,
    /// The registry to install from; the first that has the package without one.
    #[serde(default)]
    pub registry: Option<String>,
    /// The repository a workflow or rules package is proposed to, by name; the only repository without one.
    #[serde(default)]
    pub repository: Option<String>,
}

/// The packages of one registry.
pub(crate) struct Catalog {
    pub registry: String,
    pub packages: Vec<Package>,
}

/// The package of the registry to install, with the registry it is in.
pub(crate) struct Chosen {
    pub registry: String,
    pub package: Package,
}

impl Packages<'_> {
    pub(crate) fn running(&self) -> Version {
        Version::parse(self.moebius).unwrap_or(Version::parse("0").expect("0 is a version"))
    }

    pub(crate) fn data_dir(&self) -> std::path::PathBuf {
        self.live.get().data_dir.clone()
    }

    pub(crate) fn read_lock(&self) -> Result<Lock> {
        Lock::read(&self.data_dir())
    }

    /// The registries named by `only`, or all, read now: the catalogs, and why the others could not be read.
    pub(crate) fn catalogs(&self, only: Option<&str>) -> (Vec<Catalog>, Vec<RegistryProblem>) {
        let config = self.live.get();
        let registries: Vec<&RegistryConfig> = config
            .registries
            .iter()
            .filter(|r| only.is_none_or(|name| r.name == name))
            .collect();
        let (mut catalogs, mut problems) = (vec![], vec![]);
        for registry in registries {
            match registry::read(&config.data_dir, registry) {
                Ok(packages) => catalogs.push(Catalog {
                    registry: registry.name.clone(),
                    packages,
                }),
                Err(PackageError::Registry { registry, message }) => {
                    problems.push(RegistryProblem { registry, message })
                }
                Err(other) => problems.push(RegistryProblem {
                    registry: registry.name.clone(),
                    message: other.to_string(),
                }),
            }
        }
        (catalogs, problems)
    }

    /// Looks for a registry that is not configured at all when `only` names none.
    fn check_registry(&self, only: Option<&str>) -> Result<()> {
        match only {
            Some(name) if !self.live.get().registries.iter().any(|r| r.name == name) => Err(
                PackageError::Refused(format!("there is no registry {name}; see [[registries]]")),
            ),
            _ => Ok(()),
        }
    }

    /// The package `name` of the first registry (among `only`, or all) that has it, at `version` or the newest that
    /// works with this Moebius.
    pub(crate) fn choose(
        &self,
        name: &str,
        version: Option<&str>,
        only: Option<&str>,
    ) -> Result<Chosen> {
        self.check_registry(only)?;
        let (catalogs, problems) = self.catalogs(only);
        for catalog in &catalogs {
            let candidates: Vec<&Package> =
                catalog.packages.iter().filter(|p| p.name == name).collect();
            if !candidates.is_empty() {
                let package = pick(&candidates, version, self.running(), self.moebius)?;
                return Ok(Chosen {
                    registry: catalog.registry.clone(),
                    package: package.clone(),
                });
            }
        }
        match problems.into_iter().next() {
            Some(problem) => Err(PackageError::Registry {
                registry: problem.registry,
                message: problem.message,
            }),
            None => Err(PackageError::NotFound(name.to_string())),
        }
    }

    /// The packages whose name, description or kind contains `query`, one for each name and registry.
    pub fn search(&self, query: &str) -> Result<Search> {
        let lock = self.read_lock()?;
        let (catalogs, problems) = self.catalogs(None);
        let query = query.to_lowercase();
        let mut packages = vec![];
        for catalog in &catalogs {
            let mut names: Vec<&str> = catalog.packages.iter().map(|p| p.name.as_str()).collect();
            names.sort();
            names.dedup();
            for name in names {
                let versions: Vec<&Package> =
                    catalog.packages.iter().filter(|p| p.name == name).collect();
                let newest = newest(&versions);
                let best = latest_compatible(&versions, self.running()).unwrap_or(newest);
                let found = [&best.name, &best.description, &best.kind]
                    .iter()
                    .any(|field| field.to_lowercase().contains(&query));
                if found {
                    packages.push(Summary {
                        name: best.name.clone(),
                        kind: best.kind.clone(),
                        version: best.version.clone(),
                        description: best.description.clone(),
                        registry: catalog.registry.clone(),
                        compatible: best.works_with(self.running()),
                        installed: lock.get(name).map(|l| l.version.clone()),
                    });
                }
            }
        }
        Ok(Search { packages, problems })
    }

    /// Every version of the package `name` in the first registry that has it.
    pub fn info(&self, name: &str) -> Result<Info> {
        let lock = self.read_lock()?;
        let (catalogs, problems) = self.catalogs(None);
        for catalog in &catalogs {
            let mut versions: Vec<&Package> =
                catalog.packages.iter().filter(|p| p.name == name).collect();
            if versions.is_empty() {
                continue;
            }
            versions.sort_by_key(|p| std::cmp::Reverse(p.parsed_version()));
            return Ok(Info {
                name: name.to_string(),
                registry: catalog.registry.clone(),
                installed: lock.get(name).cloned(),
                versions: versions
                    .into_iter()
                    .map(|p| VersionInfo {
                        compatible: p.works_with(self.running()),
                        package: p.clone(),
                    })
                    .collect(),
            });
        }
        match problems.into_iter().next() {
            Some(problem) => Err(PackageError::Registry {
                registry: problem.registry,
                message: problem.message,
            }),
            None => Err(PackageError::NotFound(name.to_string())),
        }
    }

    /// What is installed, with the newer version the nightly check found for it, from the inbox.
    pub fn list(&self) -> Result<Vec<Installed>> {
        let lock = self.read_lock()?;
        let inbox = self.app.store.inbox().map_err(AppError::from)?;
        Ok(lock
            .packages
            .into_iter()
            .map(|locked| {
                let update_available = inbox
                    .iter()
                    .filter(|item| item.kind == check::INBOX_KIND)
                    .filter(|item| item.data["name"] == locked.name.as_str())
                    .filter_map(|item| item.data["version"].as_str())
                    .max_by_key(|v| Version::parse(v))
                    .map(str::to_string);
                Installed {
                    locked,
                    update_available,
                }
            })
            .collect())
    }

    /// Installs the package a person or the Moebius agent asks for. A plugin is for a person only.
    pub fn install(&self, by: &Actor, request: &Request) -> Result<Outcome> {
        let _only = exclusive();
        if self.read_lock()?.get(&request.name).is_some() {
            return Err(PackageError::Installed(request.name.clone()));
        }
        let chosen = self.choose(
            &request.name,
            request.version.as_deref(),
            request.registry.as_deref(),
        )?;
        if chosen.package.kind == "plugin" {
            if !matches!(by, Actor::Person) {
                return Err(PackageError::PersonOnly);
            }
            return self.install_plugin(by, &chosen);
        }
        self.propose(by, &chosen, request.repository.as_deref(), None)
    }

    /// Updates the package `name` to the newest version of its registry that works with this Moebius; nothing
    /// changes when it has that version already.
    pub fn update(&self, by: &Actor, name: &str) -> Result<Outcome> {
        let _only = exclusive();
        let lock = self.read_lock()?;
        let locked = lock
            .get(name)
            .ok_or_else(|| PackageError::NotInstalled(name.to_string()))?
            .clone();
        let current = Version::parse(&locked.version);
        let chosen = self.choose(name, None, Some(&locked.registry))?;
        if Some(chosen.package.parsed_version()) <= current {
            return Ok(Outcome {
                name: locked.name,
                version: locked.version,
                kind: locked.kind,
                action: "unchanged",
                restart_needed: false,
                change: locked.change,
                plugin: None,
                leftover: None,
            });
        }
        if locked.kind == "plugin" {
            self.update_plugin(by, &locked, &chosen)
        } else {
            let repository = locked.repository.clone();
            self.propose(by, &chosen, repository.as_deref(), Some(&locked))
        }
    }

    /// Removes the package `name`: a plugin's entry and files go at once, the files of the others are proposed to be
    /// deleted.
    pub fn remove(&self, by: &Actor, name: &str) -> Result<Outcome> {
        let _only = exclusive();
        let lock = self.read_lock()?;
        let locked = lock
            .get(name)
            .ok_or_else(|| PackageError::NotInstalled(name.to_string()))?
            .clone();
        if locked.kind == "plugin" {
            self.remove_plugin(by, &locked)
        } else {
            self.propose_removal(by, &locked)
        }
    }

    /// Records what a person did to a package and tells the event stream.
    pub(crate) fn record(&self, by: &Actor, action: &str, locked: &Locked) -> Result<()> {
        let context = std::collections::BTreeMap::from([
            ("package".to_string(), locked.name.clone()),
            ("version".to_string(), locked.version.clone()),
            ("kind".to_string(), locked.kind.clone()),
            ("registry".to_string(), locked.registry.clone()),
            ("url".to_string(), crate::blobs::redact(&locked.url)),
            ("sha256".to_string(), locked.sha256.clone()),
        ]);
        self.app
            .record_action(by, action, None, None, context, "")?;
        Ok(())
    }
}

/// The newest of `versions`.
fn newest<'p>(versions: &[&'p Package]) -> &'p Package {
    versions
        .iter()
        .copied()
        .max_by_key(|p| p.parsed_version())
        .expect("a package has a version")
}

/// The newest of `versions` that works with `running`.
pub(crate) fn latest_compatible<'p>(
    versions: &[&'p Package],
    running: Version,
) -> Option<&'p Package> {
    versions
        .iter()
        .copied()
        .filter(|p| p.works_with(running))
        .max_by_key(|p| p.parsed_version())
}

/// The version of `candidates` (all of one package) that `version` names, or the newest that works with `running`.
/// A version that does not work with `running` is refused with the range it needs.
fn pick<'p>(
    candidates: &[&'p Package],
    version: Option<&str>,
    running: Version,
    running_text: &str,
) -> Result<&'p Package> {
    let incompatible = |p: &Package| PackageError::Incompatible {
        name: p.name.clone(),
        version: p.version.clone(),
        needs: p.moebius.clone(),
        running: running_text.to_string(),
    };
    match version {
        Some(wanted) => {
            let found = Version::parse(wanted).and_then(|wanted| {
                candidates
                    .iter()
                    .copied()
                    .find(|p| p.parsed_version() == wanted)
            });
            let found = found.ok_or_else(|| PackageError::NoVersion {
                name: candidates[0].name.clone(),
                version: wanted.to_string(),
            })?;
            if found.works_with(running) {
                Ok(found)
            } else {
                Err(incompatible(found))
            }
        }
        None => {
            latest_compatible(candidates, running).ok_or_else(|| incompatible(newest(candidates)))
        }
    }
}

/// The directory where the files of installed plugins are, `<data_dir>/packages`.
pub(crate) fn packages_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("packages")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_that_change_what_is_installed_wait_for_each_other() {
        let first = exclusive();
        assert!(OPERATIONS.try_lock().is_err());
        drop(first);
        assert!(OPERATIONS.try_lock().is_ok());
    }
}
