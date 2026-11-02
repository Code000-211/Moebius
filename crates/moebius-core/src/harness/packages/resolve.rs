//! The names an agent gives a package's resources: `pkg:<name>` for all its extensions (or skills) and
//! `pkg:<name>/<path>` for one. They are resolved from `packages.lock` to the vendored files; a name that is not in the
//! lock, or a resource the package does not have, is an error of the agent, like a missing file.

use std::path::{Path, PathBuf};

use super::manifest::read_resources;
use super::{Lock, PACKAGES_DIR, valid_package_name};

/// What starts a name in an agent's `extensions` and `skills` that is a package's.
pub const PREFIX: &str = "pkg:";

/// What an agent names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Extension,
    Skill,
}

impl Kind {
    fn word(self) -> &'static str {
        match self {
            Kind::Extension => "extension",
            Kind::Skill => "skill",
        }
    }
}

/// The package and the path in it that `name` (without the prefix) stands for.
fn split(name: &str) -> (&str, Option<&str>) {
    let end = match name.strip_prefix('@') {
        Some(scoped) => scoped
            .find('/')
            .and_then(|scope| scoped[scope + 1..].find('/').map(|i| scope + 2 + i)),
        None => name.find('/'),
    };
    match end {
        Some(end) => (&name[..end], Some(&name[end + 1..])),
        None => (name, None),
    }
}

/// The package a `pkg:` name of an agent is about; none for any other name.
pub fn referenced_package(name: &str) -> Option<&str> {
    name.strip_prefix(PREFIX).map(|name| split(name).0)
}

/// The files `name` (such as `pkg:pi-web-access` or `pkg:pi-web-access/extensions/search.ts`) stands for in the
/// repository at `root`, as absolute paths: the extension files, or the folders of the skills. A name for the whole
/// package that finds none is an error, since the agent would not get what it names.
pub fn resolve(root: &Path, name: &str, kind: Kind) -> Result<Vec<PathBuf>, String> {
    let Some((package, path)) = name.strip_prefix(PREFIX).map(split) else {
        return Err(format!("{name:?} is not a package's"));
    };
    let word = kind.word();
    if !valid_package_name(package) {
        return Err(format!("{name:?} does not name a package"));
    }
    let lock = Lock::read(root).map_err(|e| e.to_string())?;
    if lock.get(package).is_none() {
        return Err(format!("the package {package:?} is not in packages.lock"));
    }
    let dir = root.join(PACKAGES_DIR).join(package);
    if !dir.is_dir() {
        return Err(format!(
            "the package {package:?} is in packages.lock but not in packages/"
        ));
    }
    let resources = read_resources(&dir).map_err(|e| format!("the package {package:?}: {e}"))?;
    let all = match kind {
        Kind::Extension => resources.extensions,
        Kind::Skill => resources.skills,
    };
    let chosen: Vec<String> = match path {
        None => all,
        Some(path) => {
            let path = path.trim_start_matches("./").trim_end_matches('/');
            if !all.iter().any(|resource| resource == path) {
                return Err(format!("the package {package:?} has no {word} {path:?}"));
            }
            vec![path.to_string()]
        }
    };
    if chosen.is_empty() {
        return Err(format!("the package {package:?} has no {word}s"));
    }
    Ok(chosen.iter().map(|resource| dir.join(resource)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::packages::Locked;

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for file in [
            "packages/tools/extensions/a.ts",
            "packages/tools/extensions/b.ts",
            "packages/tools/skills/review/SKILL.md",
            "packages/@acme/only-skills/skills/x/SKILL.md",
        ] {
            std::fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
            std::fs::write(root.join(file), "").unwrap();
        }
        let locked = |name: &str| Locked {
            name: name.into(),
            source: format!("npm:{name}"),
            version: "1.0.0".into(),
            resolved: String::new(),
            integrity: None,
            sha256: None,
        };
        let mut lock = Lock::default();
        lock.put(locked("tools"));
        lock.put(locked("@acme/only-skills"));
        lock.write(root).unwrap();
        dir
    }

    #[test]
    fn a_name_stands_for_a_whole_package_or_one_resource() {
        let dir = repository();
        let root = dir.path();
        let tools = root.join("packages/tools");
        let both = resolve(root, "pkg:tools", Kind::Extension).unwrap();
        assert_eq!(
            both,
            [tools.join("extensions/a.ts"), tools.join("extensions/b.ts")]
        );
        let one = resolve(root, "pkg:tools/extensions/b.ts", Kind::Extension).unwrap();
        assert_eq!(one, [tools.join("extensions/b.ts")]);
        assert_eq!(
            resolve(root, "pkg:tools", Kind::Skill).unwrap(),
            [tools.join("skills/review")]
        );
        let scoped = resolve(root, "pkg:@acme/only-skills/skills/x", Kind::Skill).unwrap();
        assert_eq!(scoped, [root.join("packages/@acme/only-skills/skills/x")]);
        assert_eq!(
            referenced_package("pkg:@acme/only-skills/skills/x"),
            Some("@acme/only-skills")
        );
        assert_eq!(referenced_package("pkg:tools"), Some("tools"));
        assert_eq!(referenced_package("extensions/a.ts"), None);
    }

    #[test]
    fn what_is_not_there_is_an_error_that_says_what() {
        let dir = repository();
        let root = dir.path();
        let error = |name: &str, kind| resolve(root, name, kind).unwrap_err();
        assert!(error("pkg:missing", Kind::Extension).contains("not in packages.lock"));
        assert!(error("pkg:tools/extensions/c.ts", Kind::Extension).contains("has no extension"));
        assert!(error("pkg:@acme/only-skills", Kind::Extension).contains("has no extensions"));
        assert!(error("pkg:../x", Kind::Extension).contains("does not name a package"));
        // A file of the package that its manifest does not make a resource is not one.
        assert!(error("pkg:tools/package.json", Kind::Extension).contains("has no extension"));
    }
}
