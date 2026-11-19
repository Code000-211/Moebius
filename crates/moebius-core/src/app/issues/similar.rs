//! The words of an issue, for finding the open issues that probably say the same: what the Moebius agent offers when an
//! issue is made. It compares the words of titles and bodies (lower case, without short and common words, a plural
//! as its singular), so it finds what a person would call the same issue worded alike, and offers it; it never decides.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::state_of;
use crate::store::Card;

/// How much of its words an issue shares with another (shared over all of both) for the other to be offered.
const THRESHOLD: f64 = 0.5;

/// The most issues offered for one.
pub const OFFERED: usize = 5;

/// Words that say nothing about what an issue is about.
const COMMON: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "are", "was", "were", "not", "but", "from",
    "into", "when", "then", "than", "has", "have", "had", "its", "can", "you", "your", "our",
    "should", "would", "could", "will", "does", "doesn", "don", "too", "all", "any", "out", "one",
    "get", "got", "gets",
];

/// An open issue that is likely the same as another, with how alike they are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Similar {
    pub id: i64,
    pub number: i64,
    pub title: String,
    /// `open`; only open issues are offered.
    pub state: String,
    /// From 0.5 to 1: the share of their words the two have in common.
    pub score: f64,
}

/// The words of `text` that tell issues apart.
fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .map(|word| match word.strip_suffix('s') {
            Some(single) if word.chars().count() > 3 && !word.ends_with("ss") => single.to_string(),
            _ => word,
        })
        .filter(|word| word.chars().count() >= 3 && !COMMON.contains(&word.as_str()))
        .collect()
}

/// The share of the words of `a` and `b` that both have, and how many that is.
fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>) -> (f64, usize) {
    let shared = a.intersection(b).count();
    let all = a.union(b).count();
    if all == 0 {
        (0.0, 0)
    } else {
        (shared as f64 / all as f64, shared)
    }
}

/// How alike two issues, each a title and a body, are, from 0 to 1, or nothing when they are not alike enough: by the
/// words of their titles, two of them at least (one when both titles are that one word), or by the words of titles and
/// bodies together.
pub(in crate::app) fn likeness(a: (&str, &str), b: (&str, &str)) -> Option<f64> {
    let (a_title, b_title) = (words(a.0), words(b.0));
    let (title, shared) = overlap(&a_title, &b_title);
    let title = if shared >= 2 || (shared == 1 && a_title.len() == 1 && b_title.len() == 1) {
        title
    } else {
        0.0
    };
    let all = |(title, body): (&str, &str)| words(&format!("{title} {body}"));
    let (text, shared) = overlap(&all(a), &all(b));
    let text = if shared >= 2 { text } else { 0.0 };
    let score = title.max(text);
    (score >= THRESHOLD).then_some(score)
}

/// The open issues of `others` that are likely the same as `card`, the most alike first (then the oldest), at most
/// [`OFFERED`].
pub(super) fn likely(card: &Card, others: Vec<Card>) -> Vec<Similar> {
    let mut found: Vec<Similar> = others
        .into_iter()
        .filter(|other| other.id != card.id && state_of(other) == "open")
        .filter_map(|other| {
            likeness((&card.title, &card.body), (&other.title, &other.body)).map(|score| Similar {
                id: other.id,
                number: other.number,
                title: other.title.clone(),
                state: "open".into(),
                score: (score * 100.0).round() / 100.0,
            })
        })
        .collect();
    found.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.number.cmp(&b.number)));
    found.truncate(OFFERED);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_drop_case_short_and_common_words_and_plurals() {
        assert_eq!(
            words("Cache the Model downloads, to be fast!"),
            BTreeSet::from(["cache", "model", "download", "fast"].map(String::from))
        );
    }

    #[test]
    fn an_issue_with_the_words_of_another_is_alike_it_and_an_unlike_one_is_not() {
        let new = ("Cache model downloads", "");
        assert_eq!(
            likeness(new, ("Cache the model downloads", "Downloads are slow.")),
            Some(1.0)
        );
        assert_eq!(
            likeness(new, ("Dark theme for the settings page", "")),
            None
        );
        // One word in common is not enough, in the title or in the text.
        assert_eq!(likeness(new, ("Model", "Something else entirely")), None);
        assert_eq!(likeness(("Retro page", ""), ("Retro", "")), None);
        assert_eq!(likeness(("Login", ""), ("login", "")), Some(1.0));
    }

    #[test]
    fn a_body_that_says_the_same_makes_two_titles_alike() {
        let body = "The nightly retro fails with a locked database when two chats write";
        assert!(likeness(("Fails", body), ("Retro broken", body)).is_some());
    }
}
