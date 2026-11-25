//! Moebius's own agents on the forge, the way people work there (ADR 0034): the pull request of a card opens as a draft
//! after the first round of work, its body is written from the pull request template out of the Coder's report and kept
//! current after each round, the Reviewer's verdict is a review of the pull request with comments on the exact lines, the
//! next round answers each finding in a `## Review answer` comment, and an approval takes the pull request out of draft.
//! The issue keeps only the status label and the link to the pull request.
//!
//! Everything written here is what happened, never how: plans, commits, tests with numbers, findings and waiting reasons,
//! and never a prompt, the output of a tool, a token, a cost or a link into Moebius. All of it is redacted first.
//!
//! All of it is per repository, `work_on_forge` (see `store::Flow`), on by default where the forge has drafts.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::comments::{self, Kind, Reply};
use super::pull::{self, PullTemplate, slug};
use crate::app::{App, AppError, Result};
use crate::blobs::redact;
use crate::bus::Event;
use crate::repos::forge::ReviewPost;
use crate::store::{Card, Change, Repository, ReviewComment};

/// How much of the Coder's report the pull request keeps for a section.
const SECTION_AT_MOST: usize = 6_000;

/// The review of a round as a script hands it over (`moebius.post_review(card.review)`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OwnReview {
    /// `approve` or `changes`.
    pub verdict: String,
    /// What sends the card back, as the review wrote it.
    pub findings: String,
    /// The judgement calls for the person who gets the change.
    #[serde(default)]
    pub judgements: String,
    /// Findings that name a line of the diff.
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
}

/// How a post of a review went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Posted {
    /// Written now; where it is on the forge.
    Written(String),
    /// Written before for this round and head.
    Already,
    /// Nothing to write on: the repository does not work on the forge, or the card has no pull request yet.
    Nowhere,
}

fn cut(text: &str, at_most: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= at_most {
        return text.to_string();
    }
    let kept: String = text.chars().take(at_most).collect();
    format!("{}…", kept.trim_end())
}

impl App {
    /// Whether the agents of `repository` work on its forge: the repository has it on (the default) and its forge has
    /// drafts.
    pub(crate) fn works_on_forge(&self, repository: &Repository) -> bool {
        repository.forge != "none"
            && self
                .store
                .flow(repository.id)
                .map(|flow| flow.work_on_forge)
                .unwrap_or(false)
            && self
                .forge(repository)
                .map(|forge| forge.supports_drafts())
                .unwrap_or(false)
    }

    /// What the Coder's latest successful round said last, whole.
    fn coder_report(&self, card_id: i64) -> Option<String> {
        self.store
            .runs_for_card(card_id)
            .ok()?
            .into_iter()
            .rev()
            .find(|run| run.purpose == "code" && run.status == "succeeded")?
            .summary
    }

    /// The sections of a report by the slug of their heading, with the lines the board reads left out.
    fn report_sections(report: &str) -> BTreeMap<String, String> {
        let kept: String = report
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !line.starts_with("REPRODUCE:")
                    && !line.starts_with("REPRODUCE-FILE:")
                    && !line.starts_with(crate::app::card_repositories::ASKS_FOR)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut found: BTreeMap<String, String> = BTreeMap::new();
        for (heading, text) in comments::sections(&kept) {
            let text = text.trim();
            if heading.is_empty() {
                if !text.is_empty() {
                    found.insert(String::new(), text.to_string());
                }
            } else if !text.is_empty() {
                found.insert(slug(&heading), text.to_string());
            }
        }
        found
    }

    /// The body of the pull request of the card's change in `repository`, written from the repository's template out of
    /// the Coder's report: `Fixes #N` on the first line, then each section of the template. A section the Coder did not
    /// report says so; what it reported before the first heading counts as what the change does.
    pub(crate) fn own_pull_body(
        &self,
        card: &Card,
        repository: &Repository,
    ) -> (String, PullTemplate) {
        let template = self.pull_template(repository);
        let first = if repository.id == card.repository_id {
            format!("Fixes #{}", card.number)
        } else {
            let own = self
                .repository(card.repository_id)
                .map_or_else(|_| String::new(), |r| format!(" in {}", r.name));
            format!(
                "Part of {}-{} (issue #{}{own})",
                card.project_key, card.project_number, card.number
            )
        };
        let report = self.coder_report(card.id).unwrap_or_default();
        let mut values = Self::report_sections(&report);
        values.remove(&slug("Review answer"));
        if let Some(intro) = values.remove("") {
            values.entry(slug("What the change does")).or_insert(intro);
        }
        values
            .entry(slug("Item"))
            .or_insert_with(|| format!("#{}: {}", card.number, card.title));
        for text in values.values_mut() {
            *text = cut(text, SECTION_AT_MOST);
        }
        let mut body = pull::render(&template, &first, &values);
        if card.repositories.len() > 1 {
            body.push_str(&format!(
                "\nOne of the changes of {}-{} in {}, which Moebius merges together, in that order.\n",
                card.project_key,
                card.project_number,
                card.repositories.join(", ")
            ));
        }
        (redact(&body), template)
    }

    /// Keeps what the body of the change's pull request says, read by the template, for scripts and the card window.
    pub(crate) fn note_pull_fields(
        &self,
        change: &Change,
        body: &str,
        template: &PullTemplate,
    ) -> Result<()> {
        let parsed = pull::parse(body, template);
        self.store
            .set_change_pull_fields(change.id, serde_json::to_string(&parsed).ok().as_deref())?;
        Ok(())
    }

    /// The comment that answers the findings of the last review, from the Coder's `## Review answer` section: one line
    /// for each finding, and a line that says so for a finding the Coder gave none. None when there was no review that
    /// asked for changes, or the Coder reported no answer.
    fn review_answer_text(&self, change: &Change, report: &str) -> Option<String> {
        let findings = self.store.latest_forge_post(change.id, "review").ok()??;
        let count = findings
            .text
            .lines()
            .filter(|l| l.trim_start().starts_with("- "))
            .count()
            .max(1);
        let sections = comments::sections(report);
        let (_, text) = sections
            .iter()
            .find(|(heading, _)| slug(heading) == slug("Review answer"))?;
        let given = comments::read(
            &format!("{}\n{text}", Kind::ReviewAnswer.first_line()),
            count,
        )?;
        let mut lines = comments::answers(&given);
        for n in lines.len() + 1..=count {
            lines.push(format!("(finding {n} was not answered)"));
        }
        let numbered: String = lines
            .iter()
            .enumerate()
            .map(|(at, line)| format!("{}. {line}\n", at + 1))
            .collect();
        Some(redact(&format!(
            "{}\n\n{numbered}",
            Kind::ReviewAnswer.first_line()
        )))
    }

    /// After a publish that moved the change's head: keeps the fields of its body, and answers the last review on the
    /// pull request, once for each head.
    pub(crate) fn after_round_published(
        &self,
        card: &Card,
        repository: &Repository,
        change: &Change,
        (body, template): (&str, &PullTemplate),
    ) -> Result<()> {
        self.note_pull_fields(change, body, template)?;
        let (Some(number), Some(head)) = (change.pull_request, change.head.as_deref()) else {
            return Ok(());
        };
        let Some(report) = self.coder_report(card.id) else {
            return Ok(());
        };
        let Some(answer) = self.review_answer_text(change, &report) else {
            return Ok(());
        };
        if !self
            .store
            .claim_forge_post(change.id, "answer", head, &answer)?
        {
            return Ok(());
        }
        let written = self
            .forge(repository)
            .and_then(|forge| Ok(forge.comment_request(repository, number, &answer)?));
        if let Err(e) = written {
            self.store.release_forge_post(change.id, "answer", head)?;
            self.bus.publish(Event::ForgeError {
                repository: repository.name.clone(),
                message: format!("pull request #{number}: the answer to the review: {e}"),
            });
        }
        Ok(())
    }

    /// Takes the draft changes of the card out of draft, as they are offered: the review approved them.
    pub(crate) fn make_ready(&self, changes: Vec<Change>) -> Result<Vec<Change>> {
        let mut made = Vec::new();
        for change in changes {
            if !change.draft {
                made.push(change);
                continue;
            }
            let (Some(number), Some(id), Some(card_id)) =
                (change.pull_request, change.repository_id, change.card_id)
            else {
                made.push(change);
                continue;
            };
            let repository = self.repository(id)?;
            let card = self.card(card_id)?;
            let (body, _) = self.own_pull_body(&card, &repository);
            self.forge(&repository)?
                .mark_ready(&repository, number, &card.title, &body)?;
            self.store.set_change_draft(change.id, false)?;
            let change = self.change(change.id)?;
            self.bus.publish(Event::ChangeUpdated(change.clone()));
            made.push(change);
        }
        Ok(made)
    }

    /// Writes the Reviewer's verdict on the card's pull request as a review of it: changes requested with a comment on
    /// each line the findings name, or an approval, tied to the commit that was read. Comments on lines the diff does not
    /// have go into the summary. Once for each round and head: a second call finds the first. A forge that refuses a
    /// verdict on the request of its own user takes the review as one that only comments, with the verdict in its first
    /// line. Everything is redacted.
    pub fn post_own_review(&self, card_id: i64, review: &OwnReview) -> Result<Posted> {
        let card = self.card(card_id)?;
        let repository = self.repository(card.repository_id)?;
        if !self.works_on_forge(&repository) {
            return Ok(Posted::Nowhere);
        }
        let Some(change) = self.store.open_change_in(card.id, repository.id)? else {
            return Ok(Posted::Nowhere);
        };
        let (Some(number), Some(head)) = (change.pull_request, change.head.clone()) else {
            return Ok(Posted::Nowhere);
        };
        let approve = review.verdict != "changes";
        let round = self
            .store
            .runs_for_card(card.id)?
            .iter()
            .filter(|run| run.purpose == "code" && run.status == "succeeded")
            .count();
        let key = format!("{round}:{head}");
        let findings = review.findings.trim();
        if !self
            .store
            .claim_forge_post(change.id, "review", &key, findings)?
        {
            return Ok(Posted::Already);
        }
        let (fit, rest) = self.split_comments(&repository, &head, &review.comments);
        let summary = if approve {
            let judgements = review.judgements.trim();
            if judgements.is_empty() {
                "The change does what the issue asks, and follows the repository's standards."
                    .to_string()
            } else {
                format!("Approved. Judgement calls for whoever merges:\n\n{judgements}")
            }
        } else {
            findings.to_string()
        };
        let body = redact(&format!("{}{rest}", summary.trim()));
        let comments: Vec<ReviewComment> = fit
            .into_iter()
            .map(|c| ReviewComment {
                body: redact(&c.body),
                ..c
            })
            .collect();
        let forge = self.forge(&repository)?;
        let post = ReviewPost {
            approve,
            head: &head,
            body: body.trim(),
            comments: &comments,
            comment_only: false,
        };
        // A forge may refuse a verdict on the request its own user opened (GitHub does for both); then the review is
        // written without one. A forge that refuses the lines takes it without them, and in the summary.
        let attempts = {
            let all: String = comments
                .iter()
                .map(|c| format!("\n\n`{}` line {}: {}", c.path, c.line, c.body))
                .collect();
            let flat = format!("{}{all}", body.trim());
            [
                (post.comments, post.body.to_string(), false),
                (post.comments, post.body.to_string(), true),
                (&[][..], flat, true),
            ]
        };
        let mut last = None;
        for (lines, text, comment_only) in attempts {
            let try_post = ReviewPost {
                comments: lines,
                body: &text,
                comment_only,
                ..post
            };
            match forge.review_request(&repository, number, &try_post) {
                Ok(url) => {
                    self.store
                        .set_forge_post_url(change.id, "review", &key, &url)?;
                    return Ok(Posted::Written(url));
                }
                Err(e) => last = Some(e),
            }
        }
        self.store.release_forge_post(change.id, "review", &key)?;
        Err(AppError::Conflict(format!(
            "the review could not be written on the forge: {}",
            last.map(|e| e.to_string()).unwrap_or_default()
        )))
    }

    /// Tells the issue where the work is, once: the link to the pull request, in the form of the server's replies. The
    /// issue holds nothing else of the work.
    pub(crate) fn link_pull_request(
        &self,
        card: &Card,
        repository: &Repository,
        change: &Change,
    ) -> Result<()> {
        let Some(number) = change.pull_request else {
            return Ok(());
        };
        if !self
            .store
            .claim_forge_post(change.id, "link", &number.to_string(), "")?
        {
            return Ok(());
        }
        let reply = Reply::posted("issue", "Pull request", &format!("#{number}"), "");
        self.answer(repository, card.id, &reply.render());
        Ok(())
    }
}
