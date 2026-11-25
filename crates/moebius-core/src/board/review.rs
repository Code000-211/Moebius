//! The review along two axes: what the Standards and the Spec reviews of a round said, read into one `Review`.
//!
//! Each axis is a run of its own (`review-standards`, `review-spec`) that ends with findings and a last line
//! `VERDICT: approve` or `VERDICT: changes`. A finding is a line `FINDING blocking: <what is wrong>` or
//! `FINDING judgement: <what is doubtful>` followed by `EVIDENCE: <the hunk or the spec line it quotes>`. A finding
//! without evidence is dropped, so that what a reviewer only suspects never sends a card back. A finding may say where it
//! is, on a line `FILE: <path>:<line>` after its text or its evidence: the server then writes it as a comment on that
//! line of the pull request (ADR 0034). Only blocking findings
//! send it back; judgement calls go to the person with the offered change. A finding that begins "Possible" names a
//! smell of the fixed baseline, which is always a judgement call, whatever the reviewer called it.

use super::card::Review;

/// What a reviewer of one axis found.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    blocking: bool,
    text: String,
    evidence: String,
    /// The file and the line of the file as the change leaves it, when the reviewer said where.
    location: Option<(String, i64)>,
}

/// A finding as read: blocking, its text lines, its evidence lines and where it is.
type Raw<'a> = (bool, Vec<&'a str>, Vec<&'a str>, Option<(String, i64)>);

/// A finding that names a line, for a comment on that line of the pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineComment {
    pub path: String,
    pub line: i64,
    pub body: String,
}

/// An axis as the card's script sees it: the verdict that follows from its findings, what sends the card back, the
/// judgement calls for the person, and how many findings were dropped for lacking evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisReview {
    pub name: &'static str,
    pub verdict: &'static str,
    pub findings: String,
    pub judgements: String,
    pub dropped: usize,
    /// The findings of the axis that name a line, blocking ones and judgement calls.
    pub comments: Vec<LineComment>,
}

/// The findings in `summary` that cite evidence, whether the reviewer ended with a verdict, and how many findings
/// were dropped.
fn read_axis(summary: Option<&str>) -> (Vec<Finding>, bool, usize) {
    let mut lines: Vec<&str> = summary.unwrap_or("").trim_end().lines().collect();
    let said = lines.last().is_some_and(|last| {
        let last = last.trim().to_ascii_lowercase();
        last.strip_prefix("verdict:")
            .is_some_and(|v| matches!(v.trim(), "approve" | "changes"))
    });
    if said {
        lines.pop();
    }
    // Findings as (blocking, text lines, evidence lines, where).
    let mut found: Vec<Raw> = Vec::new();
    let mut in_evidence = false;
    for line in lines {
        let trimmed = line.trim();
        if let Some(severity) = finding_severity(trimmed) {
            found.push((severity.0, vec![severity.1], Vec::new(), None));
            in_evidence = false;
        } else if let Some(current) = found.last_mut() {
            if let Some(place) = trimmed
                .get(..5)
                .filter(|p| p.eq_ignore_ascii_case("file:"))
                .and_then(|_| location(trimmed[5..].trim()))
            {
                current.3 = Some(place);
                in_evidence = false;
                continue;
            }
            match trimmed
                .get(..9)
                .filter(|p| p.eq_ignore_ascii_case("evidence:"))
            {
                Some(_) => {
                    in_evidence = true;
                    current.2.push(trimmed[9..].trim());
                }
                None if in_evidence => current.2.push(line),
                None => current.1.push(line),
            }
        }
    }
    let mut dropped = 0;
    let mut kept = Vec::new();
    for (blocking, text, evidence, location) in found {
        let text = text.join("\n").trim().to_string();
        let evidence = evidence.join("\n").trim().to_string();
        let quoted =
            evidence.trim_matches(|c: char| c.is_whitespace() || matches!(c, '>' | '`' | '"'));
        if quoted.is_empty() || text.is_empty() {
            dropped += 1;
            continue;
        }
        let blocking = blocking && !text.to_ascii_lowercase().starts_with("possible");
        kept.push(Finding {
            blocking,
            text,
            evidence,
            location,
        });
    }
    (kept, said, dropped)
}

/// Reads `path:line` into the path and the line, `` `src/a.rs:12` `` and `src/a.rs:12-14` too (the first line counts).
fn location(text: &str) -> Option<(String, i64)> {
    let text = text.trim().trim_matches('`');
    let (path, line) = text.rsplit_once(':')?;
    let line = line.split(['-', ',']).next()?.trim().parse::<i64>().ok()?;
    let path = path.trim().trim_start_matches("./");
    (line > 0 && !path.is_empty()).then(|| (path.to_string(), line))
}

/// Reads `FINDING blocking: text` into whether it blocks and its text; another severity is a judgement call.
fn finding_severity(line: &str) -> Option<(bool, &str)> {
    let rest = line
        .get(..8)
        .filter(|p| p.eq_ignore_ascii_case("finding "))?;
    let rest = &line[rest.len()..];
    let (severity, text) = rest.split_once(':')?;
    Some((
        severity.trim().eq_ignore_ascii_case("blocking"),
        text.trim(),
    ))
}

/// The findings as lines of a note: the text, then the evidence it cites.
fn render(findings: &[&Finding]) -> String {
    findings
        .iter()
        .map(|f| {
            format!(
                "- {}\n  Evidence: {}",
                f.text,
                f.evidence.replace('\n', "\n  ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The review of one axis from the summary of its run.
fn axis(name: &'static str, summary: Option<&str>) -> AxisReview {
    let (findings, said, dropped) = read_axis(summary);
    let comments = findings
        .iter()
        .filter_map(|f| {
            let (path, line) = f.location.clone()?;
            let kind = if f.blocking {
                "Blocking"
            } else {
                "Judgement call"
            };
            Some(LineComment {
                path,
                line,
                body: format!("**{kind}:** {}", f.text),
            })
        })
        .collect();
    let blocking: Vec<_> = findings.iter().filter(|f| f.blocking).collect();
    let judgements: Vec<_> = findings.iter().filter(|f| !f.blocking).collect();
    // Work that was not reviewed is not approved.
    let (verdict, findings) = if !said {
        ("changes", format!("The {name} review gave no verdict."))
    } else if blocking.is_empty() {
        ("approve", String::new())
    } else {
        ("changes", render(&blocking))
    };
    AxisReview {
        name,
        verdict,
        findings,
        judgements: render(&judgements),
        dropped,
        comments,
    }
}

/// The review of a round from the summaries of its Standards and its Spec runs: the card goes back when either axis
/// has a blocking finding, and the note it goes back with names the axis of each.
pub(super) fn two_axes(standards: Option<&str>, spec: Option<&str>) -> Review {
    let axes = vec![axis("Standards", standards), axis("Spec", spec)];
    let section = |pick: fn(&AxisReview) -> &str| {
        axes.iter()
            .filter(|a| !pick(a).is_empty())
            .map(|a| format!("{} ({}):\n{}", a.name, a.name_axis(), pick(a)))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    let findings = section(|a| &a.findings);
    let judgements = section(|a| &a.judgements);
    let changes = axes.iter().any(|a| a.verdict == "changes");
    let comments = axes.iter().flat_map(|a| a.comments.clone()).collect();
    Review {
        verdict: if changes { "changes" } else { "approve" },
        findings,
        judgements,
        axes,
        comments,
    }
}

impl AxisReview {
    /// What the axis checks, for the headline of its part of a note.
    fn name_axis(&self) -> &'static str {
        match self.name {
            "Standards" => "does the change follow the repository's standards",
            _ => "does the change do what the issue asked",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finding_without_evidence_is_dropped_and_one_with_it_counts_by_its_severity() {
        let text = "Read it.\n\
            FINDING blocking: The new branch is never tested.\n\
            EVIDENCE: > + if retries > 3 {\n\
            FINDING blocking: It feels wrong.\n\
            FINDING judgement: The name says little.\n\
            EVIDENCE: fn do_it()\n\
            VERDICT: changes";
        let r = axis("Standards", Some(text));
        assert_eq!(r.verdict, "changes");
        assert_eq!(
            r.findings,
            "- The new branch is never tested.\n  Evidence: > + if retries > 3 {"
        );
        assert_eq!(
            r.judgements,
            "- The name says little.\n  Evidence: fn do_it()"
        );
        assert_eq!(r.dropped, 1);
    }

    #[test]
    fn a_finding_may_say_which_line_it_is_about() {
        let text = "FINDING blocking: The retry is never tested.\nFILE: src/retry.rs:42\nEVIDENCE: > + if retries > 3 {\n\
            FINDING judgement: The name says little.\nEVIDENCE: fn do_it()\nFILE: `src/a.rs:7-9`\n\
            FINDING blocking: No place.\nEVIDENCE: `x`\nFILE: not a place\nVERDICT: changes";
        let r = axis("Standards", Some(text));
        assert_eq!(r.comments.len(), 2);
        assert_eq!(
            (r.comments[0].path.as_str(), r.comments[0].line),
            ("src/retry.rs", 42)
        );
        assert!(r.comments[0].body.starts_with("**Blocking:** The retry"));
        assert_eq!(
            (r.comments[1].path.as_str(), r.comments[1].line),
            ("src/a.rs", 7)
        );
        // The line a finding names does not change what it says, or what it cites.
        assert!(r.findings.contains("Evidence: > + if retries > 3 {"));
        assert!(!r.findings.contains("src/retry.rs"));
    }

    #[test]
    fn findings_that_are_all_dropped_or_judgement_calls_approve() {
        let text =
            "FINDING blocking: Wrong.\nFINDING judgement: Maybe.\nEVIDENCE: `x`\nVERDICT: changes";
        let r = axis("Spec", Some(text));
        assert_eq!(
            (r.verdict, r.findings.as_str(), r.dropped),
            ("approve", "", 1)
        );
        assert!(r.judgements.contains("Maybe."));
    }

    #[test]
    fn a_smell_of_the_baseline_is_a_judgement_call_whatever_it_is_called() {
        let text = "FINDING blocking: Possible Feature Envy in total().\nEVIDENCE: order.items.len()\nVERDICT: approve";
        let r = axis("Standards", Some(text));
        assert_eq!(r.verdict, "approve");
        assert!(r.judgements.starts_with("- Possible Feature Envy"));
    }

    #[test]
    fn evidence_and_findings_may_run_over_several_lines_and_a_missing_verdict_asks_for_changes() {
        let text = "finding Blocking: One\nand two.\nevidence: first\nsecond\nVERDICT: approve";
        let (found, said, _) = read_axis(Some(text));
        assert!(said);
        assert_eq!(found[0].text, "One\nand two.");
        assert_eq!(found[0].evidence, "first\nsecond");

        for summary in [None, Some(""), Some("Fine."), Some("VERDICT: maybe")] {
            let r = axis("Spec", summary);
            assert_eq!(
                (r.verdict, r.findings.as_str()),
                ("changes", "The Spec review gave no verdict.")
            );
        }
    }

    #[test]
    fn the_axes_are_kept_apart_in_the_note_and_either_one_sends_the_card_back() {
        let standards = "FINDING judgement: Long.\nEVIDENCE: fn a()\nVERDICT: approve";
        let spec = "FINDING blocking: Missing the retry.\nEVIDENCE: \"retry on failure\"\nVERDICT: changes";
        let r = two_axes(Some(standards), Some(spec));
        assert_eq!(r.verdict, "changes");
        assert!(r.findings.starts_with("Spec ("), "{}", r.findings);
        assert!(!r.findings.contains("Standards"));
        assert!(r.judgements.starts_with("Standards ("));
        assert_eq!(r.axes[0].verdict, "approve");
        assert_eq!(r.axes[1].verdict, "changes");

        let r = two_axes(Some(standards), Some("VERDICT: approve"));
        assert_eq!((r.verdict, r.findings.as_str()), ("approve", ""));
        assert!(r.judgements.contains("Long."));
    }
}
