//! A package's changelog: the sections between the version that is installed and the one an update installs. The file
//! is `CHANGELOG.md` of the package, or the `pi.changelogPath` of its `package.json` (`""` for none); a section starts
//! with a heading such as `## [1.2.0]` or `## 1.2.0 - 2026-01-01`, the format of Pi's own changelog.

use std::path::Path;

use regex::Regex;
use semver::Version;

use super::manifest::package_json;

/// The text of the package's changelog, if it has one that stays inside the package.
pub fn read(dir: &Path) -> Option<String> {
    let json = package_json(dir).ok().flatten();
    let path = match json.as_ref().map(|json| &json["pi"]["changelogPath"]) {
        Some(serde_json::Value::String(path)) => path.trim_start_matches("./").to_string(),
        _ => "CHANGELOG.md".to_string(),
    };
    let inside = !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.split('/').any(|part| part == "..");
    inside
        .then(|| std::fs::read_to_string(dir.join(path)).ok())
        .flatten()
}

/// The sections of `text` for the versions after `from` up to `to`, newest first as the file has them, or empty when
/// there are none.
pub fn sections_between(text: &str, from: &Version, to: &Version) -> String {
    let version = Regex::new(r"(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)").expect("a valid pattern");
    let mut sections: Vec<String> = vec![];
    let mut keep = false;
    for line in text.lines() {
        if line.starts_with("## ") {
            keep = version
                .find(line)
                .and_then(|m| Version::parse(m.as_str()).ok())
                .is_some_and(|v| &v > from && &v <= to);
            if keep {
                sections.push(String::new());
            }
        }
        if keep && let Some(section) = sections.last_mut() {
            section.push_str(line);
            section.push('\n');
        }
    }
    sections
        .iter()
        .map(|s| s.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\n## [Unreleased]\n\n- later\n\n## [1.2.0] - 2026-02-01\n\n- added b\n\n## 1.1.0 - 2026-01-10\n\n- fixed a\n\n## [1.0.0]\n\n- first\n";

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn the_sections_after_the_installed_version_up_to_the_new_one_are_given() {
        let text = sections_between(LOG, &v("1.0.0"), &v("1.2.0"));
        assert_eq!(
            text,
            "## [1.2.0] - 2026-02-01\n\n- added b\n\n## 1.1.0 - 2026-01-10\n\n- fixed a"
        );
        assert_eq!(sections_between(LOG, &v("1.1.0"), &v("1.1.0")), "");
        assert!(sections_between(LOG, &v("1.0.0"), &v("1.1.0")).starts_with("## 1.1.0"));
    }

    #[test]
    fn the_changelog_file_may_be_moved_or_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CHANGELOG.md"), "root").unwrap();
        assert_eq!(read(dir.path()).as_deref(), Some("root"));
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join("docs/LOG.md"), "moved").unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"pi": {"changelogPath": "./docs/LOG.md"}}"#,
        )
        .unwrap();
        assert_eq!(read(dir.path()).as_deref(), Some("moved"));
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"pi": {"changelogPath": ""}}"#,
        )
        .unwrap();
        assert_eq!(read(dir.path()), None);
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"pi": {"changelogPath": "../x"}}"#,
        )
        .unwrap();
        assert_eq!(read(dir.path()), None);
    }
}
