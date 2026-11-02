//! A package's files from its source, into a directory: an npm tarball whose download is checked against the integrity
//! the registry gave, or a git repository at a commit. The production dependencies are then installed with
//! `npm install --ignore-scripts --omit=dev`, so that no script of the package or of a dependency runs on the server:
//! a dependency that is not from the registry (a git repository, a path, a tarball URL), which npm builds despite that
//! flag, is refused.
//! Also: whether a newer version of a locked package exists.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use semver::Version;
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};

use super::PackagesError;
use super::lock::Locked;
use super::manifest::package_json;
use super::registry::Registry;
use super::source::{Source, valid_package_name};
use crate::blobs::redact;
use crate::repos::{git, git_bytes};

/// A package fetched into a directory, and what to lock it as.
#[derive(Debug, Clone)]
pub struct Fetched {
    pub locked: Locked,
    /// The package's `package.json` as it was fetched, if it has one.
    pub manifest: Option<Value>,
}

/// A newer version of a locked package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Latest {
    /// The version, or for a git source the short commit.
    pub version: String,
    /// The tarball URL or the commit it would be locked as.
    pub resolved: String,
}

/// A directory below the server's data directory for work that is thrown away: removed when it is dropped.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(data_dir: &Path) -> Result<Self, PackagesError> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = data_dir.join("packages-work").join(format!(
            "{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        remove_all(&self.0);
    }
}

/// Removes a directory with everything in it, read-only files too (git's objects are, on Windows).
pub fn remove_all(dir: &Path) {
    if std::fs::remove_dir_all(dir).is_ok() {
        return;
    }
    fn writable(dir: &Path) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                let mut permissions = meta.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = std::fs::set_permissions(&path, permissions);
                if meta.is_dir() {
                    writable(&path);
                }
            }
        }
    }
    writable(dir);
    let _ = std::fs::remove_dir_all(dir);
}

/// Puts the package of `source` into `dir`, which is empty or does not exist yet, using `scratch` for what is thrown
/// away. For an npm package the tarball is downloaded and its integrity checked first; a download that does not match
/// leaves `dir` untouched. With `dependencies`, the production dependencies are installed too.
pub fn fetch(
    registry: &Registry,
    source: &Source,
    dir: &Path,
    scratch: &Scratch,
    dependencies: bool,
) -> Result<Fetched, PackagesError> {
    let fetched = match source {
        Source::Npm { name, .. } => fetch_npm(registry, source, name, dir)?,
        Source::Git { url, reference, .. } => {
            fetch_git(source, url, reference.as_deref(), dir, scratch)?
        }
    };
    if dependencies {
        install_dependencies(registry, dir)?;
    }
    Ok(fetched)
}

fn fetch_npm(
    registry: &Registry,
    source: &Source,
    name: &str,
    dir: &Path,
) -> Result<Fetched, PackagesError> {
    let resolved = registry.resolve(name, &source.wanted()?)?;
    let bytes = registry.download(&resolved.tarball)?;
    let sha512 = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&bytes));
    let actual = format!("sha512-{sha512}");
    if actual != resolved.integrity {
        return Err(PackagesError::Integrity {
            name: format!("{name}@{}", resolved.version),
            expected: resolved.integrity,
            actual,
        });
    }
    unpack(flate2::read::GzDecoder::new(&bytes[..]), dir, 1)?;
    let manifest = package_json(dir)?;
    if let Some(published) = manifest.as_ref().and_then(|m| m["name"].as_str())
        && published != name
    {
        return Err(PackagesError::Invalid(format!(
            "the tarball of {name} is the package {published}"
        )));
    }
    Ok(Fetched {
        locked: Locked {
            name: name.to_string(),
            source: source.text(),
            version: resolved.version,
            resolved: resolved.tarball,
            integrity: Some(resolved.integrity),
            sha256: Some(hex::encode(Sha256::digest(&bytes))),
        },
        manifest,
    })
}

fn fetch_git(
    source: &Source,
    url: &str,
    reference: Option<&str>,
    dir: &Path,
    scratch: &Scratch,
) -> Result<Fetched, PackagesError> {
    let clone = scratch.path().join("clone");
    git(scratch.path(), &["clone", "--quiet", "--", url, "clone"])?;
    let commit = match reference {
        None => git(&clone, &["rev-parse", "HEAD"])?,
        Some(reference) => [
            format!("origin/{reference}^{{commit}}"),
            format!("{reference}^{{commit}}"),
        ]
        .iter()
        .find_map(|rev| git(&clone, &["rev-parse", "--verify", "--quiet", rev]).ok())
        .ok_or_else(|| PackagesError::NotFound(format!("the ref {reference} of {url}")))?,
    };
    let archive = git_bytes(&clone, &["archive", "--format=tar", &commit])?;
    unpack(&archive[..], dir, 0)?;
    let manifest = package_json(dir)?;
    let named = manifest
        .as_ref()
        .and_then(|m| m["name"].as_str())
        .map(String::from);
    let name = named
        .filter(|name| valid_package_name(name))
        .or_else(|| {
            let last = url.trim_end_matches('/').rsplit(['/', ':']).next()?;
            Some(
                last.strip_suffix(".git")
                    .unwrap_or(last)
                    .to_ascii_lowercase(),
            )
            .filter(|name| valid_package_name(name))
        })
        .ok_or_else(|| PackagesError::Invalid(format!("{url} has no name for a package")))?;
    let version = manifest
        .as_ref()
        .and_then(|m| m["version"].as_str())
        .map_or_else(|| commit[..commit.len().min(7)].to_string(), String::from);
    Ok(Fetched {
        locked: Locked {
            name,
            source: source.text(),
            version,
            resolved: commit,
            integrity: None,
            sha256: None,
        },
        manifest,
    })
}

/// Unpacks a tar archive into `dir`, leaving out the first `strip` parts of every path. Only files and folders are
/// made: links are left out, and a path that leaves `dir` is an error.
fn unpack(reader: impl Read, dir: &Path, strip: usize) -> Result<(), PackagesError> {
    std::fs::create_dir_all(dir)?;
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut parts = vec![];
        for part in path.components().skip(strip) {
            match part {
                std::path::Component::Normal(part) => parts.push(part.to_owned()),
                std::path::Component::CurDir => {}
                _ => {
                    return Err(PackagesError::Invalid(format!(
                        "the archive has the path {}",
                        path.display()
                    )));
                }
            }
        }
        if parts.is_empty() {
            continue;
        }
        let target = parts
            .iter()
            .fold(dir.to_path_buf(), |to, part| to.join(part));
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if kind.is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(&target)?;
        }
    }
    Ok(())
}

/// The program that runs npm.
fn npm_program() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

/// Installs the production dependencies of the package at `dir`, if it has any, with `npm install --ignore-scripts
/// --omit=dev`: no install script of the package or of a dependency runs, and the peer dependencies Pi supplies are not
/// installed.
fn install_dependencies(registry: &Registry, dir: &Path) -> Result<(), PackagesError> {
    let manifest = package_json(dir)?;
    let listed = |field: &str| {
        manifest
            .as_ref()
            .and_then(|m| m[field].as_object())
            .cloned()
            .unwrap_or_default()
    };
    let (dependencies, optional) = (listed("dependencies"), listed("optionalDependencies"));
    if dependencies.is_empty() && optional.is_empty() {
        return Ok(());
    }
    for (name, spec) in dependencies.iter().chain(optional.iter()) {
        if !registry_spec(spec.as_str()) {
            return Err(PackagesError::Invalid(format!(
                "the dependency {name} is not from the registry ({spec}): only versions, ranges and tags are installed"
            )));
        }
    }
    // The package's own npm configuration would be read as the project's.
    for config in [".npmrc", ".yarnrc"] {
        let _ = std::fs::remove_file(dir.join(config));
    }
    let mut command = Command::new(npm_program());
    crate::harness::clear_environment(&mut command);
    let output = command
        .current_dir(dir)
        .args([
            "install",
            "--ignore-scripts",
            "--omit=dev",
            "--legacy-peer-deps",
            "--no-audit",
            "--no-fund",
            "--no-package-lock",
            "--no-bin-links",
            "--no-update-notifier",
            "--loglevel=error",
            // A dependency of a dependency that is a git repository fails instead of running its `prepare` script.
            "--git=moebius-no-git-here",
            "--registry",
        ])
        .arg(registry.base())
        .output()
        .map_err(|e| PackagesError::Npm(format!("cannot run npm: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let shown = crate::server::setup::without_credentials(&redact(stderr.trim()));
        let tail: String = shown
            .chars()
            .rev()
            .take(2000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(PackagesError::Npm(tail));
    }
    Ok(())
}

/// Whether a dependency spec is a registry version, range or tag: not a git repository, a path, a link, a tarball URL
/// or an alias, which npm fetches or builds in ways that run scripts despite `--ignore-scripts`.
fn registry_spec(spec: Option<&str>) -> bool {
    spec.is_some_and(|spec| {
        spec.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '.' | '-' | '_' | '+' | '^' | '~' | '<' | '>' | '=' | '*' | '|' | ' '
                )
        })
    })
}

/// A version of the locked package newer than the one locked that its source allows, if there is one.
pub fn latest(
    registry: &Registry,
    locked: &Locked,
    scratch: &Scratch,
) -> Result<Option<Latest>, PackagesError> {
    match Source::parse(&locked.source)? {
        source @ Source::Npm { .. } => {
            let Source::Npm { name, .. } = &source else {
                unreachable!()
            };
            let found = registry.resolve(name, &source.wanted()?)?;
            let newer = match (
                Version::parse(&found.version),
                Version::parse(&locked.version),
            ) {
                (Ok(found), Ok(locked)) => found > locked,
                _ => found.version != locked.version,
            };
            Ok(newer.then_some(Latest {
                version: found.version,
                resolved: found.tarball,
            }))
        }
        Source::Git { url, reference, .. } => {
            // A commit is the one thing a ref can name that never moves.
            let pinned = reference
                .as_deref()
                .is_some_and(|r| r.len() >= 7 && r.chars().all(|c| c.is_ascii_hexdigit()));
            if pinned {
                return Ok(None);
            }
            let answer = git(
                scratch.path(),
                &[
                    "ls-remote",
                    "--",
                    &url,
                    reference.as_deref().unwrap_or("HEAD"),
                ],
            )?;
            let lines: Vec<(&str, &str)> =
                answer.lines().filter_map(|l| l.split_once('\t')).collect();
            let commit = lines
                .iter()
                .find(|(_, name)| name.ends_with("^{}"))
                .or(lines.first())
                .map(|(sha, _)| sha.to_string())
                .ok_or_else(|| {
                    PackagesError::NotFound(format!(
                        "the ref {} of {url}",
                        reference.unwrap_or_default()
                    ))
                })?;
            Ok((commit != locked.resolved).then(|| Latest {
                version: commit[..commit.len().min(7)].to_string(),
                resolved: commit,
            }))
        }
    }
}

/// Moves the directory `from` to `to`, which does not exist yet: by renaming it, or, across file systems, by copying.
pub fn place(from: &Path, to: &Path) -> Result<(), PackagesError> {
    fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            let (source, target) = (entry.path(), to.join(entry.file_name()));
            let kind = entry.file_type()?;
            if kind.is_dir() {
                copy(&source, &target)?;
            } else if kind.is_file() {
                std::fs::copy(&source, &target)?;
            }
        }
        Ok(())
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_err() {
        copy(from, to)?;
        remove_all(from);
    }
    Ok(())
}
