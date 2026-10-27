//! Rules (spec 5, ADR 0007): the things a run must not do, each with a short name and prose. They come from
//! `rules.md` files, Markdown with one `## <name>` section per rule; text before the first heading is ignored.
//!
//! The server's rules file comes first, then the repository's `.moebius/rules.md` on its default branch. A repository
//! may add rules but not weaken them: one named like a server rule is dropped, so the server's text stands.

use std::path::Path;

use crate::repos::git;
use crate::store::Repository;

/// One rule: what it is called and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub name: String,
    pub text: String,
}

/// The rules a `rules.md` lists.
pub fn parse(markdown: &str) -> Vec<Rule> {
    let mut rules: Vec<Rule> = Vec::new();
    for line in markdown.lines() {
        if let Some(name) = line.strip_prefix("## ") {
            rules.push(Rule {
                name: name.trim().to_string(),
                text: String::new(),
            });
        } else if let Some(rule) = rules.last_mut() {
            rule.text.push_str(line);
            rule.text.push('\n');
        }
    }
    for rule in &mut rules {
        rule.text = rule.text.trim().to_string();
    }
    rules.retain(|rule| !rule.name.is_empty());
    rules
}

/// The rules as `rules.md` text, for a decider to read.
pub fn to_markdown(rules: &[Rule]) -> String {
    rules
        .iter()
        .map(|rule| format!("## {}\n{}\n", rule.name, rule.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The rules that apply to a run: the server's, then those the repository adds. A missing, unset or unreadable file
/// gives no rules.
pub fn applicable(server_file: Option<&Path>, repository: Option<&Repository>) -> Vec<Rule> {
    let mut rules = server_file
        .and_then(|file| std::fs::read_to_string(file).ok())
        .map(|text| parse(&text))
        .unwrap_or_default();
    if let Some(repository) = repository {
        let spec = format!("{}:.moebius/rules.md", repository.base_ref());
        if let Ok(text) = git(Path::new(&repository.path), &["show", &spec]) {
            for rule in parse(&text) {
                if rules.iter().all(|known| known.name != rule.name) {
                    rules.push(rule);
                }
            }
        }
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(name: &str, text: &str) -> Rule {
        Rule {
            name: name.into(),
            text: text.into(),
        }
    }

    #[test]
    fn each_section_is_a_rule_and_text_before_the_first_heading_is_ignored() {
        let rules = parse(
            "# Our rules\n\nSome introduction.\n\n## no-vendoring\nDo not copy dependencies into the tree.\n\
             Use the package manager.\n\n## ask-first\n\nNever change the CI configuration.\n\n### a detail\nstays\n",
        );
        assert_eq!(
            rules,
            vec![
                rule(
                    "no-vendoring",
                    "Do not copy dependencies into the tree.\nUse the package manager."
                ),
                rule(
                    "ask-first",
                    "Never change the CI configuration.\n\n### a detail\nstays"
                ),
            ]
        );
    }

    #[test]
    fn an_empty_file_or_one_without_sections_has_no_rules() {
        assert!(parse("").is_empty());
        assert!(parse("just words\n# Title\n").is_empty());
    }

    #[test]
    fn rules_are_written_as_they_are_read() {
        let rules = vec![rule("a", "one"), rule("b", "two\nlines")];
        assert_eq!(parse(&to_markdown(&rules)), rules);
    }

    fn repository(dir: &Path, rules: Option<&str>) -> Repository {
        git(dir, &["init", "--quiet", "-b", "main"]).unwrap();
        if let Some(rules) = rules {
            std::fs::create_dir_all(dir.join(".moebius")).unwrap();
            std::fs::write(dir.join(".moebius/rules.md"), rules).unwrap();
            git(dir, &["add", "."]).unwrap();
        }
        git(
            dir,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "first",
            ],
        )
        .unwrap();
        Repository {
            id: 1,
            name: "demo".into(),
            path: dir.to_str().unwrap().into(),
            forge: "none".into(),
            default_branch: "main".into(),
            github: None,
            trusted: vec![],
        }
    }

    #[test]
    fn a_repository_adds_rules_from_its_default_branch_but_cannot_replace_a_servers() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rules.md");
        std::fs::write(&file, "## no-secrets\nNever print secrets.\n").unwrap();
        let clone = dir.path().join("clone");
        std::fs::create_dir(&clone).unwrap();
        let repository = repository(
            &clone,
            Some(
                "## no-secrets\nPrinting secrets is fine.\n\n## small-diffs\nKeep changes small.\n",
            ),
        );
        // A working-tree edit that is not committed is not read.
        std::fs::write(clone.join(".moebius/rules.md"), "## sneaky\nx\n").unwrap();

        let rules = applicable(Some(&file), Some(&repository));
        assert_eq!(
            rules,
            vec![
                rule("no-secrets", "Never print secrets."),
                rule("small-diffs", "Keep changes small."),
            ]
        );
    }

    #[test]
    fn missing_files_give_no_rules() {
        let dir = tempfile::tempdir().unwrap();
        let repository = repository(dir.path(), None);
        assert!(applicable(None, None).is_empty());
        assert!(applicable(Some(&dir.path().join("nope.md")), Some(&repository)).is_empty());
    }
}
