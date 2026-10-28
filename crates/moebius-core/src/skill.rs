//! The `moebius` skill: the Moebius guide and references, bundled into the binary and written to
//! `<data_dir>/skills/moebius/` when the server starts, where the Moebius agent finds it. The pages are the files of
//! `docs/guide/` and `docs/reference/`, so the skill cannot say anything the documentation does not.

use std::path::{Path, PathBuf};

/// The name of the skill.
pub const NAME: &str = "moebius";

/// The skill's own page, whose frontmatter describes it.
pub const PAGE: &str = include_str!("../skill/SKILL.md");

/// The `description` in the frontmatter of a `SKILL.md`, if it has one.
pub fn description(page: &str) -> Option<String> {
    let frontmatter = page.strip_prefix("---\n")?.split("\n---").next()?;
    let value = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix("description:"))?;
    let value = value.trim();
    let unquoted = [('"', '"'), ('\'', '\'')]
        .iter()
        .find_map(|(open, close)| value.strip_prefix(*open)?.strip_suffix(*close))
        .unwrap_or(value);
    Some(unquoted.to_string())
}

/// The files of the skill: where each is written under the skill's directory, and what it holds. A page added to
/// `docs/guide/` or `docs/reference/` must be added here (a test checks it).
const FILES: &[(&str, &str)] = &[
    ("SKILL.md", PAGE),
    (
        "guide/README.md",
        include_str!("../../../docs/guide/README.md"),
    ),
    (
        "guide/agents.md",
        include_str!("../../../docs/guide/agents.md"),
    ),
    (
        "guide/autoresearch.md",
        include_str!("../../../docs/guide/autoresearch.md"),
    ),
    (
        "guide/column-scripts.md",
        include_str!("../../../docs/guide/column-scripts.md"),
    ),
    (
        "guide/desktop-api.md",
        include_str!("../../../docs/guide/desktop-api.md"),
    ),
    (
        "guide/extensions.md",
        include_str!("../../../docs/guide/extensions.md"),
    ),
    (
        "guide/plugins.md",
        include_str!("../../../docs/guide/plugins.md"),
    ),
    (
        "guide/rules.md",
        include_str!("../../../docs/guide/rules.md"),
    ),
    (
        "guide/workflows.md",
        include_str!("../../../docs/guide/workflows.md"),
    ),
    (
        "reference/bridge-protocol.md",
        include_str!("../../../docs/reference/bridge-protocol.md"),
    ),
    (
        "reference/column-scripts.md",
        include_str!("../../../docs/reference/column-scripts.md"),
    ),
    (
        "reference/configuration.md",
        include_str!("../../../docs/reference/configuration.md"),
    ),
    (
        "reference/desktop-api.md",
        include_str!("../../../docs/reference/desktop-api.md"),
    ),
    (
        "reference/harness-configuration.md",
        include_str!("../../../docs/reference/harness-configuration.md"),
    ),
    (
        "reference/plugin-api.md",
        include_str!("../../../docs/reference/plugin-api.md"),
    ),
    (
        "reference/router.md",
        include_str!("../../../docs/reference/router.md"),
    ),
    (
        "reference/rules.md",
        include_str!("../../../docs/reference/rules.md"),
    ),
];

/// Where the skill is kept under the data directory.
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("skills/moebius")
}

/// Writes the skill, replacing what an earlier start left.
pub fn write(data_dir: &Path) -> std::io::Result<()> {
    let skill = dir(data_dir);
    match std::fs::remove_dir_all(&skill) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    for (path, text) in FILES {
        let file = skill.join(path);
        std::fs::create_dir_all(file.parent().expect("every file is in a directory"))?;
        std::fs::write(file, text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(directory: &str) -> Vec<String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        let mut pages: Vec<String> = std::fs::read_dir(root.join(directory))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".md"))
            .map(|name| format!("{directory}/{name}"))
            .collect();
        pages.sort();
        pages
    }

    #[test]
    fn the_skill_has_every_page_of_the_guide_and_the_reference() {
        let mut expected = vec!["SKILL.md".to_string()];
        expected.extend(pages("guide"));
        expected.extend(pages("reference"));
        let mut included: Vec<&str> = FILES.iter().map(|(path, _)| *path).collect();
        included.sort();
        assert_eq!(included, expected);
    }

    #[test]
    fn the_description_is_read_from_the_frontmatter() {
        assert_eq!(
            description("---\nname: x\ndescription: Does it.\n---\nBody\n").as_deref(),
            Some("Does it.")
        );
        assert_eq!(
            description("---\ndescription: 'Quoted: yes'\n---\n").as_deref(),
            Some("Quoted: yes")
        );
        assert_eq!(description("---\nname: x\n---\ndescription: no\n"), None);
        assert_eq!(description("No frontmatter\n"), None);
        assert!(description(PAGE).unwrap().starts_with("Build on Moebius"));
    }

    #[test]
    fn the_skill_is_written_whole_and_what_an_earlier_start_left_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let skill = super::dir(dir.path());
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("stale.md"), "old").unwrap();
        write(dir.path()).unwrap();
        assert!(!skill.join("stale.md").exists());
        for (path, text) in FILES {
            assert_eq!(&std::fs::read_to_string(skill.join(path)).unwrap(), text);
        }
        let skill_md = std::fs::read_to_string(skill.join("SKILL.md")).unwrap();
        assert!(skill_md.starts_with("---\nname: moebius\ndescription: "));
    }
}
