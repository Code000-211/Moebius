//! Intake (ADR 0019): a project can take requests. A person who may see the project, a viewer too, writes a request in
//! their own words; a grilling chat of the Moebius agent turns it into a draft issue; a member accepts it (it becomes a
//! confirmed card of the project that the requester follows), edits it, or declines it with a reason the requester is
//! told. A project without intake takes no request.
//!
//! The grilling is the one Moebius has (`harness::grilling`), started with the request in a chat of the origin
//! `automation`, so that it belongs to no one and the members read and answer it; the requester is never asked to
//! wait, and a run never waits for a person. What the chat publishes or ends in is read by
//! [`App::intake_grilled`] from `grilling`, which keeps it out of the inbox as a grilling and gives the request its
//! draft instead. A grilling that cannot be started, or that leaves no issue, gives the request a draft made of the
//! person's own words, so that no request is stranded. A new draft is an inbox item `intake_request` for the members
//! until the request is decided.

use std::sync::Arc;

use serde_json::json;

use super::{App, AppError, Result};
use crate::blobs::redact;
use crate::board::Runs;
use crate::bus::Event;
use crate::decide::Actor;
use crate::harness::grilling::Outcome;
use crate::harness::pibolt::GRILLING_AGENT;
use crate::store::{CHANGED, IntakeRequest, NewNotice, Repository, Role};

/// The most characters of a request a person may write.
pub const REQUEST_LIMIT: usize = 8000;

impl App {
    /// `by` turns intake on or off for the project `id`; requests made stay either way. A project `by` does not see is
    /// not found, and the answer is the project as `by` sees it. Announces `project_changed`.
    pub fn set_intake(&self, by: &Actor, id: i64, enabled: bool) -> Result<super::ProjectView> {
        let sight = self.sight(by)?;
        let project = self.project_seen(id, &sight)?;
        if project.intake != enabled {
            self.store.set_project_intake(id, enabled)?;
            self.bus.publish(Event::ProjectChanged(self.project(id)?));
        }
        self.project_view_of(id, &sight)
    }

    /// The repository the cards of the project's requests are made in: its first repository, else its own files.
    fn intake_repository(&self, project_id: i64) -> Result<Repository> {
        let project = self.project(project_id)?;
        match project.repositories.first() {
            Some(id) => self.repository(*id),
            None => self.project_files(project_id),
        }
    }

    /// The person acting, as a person of the organisation who is not disabled.
    fn intake_person(&self, by: &Actor) -> Result<crate::store::Person> {
        let Actor::Member { id, .. } = by else {
            return Err(AppError::Forbidden("a request is a person's".into()));
        };
        self.store
            .person(*id)?
            .filter(|person| !person.disabled)
            .ok_or_else(|| AppError::Forbidden("no such person".into()))
    }

    /// A person writes a request to the project `project_id`: stored with the status `grilling`, with a grilling chat
    /// started on it. Refused for a project without intake. When no grilling can be started, the request gets a draft
    /// of the person's own words at once. Announces `intake_changed`.
    pub fn submit_request(
        self: &Arc<Self>,
        runs: &dyn Runs,
        by: &Actor,
        project_id: i64,
        text: &str,
    ) -> Result<IntakeRequest> {
        let person = self.intake_person(by)?;
        let project = self.project(project_id)?;
        if !project.intake {
            return Err(AppError::Invalid(format!(
                "the project {} takes no requests: it has no intake",
                project.name
            )));
        }
        let text = redact(text.trim());
        if text.is_empty() {
            return Err(AppError::Invalid("a request needs words".into()));
        }
        if text.chars().count() > REQUEST_LIMIT {
            return Err(AppError::Invalid(format!(
                "a request has at most {REQUEST_LIMIT} characters"
            )));
        }
        let request = self.store.add_request(project_id, person.id, &text)?;
        self.announce_request(&request);
        let prompt = format!(
            "@{} asked for this in the project {} ({}), in their own words:\n\n{text}\n\nGrill it into one issue. They \
             do not wait for you: look at the code, and ask a question only when you cannot go on without the answer; a \
             member of the project answers. Write that one issue as the grilling's issues.json.",
            person.handle, project.name, project.key
        );
        let started = self.intake_repository(project_id).and_then(|repository| {
            self.start_chat(
                runs,
                Some(GRILLING_AGENT),
                Some(repository.id),
                false,
                Some(&prompt),
                None,
                None,
                Some(super::automations::AUTOMATION_ORIGIN),
                None,
                (None, None, &[]),
            )
        });
        match started {
            Ok(run) => self.store.set_request_run(request.id, run.id)?,
            Err(e) => {
                eprintln!("moebius: cannot grill the request {}: {e}", request.id);
                self.draft_request(&request, &title_of(&text), &text)?;
            }
        }
        self.request(by, request.id)
    }

    /// Gives the request that was `grilling` or is `drafted` the draft `title` and `body`; the first draft tells the
    /// members in the inbox. Announces `intake_changed`.
    fn draft_request(&self, request: &IntakeRequest, title: &str, body: &str) -> Result<()> {
        let title = redact(title.trim());
        let body = redact(body.trim());
        if title.is_empty() {
            return Err(AppError::Invalid("a draft issue needs a title".into()));
        }
        if !self.store.set_request_draft(request.id, &title, &body)? {
            return Err(AppError::Invalid(format!(
                "the request {} is {} and has no draft to change",
                request.id, request.status
            )));
        }
        let drafted = self.request_record(request.id)?;
        if request.status == "grilling" {
            let project = self.project(request.project_id)?;
            self.store.add_inbox_item(
                "intake_request",
                None,
                &format!("A request to {}: {title}", project.name),
                &json!({"request": request.id, "project": project.id, "requester": request.requester}),
            )?;
        }
        self.announce_request(&drafted);
        Ok(())
    }

    /// What the grilling chat `run_id` left: when it drafts a request, the draft is the first of its issues (the
    /// others are listed in its body), or the request's own words for a report or an empty result. Returns whether the
    /// chat drafts a request, which keeps its result out of the inbox (see `harness::grilling`). A request decided
    /// already keeps its decision.
    pub(crate) fn intake_grilled(&self, run_id: i64, outcome: &Outcome) -> Result<bool> {
        let Some(request) = self.store.request_for_run(run_id)? else {
            return Ok(false);
        };
        let (title, body) = match outcome {
            Outcome::Issues(issues) => {
                let first = &issues[0];
                let mut body = first.body.trim().to_string();
                if issues.len() > 1 {
                    body.push_str("\n\nThe grilling found more to do:\n");
                    for issue in &issues[1..] {
                        body.push_str(&format!("- {}\n", issue.title));
                    }
                }
                (first.title.clone(), body)
            }
            Outcome::Report(_) | Outcome::Empty(_) => {
                (title_of(&request.text), request.text.clone())
            }
        };
        // A member may have edited the draft or decided already; a later publish takes neither away.
        if request.status == "grilling" {
            self.draft_request(&request, &title, &body)?;
        }
        Ok(true)
    }

    /// The request `id` as `by` may read it: a viewer reads their own, a member or owner any.
    pub fn request(&self, by: &Actor, id: i64) -> Result<IntakeRequest> {
        let person = self.intake_person(by)?;
        let request = self.request_record(id)?;
        if person.role == Role::Viewer && request.requester != person.handle {
            return Err(AppError::NotFound(format!("request {id}")));
        }
        Ok(request)
    }

    fn request_record(&self, id: i64) -> Result<IntakeRequest> {
        self.store
            .request(id)?
            .ok_or_else(|| AppError::NotFound(format!("request {id}")))
    }

    /// The requests of the project `project_id` that `by` may read, the newest first: a viewer's own, or everyone's
    /// for a member or an owner.
    pub fn requests(&self, by: &Actor, project_id: i64) -> Result<Vec<IntakeRequest>> {
        let person = self.intake_person(by)?;
        self.project(project_id)?;
        let only = (person.role == Role::Viewer).then_some(person.id);
        Ok(self.store.requests(project_id, only)?)
    }

    /// A member edits the draft of the request `id`; a request still being grilled takes the draft in place of the
    /// grilling's.
    pub fn edit_request(
        &self,
        by: &Actor,
        id: i64,
        title: &str,
        body: &str,
    ) -> Result<IntakeRequest> {
        self.intake_person(by)?;
        let request = self.request_record(id)?;
        self.draft_request(&request, title, body)?;
        self.request_record(id)
    }

    /// A member accepts the request `id`, with the draft as it is or as they edit it (`edited` is a title and a
    /// body): a confirmed card of the project is made, with the requester as its author, who follows it and is told.
    /// The grilling chat is ended and its proposal dropped. Recorded as an `accept_request` decision. A card that
    /// cannot be made leaves the request to accept again.
    pub fn accept_request(
        &self,
        runs: &dyn Runs,
        by: &Actor,
        id: i64,
        edited: Option<(&str, &str)>,
    ) -> Result<IntakeRequest> {
        self.intake_person(by)?;
        if let Some((title, body)) = edited {
            let request = self.request_record(id)?;
            self.draft_request(&request, title, body)?;
        }
        let request = self.request_record(id)?;
        if request.status != "drafted" {
            return Err(AppError::Invalid(format!(
                "the request {id} is {}, not a draft to accept",
                request.status
            )));
        }
        let requester = self
            .store
            .person_by_handle(&request.requester)?
            .ok_or_else(|| AppError::NotFound(format!("person {}", request.requester)))?;
        if !self.store.accept_request(id)? {
            return Err(AppError::Invalid(format!(
                "the request {id} was decided already"
            )));
        }
        let made = self
            .intake_repository(request.project_id)
            .and_then(|repository| {
                self.create_confirmed_issue(
                    &repository,
                    &request.title,
                    &request.body,
                    &requester.author(),
                    &Default::default(),
                )
            });
        let card = match made {
            Ok(card) => card,
            Err(e) => {
                self.store.reopen_request(id)?;
                return Err(e);
            }
        };
        self.store.set_request_card(id, card.id)?;
        self.store.set_following(card.id, requester.id, true)?;
        let accepted = self.request_record(id)?;
        self.tell_person(
            &requester,
            NewNotice {
                kind: CHANGED,
                by: super::notices::handle_of(by).as_deref(),
                card_id: Some(card.id),
                inbox_item_id: None,
                title: &card.title,
                body: "Your request was accepted",
            },
        );
        // The card exists and the request is accepted: what follows is reported, not made a failure to retry.
        if let Err(e) = self.finish_request(runs, by, &accepted, "accept_request", "accepted") {
            eprintln!("moebius: cannot finish the request {id}: {e}");
        }
        Ok(accepted)
    }

    /// A member declines the request `id` for the `reason`, which the requester is told. Recorded as a
    /// `decline_request` decision.
    pub fn decline_request(
        &self,
        runs: &dyn Runs,
        by: &Actor,
        id: i64,
        reason: &str,
    ) -> Result<IntakeRequest> {
        self.intake_person(by)?;
        let reason = redact(reason.trim());
        if reason.is_empty() {
            return Err(AppError::Invalid(
                "a declined request needs a reason, which the requester is told".into(),
            ));
        }
        let request = self.request_record(id)?;
        if !self.store.decline_request(id, &reason)? {
            return Err(AppError::Invalid(format!(
                "the request {id} is {} and was decided already",
                request.status
            )));
        }
        let declined = self.request_record(id)?;
        if let Some(requester) = self.store.person_by_handle(&request.requester)? {
            let title = if request.title.is_empty() {
                title_of(&request.text)
            } else {
                request.title.clone()
            };
            self.tell_person(
                &requester,
                NewNotice {
                    kind: CHANGED,
                    by: super::notices::handle_of(by).as_deref(),
                    card_id: None,
                    inbox_item_id: None,
                    title: &title,
                    body: &format!("Your request was declined: {reason}"),
                },
            );
        }
        self.finish_request(runs, by, &declined, "decline_request", &reason)?;
        Ok(declined)
    }

    /// What follows a decision: its record, the inbox item resolved, the grilling's proposal dropped and its chat
    /// ended, and the announcement.
    fn finish_request(
        &self,
        runs: &dyn Runs,
        by: &Actor,
        request: &IntakeRequest,
        kind: &str,
        reason: &str,
    ) -> Result<()> {
        self.store.resolve_inbox_for_request(request.id)?;
        if let Some(run) = request.run_id {
            if let Some(grilling) = self.store.grilling_for_run(run)? {
                self.store
                    .move_grilling(grilling.id, &["proposed", "reported"], "dismissed")?;
                self.store.resolve_inbox_for_grilling(grilling.id)?;
            }
            // Ending a chat that ended already is no harm.
            let _ = self.end_chat(runs, run);
        }
        self.record_action(
            by,
            kind,
            request.card_id,
            request.run_id,
            [
                ("request".to_string(), request.id.to_string()),
                ("project".to_string(), request.project_id.to_string()),
            ]
            .into(),
            reason,
        )?;
        self.announce_request(request);
        Ok(())
    }

    fn announce_request(&self, request: &IntakeRequest) {
        self.bus.publish(Event::IntakeChanged {
            project: request.project_id,
            request: request.id,
            status: request.status.clone(),
        });
    }
}

/// A title for a request in its own words: its first line, cut to 80 characters.
fn title_of(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let line = line.trim();
    if line.chars().count() <= 80 {
        return line.to_string();
    }
    let cut: String = line.chars().take(79).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_the_first_line_cut_to_a_line() {
        assert_eq!(title_of("\n  Add dark mode \nPlease"), "Add dark mode");
        let long = "x".repeat(200);
        assert_eq!(title_of(&long).chars().count(), 80);
        assert_eq!(title_of(""), "");
    }
}
