//! A package as `moebius-package.toml` says it and as an entry `[[package]]` of a registry's `index.toml` says it: the
//! index entry is the manifest plus where to download what it installs and the sha256 of each download. Both are
//! read as a [`Package`]; [`Package::check`] says whether an entry of an index can be installed from. The exact
//! format is in `docs/reference/packages.md`.

use serde::{Deserialize, Serialize};

use super::PackageError;
use super::version::{Range, Version};
use crate::config::valid_agent_name;
use crate::repos::files::inside;

/// The kinds of package.
pub const KINDS: [&str; 4] = ["plugin", "workflow", "rules", "skills"];

/// The format of `index.toml` this build reads.
pub const INDEX_FORMAT: u32 = 1;

/// A registry's `index.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Index {
    pub format: u32,
    #[serde(default)]
    pub package: Vec<Package>,
}

/// One version of a package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    /// `plugin`, `workflow`, `rules` or `skills`.
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// The Moebius versions it works with, as a range such as `>=0.2`.
    pub moebius: String,
    /// The targets a plugin is built for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    /// What a plugin is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginSpec>,
    /// The program of a plugin for each target (index only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub downloads: Vec<Download>,
    /// The files of a workflow, rules or skills package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<PackageFile>,
}

/// What a plugin package is: a program speaking the process plugin protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginSpec {
    /// `decider` or `launcher`.
    pub kind: String,
    /// The file name of the program inside `<data_dir>/packages/<name>/<version>/`.
    pub program: String,
    /// Whether the `[[plugins]]` entry gets `launches = true`; always so for a launcher.
    #[serde(default)]
    pub launches: bool,
    /// The kinds of decision a decider answers, for the `decides` of its entry.
    #[serde(default)]
    pub decides: Vec<String>,
    /// How the settings reach the program; informative, `args` for the settings that are flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_as: Option<String>,
    /// The programs it needs besides Moebius, with their versions; informative.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub requires: std::collections::BTreeMap<String, String>,
    /// The settings it takes, for a form.
    #[serde(default)]
    pub settings: Vec<Setting>,
}

/// One setting of a plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setting {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default)]
    pub repeatable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub description: String,
}

/// The program of a plugin for one target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Download {
    /// A target triple such as `x86_64-unknown-linux-gnu`.
    pub target: String,
    pub url: String,
    pub sha256: String,
}

/// A file of a workflow, rules or skills package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageFile {
    /// Where it is in the package's directory of the registry (manifest only; informative in the index).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Where it goes: under `.moebius/` of a repository for a workflow, under `skills/` of the harness configuration
    /// repository for skills; the name of the rules package's one file is free (`rules.md` by convention).
    pub to: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
}

impl Package {
    /// The version, which `check` has made sure parses.
    pub fn parsed_version(&self) -> Version {
        Version::parse(&self.version).unwrap_or(Version::parse("0").expect("0 is a version"))
    }

    /// Whether the Moebius version `moebius` is in the range the package works with.
    pub fn works_with(&self, moebius: Version) -> bool {
        Range::parse(&self.moebius).is_some_and(|range| range.contains(moebius))
    }

    /// Whether this is an entry an index can offer: names and versions are valid, every download and file has an
    /// `http`/`https` URL and a sha256, and the destinations stay inside their folders.
    pub fn check(&self) -> Result<(), PackageError> {
        let bad = |why: &str| PackageError::Index(format!("{} {}: {why}", self.name, self.version));
        if !valid_agent_name(&self.name) {
            return Err(PackageError::Index(format!(
                "{:?} is not a package name (letters, digits, - and _)",
                self.name
            )));
        }
        if Version::parse(&self.version).is_none() {
            return Err(bad("the version is not numbers separated by dots"));
        }
        if Range::parse(&self.moebius).is_none() {
            return Err(bad("`moebius` is not a range of versions"));
        }
        if !KINDS.contains(&self.kind.as_str()) {
            return Err(bad("`kind` is not plugin, workflow, rules or skills"));
        }
        let download = |url: &str, sha256: &str| -> Result<(), PackageError> {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(bad("a URL is not http or https"));
            }
            if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(bad("a sha256 is not 64 hexadecimal digits"));
            }
            Ok(())
        };
        if self.kind == "plugin" {
            let plugin = self
                .plugin
                .as_ref()
                .ok_or_else(|| bad("a plugin needs a [plugin] table"))?;
            if !matches!(plugin.kind.as_str(), "decider" | "launcher") {
                return Err(bad("`plugin.kind` is not decider or launcher"));
            }
            if plugin.kind == "decider" && plugin.decides.is_empty() {
                return Err(bad("a decider needs `plugin.decides`"));
            }
            let program = &plugin.program;
            if program.is_empty()
                || program.contains(['/', '\\', ':'])
                || program.starts_with('.')
                || program.starts_with('-')
            {
                return Err(bad("`plugin.program` is not a file name"));
            }
            if self.downloads.is_empty() {
                return Err(bad("a plugin needs a download for each target"));
            }
            for each in &self.downloads {
                download(&each.url, &each.sha256)?;
            }
            return Ok(());
        }
        if self.files.is_empty() {
            return Err(bad("it has no files"));
        }
        if self.kind == "rules" && self.files.len() != 1 {
            return Err(bad("a rules package has exactly one file"));
        }
        for file in &self.files {
            download(&file.url, &file.sha256)?;
            let to = inside(&file.to).map_err(|_| bad("`to` is not a path inside its folder"))?;
            if to.is_empty() || to != file.to {
                return Err(bad("`to` is not a plain relative path"));
            }
            if self.kind == "workflow" && !to.ends_with(".luau") {
                return Err(bad("a workflow's files are column scripts (.luau)"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest of the nono sandbox plugin of Code000-211/moebius-plugins, as that repository has it.
    const NONO: &str = include_str!("../../../testdata/packages/nono-sandbox.toml");

    fn index_entry() -> Package {
        let mut package: Package = toml::from_str(NONO).unwrap();
        package.downloads = vec![Download {
            target: "x86_64-unknown-linux-gnu".into(),
            url: "https://example.com/nono-sandbox".into(),
            sha256: "ab".repeat(32),
        }];
        package
    }

    #[test]
    fn the_manifest_of_the_nono_sandbox_is_a_package() {
        let package = index_entry();
        assert_eq!(package.name, "nono-sandbox");
        assert_eq!(package.kind, "plugin");
        assert_eq!(package.targets.len(), 4);
        let plugin = package.plugin.as_ref().unwrap();
        assert_eq!((plugin.kind.as_str(), plugin.launches), ("launcher", true));
        assert_eq!(plugin.requires["nono"], "0.81.0");
        let names: Vec<&str> = plugin.settings.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names[..3], ["nono", "nono_version", "network"]);
        assert_eq!(plugin.settings[2].choices, ["moebius", "internet"]);
        assert_eq!(plugin.settings[3].default, Some(serde_json::json!([])));
        package.check().unwrap();
        // It works with the Moebius versions that have the launcher kind.
        assert!(package.works_with(Version::parse("0.2.0").unwrap()));
        assert!(!package.works_with(Version::parse("0.1.9").unwrap()));
    }

    #[test]
    fn an_entry_that_cannot_be_installed_from_is_refused_with_a_reason() {
        let refused = |change: fn(&mut Package)| {
            let mut package = index_entry();
            change(&mut package);
            package.check().unwrap_err().to_string()
        };
        assert!(refused(|p| p.name = "a b".into()).contains("package name"));
        assert!(refused(|p| p.version = "1.x".into()).contains("version"));
        assert!(refused(|p| p.moebius = "^1".into()).contains("range"));
        assert!(refused(|p| p.kind = "theme".into()).contains("kind"));
        assert!(refused(|p| p.downloads[0].sha256 = "abc".into()).contains("sha256"));
        assert!(refused(|p| p.downloads[0].url = "file:///etc/passwd".into()).contains("URL"));
        assert!(refused(|p| p.downloads.clear()).contains("download"));
        assert!(
            refused(|p| p.plugin.as_mut().unwrap().program = "../x".into()).contains("program")
        );
        assert!(refused(|p| p.plugin = None).contains("[plugin]"));
    }

    #[test]
    fn file_packages_keep_their_files_inside_their_folders() {
        let file = |to: &str| PackageFile {
            source: None,
            to: to.into(),
            url: "https://example.com/f".into(),
            sha256: "cd".repeat(32),
        };
        let package = |kind: &str, files: Vec<PackageFile>| Package {
            name: "p".into(),
            version: "1.0.0".into(),
            kind: kind.into(),
            description: String::new(),
            license: None,
            homepage: None,
            moebius: "*".into(),
            targets: vec![],
            plugin: None,
            downloads: vec![],
            files,
        };
        package("workflow", vec![file("Review.luau")])
            .check()
            .unwrap();
        package("rules", vec![file("rules.md")]).check().unwrap();
        package("skills", vec![file("tdd/SKILL.md")])
            .check()
            .unwrap();
        for bad in ["../Review.luau", "/Review.luau", "a//b.luau", ".git/x.luau"] {
            assert!(
                package("workflow", vec![file(bad)]).check().is_err(),
                "{bad}"
            );
        }
        assert!(package("workflow", vec![file("notes.md")]).check().is_err());
        assert!(
            package("rules", vec![file("a.md"), file("b.md")])
                .check()
                .is_err()
        );
        assert!(package("skills", vec![]).check().is_err());
    }
}
