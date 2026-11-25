//! What people write on a forge, read by the templates (ADR 0034): the body of a followed pull request into the fields of
//! the pull request template, once for each poll, and the reply that names a required section it lacks, once; and the
//! comments of the issue that are of a kind with fields (`/done` and `## Review answer`), refused with a reply that names
//! the field they lack.

use super::comments::{self, Kind, Read, Reply};
use super::keep::PULL_REQUEST_PATH;
use super::pull;
use crate::app::{App, OpenRequest, Result};
use crate::blobs::redact;
use crate::bus::Event;
use crate::repos::forge::Comment;
use crate::store::{Card, Change, Repository};

impl App {
    /// Reads the bodies of the open pull requests of `repository` that the server follows, by the repository's pull request
    /// template, and keeps the fields on the change. A required section that is missing is named once, in a reply on the
    /// pull request, where commands and answers are on; nothing is guessed in its place.
    pub fn read_pull_requests(
        &self,
        repository: &Repository,
        requests: &[OpenRequest],
    ) -> Result<()> {
        if repository.forge == "none" {
            return Ok(());
        }
        let template = self.pull_template(repository);
        let active = repository.commands && repository.answer_on_forge;
        for change in self.store.changes(repository.id)? {
            let (true, Some(number), Some(_)) = (
                change.state == "open",
                change.pull_request,
                change.outside_author.as_ref(),
            ) else {
                continue;
            };
            let Some(request) = requests.iter().find(|r| r.number == number) else {
                continue;
            };
            let parsed = pull::parse(&request.body, &template);
            let json = serde_json::to_string(&parsed).ok();
            if json != change.pull_fields {
                self.store
                    .set_change_pull_fields(change.id, json.as_deref())?;
                if let Ok(card) = self.followed_card(&change) {
                    self.bus.publish(Event::CardUpdated(card));
                }
            }
            if parsed.missing.is_empty() || !active {
                continue;
            }
            self.name_what_is_missing(repository, &change, &parsed.missing)?;
        }
        Ok(())
    }

    fn followed_card(&self, change: &Change) -> Result<Card> {
        self.card(change.card_id.unwrap_or_default())
    }

    /// Tells the pull request, once, what its body lacks.
    fn name_what_is_missing(
        &self,
        repository: &Repository,
        change: &Change,
        missing: &[String],
    ) -> Result<()> {
        let Some(number) = change.pull_request else {
            return Ok(());
        };
        if !self
            .store
            .claim_forge_post(change.id, "missing", "pull", "")?
        {
            return Ok(());
        }
        let reply = Reply::missing(
            "pull request",
            "Pull request",
            missing,
            PULL_REQUEST_PATH,
            "Please add what is missing under its heading, as the template shows. Until then the Reviewer reads the pull \
             request with the sections it has.",
        );
        let written = self.forge(repository).and_then(|forge| {
            Ok(forge.comment_request(repository, number, &redact(&reply.render()))?)
        });
        if let Err(e) = written {
            self.store
                .release_forge_post(change.id, "missing", "pull")?;
            self.bus.publish(Event::ForgeError {
                repository: repository.name.clone(),
                message: format!(
                    "pull request #{number}: the reply that names a missing section: {e}"
                ),
            });
        }
        Ok(())
    }

    /// How many findings the last round of the Reviewer gave on the followed pull request: one for its summary and one
    /// for each comment on a line, at least one.
    fn findings_asked(&self, change: &Change) -> usize {
        self.store
            .external_reviews(change.id)
            .ok()
            .and_then(|rounds| rounds.into_iter().rev().find(|r| r.state == "posted"))
            .map_or(1, |round| round.comments.len().max(1))
    }

    /// Reads a comment of a kind that has fields, on the issue of `card`. None when it is nothing for the server to
    /// answer; else the reply to write. `/done` with sections it lacks is refused, so is a review answer without a line
    /// for each finding; a review answer that holds is kept for the next round of the Reviewer.
    pub(in crate::app) fn read_filled_comment(
        &self,
        card: &Card,
        comment: &Comment,
    ) -> Result<Option<(Read, Option<String>)>> {
        let Some(kind) = comments::kind_of(&comment.body) else {
            return Ok(None);
        };
        let change = self.followed_change(card)?;
        let findings = change.as_ref().map_or(1, |c| self.findings_asked(c));
        let Some(read) = comments::read(&comment.body, findings) else {
            return Ok(None);
        };
        let by_the_author = change.as_ref().is_some_and(|c| {
            c.outside_author
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(&comment.author))
        });
        if kind == Kind::ReviewAnswer && !by_the_author {
            return Ok(None);
        }
        if !matches!(kind, Kind::Done | Kind::ReviewAnswer) {
            return Ok(Some((read, None)));
        }
        if !read.missing.is_empty() {
            let reply = Reply::missing(
                "comment",
                kind.first_line(),
                &read.missing,
                &format!("{}/{}.md", super::keep::COMMENT_DIR, kind.file()),
                "Post the comment again with what is missing. Nothing was done with this one.",
            );
            return Ok(Some((read, Some(reply.render()))));
        }
        if kind == Kind::ReviewAnswer
            && let Some(change) = &change
        {
            let key = comment.id.to_string();
            self.store
                .claim_forge_post(change.id, "answer", &key, &redact(comment.body.trim()))?;
            let reply = Reply::posted(
                "comment",
                kind.first_line(),
                &format!("{} answers", comments::answers(&read).len()),
                "The Reviewer reads this answer with the next round.",
            );
            return Ok(Some((read, Some(reply.render()))));
        }
        Ok(Some((read, None)))
    }
}
