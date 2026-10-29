//! The baseline of `localize`: BM25 over the files of the default branch for the issue's words, with a boost for the
//! files and identifiers the issue names literally and for the files that earlier cards with similar issues changed.

use std::collections::{BTreeMap, HashSet};

use super::text::{file_names, identifiers, terms};
use super::{Hint, KIND, Outcome};
use crate::store::{Result, Store};

/// How many files the search of the words brings in before the boosts are added.
const CANDIDATES: usize = 100;

/// What a file whose full path the issue names gains, and one whose bare name it names.
const NAMED_PATH: f64 = 1.5;
const NAMED_NAME: f64 = 1.0;

/// What a file gains for each identifier of the issue it holds, and the most it gains that way.
const NAMED_IDENTIFIER: f64 = 0.3;
const MOST_FOR_IDENTIFIERS: f64 = 0.9;

/// What a file that earlier cards changed gains, at most, from a card whose issue is as similar as can be. Cards whose
/// issues share less than [`SIMILAR`] of their words with this one count for nothing.
const SIMILARITY_WEIGHT: f64 = 1.0;
const SIMILAR: f64 = 0.15;

/// The issue to find files for.
pub struct Issue<'a> {
    pub title: &'a str,
    pub body: &'a str,
}

impl Issue<'_> {
    fn text(&self) -> String {
        format!("{}\n{}", self.title, self.body)
    }
}

/// An earlier card of the repository: the words of its issue and the files its change touched.
pub struct Past {
    pub card: Option<i64>,
    pub terms: HashSet<String>,
    pub touched: Vec<String>,
}

/// The earlier cards of the repository `repository` that have a known outcome, newest first, without the card
/// `except`.
pub fn past(store: &Store, repository: &str, except: Option<i64>) -> Result<Vec<Past>> {
    let mut past = Vec::new();
    for judgement in store.judgements_with_outcome(KIND, 300)? {
        if judgement.card_id.is_none() || judgement.card_id == except {
            continue;
        }
        let Some(outcome) = judgement
            .outcome
            .as_deref()
            .and_then(|outcome| serde_json::from_str::<Outcome>(outcome).ok())
            .filter(|outcome| !outcome.touched.is_empty())
        else {
            continue;
        };
        let Some(context) = judgement
            .context
            .first()
            .and_then(|blob| store.blobs().get(blob).ok())
            .and_then(|text| serde_json::from_str::<BTreeMap<String, String>>(&text).ok())
        else {
            continue;
        };
        if context.get("repository").map(String::as_str) != Some(repository) {
            continue;
        }
        let text = format!(
            "{}\n{}",
            context.get("title").map_or("", String::as_str),
            context.get("body").map_or("", String::as_str)
        );
        past.push(Past {
            card: judgement.card_id,
            terms: terms(&text).into_iter().collect(),
            touched: outcome.touched,
        });
    }
    Ok(past)
}

/// What a file has earned, and why.
#[derive(Default)]
struct Earned {
    score: f64,
    named: bool,
    identifiers: Vec<String>,
    card: Option<i64>,
    text: bool,
}

impl Earned {
    fn reason(&self) -> String {
        let mut reasons = Vec::new();
        if self.named {
            reasons.push("named in the issue".to_string());
        }
        if !self.identifiers.is_empty() {
            let shown: Vec<_> = self
                .identifiers
                .iter()
                .take(2)
                .map(|name| format!("`{name}`"))
                .collect();
            reasons.push(format!("holds {}", shown.join(", ")));
        }
        if let Some(card) = self.card {
            reasons.push(format!("changed by card {card}, whose issue is similar"));
        }
        if self.text {
            reasons.push("its text matches the issue".to_string());
        }
        reasons.join("; ")
    }
}

/// The files whose text best matches the words `issue_terms`, best first, with their BM25 score.
fn search(
    store: &Store,
    repository_id: i64,
    issue_terms: &[String],
    limit: usize,
) -> Result<Vec<(String, f64)>> {
    if issue_terms.is_empty() {
        return Ok(Vec::new());
    }
    let query = issue_terms
        .iter()
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>()
        .join(" OR ");
    store.localize_search(repository_id, &query, limit)
}

/// The `limit` files whose text best matches the issue, best first: what a decider with no index of its own is shown.
pub fn candidates(
    store: &Store,
    repository_id: i64,
    issue: &Issue,
    limit: usize,
) -> Result<Vec<String>> {
    let found = search(store, repository_id, &terms(&issue.text()), limit)?;
    Ok(found.into_iter().map(|(path, _)| path).collect())
}

/// The files of the repository `repository_id` most likely to be touched by the change that settles `issue`, best
/// first, at most `limit`, each with why. Files the index does not hold are never named.
pub fn rank(
    store: &Store,
    repository_id: i64,
    issue: &Issue,
    past: &[Past],
    limit: usize,
) -> Result<Vec<Hint>> {
    let text = issue.text();
    let issue_terms = terms(&text);
    let mut earned: BTreeMap<String, Earned> = BTreeMap::new();

    let found = search(store, repository_id, &issue_terms, CANDIDATES)?;
    let best = found.iter().map(|(_, score)| *score).fold(0.0, f64::max);
    for (path, score) in found {
        if best > 0.0 && score > 0.0 {
            let file = earned.entry(path).or_default();
            file.score += score / best;
            file.text = true;
        }
    }

    let named = file_names(&text);
    if !named.is_empty() {
        for path in store.localize_paths(repository_id)? {
            let lower = path.to_lowercase();
            let name = lower.rsplit('/').next().unwrap_or(&lower);
            let boost = named
                .iter()
                .map(|named| {
                    if *named == lower
                        || (named.contains('/') && lower.ends_with(&format!("/{named}")))
                    {
                        NAMED_PATH
                    } else if !named.contains('/') && named == name {
                        NAMED_NAME
                    } else {
                        0.0
                    }
                })
                .fold(0.0, f64::max);
            if boost > 0.0 {
                let file = earned.entry(path).or_default();
                file.score += boost;
                file.named = true;
            }
        }
    }

    for identifier in identifiers(&text) {
        for (path, _) in
            store.localize_search(repository_id, &format!("\"{identifier}\""), CANDIDATES)?
        {
            let file = earned.entry(path).or_default();
            if file.identifiers.len() as f64 * NAMED_IDENTIFIER < MOST_FOR_IDENTIFIERS {
                file.score += NAMED_IDENTIFIER;
                file.identifiers.push(identifier.clone());
            }
        }
    }

    let words: HashSet<&String> = issue_terms.iter().collect();
    if !words.is_empty() {
        let known: HashSet<String> = store.localize_paths(repository_id)?.into_iter().collect();
        for earlier in past {
            let shared = earlier
                .terms
                .iter()
                .filter(|term| words.contains(term))
                .count();
            let union = words.len() + earlier.terms.len() - shared;
            let similarity = if union == 0 {
                0.0
            } else {
                shared as f64 / union as f64
            };
            if similarity < SIMILAR {
                continue;
            }
            for path in earlier.touched.iter().filter(|path| known.contains(*path)) {
                let file = earned.entry(path.clone()).or_default();
                file.score += similarity * SIMILARITY_WEIGHT;
                file.card = file.card.or(earlier.card);
            }
        }
    }

    let mut ranked: Vec<(String, Earned)> = earned
        .into_iter()
        .filter(|(_, file)| file.score > 0.0)
        .collect();
    ranked.sort_by(|a, b| b.1.score.total_cmp(&a.1.score).then_with(|| a.0.cmp(&b.0)));
    Ok(ranked
        .into_iter()
        .take(limit)
        .map(|(path, file)| Hint {
            reason: file.reason(),
            path,
        })
        .collect())
}
