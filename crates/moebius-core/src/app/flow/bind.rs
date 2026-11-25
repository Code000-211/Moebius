//! Binding a pull request that has no closing keyword to the issue it is for (ADR 0033): by the ID of a backlog item in
//! its title or branch, when that names exactly one open issue the backlog sync made; or by `#N` in its title or
//! description, when that is an issue its author is assigned to. The result is used as if the pull request had written
//! `Fixes #N`: `App::followable` still asks that the author may write or is assigned to the issue.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::app::{App, OpenRequest, Result};
use crate::store::Repository;

static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#(\d+)\b").expect("the pattern is valid"));

/// Whether `item` stands in `text` as a word of its own: the characters around it are no letters or digits, so `K-4` is
/// not in `K-47`. Case does not matter.
fn names(text: &str, item: &str) -> bool {
    let (text, item) = (text.to_lowercase(), item.to_lowercase());
    if item.is_empty() {
        return false;
    }
    text.match_indices(&item).any(|(at, found)| {
        let before = text[..at].chars().next_back();
        let after = text[at + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

impl App {
    /// The numbers of the issues `request` names without a closing keyword: the one open issue of a backlog item whose ID
    /// its title or branch holds, else the one issue `#N` names in its title or description that its author is assigned
    /// to. Nothing when no issue, or more than one, fits.
    pub(in crate::app) fn named_issues(
        &self,
        repository: &Repository,
        request: &OpenRequest,
    ) -> Result<BTreeSet<i64>> {
        let mut by_item = BTreeSet::new();
        let place = format!("{}\n{}", request.title, request.branch);
        for (card_id, item) in self.store.backlog_cards_of(repository.id)? {
            if !names(&place, &item) {
                continue;
            }
            if let Some(card) = self.store.card(card_id)?
                && !card.closed
            {
                by_item.insert(card.number);
            }
        }
        if by_item.len() == 1 {
            return Ok(by_item);
        }
        if !by_item.is_empty() {
            // Two issues are for the same words: it is not for the server to choose.
            return Ok(BTreeSet::new());
        }
        let author = request.author.to_lowercase();
        let mut assigned = BTreeSet::new();
        let text = format!("{}\n{}", request.title, request.body);
        for found in NUMBER.captures_iter(&text) {
            let Ok(number) = found[1].parse::<i64>() else {
                continue;
            };
            let Some(card) = self.store.card_by_number(repository.id, number)? else {
                continue;
            };
            if self
                .store
                .forge_assignees(card.id)?
                .iter()
                .any(|login| login.to_lowercase() == author)
            {
                assigned.insert(number);
            }
        }
        Ok(if assigned.len() == 1 {
            assigned
        } else {
            BTreeSet::new()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::names;

    #[test]
    fn an_item_is_named_as_a_word_of_its_own() {
        assert!(names("K-47: cache the plan", "K-47"));
        assert!(names("perf/k-47-cache", "K-47"));
        assert!(names("Fix (K-47)", "k-47"));
        assert!(!names("K-471 cache", "K-47"));
        assert!(!names("XK-47", "K-47"));
        assert!(!names("K-4", "K-47"));
        assert!(!names("anything", ""));
    }
}
