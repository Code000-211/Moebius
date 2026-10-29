//! The built-in skills, bundled into the binary and written to `<data_dir>/skills/<name>/` when the server starts, where
//! the built-in agents find them. The `moebius` skill is the reference library: its pages are the files of `docs/guide/`
//! and `docs/reference/`, so it cannot say anything the documentation does not, and every built-in agent has it. The
//! task skills (`moebius-operate`, `moebius-configure`, `moebius-workflows`, `moebius-extensions`, `moebius-plugins`)
//! are one `SKILL.md` each with the steps of one kind of task, and only the Moebius agent has them.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// The name of the reference library.
pub const NAME: &str = "moebius";

/// The reference library's own page, whose frontmatter describes it.
pub const PAGE: &str = include_str!("../../skill/SKILL.md");

/// A skill Moebius brings itself: its name and its files, each with where it is written under the skill's directory
/// and what it holds. The first file is its `SKILL.md`.
pub struct BuiltIn {
    pub name: &'static str,
    files: &'static [(&'static str, &'static str)],
}

/// A built-in skill as an agent's page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub name: String,
    pub description: String,
}

impl BuiltIn {
    /// Its `SKILL.md`.
    pub fn page(&self) -> &'static str {
        self.files[0].1
    }

    /// The `description` of its frontmatter.
    pub fn description(&self) -> String {
        description(self.page()).unwrap_or_default()
    }

    /// The paths of its files besides `SKILL.md`.
    pub fn files(&self) -> Vec<&'static str> {
        self.files.iter().skip(1).map(|(path, _)| *path).collect()
    }

    pub fn summary(&self) -> Summary {
        Summary {
            name: self.name.into(),
            description: self.description(),
        }
    }
}

/// The reference library.
pub const MOEBIUS: BuiltIn = BuiltIn {
    name: NAME,
    files: FILES,
};

/// The task skills, which the Moebius agent has besides the reference library.
pub const TASKS: [BuiltIn; 5] = [
    BuiltIn {
        name: "moebius-operate",
        files: &[(
            "SKILL.md",
            include_str!("../../skill/moebius-operate/SKILL.md"),
        )],
    },
    BuiltIn {
        name: "moebius-configure",
        files: &[(
            "SKILL.md",
            include_str!("../../skill/moebius-configure/SKILL.md"),
        )],
    },
    BuiltIn {
        name: "moebius-workflows",
        files: &[(
            "SKILL.md",
            include_str!("../../skill/moebius-workflows/SKILL.md"),
        )],
    },
    BuiltIn {
        name: "moebius-extensions",
        files: &[(
            "SKILL.md",
            include_str!("../../skill/moebius-extensions/SKILL.md"),
        )],
    },
    BuiltIn {
        name: "moebius-plugins",
        files: &[(
            "SKILL.md",
            include_str!("../../skill/moebius-plugins/SKILL.md"),
        )],
    },
];

/// Every built-in skill: the reference library first, then the task skills.
pub fn all() -> impl Iterator<Item = &'static BuiltIn> {
    std::iter::once(&MOEBIUS).chain(TASKS.iter())
}

/// The built-in skill `name`.
pub fn built_in(name: &str) -> Option<&'static BuiltIn> {
    all().find(|skill| skill.name == name)
}

/// The `description` in the frontmatter of a `SKILL.md`, if it has one. A double-quoted value is read with `\"` and
/// `\\`, a single-quoted one with `''`, as YAML has them.
pub fn description(page: &str) -> Option<String> {
    let frontmatter = page.strip_prefix("---\n")?.split("\n---").next()?;
    let value = frontmatter
        .lines()
        .find_map(|line| line.strip_prefix("description:"))?;
    let value = value.trim();
    if let Some(quoted) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        return Some(quoted.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    if let Some(quoted) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return Some(quoted.replace("''", "'"));
    }
    Some(value.to_string())
}

/// The text of a `SKILL.md` after its frontmatter; the whole page when it has none.
pub fn body(page: &str) -> &str {
    page.strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(page, |(_, body)| body)
}

/// The `SKILL.md` of the skill `name`: frontmatter with its `name` and `description`, then `text`.
pub fn page(name: &str, description: &str, text: &str) -> String {
    let description = description.replace('\\', "\\\\").replace('"', "\\\"");
    format!("---\nname: {name}\ndescription: \"{description}\"\n---\n{text}")
}

/// The files of the reference library: where each is written under its directory, and what it holds. A page added to
/// `docs/guide/` or `docs/reference/` must be added here (a test checks it).
const FILES: &[(&str, &str)] = &[
    ("SKILL.md", PAGE),
    (
        "guide/README.md",
        include_str!("../../../../docs/guide/README.md"),
    ),
    (
        "guide/agents.md",
        include_str!("../../../../docs/guide/agents.md"),
    ),
    (
        "guide/autoresearch.md",
        include_str!("../../../../docs/guide/autoresearch.md"),
    ),
    (
        "guide/column-scripts.md",
        include_str!("../../../../docs/guide/column-scripts.md"),
    ),
    (
        "guide/command-line.md",
        include_str!("../../../../docs/guide/command-line.md"),
    ),
    (
        "guide/desktop-api.md",
        include_str!("../../../../docs/guide/desktop-api.md"),
    ),
    (
        "guide/extensions.md",
        include_str!("../../../../docs/guide/extensions.md"),
    ),
    (
        "guide/grilling.md",
        include_str!("../../../../docs/guide/grilling.md"),
    ),
    (
        "guide/plugins.md",
        include_str!("../../../../docs/guide/plugins.md"),
    ),
    (
        "guide/running-moebius.md",
        include_str!("../../../../docs/guide/running-moebius.md"),
    ),
    (
        "guide/rules.md",
        include_str!("../../../../docs/guide/rules.md"),
    ),
    (
        "guide/workflows.md",
        include_str!("../../../../docs/guide/workflows.md"),
    ),
    (
        "reference/bridge-protocol.md",
        include_str!("../../../../docs/reference/bridge-protocol.md"),
    ),
    (
        "reference/cli.md",
        include_str!("../../../../docs/reference/cli.md"),
    ),
    (
        "reference/column-scripts.md",
        include_str!("../../../../docs/reference/column-scripts.md"),
    ),
    (
        "reference/configuration.md",
        include_str!("../../../../docs/reference/configuration.md"),
    ),
    (
        "reference/desktop-api.md",
        include_str!("../../../../docs/reference/desktop-api.md"),
    ),
    (
        "reference/decision-model.md",
        include_str!("../../../../docs/reference/decision-model.md"),
    ),
    (
        "reference/harness-configuration.md",
        include_str!("../../../../docs/reference/harness-configuration.md"),
    ),
    (
        "reference/plugin-api.md",
        include_str!("../../../../docs/reference/plugin-api.md"),
    ),
    (
        "reference/router.md",
        include_str!("../../../../docs/reference/router.md"),
    ),
    (
        "reference/rules.md",
        include_str!("../../../../docs/reference/rules.md"),
    ),
    (
        "reference/training-export.md",
        include_str!("../../../../docs/reference/training-export.md"),
    ),
];

/// Where the built-in skill `name` is kept under the data directory.
pub fn dir(data_dir: &Path, name: &str) -> PathBuf {
    data_dir.join("skills").join(name)
}

/// Writes every built-in skill, replacing what an earlier start left.
pub fn write(data_dir: &Path) -> std::io::Result<()> {
    for skill in all() {
        let root = dir(data_dir, skill.name);
        match std::fs::remove_dir_all(&root) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        for (path, text) in skill.files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("every file is in a directory"))?;
            std::fs::write(file, text)?;
        }
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
        assert!(
            description(PAGE)
                .unwrap()
                .starts_with("Operate and build on Moebius")
        );
    }

    #[test]
    fn a_page_is_written_and_read_back() {
        let text = "# Title\n\n---\nMore.\n";
        let description_text = "Say \"it\": a \\ b";
        let written = page("house-style", description_text, text);
        assert!(written.starts_with(
            "---\nname: house-style\ndescription: \"Say \\\"it\\\": a \\\\ b\"\n---\n"
        ));
        assert_eq!(description(&written).as_deref(), Some(description_text));
        assert_eq!(body(&written), text);
        assert_eq!(body("No frontmatter\n"), "No frontmatter\n");
        assert!(body(PAGE).contains("Moebius"));
        let files = MOEBIUS.files();
        assert!(files.contains(&"guide/agents.md") && !files.contains(&"SKILL.md"));
    }

    #[test]
    fn single_quotes_double_when_read() {
        assert_eq!(
            description("---\ndescription: 'It''s'\n---\n").as_deref(),
            Some("It's")
        );
    }

    #[test]
    fn every_skill_is_written_whole_and_what_an_earlier_start_left_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let skill = super::dir(dir.path(), NAME);
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("stale.md"), "old").unwrap();
        write(dir.path()).unwrap();
        assert!(!skill.join("stale.md").exists());
        for built_in in all() {
            let root = super::dir(dir.path(), built_in.name);
            for (path, text) in built_in.files {
                assert_eq!(&std::fs::read_to_string(root.join(path)).unwrap(), text);
            }
            let page = std::fs::read_to_string(root.join("SKILL.md")).unwrap();
            assert!(page.starts_with(&format!("---\nname: {}\ndescription: ", built_in.name)));
        }
        assert_eq!(all().count(), 6);
    }

    #[test]
    fn every_skill_has_a_name_and_a_description_pi_accepts() {
        for skill in all() {
            let frontmatter = skill.page().split("\n---\n").next().unwrap();
            assert!(
                frontmatter
                    .lines()
                    .any(|line| line == format!("name: {}", skill.name)),
                "{}",
                skill.name
            );
            let valid = skill.name.len() <= 64
                && skill
                    .name
                    .split('-')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase()));
            assert!(valid, "{}", skill.name);
            let description = skill.description();
            assert!(
                (60..=1024).contains(&description.len()),
                "{}: {description}",
                skill.name
            );
            assert!(
                description.contains("Use it"),
                "{}: {description}",
                skill.name
            );
            // Unquoted YAML: a colon and a space, or a space and a hash, would end the value.
            assert!(
                !description.contains(": ") && !description.contains(" #"),
                "{}: {description}",
                skill.name
            );
        }
        assert_eq!(built_in("moebius-operate").unwrap().name, "moebius-operate");
        assert!(built_in("house-style").is_none());
    }

    #[test]
    fn the_reference_library_points_to_every_task_skill_and_the_task_skills_to_its_pages() {
        for task in &TASKS {
            assert!(PAGE.contains(&format!("`{}`", task.name)), "{}", task.name);
            assert!(task.files().is_empty());
            // Every page a task skill sends the agent to is in the reference library.
            let body = body(task.page());
            let named = body
                .split('`')
                .filter(|word| word.starts_with("guide/") || word.starts_with("reference/"));
            let mut count = 0;
            for page in named {
                assert!(
                    FILES.iter().any(|(path, _)| *path == page),
                    "{}: {page}",
                    task.name
                );
                count += 1;
            }
            assert!(
                count > 0,
                "{} names no page of the moebius skill",
                task.name
            );
        }
    }

    #[test]
    fn the_column_scripts_of_the_task_skills_pass_the_check() {
        let mut checked = 0;
        for task in &TASKS {
            let mut rest = task.page();
            while let Some(at) = rest.find("<!-- column: ") {
                let after = &rest[at + "<!-- column: ".len()..];
                let (column, after) = after.split_once(" -->\n```luau\n").unwrap();
                let (script, after) = after.split_once("```").unwrap();
                let did = crate::board::check_script(
                    "example.luau",
                    script,
                    column,
                    "[large] Rewrite the parser",
                    "person",
                )
                .unwrap_or_else(|e| panic!("{}: {e}", task.name));
                assert!(!did.is_empty());
                checked += 1;
                rest = after;
            }
        }
        assert!(checked >= 1);
    }
}
