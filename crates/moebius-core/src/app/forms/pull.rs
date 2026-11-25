//! The pull request template (ADR 0034): the fixed Markdown headings every pull request carries, how a body is read
//! into fields by those headings, and how Moebius writes the body of its own pull requests from the same template.
//!
//! The template is itself a Markdown file, the one GitHub shows to a person who opens a pull request: a first line
//! `Fixes #N`, then a section for each `### Heading`, whose text below it (an HTML comment) is the hint. A heading that
//! ends in `(optional)` may be left empty; every other section is required. A project replaces the file in the harness
//! configuration repository, with its own headings and sections of its own.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// The template Moebius ships, which is also the content of `.github/PULL_REQUEST_TEMPLATE.md` it keeps.
pub const DEFAULT_TEMPLATE: &str = "Fixes #N

### Item
<!-- The issue or backlog item this is for, in one line. -->

### What the change does
<!-- Say what changed, in sentences a reviewer can check. -->

### Tests I ran
<!-- A checklist with numbers, one test run each:
- [x] `cargo test`: 412 passed, 0 failed -->

### Measurement
<!-- One of `measured`, `needs testing` or `not needed`, then the reason. -->
";

/// The heading of the section that holds the measurement state, which the server reads further than its text.
pub const MEASUREMENT_HEADING: &str = "Measurement";
/// The heading of the section that holds the checklist of tests.
pub const TESTS_HEADING: &str = "Tests I ran";
/// The name `missing` gives to the first line.
pub const FIXES_LINE: &str = "Fixes #N";

static HTML_COMMENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").expect("the pattern is valid"));
static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s{0,3}#{2,4}\s+(.+?)\s*#*\s*$").expect("the pattern is valid"));
static FIXES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:fix(?:e[sd])?|close[sd]?|resolve[sd]?)\s*:?\s+#(\d+)\b")
        .expect("the pattern is valid")
});
static CHECK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*[-*+]\s+\[([ xX])\]\s+(.*\S)\s*$").expect("the pattern is valid")
});

/// One section of the template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Section {
    pub heading: String,
    pub required: bool,
    /// What the template says under the heading, as a hint to whoever writes the section.
    pub hint: String,
}

/// The sections of a template, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PullTemplate {
    pub sections: Vec<Section>,
}

/// The name a heading has in `fields` and in column scripts: lower case, every run of other characters one `_`
/// (`Tests I ran` is `tests_i_ran`).
pub fn slug(heading: &str) -> String {
    let mut slug = String::new();
    for c in heading.trim().chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('_') && !slug.is_empty() {
            slug.push('_');
        }
    }
    slug.trim_end_matches('_').to_string()
}

/// The heading a template line names: without a trailing colon, and the marker `(optional)` told apart.
fn heading_of(line: &str) -> Option<(String, bool)> {
    let text = HEADING.captures(line)?[1].trim().to_string();
    let text = text.trim_end_matches(':').trim().to_string();
    let optional = text.to_lowercase().ends_with("(optional)");
    let name = if optional {
        text[..text.len() - "(optional)".len()].trim().to_string()
    } else {
        text
    };
    (!name.is_empty()).then_some((name, optional))
}

impl PullTemplate {
    /// The template Moebius ships.
    pub fn standard() -> Self {
        Self::read(DEFAULT_TEMPLATE).expect("the shipped template has sections")
    }

    /// Reads a template file. A file without a section is no template: `Err` says so, and the caller falls back to the
    /// standard one.
    pub fn read(text: &str) -> Result<Self, String> {
        let mut sections: Vec<Section> = Vec::new();
        let mut fence = false;
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                fence = !fence;
            }
            if let (false, Some((heading, optional))) = (fence, heading_of(line)) {
                if sections.iter().any(|s| slug(&s.heading) == slug(&heading)) {
                    return Err(format!("the heading {heading:?} is in the template twice"));
                }
                sections.push(Section {
                    heading,
                    required: !optional,
                    hint: String::new(),
                });
            } else if let Some(last) = sections.last_mut() {
                last.hint.push_str(line);
                last.hint.push('\n');
            }
        }
        if sections.is_empty() {
            return Err(
                "it has no section: a section is a line such as `### What the change does`".into(),
            );
        }
        for section in &mut sections {
            section.hint = comment_text(&section.hint);
        }
        Ok(Self { sections })
    }

    /// The template as the Markdown file a forge shows, for `.github/PULL_REQUEST_TEMPLATE.md`.
    pub fn file(&self) -> String {
        let mut text = format!("{FIXES_LINE}\n");
        for section in &self.sections {
            let optional = if section.required { "" } else { " (optional)" };
            text.push_str(&format!("\n### {}{optional}\n", section.heading));
            if !section.hint.trim().is_empty() {
                text.push_str(&format!("<!-- {} -->\n", section.hint.trim()));
            }
        }
        text
    }
}

/// The text inside the HTML comments of `text`.
fn comment_text(text: &str) -> String {
    let mut found = Vec::new();
    for comment in HTML_COMMENT.find_iter(text) {
        let inner = comment.as_str();
        found.push(inner[4..inner.len() - 3].trim().to_string());
    }
    found.join("\n")
}

/// The state of a measurement a pull request says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasurementField {
    /// `measured`, `needs testing` or `not needed`.
    pub state: String,
    pub reason: String,
}

/// One line of the tests checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestLine {
    pub done: bool,
    pub text: String,
}

/// What a pull request body says, read by the template's headings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pull {
    /// The numbers the first lines close, `Fixes #N`.
    pub fixes: Vec<i64>,
    /// Each section's text by the slug of its heading, for the template's headings and any other the body has; empty
    /// sections are left out.
    pub fields: BTreeMap<String, String>,
    /// The tests checklist.
    pub tests: Vec<TestLine>,
    pub measurement: Option<MeasurementField>,
    /// What the body lacks that the template requires, by name: `Fixes #N` or a heading as the template writes it. An
    /// empty section, one holding only the template's hint, counts as missing, and so does a measurement that begins
    /// with none of the three states.
    pub missing: Vec<String>,
}

/// The measurement a section's text says: it begins with one of the three states.
fn measurement(text: &str) -> Option<MeasurementField> {
    let lower = text.trim().to_lowercase();
    ["measured", "needs testing", "not needed"]
        .into_iter()
        .find(|state| {
            lower
                .strip_prefix(state)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric()))
        })
        .map(|state| MeasurementField {
            state: state.to_string(),
            reason: text.trim()[state.len()..]
                .trim_start_matches(|c: char| {
                    c.is_whitespace() || matches!(c, ':' | '-' | ',' | '.')
                })
                .trim()
                .to_string(),
        })
}

/// Reads `body` by the headings of `template`. A section is the text up to the next heading; the first lines before the
/// first heading are searched for what the pull request closes.
pub fn parse(body: &str, template: &PullTemplate) -> Pull {
    let body = HTML_COMMENT.replace_all(body, "");
    let mut intro = String::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut fence = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        match (fence, heading_of(line)) {
            (false, Some((heading, _))) => sections.push((heading, String::new())),
            _ => {
                let into = sections.last_mut().map_or(&mut intro, |s| &mut s.1);
                into.push_str(line);
                into.push('\n');
            }
        }
    }
    let mut pull = Pull {
        fixes: FIXES
            .captures_iter(&intro)
            .filter_map(|c| c[1].parse().ok())
            .collect(),
        ..Pull::default()
    };
    for (heading, text) in &sections {
        let text = text.trim();
        if !text.is_empty() {
            pull.fields
                .entry(slug(heading))
                .and_modify(|had| {
                    had.push('\n');
                    had.push_str(text);
                })
                .or_insert_with(|| text.to_string());
        }
    }
    pull.tests = pull
        .fields
        .get(&slug(TESTS_HEADING))
        .map(|text| {
            text.lines()
                .filter_map(|l| CHECK.captures(l))
                .map(|c| TestLine {
                    done: &c[1] != " ",
                    text: c[2].to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    pull.measurement = pull
        .fields
        .get(&slug(MEASUREMENT_HEADING))
        .and_then(|text| measurement(text));
    if pull.fixes.is_empty() {
        pull.missing.push(FIXES_LINE.to_string());
    }
    for section in template.sections.iter().filter(|s| s.required) {
        let key = slug(&section.heading);
        let lacking = match pull.fields.get(&key) {
            None => true,
            Some(_) if key == slug(MEASUREMENT_HEADING) => pull.measurement.is_none(),
            Some(_) if key == slug(TESTS_HEADING) => pull.tests.is_empty(),
            Some(_) => false,
        };
        if lacking {
            pull.missing.push(section.heading.clone());
        }
    }
    pull
}

/// A pull request body written from the template: the first line, then each section of the template with its text from
/// `values` (by slug), or the words `Not reported.` for a section that has none.
pub fn render(
    template: &PullTemplate,
    first_line: &str,
    values: &BTreeMap<String, String>,
) -> String {
    let mut body = format!("{}\n", first_line.trim());
    for section in &template.sections {
        let text = values
            .get(&slug(&section.heading))
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .unwrap_or("Not reported.");
        body.push_str(&format!("\n### {}\n\n{text}\n", section.heading));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "Fixes #12\n\n### Item\nK-4: cache the plan\n\n### What the change does\nPlans are cached.\n\n\
        ### Tests I ran\n- [x] `cargo test`: 412 passed\n- [ ] `cargo bench`: not run\n\n### Measurement\nmeasured: 4.2% faster\n";

    #[test]
    fn every_section_of_the_default_template_is_read_into_a_field() {
        let pull = parse(FULL, &PullTemplate::standard());
        assert!(pull.missing.is_empty(), "{:?}", pull.missing);
        assert_eq!(pull.fixes, vec![12]);
        assert_eq!(pull.fields["item"], "K-4: cache the plan");
        assert_eq!(pull.fields["what_the_change_does"], "Plans are cached.");
        assert_eq!(pull.tests.len(), 2);
        assert!(pull.tests[0].done && !pull.tests[1].done);
        let measured = pull.measurement.unwrap();
        assert_eq!(
            (measured.state.as_str(), measured.reason.as_str()),
            ("measured", "4.2% faster")
        );
    }

    #[test]
    fn a_missing_empty_or_hint_only_section_is_missing() {
        let body = "Fixes #3\n\n### Item\nK-1\n\n### What the change does\n<!-- Say what changed -->\n\n### Measurement\nsoon\n";
        let pull = parse(body, &PullTemplate::standard());
        assert_eq!(
            pull.missing,
            vec!["What the change does", "Tests I ran", "Measurement"]
        );
        let none = parse("Plain text.", &PullTemplate::standard());
        assert_eq!(none.missing[0], FIXES_LINE);
        assert_eq!(none.missing.len(), 5);
    }

    #[test]
    fn a_project_template_adds_sections_renames_headings_and_marks_some_optional() {
        let template = PullTemplate::read(
            "Fixes #N\n\n### Summary\n<!-- why -->\n\n### Rollback plan\n\n### Notes (optional)\n",
        )
        .unwrap();
        assert_eq!(template.sections.len(), 3);
        assert!(!template.sections[2].required);
        assert_eq!(template.sections[0].hint, "why");
        let pull = parse("Closes #8\n### Summary\nx\n### Notes\nsome\n", &template);
        assert_eq!(pull.missing, vec!["Rollback plan"]);
        assert_eq!(pull.fields["notes"], "some");
        // The file a forge shows reads back to the same template.
        assert_eq!(PullTemplate::read(&template.file()).unwrap(), template);
    }

    #[test]
    fn a_file_without_a_section_or_with_a_heading_twice_is_no_template() {
        assert!(PullTemplate::read("Fixes #N\nnothing else").is_err());
        assert!(PullTemplate::read("### A\n### a\n").is_err());
    }

    #[test]
    fn headings_in_a_code_fence_are_text() {
        let body = "Fixes #1\n### Item\n```\n### Measurement\nmeasured\n```\n";
        let pull = parse(body, &PullTemplate::standard());
        assert!(pull.fields["item"].contains("### Measurement"));
        assert!(pull.measurement.is_none());
    }

    #[test]
    fn a_body_is_written_from_the_template_and_reads_back() {
        let template = PullTemplate::standard();
        let mut values = BTreeMap::new();
        values.insert("item".to_string(), "K-9".to_string());
        values.insert("what_the_change_does".to_string(), "It does.".to_string());
        values.insert(
            "tests_i_ran".to_string(),
            "- [x] `cargo test`: 3 passed".to_string(),
        );
        values.insert(
            "measurement".to_string(),
            "not needed: no hot path".to_string(),
        );
        let body = render(&template, "Fixes #9", &values);
        assert!(body.starts_with("Fixes #9\n\n### Item\n\nK-9"));
        assert!(parse(&body, &template).missing.is_empty());
        let bare = render(&template, "Fixes #9", &BTreeMap::new());
        assert!(bare.contains("Not reported."));
    }

    #[test]
    fn a_heading_is_a_slug_for_scripts() {
        assert_eq!(slug("Tests I ran"), "tests_i_ran");
        assert_eq!(slug(" Rollback plan! "), "rollback_plan");
    }
}
