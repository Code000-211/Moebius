//! The structured review (ADR 0029): how the Reviewer reports on a pull request someone else opened, so that the server
//! can write it on the forge as a review with comments on lines. The answer ends with one JSON object in a fenced
//! block:
//!
//! ````text
//! ```json
//! {"verdict": "changes", "summary": "One bug.", "comments": [{"path": "src/a.rs", "line": 12, "body": "..."}]}
//! ```
//! ````
//!
//! `verdict` is `approve` or `changes`; `summary` is the review's body; each comment names a file as the pull request
//! has it and a line of the file after the change. Anything that does not read as that is no verdict: work that was
//! not reviewed is not approved.

use serde::Deserialize;

use crate::store::ReviewComment;

/// What the Reviewer said, read from the end of its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structured {
    /// `approve` or `changes`.
    pub verdict: &'static str,
    pub summary: String,
    pub comments: Vec<ReviewComment>,
}

#[derive(Deserialize)]
struct Said {
    verdict: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    comments: Vec<Comment>,
}

#[derive(Deserialize)]
struct Comment {
    path: String,
    line: i64,
    body: String,
}

/// The last fenced `json` block of `text` when it ends the answer (nothing but blank lines after it), or else the last
/// line that starts a JSON object and runs to the end. A block quoted earlier, with more text after it, is no verdict.
fn last_object(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found: Option<(String, usize)> = None;
    let mut open: Option<Vec<&str>> = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        match (&mut open, trimmed) {
            (None, t) if t.starts_with("```json") || t == "```JSON" => open = Some(Vec::new()),
            (Some(body), "```") => {
                found = Some((body.join("\n"), i));
                open = None;
            }
            (Some(body), _) => body.push(line),
            _ => {}
        }
    }
    if let Some((body, closed)) = found {
        let ends_the_answer = lines[closed + 1..].iter().all(|l| l.trim().is_empty());
        return ends_the_answer.then_some(body);
    }
    (0..lines.len())
        .rev()
        .find(|&i| lines[i].trim_start().starts_with('{'))
        .map(|i| lines[i..].join("\n"))
}

/// The structured review at the end of `answer`: none when there is no JSON object, it is not a verdict of `approve`
/// or `changes`, a verdict of `changes` says nothing (no summary and no comment), or a comment is not on a file and
/// a line of it. Comments with an empty body are dropped, and so are the ones past the first 50.
pub fn read(answer: Option<&str>) -> Option<Structured> {
    let object = last_object(answer?)?;
    let said: Said = serde_json::from_str(object.trim()).ok()?;
    let verdict = match said.verdict.trim().to_ascii_lowercase().as_str() {
        "approve" => "approve",
        "changes" => "changes",
        _ => return None,
    };
    let mut comments = Vec::new();
    for comment in said.comments {
        if comment.body.trim().is_empty() {
            continue;
        }
        if comment.path.trim().is_empty() || comment.line < 1 {
            return None;
        }
        comments.push(ReviewComment {
            path: comment.path.trim().trim_start_matches("./").to_string(),
            line: comment.line,
            body: comment.body.trim().to_string(),
        });
    }
    comments.truncate(50);
    let summary = said.summary.trim().to_string();
    if verdict == "changes" && summary.is_empty() && comments.is_empty() {
        return None;
    }
    Some(Structured {
        verdict,
        summary,
        comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_fenced_object_is_the_verdict() {
        let answer =
            "Looked at it.\n```json\n{\"verdict\": \"approve\", \"summary\": \"Fine.\"}\n```\n";
        let read = read(Some(answer)).unwrap();
        assert_eq!(read.verdict, "approve");
        assert_eq!(read.summary, "Fine.");
        assert!(read.comments.is_empty());
        let later = "```json\n{\"verdict\": \"approve\"}\n```\nOn second thought:\n```json\n{\"verdict\": \"changes\", \"summary\": \"No.\"}\n```";
        assert_eq!(super::read(Some(later)).unwrap().verdict, "changes");
        // A bare object on the last lines counts too.
        assert_eq!(
            super::read(Some("Fine.\n{\"verdict\": \"approve\"}"))
                .unwrap()
                .verdict,
            "approve"
        );
    }

    #[test]
    fn comments_name_a_file_and_a_line_of_it() {
        let answer = r#"```json
{"verdict": "changes", "summary": "Two things.", "comments": [
  {"path": "./src/a.rs", "line": 12, "body": " Off by one. "},
  {"path": "src/b.rs", "line": 3, "body": ""}]}
```"#;
        let read = read(Some(answer)).unwrap();
        assert_eq!(
            read.comments,
            vec![ReviewComment {
                path: "src/a.rs".into(),
                line: 12,
                body: "Off by one.".into()
            }]
        );
        // A comment that names no line is no review at all.
        for bad in [
            r#"{"verdict": "changes", "summary": "x", "comments": [{"path": "a", "line": 0, "body": "b"}]}"#,
            r#"{"verdict": "changes", "summary": "x", "comments": [{"path": "", "line": 2, "body": "b"}]}"#,
        ] {
            assert!(
                super::read(Some(&format!("```json\n{bad}\n```"))).is_none(),
                "{bad}"
            );
        }
    }

    #[test]
    fn what_is_not_a_verdict_is_none() {
        for answer in [
            None,
            Some(""),
            Some("Fine.\nVERDICT: approve"),
            Some("```json\n{\"verdict\": \"maybe\"}\n```"),
            Some("```json\nnot json\n```"),
            // Changes without a word about what.
            Some("```json\n{\"verdict\": \"changes\"}\n```"),
        ] {
            assert!(read(answer).is_none(), "{answer:?}");
        }
    }
}
