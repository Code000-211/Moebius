//! A card's priority and due date. They are kept as labels of the card, `priority:urgent`, `priority:high`,
//! `priority:medium` or `priority:low`, and `due:YYYY-MM-DD`, so the forge shows and sets them too and a poll that
//! brings its labels brings them. This module reads them out of a list of labels and writes them back into one; it
//! keeps nothing of its own.

use serde::{Deserialize, Serialize};

/// The prefix of the label that holds a card's priority.
const PRIORITY_PREFIX: &str = "priority:";
/// The prefix of the label that holds a card's due date.
const DUE_PREFIX: &str = "due:";

/// How soon a card should be done. The order is the order of urgency: `Urgent` first, `None` last.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Urgent,
    High,
    Medium,
    Low,
    #[default]
    None,
}

impl Priority {
    /// The name of the priority as scripts and the API have it.
    pub fn name(self) -> &'static str {
        match self {
            Priority::Urgent => "urgent",
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
            Priority::None => "none",
        }
    }

    /// The priority called `name`, in any case; `None` for a name that is not one.
    pub fn parse(name: &str) -> Option<Priority> {
        [
            Priority::Urgent,
            Priority::High,
            Priority::Medium,
            Priority::Low,
            Priority::None,
        ]
        .into_iter()
        .find(|p| p.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// Whether `text` is a date as `YYYY-MM-DD`, with a month and a day that exist (leap years included).
pub fn is_date(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = &text[range];
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// The part of `label` after `prefix`, if it starts with it in any case. A label that has a multi-byte character where
/// the prefix ends is no match, not a panic.
fn after_prefix<'a>(label: &'a str, prefix: &str) -> Option<&'a str> {
    let label = label.trim();
    let head = label.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| label[prefix.len()..].trim())
}

/// The value of the label of `labels` that starts with `prefix`, in any case.
fn values<'a>(labels: &'a [String], prefix: &'a str) -> impl Iterator<Item = &'a str> {
    labels
        .iter()
        .filter_map(move |label| after_prefix(label, prefix))
}

/// The priority the labels say: the most urgent of the `priority:` labels that name one, else none.
pub fn priority_of(labels: &[String]) -> Priority {
    values(labels, PRIORITY_PREFIX)
        .filter_map(Priority::parse)
        .min()
        .unwrap_or(Priority::None)
}

/// The due date the labels say, the earliest of the `due:` labels that hold a date, else none.
pub fn due_of(labels: &[String]) -> Option<String> {
    values(labels, DUE_PREFIX)
        .filter(|v| is_date(v))
        .min()
        .map(String::from)
}

/// `labels` with every `priority:` label replaced by the one for `priority` (none for [`Priority::None`]).
pub fn with_priority(labels: &[String], priority: Priority) -> Vec<String> {
    let mut labels = without(labels, PRIORITY_PREFIX);
    if priority != Priority::None {
        labels.push(format!("{PRIORITY_PREFIX}{}", priority.name()));
    }
    labels
}

/// `labels` with every `due:` label replaced by the one for `due` (none for `None`). `due` must be a date.
pub fn with_due(labels: &[String], due: Option<&str>) -> Vec<String> {
    let mut labels = without(labels, DUE_PREFIX);
    if let Some(due) = due {
        labels.push(format!("{DUE_PREFIX}{due}"));
    }
    labels
}

fn without(labels: &[String], prefix: &str) -> Vec<String> {
    labels
        .iter()
        .filter(|l| after_prefix(l, prefix).is_none())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_most_urgent_priority_label_and_the_earliest_due_label_win() {
        assert_eq!(priority_of(&labels(&["bug"])), Priority::None);
        assert_eq!(
            priority_of(&labels(&["Priority:Low", "priority:high", "priority:nope"])),
            Priority::High
        );
        assert_eq!(due_of(&labels(&["due:soon", "bug"])), None);
        assert_eq!(
            due_of(&labels(&[
                "due:2026-12-02",
                "DUE:2026-12-01",
                "due:2026-02-30"
            ])),
            Some("2026-12-01".into())
        );
    }

    #[test]
    fn writing_replaces_the_old_label_and_keeps_the_others() {
        let l = labels(&["bug", "priority:low", "Priority:high"]);
        assert_eq!(
            with_priority(&l, Priority::Urgent),
            ["bug", "priority:urgent"]
        );
        assert_eq!(with_priority(&l, Priority::None), ["bug"]);
        assert_eq!(
            with_due(&labels(&["bug", "due:2026-01-01"]), Some("2026-03-04")),
            ["bug", "due:2026-03-04"]
        );
        assert_eq!(
            with_due(&labels(&["due:2026-01-01"]), None),
            Vec::<String>::new()
        );
    }

    #[test]
    fn only_real_dates_are_dates() {
        for ok in ["2026-12-31", "2024-02-29", "2000-02-29"] {
            assert!(is_date(ok), "{ok}");
        }
        for bad in [
            "2026-02-29",
            "1900-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-1-01",
            "tomorrow",
            "2026-12-3x",
            "",
            "+026-12-31",
        ] {
            assert!(!is_date(bad), "{bad}");
        }
    }

    #[test]
    fn a_label_with_multibyte_characters_is_just_a_label() {
        // Ten bytes: byte 9 and byte 4 fall inside a character.
        let l = labels(&["ééééé", "日本語日本語", "priority:high"]);
        assert_eq!(priority_of(&l), Priority::High);
        assert_eq!(due_of(&l), None);
        assert_eq!(
            with_priority(&l, Priority::Low),
            ["ééééé", "日本語日本語", "priority:low"]
        );
        assert_eq!(
            with_due(&l, Some("2026-01-02")),
            ["ééééé", "日本語日本語", "priority:high", "due:2026-01-02"]
        );
    }
}
