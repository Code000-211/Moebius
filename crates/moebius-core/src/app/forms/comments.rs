//! The comment templates (ADR 0034): every kind of comment Moebius reads or writes starts with a fixed first line that
//! names it, and has documented fields. [`read`] tells which kind a comment is and parses its fields; a comment that
//! lacks a required field is `missing` it, and the server replies naming the field instead of guessing. The server's own
//! replies are written in the same form ([`Reply`]), so a person or another server can parse them.
//!
//! | kind | first line | fields |
//! |---|---|---|
//! | Measurement | `## Measurement` | `before`, `after`, `GPUs`, `Power cap`, `ROCm`/`Driver`, `Container`, `Runs before`, `Runs after` (the form of ADR 0033, read by `flow::measure`) |
//! | Done | `/done` or `## Done` | sections `### What was done`, `### Tests run`, `### Open points` |
//! | Review answer | `## Review answer` | one list line per finding of the last review |
//! | Take | `/take` | none |
//! | Release | `/release` | none |
//! | Missing (a reply) | `## Missing` | `About`, `Kind`, `Missing`, `Template` |
//! | Posted (a reply) | `## Posted` | `About`, `Kind`, `Detail` |
//!
//! `/done` followed by plain words, which ADR 0028 accepts, still works: it is the old form, and has no field to lack.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::app::flow::measure;

/// The sections a done report has, in order.
pub const DONE_SECTIONS: [&str; 3] = ["What was done", "Tests run", "Open points"];

static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s{0,3}#{2,4}\s+(.+?)\s*#*\s*$").expect("the pattern is valid"));
static LIST_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:[-*+]|\d+[.)])\s+(\S.*)$").expect("the pattern is valid"));

/// A kind of comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Measurement,
    Done,
    ReviewAnswer,
    Take,
    Release,
}

impl Kind {
    /// Every kind a person or an agent writes.
    pub const ALL: [Kind; 5] = [
        Kind::Measurement,
        Kind::Done,
        Kind::ReviewAnswer,
        Kind::Take,
        Kind::Release,
    ];

    /// The fixed first line that names the kind.
    pub fn first_line(self) -> &'static str {
        match self {
            Kind::Measurement => "## Measurement",
            Kind::Done => "/done",
            Kind::ReviewAnswer => "## Review answer",
            Kind::Take => "/take",
            Kind::Release => "/release",
        }
    }

    /// The name of the template file, `<name>.md`.
    pub fn file(self) -> &'static str {
        match self {
            Kind::Measurement => "measurement",
            Kind::Done => "done",
            Kind::ReviewAnswer => "review-answer",
            Kind::Take => "take",
            Kind::Release => "release",
        }
    }

    /// The template as the Markdown file kept in `.github/moebius/`.
    pub fn template(self) -> String {
        match self {
            Kind::Measurement => format!(
                "{}\nbefore <the merge base of the pull request with its base branch>\nafter <the head commit of the pull request>\n\
                 GPUs: <the cards, as `2x MI300X`>\nPower cap: <watts>\nROCm: <version> (or Driver: <version>)\n\
                 Container: <image>\nRuns before: <one number per run, at least {}>\nRuns after: <one number per run, at least {}>\n",
                self.first_line(),
                measure::RUNS_AT_LEAST,
                measure::RUNS_AT_LEAST
            ),
            Kind::Done => format!(
                "{}\n\n### What was done\n<what the change does, in a few sentences>\n\n### Tests run\n<each test and its numbers>\n\n### Open points\n<what is left, or `none`>\n",
                self.first_line()
            ),
            Kind::ReviewAnswer => format!(
                "{}\n\n1. <what you changed for finding 1, with the file>\n2. <what you changed for finding 2>\n",
                self.first_line()
            ),
            Kind::Take | Kind::Release => format!("{}\n", self.first_line()),
        }
    }
}

/// A comment read by its template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Read {
    pub kind: Kind,
    /// The fields the comment has, by name: a section's slug, or `before` and `after` for a measurement, `answer_1`,
    /// `answer_2` and so on for the lines of a review answer, `free_text` for the old form of `/done`.
    pub fields: BTreeMap<String, String>,
    /// The required fields the comment lacks, by name as the template writes it.
    pub missing: Vec<String>,
    /// Whether the comment is in an old form that is still accepted.
    pub legacy: bool,
}

/// The first line of `body` that has words, trimmed.
fn first_line(body: &str) -> Option<&str> {
    body.lines().map(str::trim).find(|l| !l.is_empty())
}

/// Whether `line` is `marker` as a word of its own, in any case.
fn is_marker(line: &str, marker: &str) -> bool {
    let line = line.to_lowercase();
    line.strip_prefix(&marker.to_lowercase())
        .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric()))
}

/// The kind of comment `body` is, by its first line.
pub fn kind_of(body: &str) -> Option<Kind> {
    let first = first_line(body)?;
    if measure::is_measurement(body) {
        return Some(Kind::Measurement);
    }
    [
        (Kind::ReviewAnswer, "## review answer"),
        (Kind::Done, "## done"),
        (Kind::Done, "/done"),
        (Kind::Take, "/take"),
        (Kind::Release, "/release"),
    ]
    .into_iter()
    .find(|(_, marker)| is_marker(first, marker))
    .map(|(kind, _)| kind)
}

/// The sections of `body` by their headings, `(heading, text)` in order; text before the first heading is under `""`.
pub fn sections(body: &str) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = vec![(String::new(), String::new())];
    let mut fence = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        match (fence, HEADING.captures(line)) {
            (false, Some(heading)) => found.push((
                heading[1].trim_end_matches(':').trim().into(),
                String::new(),
            )),
            _ => {
                let text = &mut found.last_mut().expect("there is a first part").1;
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    found
}

/// Reads `body` by its template: its kind, its fields and what it lacks. None when it is none of the kinds. A review
/// answer is read for `findings` findings: it lacks `answer N` for each finding it gives no line to.
pub fn read(body: &str, findings: usize) -> Option<Read> {
    let kind = kind_of(body)?;
    let mut read = Read {
        kind,
        fields: BTreeMap::new(),
        missing: Vec::new(),
        legacy: false,
    };
    match kind {
        Kind::Measurement => {
            let parsed = measure::parse(body);
            if let Some(before) = parsed.before {
                read.fields.insert("before".into(), before);
            }
            if let Some(after) = parsed.after {
                read.fields.insert("after".into(), after);
            }
            read.fields
                .insert("runs_before".into(), parsed.runs_before.to_string());
            read.fields
                .insert("runs_after".into(), parsed.runs_after.to_string());
            read.missing = parsed.missing.iter().map(|m| m.to_string()).collect();
            for (side, runs) in [
                ("Runs before", parsed.runs_before),
                ("Runs after", parsed.runs_after),
            ] {
                if runs < measure::RUNS_AT_LEAST {
                    read.missing.push(side.to_string());
                }
            }
            if !read.fields.contains_key("before") {
                read.missing.push("before".into());
            }
            if !read.fields.contains_key("after") {
                read.missing.push("after".into());
            }
        }
        Kind::Done => {
            let parts = sections(body);
            let headed = parts.iter().skip(1).any(|(h, _)| !h.is_empty());
            let first = first_line(body).unwrap_or_default();
            if !headed && !is_marker(first, "## done") {
                // `/done` and plain words: the old form.
                read.legacy = true;
                let free = body
                    .trim()
                    .get(first.len().min(5)..)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !free.is_empty() {
                    read.fields.insert("free_text".into(), free);
                }
            } else {
                for name in DONE_SECTIONS {
                    let text = parts
                        .iter()
                        .find(|(h, _)| super::slug(h) == super::slug(name))
                        .map(|(_, t)| t.trim())
                        .filter(|t| !t.is_empty());
                    match text {
                        Some(text) => {
                            read.fields.insert(super::slug(name), text.to_string());
                        }
                        None => read.missing.push(name.to_string()),
                    }
                }
            }
        }
        Kind::ReviewAnswer => {
            let answers: Vec<String> = body
                .lines()
                .skip_while(|l| l.trim().is_empty())
                .skip(1)
                .filter_map(|l| LIST_LINE.captures(l).map(|c| c[1].trim().to_string()))
                .collect();
            for (at, answer) in answers.iter().enumerate() {
                read.fields
                    .insert(format!("answer_{}", at + 1), answer.clone());
            }
            for n in answers.len() + 1..=findings.max(1) {
                read.missing.push(format!("answer {n}"));
            }
        }
        Kind::Take | Kind::Release => {}
    }
    Some(read)
}

/// The lines of a review answer, in order, for the next round's prompt.
pub fn answers(read: &Read) -> Vec<String> {
    (1..)
        .map_while(|n| read.fields.get(&format!("answer_{n}")).cloned())
        .collect()
}

/// A reply the server writes, in the form of a template: a first line that names it, then one `Name: value` line for
/// each field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// `Missing` or `Posted`.
    pub name: &'static str,
    pub fields: Vec<(&'static str, String)>,
    /// Words for a person, after the fields.
    pub text: String,
}

impl Reply {
    /// The reply as the comment is written.
    pub fn render(&self) -> String {
        let mut out = format!("## {}\n\n", self.name);
        for (name, value) in &self.fields {
            out.push_str(&format!("{name}: {value}\n"));
        }
        if !self.text.trim().is_empty() {
            out.push_str(&format!("\n{}\n", self.text.trim()));
        }
        out
    }

    /// A reply that names what a pull request or comment lacks.
    pub fn missing(
        about: &'static str,
        kind: &str,
        missing: &[String],
        template: &str,
        text: &str,
    ) -> Self {
        Self {
            name: "Missing",
            fields: vec![
                ("About", about.to_string()),
                ("Kind", kind.to_string()),
                ("Missing", missing.join(", ")),
                ("Template", template.to_string()),
            ],
            text: text.to_string(),
        }
    }

    /// A reply that tells where something was posted.
    pub fn posted(about: &'static str, kind: &str, detail: &str, text: &str) -> Self {
        Self {
            name: "Posted",
            fields: vec![
                ("About", about.to_string()),
                ("Kind", kind.to_string()),
                ("Detail", detail.to_string()),
            ],
            text: text.to_string(),
        }
    }

    /// Reads a reply back: its name and fields. None when `body` is not in the form.
    pub fn parse(body: &str) -> Option<(String, BTreeMap<String, String>)> {
        let first = first_line(body)?;
        let name = first.strip_prefix("## ")?.trim().to_string();
        if !matches!(name.as_str(), "Missing" | "Posted") {
            return None;
        }
        let fields = body
            .lines()
            .skip_while(|l| l.trim().is_empty())
            .skip(1)
            .skip_while(|l| l.trim().is_empty())
            .map_while(|l| {
                let l = l.trim();
                if l.is_empty() {
                    None
                } else {
                    l.split_once(": ")
                }
            })
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        Some((name, fields))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_is_known_by_its_first_line() {
        assert_eq!(
            kind_of("## Measurement\nbefore abc"),
            Some(Kind::Measurement)
        );
        assert_eq!(kind_of("\n\n/done all good"), Some(Kind::Done));
        assert_eq!(kind_of("## DONE\n### What was done\nx"), Some(Kind::Done));
        assert_eq!(kind_of("## Review answer\n1. x"), Some(Kind::ReviewAnswer));
        assert_eq!(kind_of("/take"), Some(Kind::Take));
        assert_eq!(kind_of("/release please"), Some(Kind::Release));
        assert_eq!(kind_of("/taken"), None);
        assert_eq!(kind_of("hello\n/take"), None);
    }

    #[test]
    fn a_done_report_has_three_sections_and_the_old_form_still_passes() {
        let full = "/done\n\n### What was done\nCached it.\n\n### Tests run\n- cargo test: 5 passed\n\n### Open points\nnone\n";
        let read = read(full, 0).unwrap();
        assert!(read.missing.is_empty() && !read.legacy);
        assert_eq!(read.fields["tests_run"], "- cargo test: 5 passed");

        let partial = super::read("## Done\n### What was done\nx\n", 0).unwrap();
        assert_eq!(partial.missing, vec!["Tests run", "Open points"]);

        let old = super::read("/done it works, please look", 0).unwrap();
        assert!(old.legacy && old.missing.is_empty());
        assert_eq!(old.fields["free_text"], "it works, please look");
    }

    #[test]
    fn a_review_answer_has_a_line_for_each_finding() {
        let read = read(
            "## Review answer\n1. Added the test in a.rs.\n- Renamed `x` to `count`.\n",
            3,
        )
        .unwrap();
        assert_eq!(read.missing, vec!["answer 3"]);
        assert_eq!(answers(&read).len(), 2);
        assert!(super::read("## Review answer\n", 2).unwrap().missing.len() == 2);
    }

    #[test]
    fn a_measurement_names_what_it_lacks_by_the_rules_of_adr_0033() {
        let read = read("## Measurement\nGPUs: 2x MI300X\nRuns before: 1, 2\n", 0).unwrap();
        for lacking in [
            "power cap",
            "Runs before",
            "Runs after",
            "before",
            "after",
            "container",
        ] {
            assert!(
                read.missing.iter().any(|m| m == lacking),
                "{lacking}: {:?}",
                read.missing
            );
        }
    }

    #[test]
    fn a_reply_reads_back_as_the_template_that_wrote_it() {
        let reply = Reply::missing(
            "comment",
            "Done",
            &["Tests run".into(), "Open points".into()],
            ".github/moebius/done.md",
            "Please add them.",
        );
        let text = reply.render();
        assert!(text.starts_with("## Missing\n"));
        let (name, fields) = Reply::parse(&text).unwrap();
        assert_eq!(name, "Missing");
        assert_eq!(fields["Missing"], "Tests run, Open points");
        assert_eq!(fields["Kind"], "Done");
        assert!(Reply::parse("hello").is_none());
    }

    #[test]
    fn every_template_file_names_its_kind_on_the_first_line() {
        for kind in Kind::ALL {
            assert_eq!(kind_of(&kind.template()), Some(kind), "{kind:?}");
        }
    }
}
