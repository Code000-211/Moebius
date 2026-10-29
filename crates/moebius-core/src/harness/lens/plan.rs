//! What the lens checks in a worktree: the repository's `.moebius/lint.toml`, or, without checks in it, what the
//! repository's files say about its tools. The result is the [`Plan`] that is handed to the extension as JSON (the
//! `MOEBIUS_LENS` variable); the extension runs the commands, this module only decides which, for which files.

use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// The file of a repository that configures the lens, relative to the worktree.
pub const LINT_FILE: &str = ".moebius/lint.toml";

/// How long the per-edit checks of one edit may take together, in milliseconds.
const BUDGET_MS: u64 = 1500;
/// How many lines of findings one edit's result and one end-of-turn message carry at most.
const MAX_LINES: usize = 10;
/// How long the end-of-turn commands may take each, in seconds.
const DEEP_TIMEOUT_SECONDS: u64 = 120;

/// How a command's output is read into findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    /// `<file>:<line>[:<column>]: <message>`, one finding per line; other lines are ignored.
    #[default]
    Lines,
    /// One file name per line is one finding, "not formatted" (`gofmt -l`); a line in the form of `lines` is read as
    /// that.
    Files,
    /// `Diff in <file>:<line>:` (`rustfmt --check`).
    DiffIn,
    /// GitHub workflow commands, `::error file=<file>,line=<line>,col=<column>::<message>` (`biome --reporter=github`).
    Github,
}

/// How serious a finding is when the output does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    #[default]
    Warning,
    Error,
}

/// One entry of `.moebius/lint.toml`: for the files a glob names, the per-edit command and the end-of-turn command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckFile {
    glob: Option<String>,
    #[serde(default)]
    globs: Vec<String>,
    edit: Option<String>,
    deep: Option<String>,
    syntax: Option<String>,
    #[serde(default)]
    format: Format,
    pattern: Option<String>,
    #[serde(default)]
    severity: Severity,
}

/// `.moebius/lint.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct LintFile {
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default = "budget")]
    budget_ms: u64,
    #[serde(default = "max_lines")]
    max_lines: usize,
    #[serde(default = "deep_timeout")]
    deep_timeout_seconds: u64,
    #[serde(default = "yes")]
    syntax: bool,
    #[serde(default)]
    check: Vec<CheckFile>,
}

fn yes() -> bool {
    true
}
fn budget() -> u64 {
    BUDGET_MS
}
fn max_lines() -> usize {
    MAX_LINES
}
fn deep_timeout() -> u64 {
    DEEP_TIMEOUT_SECONDS
}

/// One check as the extension gets it: the files it is for as a regular expression over the path from the worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// The first program of the commands, which names the check in events and findings.
    pub name: String,
    pub files: String,
    pub edit: Option<String>,
    pub deep: Option<String>,
    pub syntax: Option<String>,
    pub format: Format,
    pub pattern: Option<String>,
    pub severity: Severity,
}

/// Everything the extension needs to know, as JSON in `MOEBIUS_LENS`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub enabled: bool,
    pub budget_ms: u64,
    pub max_lines: usize,
    pub deep_timeout_ms: u64,
    /// Whether the built-in syntax checks run for files without a `syntax` command of their own.
    pub syntax: bool,
    pub checks: Vec<Check>,
    /// Why `.moebius/lint.toml` cannot be used, when it cannot; the lens then does nothing but say so.
    pub error: Option<String>,
}

impl Plan {
    fn off() -> Self {
        Self {
            enabled: false,
            budget_ms: BUDGET_MS,
            max_lines: MAX_LINES,
            deep_timeout_ms: DEEP_TIMEOUT_SECONDS * 1000,
            syntax: true,
            checks: Vec::new(),
            error: None,
        }
    }

    /// The plan for the worktree at `root`.
    pub fn for_worktree(root: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(root.join(LINT_FILE)) else {
            return Self {
                enabled: true,
                checks: detect(root),
                ..Self::off()
            };
        };
        match parse(&text) {
            Ok(file) if !file.enabled => Self::off(),
            Ok(file) => {
                let detected = file.check.is_empty();
                Self {
                    enabled: true,
                    budget_ms: file.budget_ms,
                    max_lines: file.max_lines,
                    deep_timeout_ms: file.deep_timeout_seconds.saturating_mul(1000),
                    syntax: file.syntax,
                    checks: if detected {
                        detect(root)
                    } else {
                        configured(&file)
                    },
                    error: None,
                }
            }
            Err(why) => Self {
                enabled: true,
                error: Some(format!("{LINT_FILE}: {why}")),
                ..Self::off()
            },
        }
    }
}

fn parse(text: &str) -> Result<LintFile, String> {
    let file: LintFile = toml::from_str(text).map_err(|e| e.to_string())?;
    for (n, check) in file.check.iter().enumerate() {
        let at = |why: &str| format!("check {}: {why}", n + 1);
        if check.glob.is_none() && check.globs.is_empty() {
            return Err(at("needs a glob or globs"));
        }
        if check.edit.is_none() && check.deep.is_none() && check.syntax.is_none() {
            return Err(at("needs an edit, deep or syntax command"));
        }
        for command in [&check.edit, &check.deep, &check.syntax]
            .into_iter()
            .flatten()
        {
            if command.trim().is_empty() {
                return Err(at("a command is empty"));
            }
        }
        if let Some(pattern) = &check.pattern {
            Regex::new(pattern)
                .map_err(|e| at(&format!("the pattern is not a regular expression: {e}")))?;
        }
    }
    Ok(file)
}

fn configured(file: &LintFile) -> Vec<Check> {
    file.check
        .iter()
        .map(|check| {
            let globs: Vec<&str> = check
                .glob
                .iter()
                .map(String::as_str)
                .chain(check.globs.iter().map(String::as_str))
                .collect();
            let files = format!(
                "^(?:{})$",
                globs
                    .iter()
                    .map(|g| glob_regex(g))
                    .collect::<Vec<_>>()
                    .join("|")
            );
            let program = [&check.edit, &check.deep, &check.syntax]
                .into_iter()
                .flatten()
                .next()
                .and_then(|command| command.split_whitespace().next())
                .unwrap_or("check");
            Check {
                name: program.rsplit('/').next().unwrap_or(program).to_string(),
                files,
                edit: check.edit.clone(),
                deep: check.deep.clone(),
                syntax: check.syntax.clone(),
                format: check.format,
                pattern: check.pattern.clone(),
                severity: check.severity,
            }
        })
        .collect()
}

/// A regular expression for `glob` over a path from the worktree, without anchors. `*` is any characters but `/`, `**`
/// any characters, `**/` any folders, `?` one character, `{a,b}` either; a glob without a `/` names files at any depth.
pub fn glob_regex(glob: &str) -> String {
    let mut out = String::new();
    if !glob.contains('/') {
        out.push_str("(?:.*/)?");
    }
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    let mut depth = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                if chars.get(i + 2) == Some(&'/') {
                    out.push_str("(?:.*/)?");
                    i += 2;
                } else {
                    out.push_str(".*");
                    i += 1;
                }
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            '{' => {
                depth += 1;
                out.push_str("(?:");
            }
            '}' if depth > 0 => {
                depth -= 1;
                out.push(')');
            }
            ',' if depth > 0 => out.push('|'),
            c => out.push_str(&regex::escape(&c.to_string())),
        }
        i += 1;
    }
    for _ in 0..depth {
        out.push(')');
    }
    out
}

fn check(files: &str, edit: Option<&str>, deep: Option<&str>) -> Check {
    let command = edit.or(deep).unwrap_or("check");
    let program = command.split_whitespace().next().unwrap_or("check");
    Check {
        name: program.to_string(),
        files: format!("^(?:{})$", glob_regex(files)),
        edit: edit.map(str::to_string),
        deep: deep.map(str::to_string),
        syntax: None,
        format: Format::Lines,
        pattern: None,
        severity: Severity::Warning,
    }
}

fn has(root: &Path, names: &[&str]) -> bool {
    names.iter().any(|name| root.join(name).exists())
}

fn mentions(root: &Path, file: &str, word: &str) -> bool {
    std::fs::read_to_string(root.join(file)).is_ok_and(|text| text.contains(word))
}

/// The edition of the Rust code of `root`, which `rustfmt` needs to read it.
fn rust_edition(root: &Path) -> String {
    std::fs::read_to_string(root.join("Cargo.toml"))
        .ok()
        .and_then(|text| {
            Regex::new(r#"edition\s*=\s*"(\d{4})""#)
                .ok()?
                .captures(&text)
                .map(|c| c[1].to_string())
        })
        .unwrap_or_else(|| "2021".into())
}

/// The checks that the files of `root` suggest: the fast linter of the language for each edit and, where the
/// repository configures one, the type check for the end of the turn.
fn detect(root: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    if has(
        root,
        &[
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
            "requirements.txt",
            "ruff.toml",
            ".ruff.toml",
            "Pipfile",
        ],
    ) {
        let mut ruff = check(
            "*.py",
            Some("ruff check --output-format concise --no-cache --quiet {file}"),
            None,
        );
        ruff.name = "ruff".into();
        checks.push(ruff);
        if has(root, &["pyrightconfig.json"]) || mentions(root, "pyproject.toml", "[tool.pyright]")
        {
            let mut pyright = check("*.py", None, Some("pyright {files}"));
            pyright.pattern = Some(
                r"^\s*(?<file>.+?):(?<line>\d+):(?<column>\d+) - (?<severity>error|warning)\w*: (?<message>.*)$"
                    .into(),
            );
            checks.push(pyright);
        }
    }
    if has(root, &["Cargo.toml"]) {
        let edit = format!("rustfmt --check --edition {} {{file}}", rust_edition(root));
        let mut rustfmt = check("*.rs", Some(&edit), None);
        rustfmt.format = Format::DiffIn;
        rustfmt.severity = Severity::Warning;
        checks.push(rustfmt);
        checks.push(check(
            "*.rs",
            None,
            Some("cargo check --quiet --message-format short"),
        ));
    }
    if has(root, &["go.mod"]) {
        let mut gofmt = check("*.go", Some("gofmt -l {file}"), None);
        gofmt.format = Format::Files;
        checks.push(gofmt);
        checks.push(check("*.go", None, Some("go vet {dirs}")));
    }
    if has(root, &["biome.json", "biome.jsonc"]) {
        let mut biome = check(
            "*.{js,jsx,ts,tsx,mjs,cjs}",
            Some("biome lint --reporter=github --max-diagnostics=20 {file}"),
            None,
        );
        biome.format = Format::Github;
        checks.push(biome);
    } else if has(root, &[".oxlintrc.json"]) || mentions(root, "package.json", "oxlint") {
        checks.push(check(
            "*.{js,jsx,ts,tsx,mjs,cjs}",
            Some("oxlint --format unix {file}"),
            None,
        ));
    }
    if has(root, &["tsconfig.json"]) {
        let mut tsc = check(
            "*.{ts,tsx}",
            None,
            Some(
                "tsc --noEmit --pretty false --incremental \
                 --tsBuildInfoFile node_modules/.cache/moebius.tsbuildinfo -p tsconfig.json",
            ),
        );
        tsc.pattern = Some(r"^(?<file>.+?)\((?<line>\d+),(?<column>\d+)\): (?<message>.*)$".into());
        checks.push(tsc);
    }
    let mut shellcheck = check("*.{sh,bash}", Some("shellcheck -f gcc {file}"), None);
    shellcheck.severity = Severity::Warning;
    checks.push(shellcheck);
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(glob: &str, path: &str) -> bool {
        Regex::new(&format!("^(?:{})$", glob_regex(glob)))
            .unwrap()
            .is_match(path)
    }

    #[test]
    fn globs_name_files_by_their_path() {
        assert!(matches("*.py", "a.py") && matches("*.py", "pkg/sub/a.py"));
        assert!(!matches("*.py", "a.pyc") && !matches("*.py", "a.rs"));
        assert!(matches("src/*.rs", "src/a.rs") && !matches("src/*.rs", "src/x/a.rs"));
        assert!(matches("src/**/*.rs", "src/a.rs") && matches("src/**/*.rs", "src/x/y/a.rs"));
        assert!(matches("**/*.ts", "a.ts") && matches("**/*.ts", "x/a.ts"));
        assert!(matches("*.{ts,tsx}", "a.tsx") && !matches("*.{ts,tsx}", "a.js"));
        assert!(matches("a?.c", "ab.c") && !matches("a?.c", "a/.c"));
        assert!(matches("a.b", "a.b") && !matches("a.b", "axb"));
    }

    #[test]
    fn a_file_names_the_checks_and_the_limits() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".moebius")).unwrap();
        std::fs::write(
            dir.path().join(LINT_FILE),
            "budget_ms = 500\nmax_lines = 3\ndeep_timeout_seconds = 9\n\
             [[check]]\nglob = \"*.py\"\nedit = \"./tools/lint {file}\"\ndeep = \"./tools/types {files}\"\n\
             severity = \"error\"\n",
        )
        .unwrap();
        let plan = Plan::for_worktree(dir.path());
        assert_eq!(
            (plan.budget_ms, plan.max_lines, plan.deep_timeout_ms),
            (500, 3, 9000)
        );
        assert_eq!(plan.error, None);
        assert_eq!(plan.checks.len(), 1);
        let check = &plan.checks[0];
        assert_eq!(check.name, "lint");
        assert_eq!(check.edit.as_deref(), Some("./tools/lint {file}"));
        assert_eq!(check.severity, Severity::Error);
        assert!(Regex::new(&check.files).unwrap().is_match("x/a.py"));
    }

    #[test]
    fn a_file_without_checks_leaves_the_detection_and_a_bad_one_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".moebius")).unwrap();
        std::fs::write(dir.path().join("go.mod"), "module x\n").unwrap();
        std::fs::write(dir.path().join(LINT_FILE), "max_lines = 4\n").unwrap();
        let plan = Plan::for_worktree(dir.path());
        assert_eq!(plan.max_lines, 4);
        assert!(plan.checks.iter().any(|c| c.name == "gofmt"));

        for bad in [
            "unknown = 1\n",
            "[[check]]\nedit = \"x {file}\"\n",
            "[[check]]\nglob = \"*.x\"\n",
            "[[check]]\nglob = \"*.x\"\nedit = \"\"\n",
            "[[check]]\nglob = \"*.x\"\nedit = \"x\"\npattern = \"(\"\n",
        ] {
            std::fs::write(dir.path().join(LINT_FILE), bad).unwrap();
            let plan = Plan::for_worktree(dir.path());
            assert!(plan.checks.is_empty(), "{bad}");
            assert!(
                plan.error.as_deref().unwrap().starts_with(LINT_FILE),
                "{bad}: {:?}",
                plan.error
            );
        }

        std::fs::write(dir.path().join(LINT_FILE), "enabled = false\n").unwrap();
        let plan = Plan::for_worktree(dir.path());
        assert!(!plan.enabled && plan.checks.is_empty() && plan.error.is_none());
    }

    #[test]
    fn the_repositorys_files_say_which_tools_to_use() {
        let dir = tempfile::tempdir().unwrap();
        let names = |dir: &Path| -> Vec<String> {
            Plan::for_worktree(dir)
                .checks
                .into_iter()
                .map(|c| c.name)
                .collect()
        };
        // Shell scripts are always checked.
        assert_eq!(names(dir.path()), ["shellcheck"]);

        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n[tool.pyright]\nstrict = true\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"x\"\nedition = \"2024\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        std::fs::write(dir.path().join("biome.json"), "{}").unwrap();
        let plan = Plan::for_worktree(dir.path());
        let names: Vec<_> = plan.checks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "ruff",
                "pyright",
                "rustfmt",
                "cargo",
                "biome",
                "tsc",
                "shellcheck"
            ]
        );
        let rustfmt = plan.checks.iter().find(|c| c.name == "rustfmt").unwrap();
        assert_eq!(
            rustfmt.edit.as_deref(),
            Some("rustfmt --check --edition 2024 {file}")
        );
        assert_eq!(rustfmt.format, Format::DiffIn);
        // Every pattern of the detection is a regular expression.
        for check in &plan.checks {
            Regex::new(&check.files).unwrap();
            if let Some(pattern) = &check.pattern {
                Regex::new(pattern).unwrap();
            }
        }
    }
}
