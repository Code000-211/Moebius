//! Reading a registry: its git repository is cloned (or fetched) to `<data_dir>/registries/<name>` and its `index.toml`
//! parsed and checked as a whole. An index that does not pass is refused as a whole, since one entry that cannot be
//! trusted is a reason not to trust the rest.

use std::path::Path;
use std::sync::Mutex;

use super::PackageError;
use super::manifest::{INDEX_FORMAT, Index, Package};
use crate::blobs::redact;
use crate::config::RegistryConfig;
use crate::repos::git;

/// Held while a clone is fetched, since two git commands in one clone fail on its index lock.
static READING: Mutex<()> = Mutex::new(());

/// The packages of `registry`, fetched now.
pub fn read(data_dir: &Path, registry: &RegistryConfig) -> Result<Vec<Package>, PackageError> {
    let _only = READING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let failed = |why: String| PackageError::Registry {
        registry: registry.name.clone(),
        message: redact(&why),
    };
    let parent = data_dir.join("registries");
    let clone = parent.join(&registry.name);
    std::fs::create_dir_all(&parent)?;
    if clone.join(".git").exists() {
        git(
            &clone,
            &["fetch", "--quiet", "--depth", "1", "origin", "HEAD"],
        )
        .and_then(|_| git(&clone, &["reset", "--quiet", "--hard", "FETCH_HEAD"]))
        .map_err(|e| failed(e.to_string()))?;
    } else {
        if clone.exists() {
            std::fs::remove_dir_all(&clone)?;
        }
        git(
            &parent,
            &[
                "clone",
                "--quiet",
                "--depth",
                "1",
                "--",
                &registry.url,
                &registry.name,
            ],
        )
        .map_err(|e| failed(e.to_string()))?;
    }
    let text = std::fs::read_to_string(clone.join("index.toml"))
        .map_err(|e| failed(format!("cannot read index.toml: {e}")))?;
    parse(&text).map_err(|e| failed(e.to_string()))
}

/// The packages of the text of an `index.toml`.
pub fn parse(text: &str) -> Result<Vec<Package>, PackageError> {
    let index: Index = toml::from_str(text)
        .map_err(|e| PackageError::Index(format!("index.toml is not valid: {e}")))?;
    if index.format != INDEX_FORMAT {
        return Err(PackageError::Index(format!(
            "index.toml has format {}, and this Moebius reads format {INDEX_FORMAT}",
            index.format
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for package in &index.package {
        package.check()?;
        if !seen.insert((package.name.clone(), package.parsed_version())) {
            return Err(PackageError::Index(format!(
                "{} {} is in the index twice",
                package.name, package.version
            )));
        }
    }
    Ok(index.package)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const PLUGIN: &str = "[[package]]\nname = \"gate\"\nversion = \"1.0.0\"\nkind = \"plugin\"\n\
        moebius = \">=0.1\"\n[package.plugin]\nkind = \"decider\"\nprogram = \"gate\"\ndecides = [\"start_card\"]\n\
        [[package.downloads]]\ntarget = \"x86_64-unknown-linux-gnu\"\nurl = \"https://example.com/gate\"\n\
        sha256 = \"abababababababababababababababababababababababababababababababab\"\n";

    #[test]
    fn an_index_is_read_whole_or_refused_whole() {
        let packages = parse(&format!("format = 1\n{PLUGIN}")).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].downloads[0].target, "x86_64-unknown-linux-gnu");
        assert!(parse("format = 1\n").unwrap().is_empty());
        let error = |text: String| parse(&text).unwrap_err().to_string();
        assert!(error(format!("format = 2\n{PLUGIN}")).contains("format 2"));
        assert!(error("format = 1\n[[package]]\nname = 3\n".into()).contains("not valid"));
        assert!(error(format!("format = 1\n{PLUGIN}{PLUGIN}")).contains("twice"));
        let broken = PLUGIN.replace("sha256 = \"abab", "sha256 = \"zz");
        assert!(error(format!("format = 1\n{PLUGIN}{broken}")).contains("sha256"));
    }

    #[test]
    fn a_registry_is_cloned_then_fetched_and_one_without_an_index_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        let registry = RegistryConfig {
            name: "mine".into(),
            url: origin.display().to_string(),
        };
        std::fs::write(origin.join("README.md"), "no index\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let data = dir.path().join("data");
        let error = read(&data, &registry).unwrap_err().to_string();
        assert!(
            error.contains("mine") && error.contains("index.toml"),
            "{error}"
        );

        std::fs::write(origin.join("index.toml"), format!("format = 1\n{PLUGIN}")).unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "index"]).unwrap();
        assert_eq!(read(&data, &registry).unwrap().len(), 1);
        // A newer index is fetched into the clone that is there.
        std::fs::write(origin.join("index.toml"), "format = 1\n").unwrap();
        git(&origin, &["commit", "--quiet", "-am", "empty"]).unwrap();
        assert!(read(&data, &registry).unwrap().is_empty());
        let missing = RegistryConfig {
            name: "gone".into(),
            url: dir.path().join("nowhere").display().to_string(),
        };
        assert!(matches!(
            read(&data, &missing),
            Err(PackageError::Registry { .. })
        ));
    }
}
