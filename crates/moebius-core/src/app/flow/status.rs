//! Status labels on the forge (ADR 0033). So that people who do not use Moebius see where an issue stands on the forge
//! itself, a repository with `status_labels` on has exactly one label that starts with its `status_prefix` (default
//! `status:`) on every open issue that has a card. It is looked at after each poll, so a move, a review or a freeze shows
//! within a poll, and a person who removes the label by hand has it put back at the next.
//!
//! The label is the state's name after the prefix. The first of these that holds is the state:
//!
//! 1. `frozen`: the repository is frozen;
//! 2. `awaiting-measurement`: the Reviewer approved the code of the pull request's current head and a measurement is missing;
//! 3. `changes-requested`: the Reviewer's last review of the current head asked for changes;
//! 4. `taken`: somebody took the issue and no pull request is followed yet;
//! 5. else the card's column, as a slug: `in-review`, `ready-to-merge`, and for a custom column of the project
//!    (ADR 0030) its own, such as `ready-for-testing`.
//!
//! The swap is one write of the issue's labels: the new status label in, the old ones out, the others as they were. A label
//! that is missing on the forge is made with a colour fixed for its state. A closed issue is left as it was, with the last
//! label it had; nothing is written for it. The labels of the followed pull request are not touched.

use super::{App, Result};
use crate::store::{Card, Repository};

/// The colour of the label of a state: the finer states have their own, a column's label is grey.
fn colour(state: &str) -> &'static str {
    match state {
        "frozen" => "5319e7",
        "awaiting-measurement" => "fbca04",
        "changes-requested" => "d93f0b",
        "taken" => "0e8a16",
        _ => "ededed",
    }
}

/// A column's name as a label: lower case, every run of other characters a hyphen.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

impl App {
    /// The state of the card's issue, as the module says.
    fn status_of(&self, card: &Card) -> Result<String> {
        if self.frozen(card.repository_id)?.is_some() {
            return Ok("frozen".into());
        }
        let followed = self.followed_change(card)?;
        if let Some(change) = &followed
            && let Some(round) = self
                .store
                .external_reviews(change.id)?
                .into_iter()
                .rev()
                .find(|r| r.kind != "person" && r.head == change.head.clone().unwrap_or_default())
        {
            if round.state == "awaiting_measurement" {
                return Ok("awaiting-measurement".into());
            }
            if round.verdict.as_deref() == Some("changes") && round.state == "posted" {
                return Ok("changes-requested".into());
            }
        }
        if card.taken_by.is_some() && followed.is_none() {
            return Ok("taken".into());
        }
        Ok(slug(&card.column))
    }

    /// Keeps one status label on each open issue of `repository` that has a card, when the repository asks for them. A
    /// forge that refuses a write is reported with the forge's other errors and tried again at the next poll.
    pub fn sync_status_labels(&self, repository: &Repository) -> Result<()> {
        let flow = self.store.flow(repository.id)?;
        if !flow.status_labels || repository.forge == "none" || repository.files_of.is_some() {
            return Ok(());
        }
        let prefix = flow.status_prefix.to_lowercase();
        for card in self.store.cards(repository.id)? {
            if card.closed {
                continue;
            }
            let state = self.status_of(&card)?;
            if state.is_empty() {
                continue;
            }
            let wanted = format!("{}{state}", flow.status_prefix);
            let old: Vec<&String> = card
                .labels
                .iter()
                .filter(|l| l.to_lowercase().starts_with(&prefix) && **l != wanted)
                .collect();
            let has = card.labels.iter().any(|l| *l == wanted);
            if has && old.is_empty() {
                continue;
            }
            let report = |what: &str, e: &dyn std::fmt::Display| {
                self.bus.publish(crate::bus::Event::ForgeError {
                    repository: repository.name.clone(),
                    message: format!("#{}: the status label {what}: {e}", card.number),
                });
            };
            if !has
                && let Err(e) =
                    self.forge(repository)?
                        .ensure_label(repository, &wanted, colour(&state))
            {
                report("could not be made", &e);
            }
            let mut changes = vec![(wanted, true)];
            changes.extend(old.into_iter().map(|l| (l.clone(), false)));
            if let Err(e) = self.set_labels_for_script(card.id, &changes) {
                report("could not be written", &e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn a_column_is_a_label_in_lower_case_with_hyphens() {
        assert_eq!(slug("Ready for testing"), "ready-for-testing");
        assert_eq!(slug("Not started"), "not-started");
        assert_eq!(slug("Being measured!"), "being-measured");
        assert_eq!(slug("  In  review "), "in-review");
    }
}
