//! The daily digest (ADR 0032): once a day, from `digest_at` on, one inbox item tells the owner what happened since the
//! last one, repository by repository: what merged and who wrote it, which merges were refused, what was reverted,
//! which review rounds ran and how they ended, who took or released an issue, what the stale handling did, what was
//! spent against the budget, what is frozen or held, and what waits for a person, grouped by reason with links.
//!
//! The day's digest is claimed in the store before anything is done, so one day has one however often the clock looks
//! and whether or not the server restarts. A server that was down at `digest_at` makes it when it is up again the same
//! day; a day it missed is not made up, and the next digest reports from the last one on.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Value, json};

use super::{App, Result};
use crate::bus::Event;
use crate::config::TimeOfDay;
use crate::store::{Activity, InboxItem, Repository, now};

/// The kind of the inbox item a digest is.
pub const DIGEST_ITEM: &str = "daily_digest";

/// How many lines of one kind a repository's section lists before it says how many more there were.
const LIST_LIMIT: usize = 8;

/// A line about an issue or a pull request.
#[derive(Debug, Clone, Default, Serialize)]
struct Line {
    /// The card the line is about, so that a person who may not see it is not told of it.
    card: Option<i64>,
    number: Option<i64>,
    title: String,
    who: Option<String>,
    detail: String,
    measured: bool,
}

/// What one repository's section holds, by kind.
#[derive(Debug, Default, Serialize)]
struct Section {
    id: i64,
    name: String,
    merged: Vec<Line>,
    refused: Vec<Line>,
    reverts: Vec<Line>,
    review_runs: i64,
    rounds: Vec<Line>,
    taken: Vec<Line>,
    released: Vec<Line>,
    upkeep: Vec<Line>,
    frozen: Vec<Line>,
    /// How many lines were left out of each list, by the list's name.
    more: BTreeMap<String, usize>,
}

impl Section {
    fn is_empty(&self) -> bool {
        self.merged.is_empty()
            && self.refused.is_empty()
            && self.reverts.is_empty()
            && self.review_runs == 0
            && self.rounds.is_empty()
            && self.taken.is_empty()
            && self.released.is_empty()
            && self.upkeep.is_empty()
            && self.frozen.is_empty()
    }

    /// Cuts every list to [`LIST_LIMIT`] lines and remembers how many it cut.
    fn trim(&mut self) {
        let mut cut = |name: &str, list: &mut Vec<Line>| {
            if list.len() > LIST_LIMIT {
                self.more.insert(name.to_string(), list.len() - LIST_LIMIT);
                list.truncate(LIST_LIMIT);
            }
        };
        cut("merged", &mut self.merged);
        cut("refused", &mut self.refused);
        cut("reverts", &mut self.reverts);
        cut("rounds", &mut self.rounds);
        cut("taken", &mut self.taken);
        cut("released", &mut self.released);
        cut("upkeep", &mut self.upkeep);
        cut("frozen", &mut self.frozen);
    }
}

/// The words for what an inbox item of `kind` waits for.
fn reason_of(kind: &str) -> String {
    match kind {
        "merge_stopped" => "A merge stopped".into(),
        "ready_to_merge" => "Ready to merge".into(),
        "done_asked" => "A helper says an issue is done".into(),
        "outside_change" => "A pull request from outside".into(),
        "review_not_possible" => "A review could not run".into(),
        "change_closed" => "A change was closed".into(),
        "intake_request" => "A request to approve".into(),
        "workflow_approval" => "A workflow waits for you".into(),
        other => {
            let spaced = other.replace('_', " ");
            let mut chars = spaced.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    }
}

impl App {
    /// The schedule's check, once a minute: makes today's digest when `digest_at` has passed and today has none.
    /// `day_tokens` is the budget a day's tokens are told against.
    pub fn on_digest_schedule(&self, digest_at: TimeOfDay, day_tokens: Option<i64>) -> Result<()> {
        let day = self.store.today()?;
        let now = TimeOfDay(self.store.minutes_into_the_day()?);
        // The first day has no digest: nothing before the server's first start is told, so a server that starts after
        // `digest_at` makes none that day, and a period with nothing in it is not written at all (see `make_digest`).
        if now.0 < digest_at.0 || self.store.digest_of(&day)?.is_some() || self.store.first_day()? {
            return Ok(());
        }
        self.make_digest(&day, day_tokens).map(drop)
    }

    /// Makes the digest of `day` if it has none: the stale handling first, so that what it did is in it, then the
    /// inbox item. Says which item, or none when the day had a digest already.
    pub fn make_digest(&self, day: &str, day_tokens: Option<i64>) -> Result<Option<InboxItem>> {
        // Never from before the first start.
        let since = self
            .store
            .last_digest_at()?
            .unwrap_or(now() - 86_400)
            .max(self.store.digest_epoch()?);
        if !self.store.claim_digest(day, since)? {
            return Ok(None);
        }
        // What it does is written to the log, which the digest reads; one that fails is said and the digest still comes.
        if let Err(e) = self.run_upkeep() {
            self.bus.publish(Event::RetroError {
                message: format!("the stale handling failed: {e}"),
            });
        }
        // One second past now, so that what happened in this second is in this digest and not the next.
        let until = now() + 1;
        let upto = (self.store.last_activity()?, self.store.last_revert()?);
        let (data, text) = self.compose_digest(day, (since, until), upto, day_tokens)?;
        let needs = data["needs_a_person"]
            .as_array()
            .map(|groups| {
                groups
                    .iter()
                    .map(|g| g["items"].as_array().map_or(0, Vec::len))
                    .sum::<usize>()
            })
            .unwrap_or(0);
        let merged: usize = data["repositories"]
            .as_array()
            .map(|all| {
                all.iter()
                    .map(|r| r["merged"].as_array().map_or(0, Vec::len))
                    .sum()
            })
            .unwrap_or(0);
        // No figures in the title: what a person sees of the digest depends on the projects they see.
        let _ = (merged, needs);
        // A period with nothing in it is not written: the owner is not told that nothing happened.
        if data["repositories"].as_array().is_none_or(Vec::is_empty)
            && data["needs_a_person"].as_array().is_none_or(Vec::is_empty)
            && data["tokens"]["spent"].as_i64().unwrap_or(0) == 0
        {
            self.store.set_digest_item(day, 0, until, upto)?;
            return Ok(None);
        }
        let title = format!("Daily digest {day}");
        let mut data = data;
        data["text"] = Value::String(text);
        let item = self
            .store
            .add_inbox_item(DIGEST_ITEM, None, &title, &data)?;
        self.store.set_digest_item(day, item.id, until, upto)?;
        Ok(Some(item))
    }

    fn compose_digest(
        &self,
        day: &str,
        (since, until): (i64, i64),
        upto: (i64, i64),
        day_tokens: Option<i64>,
    ) -> Result<(Value, String)> {
        let repositories: Vec<Repository> = self
            .store
            .repositories()?
            .into_iter()
            .filter(|r| r.files_of.is_none())
            .collect();
        let mut sections: BTreeMap<i64, Section> = repositories
            .iter()
            .map(|r| {
                (
                    r.id,
                    Section {
                        id: r.id,
                        name: r.name.clone(),
                        ..Section::default()
                    },
                )
            })
            .collect();
        let log = self
            .store
            .activity_after(self.store.digested_activity()?, upto.0)?;
        // The stale handling releases through the same call a person's release is, so the digest tells it once.
        let upkeep_releases: std::collections::BTreeSet<(Option<i64>, String)> = log
            .iter()
            .filter(|a| matches!(a.kind.as_str(), "auto_release" | "stale_closed"))
            .map(|a| {
                (
                    a.data["card"].as_i64(),
                    a.who.clone().unwrap_or_default().to_lowercase(),
                )
            })
            .collect();
        for activity in log {
            if activity.kind == "release"
                && upkeep_releases.contains(&(
                    activity.data["card"].as_i64(),
                    activity.who.clone().unwrap_or_default().to_lowercase(),
                ))
            {
                continue;
            }
            let Some(section) = activity.repository_id.and_then(|id| sections.get_mut(&id)) else {
                continue;
            };
            self.add_activity(section, &activity)?;
        }
        // The reverts the merge gate kept since the last digest.
        for (repository, change, commit) in self
            .store
            .reverts_after(self.store.digested_reverts()?, upto.1)?
        {
            let Some(section) = sections.get_mut(&repository) else {
                continue;
            };
            let change = self.change(change)?;
            let card = change
                .card_id
                .and_then(|id| self.store.card(id).ok().flatten());
            section.reverts.push(Line {
                card: change.card_id,
                number: change.pull_request.or(card.as_ref().map(|c| c.number)),
                title: card.map(|c| c.title).unwrap_or(change.branch.clone()),
                who: None,
                detail: format!("reverted by {}", commit.get(..8).unwrap_or(&commit)),
                measured: false,
            });
        }
        for (repository, pull_request, round, verdict) in
            self.store.review_rounds_between(since, until)?
        {
            if let Some(section) = sections.get_mut(&repository) {
                section.rounds.push(Line {
                    number: Some(pull_request),
                    detail: format!(
                        "round {round}: {}",
                        match verdict.as_deref() {
                            Some("approve") => "approved",
                            Some("changes") => "changes requested",
                            _ => "no verdict",
                        }
                    ),
                    ..Line::default()
                });
            }
        }
        for (repository, runs) in self.store.review_runs_between(since, until)? {
            if let Some(section) = sections.get_mut(&repository) {
                section.review_runs = runs;
            }
        }
        for repository in &repositories {
            let section = sections
                .get_mut(&repository.id)
                .expect("every repository has one");
            // A frozen repository is told with why, whatever froze it.
            if let Some(frozen) = &repository.frozen {
                section.frozen.push(Line {
                    title: "The repository is frozen".into(),
                    who: Some(frozen.source.clone()),
                    detail: frozen.reason.clone(),
                    ..Line::default()
                });
            }
            for card in self.store.cards(repository.id)? {
                if let Some(held) = &card.held
                    && !card.closed
                {
                    section.frozen.push(Line {
                        card: Some(card.id),
                        number: Some(card.number),
                        title: card.title.clone(),
                        who: Some(held.by.clone()),
                        detail: held.reason.clone().unwrap_or_default(),
                        ..Line::default()
                    });
                }
            }
        }
        let mut sections: Vec<Section> = sections.into_values().filter(|s| !s.is_empty()).collect();
        for section in &mut sections {
            section.trim();
        }
        let spent = self.store.tokens_between(since, until)?;
        let needs = self.needs_a_person()?;
        let data = json!({
            "day": day,
            "since": since,
            "until": until,
            "repositories": sections,
            "tokens": { "spent": spent, "day_budget": day_tokens, "today": self.store.tokens_today()? },
            "needs_a_person": needs,
        });
        let text = digest_text(&data);
        Ok((data, text))
    }

    /// Files one log line under the list it belongs to.
    fn add_activity(&self, section: &mut Section, a: &Activity) -> Result<()> {
        let title_of = |number: Option<i64>| -> String {
            number
                .and_then(|n| self.store.card_by_number(section.id, n).ok().flatten())
                .map(|c| c.title)
                .unwrap_or_default()
        };
        let mut line = Line {
            card: a.data["card"].as_i64(),
            number: a.number,
            title: a.data["title"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| title_of(a.number)),
            who: a.who.clone(),
            detail: a.detail.clone(),
            measured: false,
        };
        match a.kind.as_str() {
            "merged" => {
                // A measurement was accepted for the head that merged (the merge gate keeps it), or a part that builds on
                // the factory noted one.
                line.measured = match a.data["change"].as_i64() {
                    Some(change) => {
                        let head = self.store.change(change)?.and_then(|c| c.head);
                        let accepted = match head {
                            Some(head) => self.store.has_measurement(change, &head)?,
                            None => false,
                        };
                        accepted || self.store.has_activity_for_change("measurement", change)?
                    }
                    None => false,
                };
                section.merged.push(line);
            }
            "merge_failed" | "merge_refused" => section.refused.push(line),
            "revert" => section.reverts.push(line),
            "take" => section.taken.push(line),
            "release" => section.released.push(line),
            "auto_release" | "stale_reminder" | "stale_closed" | "stale_failed"
            | "backlog_clash" => {
                line.detail = match a.kind.as_str() {
                    "auto_release" => format!("released: {}", a.detail),
                    "stale_reminder" => format!("reminded: {}", a.detail),
                    "stale_failed" => format!(
                        "the forge refused to {}: {}",
                        a.data["action"].as_str().unwrap_or("act"),
                        a.detail
                    ),
                    "backlog_clash" => {
                        format!("not touched, not made by the backlog sync: {}", a.detail)
                    }
                    _ => format!("closed: {}", a.detail),
                };
                section.upkeep.push(line);
            }
            "frozen" => section.frozen.push(line),
            _ => {}
        }
        Ok(())
    }

    /// What waits for a person: the open inbox items but the digests, grouped by what they wait for, each with the card
    /// it is about so that a page can link it.
    fn needs_a_person(&self) -> Result<Vec<Value>> {
        let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for item in self.store.inbox()? {
            if item.kind == DIGEST_ITEM {
                continue;
            }
            let card = item
                .card_id
                .and_then(|id| self.store.card(id).ok().flatten());
            groups.entry(item.kind.clone()).or_default().push(json!({
                "id": item.id,
                "title": item.title,
                "card_id": item.card_id,
                "number": card.as_ref().map(|c| c.number),
                "repository": card
                    .as_ref()
                    .and_then(|c| self.store.repository(c.repository_id).ok().flatten())
                    .map(|r| r.name),
            }));
        }
        Ok(groups
            .into_iter()
            .map(
                |(kind, items)| json!({ "kind": kind, "reason": reason_of(&kind), "items": items }),
            )
            .collect())
    }
}

/// The digest as a few lines of Markdown, for a notification or an agent that reads it.
pub fn digest_text(data: &Value) -> String {
    let mut out = Vec::new();
    let list = |value: &Value, key: &str| value[key].as_array().cloned().unwrap_or_default();
    let name = |l: &Value| {
        let number = l["number"]
            .as_i64()
            .map(|n| format!("#{n} "))
            .unwrap_or_default();
        format!("{number}{}", l["title"].as_str().unwrap_or_default())
            .trim()
            .to_string()
    };
    for section in list(data, "repositories") {
        out.push(format!(
            "**{}**",
            section["name"].as_str().unwrap_or_default()
        ));
        for l in list(&section, "merged") {
            out.push(format!(
                "- merged {} by {}{}",
                name(&l),
                l["who"].as_str().unwrap_or("moebius"),
                if l["measured"].as_bool().unwrap_or(false) {
                    ", measured"
                } else {
                    ""
                }
            ));
        }
        for (key, word) in [("refused", "merge refused"), ("reverts", "reverted")] {
            for l in list(&section, key) {
                out.push(format!(
                    "- {word} {}: {}",
                    name(&l),
                    l["detail"].as_str().unwrap_or_default()
                ));
            }
        }
        let rounds =
            list(&section, "rounds").len() as i64 + section["review_runs"].as_i64().unwrap_or(0);
        if rounds > 0 {
            out.push(format!("- {rounds} review rounds"));
        }
        for (key, word) in [("taken", "taken"), ("released", "released")] {
            for l in list(&section, key) {
                out.push(format!(
                    "- {word} {} by @{}",
                    name(&l),
                    l["who"].as_str().unwrap_or_default()
                ));
            }
        }
        for l in list(&section, "upkeep") {
            out.push(format!(
                "- {} ({})",
                name(&l),
                l["detail"].as_str().unwrap_or_default()
            ));
        }
        for l in list(&section, "frozen") {
            out.push(format!("- held {}", name(&l)));
        }
    }
    out.push(format!(
        "Tokens: {} since the last digest{}.",
        data["tokens"]["spent"],
        match data["tokens"]["day_budget"].as_i64() {
            Some(budget) => format!(" (budget {budget} a day)"),
            None => String::new(),
        }
    ));
    for group in list(data, "needs_a_person") {
        out.push(format!(
            "Needs a person, {}: {}",
            group["reason"].as_str().unwrap_or_default().to_lowercase(),
            list(&group, "items").len()
        ));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_kind_is_worded_from_its_name() {
        assert_eq!(reason_of("merge_stopped"), "A merge stopped");
        assert_eq!(reason_of("some_new_kind"), "Some new kind");
    }

    #[test]
    fn a_list_is_cut_and_the_cut_is_counted() {
        let mut section = Section {
            taken: (0..LIST_LIMIT as i64 + 3)
                .map(|n| Line {
                    number: Some(n),
                    ..Line::default()
                })
                .collect(),
            ..Section::default()
        };
        section.trim();
        assert_eq!(section.taken.len(), LIST_LIMIT);
        assert_eq!(section.more["taken"], 3);
    }
}
