//! Rules (spec 5, ADR 0007): the things a run must not do, each with a short name and prose. They come from
//! `rules.md` files, Markdown with one `## <name>` section per rule; text before the first heading is ignored.
//!
//! The server's rules file comes first, then the repository's `.moebius/rules.md` on its default branch. A repository
//! may add rules but not weaken them: one named like a server rule is dropped, so the server's text stands.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::guard;
use crate::repos::git;
use crate::store::{Repository, now};

/// One rule: what it is called and what it says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub text: String,
}

/// The rules a `rules.md` lists. When a name is repeated the first rule of that name stands.
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
    let mut seen = HashSet::new();
    rules.retain(|rule| !rule.name.is_empty() && seen.insert(rule.name.clone()));
    rules
}

/// Why `markdown` is not fit to be saved as a rules file, if it is not: a `## ` line without a name, or a name given
/// twice (the parser would drop the second rule without a word).
pub fn check(markdown: &str) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (number, line) in markdown.lines().enumerate() {
        let Some(name) = line.strip_prefix("## ") else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(format!(
                "line {}: a rule needs a name after \"## \"",
                number + 1
            ));
        }
        if !seen.insert(name) {
            return Err(format!(
                "line {}: the rule {name:?} is named twice",
                number + 1
            ));
        }
    }
    Ok(())
}

/// The rules as `rules.md` text, for a decider to read.
pub fn to_markdown(rules: &[Rule]) -> String {
    rules
        .iter()
        .map(|rule| format!("## {}\n{}\n", rule.name, rule.text))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The server's rules file could not be read.
#[derive(Debug, thiserror::Error)]
#[error("cannot read the rules file {}: {source}", path.display())]
pub struct RulesError {
    path: PathBuf,
    source: std::io::Error,
}

/// The rules that apply to a run: the server's, then those the repository adds, each name once. An unset or missing
/// file gives no rules; a server file that exists but cannot be read is an error, so that nothing runs unchecked.
pub fn applicable(
    server_file: Option<&Path>,
    repository: Option<&Repository>,
) -> Result<Vec<Rule>, RulesError> {
    let mut rules = Vec::new();
    if let Some(path) = server_file {
        match std::fs::read_to_string(path) {
            Ok(text) => rules = parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(RulesError {
                    path: path.into(),
                    source,
                });
            }
        }
    }
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
    Ok(rules)
}

/// A built-in rule, as `GET /v1/rules` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pattern {
    pub name: String,
    pub description: String,
}

/// The rules of a `rules.md` file, as it is and as the parser reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prose {
    /// Where the file is from: `harness` (the harness configuration repository's `rules.md`) or `file` (the
    /// configuration's `rules_file`); none when the server has no rules file.
    pub source: Option<String>,
    /// Whether `PUT /v1/rules` can change the file: always, unless the server has no rules file.
    pub editable: bool,
    /// The file's text; empty when it is missing or cannot be read.
    pub text: String,
    pub rules: Vec<Rule>,
    /// Why the file cannot be used, when it cannot.
    pub error: Option<String>,
}

/// A repository's own `.moebius/rules.md` on its default branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRules {
    pub repository: String,
    pub text: String,
    pub rules: Vec<Rule>,
    pub error: Option<String>,
}

/// What `GET /v1/rules` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub patterns: Vec<Pattern>,
    pub prose: Prose,
    /// The repositories that have a `.moebius/rules.md`.
    pub repositories: Vec<RepositoryRules>,
    /// The tool calls refused in the last 7 days, by the name of the rule that refused them.
    pub refusals: BTreeMap<String, i64>,
}

const WEEK: i64 = 7 * 24 * 60 * 60;

/// Writes the server's rules file `file` (a server without a harness configuration repository) after checking `text`,
/// and records the person's or agent's decision `edit_rules`. Text that changes nothing writes and records nothing.
/// The file is replaced whole, so a run reading it sees the old rules or the new ones.
pub fn save_file(
    app: &App,
    by: &crate::decide::Actor,
    file: &Path,
    text: &str,
) -> crate::app::Result<bool> {
    check(text).map_err(crate::app::AppError::Invalid)?;
    let mut text = text.to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if std::fs::read_to_string(file).ok().as_deref() == Some(text.as_str()) {
        return Ok(false);
    }
    let staged = file.with_extension("md.saving");
    std::fs::write(&staged, &text)?;
    std::fs::rename(&staged, file)?;
    let context = BTreeMap::from([
        ("text".to_string(), text),
        ("file".to_string(), file.display().to_string()),
    ]);
    app.decisions.record_by(
        by,
        &crate::decide::Question {
            kind: "edit_rules".into(),
            context,
            options: vec!["edit_rules".into(), "leave".into()],
            fail_closed: "leave".into(),
            run_id: None,
            card_id: None,
        },
        "edit_rules",
        "",
    )?;
    Ok(true)
}

/// The rules page: the built-in patterns, the rules in `file` (`harness` when it is the harness configuration
/// repository's), those of each repository and the refusals of the last week.
pub fn view(app: &App, file: Option<&Path>, harness: bool) -> crate::app::Result<View> {
    let (text, error) = match file.map(std::fs::read_to_string) {
        Some(Ok(text)) => (text, None),
        Some(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), None),
        Some(Err(e)) => (
            String::new(),
            Some(format!("cannot read the rules file: {e}")),
        ),
        None => (String::new(), None),
    };
    let source = file.map(|_| if harness { "harness" } else { "file" }.to_string());
    let prose = Prose {
        source,
        editable: file.is_some(),
        rules: parse(&text),
        error: error.or_else(|| check(&text).err()),
        text,
    };
    let repositories = app
        .store
        .repositories()?
        .iter()
        .filter_map(|repository| {
            let spec = format!("{}:.moebius/rules.md", repository.base_ref());
            let text = git(Path::new(&repository.path), &["show", &spec]).ok()?;
            Some(RepositoryRules {
                repository: repository.name.clone(),
                rules: parse(&text),
                error: check(&text).err(),
                text,
            })
        })
        .collect();
    Ok(View {
        patterns: guard::patterns()
            .into_iter()
            .map(|(name, description)| Pattern {
                name: name.into(),
                description: description.into(),
            })
            .collect(),
        prose,
        repositories,
        refusals: app.store.refusals_by_rule(now() - WEEK)?,
    })
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
    fn a_file_with_a_nameless_or_repeated_rule_is_not_fit_to_be_saved() {
        assert_eq!(check("intro\n\n## a\none\n\n## b\ntwo\n"), Ok(()));
        assert_eq!(check(""), Ok(()));
        assert!(
            check("## a\n## \nx")
                .unwrap_err()
                .starts_with("line 2: a rule needs a name")
        );
        assert_eq!(
            check("## a\none\n## b\n## a\n").unwrap_err(),
            "line 4: the rule \"a\" is named twice"
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

        let rules = applicable(Some(&file), Some(&repository)).unwrap();
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
        assert!(applicable(None, None).unwrap().is_empty());
        let missing = dir.path().join("nope.md");
        assert!(
            applicable(Some(&missing), Some(&repository))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_server_file_that_cannot_be_read_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let invalid = dir.path().join("invalid.md");
        std::fs::write(&invalid, [0xff, 0xfe]).unwrap();
        for file in [dir.path(), invalid.as_path()] {
            let error = applicable(Some(file), None).unwrap_err();
            assert!(error.to_string().contains("cannot read the rules file"));
        }
    }

    #[test]
    fn the_first_rule_of_a_name_stands_in_either_file() {
        assert_eq!(
            parse("## a\none\n## b\nb\n## a\ntwo\n"),
            vec![rule("a", "one"), rule("b", "b")]
        );
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("rules.md");
        std::fs::write(&file, "## a\nserver\n## a\nagain\n").unwrap();
        let clone = dir.path().join("clone");
        std::fs::create_dir(&clone).unwrap();
        let repository = repository(&clone, Some("## c\none\n## c\ntwo\n## a\nmine\n"));
        assert_eq!(
            applicable(Some(&file), Some(&repository)).unwrap(),
            vec![rule("a", "server"), rule("c", "one")]
        );
    }
}
