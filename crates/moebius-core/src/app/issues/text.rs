//! The lines of an issue's description that hold what a forge cannot: "Part of #n" for a parent (GitLab and Forgejo have
//! no sub-issues) and "Blocked by #n" for a blocker where the forge does not keep blocking. They are read, written and
//! removed here. Anything inside a code fence is an example and not a line of the board's, so it is neither read as one
//! nor changed.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

/// The lines of `body` with their line endings, and whether each is in a code fence (the fence's own lines included).
fn lines(body: &str) -> Vec<(&str, bool)> {
    let mut fenced = false;
    body.split_inclusive('\n')
        .map(|line| {
            let trimmed = line.trim_start();
            let fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
            let inside = fenced || fence;
            if fence {
                fenced = !fenced;
            }
            (line, inside)
        })
        .collect()
}

fn parent_line() -> &'static Regex {
    static PART: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^[ \t]*part of:?[ \t]*#(\d+)[ \t]*$").unwrap());
    &PART
}

/// The issue number a body names as its parent: a line "Part of #12" that is not in a code fence.
pub(in crate::app) fn part_of(body: &str) -> Option<i64> {
    lines(body)
        .into_iter()
        .filter(|(_, fenced)| !fenced)
        .find_map(|(line, _)| {
            parent_line()
                .captures(line.trim_end_matches(['\r', '\n']))
                .and_then(|c| c[1].parse().ok())
        })
}

/// `body` with its "Part of #n" line replaced by one for `parent`, or removed when there is none.
pub(in crate::app) fn with_parent_line(body: &str, parent: Option<i64>) -> String {
    let kept: String = lines(body)
        .into_iter()
        .filter(|(line, fenced)| {
            *fenced || !parent_line().is_match(line.trim_end_matches(['\r', '\n']))
        })
        .map(|(line, _)| line)
        .collect();
    let rest = kept.trim_end();
    match parent {
        Some(parent) if rest.is_empty() => format!("Part of #{parent}"),
        Some(parent) => format!("{rest}\n\nPart of #{parent}"),
        None => rest.to_string(),
    }
}

fn blocked_line() -> &'static Regex {
    static BLOCKED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)blocked by:?[ \t]*((?:#\d+[ \t,]*)+)").unwrap());
    &BLOCKED
}

/// `body` with the issue `number` taken out of what it says blocks it ("Blocked by #3, #4" becomes "Blocked by #4", and a
/// line that names only it goes), outside code fences.
pub(in crate::app) fn without_blocker(body: &str, number: i64) -> String {
    let mut out = String::new();
    for (line, fenced) in lines(body) {
        if fenced {
            out.push_str(line);
            continue;
        }
        let ending = &line[line.trim_end_matches(['\r', '\n']).len()..];
        let text = line.trim_end_matches(['\r', '\n']);
        let rewritten = blocked_line().replace_all(text, |c: &regex::Captures| {
            let rest: Vec<String> = numbers(&c[1])
                .into_iter()
                .filter(|n| *n != number)
                .map(|n| format!("#{n}"))
                .collect();
            if rest.is_empty() {
                String::new()
            } else {
                format!("Blocked by {}", rest.join(", "))
            }
        });
        if rewritten.trim().is_empty() && !text.trim().is_empty() {
            continue;
        }
        out.push_str(&rewritten);
        out.push_str(ending);
    }
    out.trim_end().to_string()
}

fn numbers(list: &str) -> Vec<i64> {
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"#(\d+)").unwrap());
    let mut seen = BTreeSet::new();
    NUMBER
        .captures_iter(list)
        .filter_map(|c| c[1].parse().ok())
        .filter(|n| seen.insert(*n))
        .collect()
}

/// `body` with a line "Blocked by #n" added for the issue `number`, unless it says so already.
pub(in crate::app) fn with_blocker_line(body: &str, number: i64) -> String {
    if crate::app::cards::blocked_by(body).contains(&number) {
        return body.to_string();
    }
    let rest = body.trim_end();
    let last_line = rest.rsplit('\n').next().unwrap_or("");
    match rest {
        "" => format!("Blocked by #{number}"),
        _ if last_line
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("blocked by") =>
        {
            format!("{rest}\nBlocked by #{number}")
        }
        _ => format!("{rest}\n\nBlocked by #{number}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parent_line_is_read_and_written_once() {
        assert_eq!(part_of("Do it.\n\nPart of #12"), Some(12));
        assert_eq!(part_of("part of: #3\nmore"), Some(3));
        assert_eq!(part_of("This is part of #4 of the work"), None);
        assert_eq!(with_parent_line("Do it.", Some(2)), "Do it.\n\nPart of #2");
        assert_eq!(
            with_parent_line("Do it.\n\nPart of #2", Some(5)),
            "Do it.\n\nPart of #5"
        );
        assert_eq!(with_parent_line("Do it.\n\nPart of #2", None), "Do it.");
        assert_eq!(with_parent_line("", Some(7)), "Part of #7");
    }

    #[test]
    fn a_parent_line_in_a_code_fence_is_an_example_and_is_left_alone() {
        let body = "Write it like this:\n\n```\nPart of #9\n```\n\nPart of #2";
        assert_eq!(part_of(body), Some(2));
        assert_eq!(part_of("```\nPart of #9\n```"), None);
        assert_eq!(
            with_parent_line(body, Some(3)),
            "Write it like this:\n\n```\nPart of #9\n```\n\nPart of #3"
        );
        assert_eq!(
            with_parent_line(body, None),
            "Write it like this:\n\n```\nPart of #9\n```"
        );
        assert_eq!(
            with_parent_line("~~~\nPart of #9\n~~~", Some(1)),
            "~~~\nPart of #9\n~~~\n\nPart of #1"
        );
    }

    #[test]
    fn a_blocker_line_is_added_once_and_taken_out_again() {
        assert_eq!(with_blocker_line("", 4), "Blocked by #4");
        assert_eq!(with_blocker_line("Text.", 4), "Text.\n\nBlocked by #4");
        assert_eq!(
            with_blocker_line("Text.\n\nBlocked by #3", 4),
            "Text.\n\nBlocked by #3\nBlocked by #4"
        );
        assert_eq!(with_blocker_line("Blocked by: #4", 4), "Blocked by: #4");

        assert_eq!(
            without_blocker("Text.\n\nBlocked by #3, #4\nMore.", 4),
            "Text.\n\nBlocked by #3\nMore."
        );
        assert_eq!(without_blocker("Text.\n\nBlocked by #4", 4), "Text.");
        assert_eq!(
            without_blocker("Blocked by #3\nBlocked by #4", 4),
            "Blocked by #3"
        );
        assert_eq!(without_blocker("Text.", 4), "Text.");
        // An example in a fence stays.
        assert_eq!(
            without_blocker("```\nBlocked by #4\n```", 4),
            "```\nBlocked by #4\n```"
        );
    }
}
