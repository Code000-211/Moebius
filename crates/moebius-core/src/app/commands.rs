//! Commands in issue comments (ADR 0028). On a forge where a helper may comment on an issue but not assign it, the
//! owner's server does the assigning for them: `/take`, `/release` and `/done` as the first word of a line of a
//! comment are read at each poll (each comment once) and answered with a short comment on the issue.
//!
//! - `/take` assigns the commenter's login with the server's own token. It is refused for an issue that is closed or
//!   that anybody has taken, for an issue without the ready label while the repository marks ready issues, and for a
//!   login that holds as many issues taken by command as the repository allows.
//! - `/release` takes the commenter off the issue; only an assignee may.
//! - `/done` asks the owner to look: an inbox item and the "Moebius needs you" block of the card, which the owner
//!   answers by closing the issue, giving it back to the assignee or releasing it ([`App::answer_done`]). Nobody but
//!   the owner closes anything. When a followed pull request closes the issue already, its author's `/done` has the
//!   Reviewer read it (see `reviews`), and anybody else's only says thanks.
//!
//! The server's own comments are never read as commands, and neither is a line quoted with `>`, indented as code or
//! inside a fenced block.

use std::collections::BTreeMap;

use super::forms;
use super::{App, AppError, Result};
use crate::bus::Event;
use crate::decide::Actor;
use crate::repos::forge::Comment as ForgeComment;
use crate::store::{Card, InboxItem, Repository};

/// The kind of the inbox item that asks the owner to look at an issue its assignee says is done.
pub const DONE_ASKED: &str = "done_asked";

/// How many characters of what a person wrote with `/done` are kept as the reason.
const REASON_LIMIT: usize = 500;

/// A command a comment gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Take,
    Release,
    Done,
}

/// The command a comment gives, with the rest of the comment: the first line whose first word is a command, outside
/// quotes and code. The rest is what follows the command on its line, then the other lines of the comment.
pub fn command_in(body: &str) -> Option<(Command, String)> {
    // A report that starts with the heading `## Done` is a `/done` too (ADR 0034).
    if let Some(first) = body.lines().find(|l| !l.trim().is_empty())
        && first.trim().eq_ignore_ascii_case("## done")
    {
        let rest = body
            .trim_start()
            .get(first.trim().len()..)
            .unwrap_or_default();
        return Some((
            Command::Done,
            rest.trim().chars().take(REASON_LIMIT).collect(),
        ));
    }
    let mut fence: Option<char> = None;
    let lines: Vec<&str> = body.lines().collect();
    let mut found = None;
    for (at, line) in lines.iter().enumerate() {
        let text = line.trim_start();
        let marker = ["```", "~~~"]
            .iter()
            .find(|m| text.starts_with(**m))
            .and_then(|m| m.chars().next());
        if let Some(marker) = marker {
            fence = match fence {
                Some(open) if open == marker => None,
                Some(open) => Some(open),
                None => Some(marker),
            };
            continue;
        }
        let indented = line.starts_with('\t') || line.starts_with("    ");
        if fence.is_some() || indented || text.starts_with('>') {
            continue;
        }
        let mut words = text.splitn(2, char::is_whitespace);
        let command = match words.next().unwrap_or_default().to_lowercase().as_str() {
            "/take" => Command::Take,
            "/release" => Command::Release,
            "/done" => Command::Done,
            _ => continue,
        };
        found = Some((at, command, words.next().unwrap_or_default().to_string()));
        break;
    }
    let (at, command, same_line) = found?;
    let others: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != at)
        .map(|(_, l)| *l)
        .collect();
    let rest = format!("{same_line}\n{}", others.join("\n"));
    let rest = rest.trim();
    Some((command, rest.chars().take(REASON_LIMIT).collect()))
}

/// The owner's answers are a person's: the Moebius agent and column scripts cannot close, give back or release.
fn only_a_person(by: &Actor) -> Result<()> {
    if by.is_person() {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "only a person answers what an assignee asked or releases an issue taken with /take"
                .into(),
        ))
    }
}

/// "1 issue" or "3 issues".
fn issues(n: i64) -> String {
    format!("{n} issue{}", if n == 1 { "" } else { "s" })
}

impl App {
    /// Reads the commands of the comments the poll read on the card's issue, newer than the last it read, and does
    /// each. Skipped: the server's own comments, comments from before the commands were turned on, and a repository
    /// whose commands are off. Each comment is read once, whatever came of it: the newest is remembered before it is
    /// acted on, and a command that fails on the forge or the store is answered with the reason and "try again" (the
    /// person comments again), reported as a forge error, and never stops the comments after it.
    pub(in crate::app) fn read_commands(
        &self,
        card: &Card,
        comments: &[ForgeComment],
    ) -> Result<()> {
        let repository = self.repository(card.repository_id)?;
        // Commands are read where they are on; measurements where measurers are registered (ADR 0033).
        if (!repository.commands && repository.measurers.is_empty()) || repository.forge == "none" {
            return Ok(());
        }
        let seen = self.store.command_seen(repository.id, card.number)?;
        let since = self.store.commands_since(repository.id)?;
        let mut fresh: Vec<&ForgeComment> = comments.iter().filter(|c| c.id > seen).collect();
        fresh.sort_by_key(|c| c.id);
        for comment in fresh {
            self.store
                .set_command_seen(repository.id, card.number, comment.id)?;
            if (comment.at > 0 && comment.at < since)
                || self.store.comment_is_ours(card.id, comment.id)?
            {
                continue;
            }
            if super::flow::measure::is_measurer(&repository.measurers, &comment.author)
                && super::flow::measure::is_measurement(&comment.body)
            {
                let reply = self
                    .read_measurement(&repository, card, comment)
                    .unwrap_or_else(|e| {
                        self.bus.publish(Event::ForgeError {
                            repository: repository.name.clone(),
                            message: format!(
                                "#{}: a measurement of @{}: {e}",
                                card.number, comment.author
                            ),
                        });
                        Some("Could not read the measurement. Try again.".to_string())
                    });
                if let Some(reply) = reply {
                    self.answer(&repository, card.id, &reply);
                }
                continue;
            }
            if !repository.commands {
                continue;
            }
            // A comment of a kind with fields that lacks one is refused with a reply that names it (ADR 0034); a
            // review answer that holds is kept for the next round and thanked; a `/done` that holds goes on.
            let filled = match self.read_filled_comment(card, comment) {
                Ok(filled) => filled,
                Err(e) => {
                    self.bus.publish(Event::ForgeError {
                        repository: repository.name.clone(),
                        message: format!("#{}: a comment of @{}: {e}", card.number, comment.author),
                    });
                    None
                }
            };
            if let Some((read, Some(reply))) = &filled {
                let refused = !read.missing.is_empty();
                self.answer(&repository, card.id, reply);
                if refused || read.kind == forms::Kind::ReviewAnswer {
                    continue;
                }
            }
            let Some((command, mut rest)) = command_in(&comment.body) else {
                continue;
            };
            // A report with sections says what was done, the tests and the open points in the owner's question.
            if let Some((read, _)) = &filled
                && read.kind == forms::Kind::Done
                && !read.legacy
            {
                let field = |name: &str| read.fields.get(name).cloned().unwrap_or_default();
                rest = format!(
                    "{}\n\nTests: {}\nOpen points: {}",
                    field("what_was_done"),
                    field("tests_run"),
                    field("open_points")
                )
                .chars()
                .take(REASON_LIMIT)
                .collect();
            }
            let done = self.card(card.id).and_then(|current| {
                self.run_command(&repository, &current, &comment.author, command, &rest)
            });
            let reply = done.unwrap_or_else(|e| {
                self.bus.publish(Event::ForgeError {
                    repository: repository.name.clone(),
                    message: format!("#{}: a command of @{}: {e}", card.number, comment.author),
                });
                let reason = e.to_string();
                let reason = reason.lines().next().unwrap_or_default();
                let reason: String = reason.chars().take(120).collect();
                format!("Could not do that: {reason}. Try again.")
            });
            self.answer(&repository, card.id, &reply);
        }
        Ok(())
    }

    fn run_command(
        &self,
        repository: &Repository,
        card: &Card,
        login: &str,
        command: Command,
        rest: &str,
    ) -> Result<String> {
        let assignees = self.store.forge_assignees(card.id)?;
        let assigned = assignees.iter().any(|l| l.eq_ignore_ascii_case(login));
        // The author of a followed pull request has it reviewed with `/done`, whoever the issue is assigned to.
        if command == Command::Done
            && let Some(change) = self.followed_change(card)?
            && change
                .outside_author
                .as_deref()
                .is_some_and(|author| author.eq_ignore_ascii_case(login))
        {
            return self.review_on_done(repository, card, &change);
        }
        Ok(match command {
            Command::Take => self.take_by_command(repository, card, login, &assignees)?,
            _ if !assigned => match assignees.first() {
                None => "Nobody has this issue.".to_string(),
                Some(first) => format!("Only @{first} can do that."),
            },
            Command::Release => {
                self.unassign_taken(repository, card, login)?;
                "Released.".to_string()
            }
            Command::Done => self.ask_done(card, login, rest)?,
        })
    }

    /// `/take`: assigns `login`, or says why not.
    fn take_by_command(
        &self,
        repository: &Repository,
        card: &Card,
        login: &str,
        assignees: &[String],
    ) -> Result<String> {
        if card.closed {
            return Ok("This issue is closed.".into());
        }
        if assignees.iter().any(|l| l.eq_ignore_ascii_case(login)) {
            return Ok("Already assigned to you.".into());
        }
        if let Some(first) = assignees.first() {
            return Ok(format!("Already taken by @{first}."));
        }
        if card.claimed_by.is_some() {
            return Ok("Already taken.".into());
        }
        let label = &repository.ready_label;
        if repository.mark_ready && !card.labels.iter().any(|l| l.eq_ignore_ascii_case(label)) {
            return Ok(format!("Only issues labelled {label} can be taken."));
        }
        let held = self.store.command_takes(repository.id, login)?;
        if held >= repository.command_limit {
            return Ok(format!(
                "You already have {}; release one first.",
                issues(held)
            ));
        }
        let after = match self
            .forge(repository)?
            .assign_issue(repository, card.number, login)
        {
            Ok(after) => after,
            Err(e) => return Ok(format!("Could not assign @{login}: {e}")),
        };
        if !after.iter().any(|l| l.eq_ignore_ascii_case(login)) {
            return Ok(format!(
                "Could not assign @{login}: the forge did not accept the login."
            ));
        }
        self.store.set_forge_assignees(card.id, &after)?;
        self.store.set_command_taker(card.id, Some(login))?;
        self.bus.publish(Event::CardUpdated(self.card(card.id)?));
        Ok(format!("Assigned to @{login}."))
    }

    /// Takes `login` off the issue on the forge and in the store, and resolves what was asked about it.
    pub(in crate::app) fn unassign_taken(
        &self,
        repository: &Repository,
        card: &Card,
        login: &str,
    ) -> Result<()> {
        self.forge(repository)?
            .unassign_issue(repository, card.number, login)?;
        let rest: Vec<String> = self
            .store
            .forge_assignees(card.id)?
            .into_iter()
            .filter(|l| !l.eq_ignore_ascii_case(login))
            .collect();
        self.store.set_forge_assignees(card.id, &rest)?;
        self.store.set_command_taker(card.id, None)?;
        self.store
            .resolve_inbox_kind_for_card(card.id, DONE_ASKED)?;
        self.bus.publish(Event::CardUpdated(self.card(card.id)?));
        Ok(())
    }

    /// `/done` by an assignee: asks the owner, unless a followed pull request closes the issue already.
    fn ask_done(&self, card: &Card, login: &str, reason: &str) -> Result<String> {
        if self.follows_outside_request(card)? {
            return Ok(
                "Thanks, the pull request is followed; only its author can have it reviewed with /done."
                    .into(),
            );
        }
        if self.store.has_open_inbox_item(card.id, DONE_ASKED)? {
            return Ok("The owner has been asked already.".into());
        }
        self.store.add_inbox_item(
            DONE_ASKED,
            Some(card.id),
            &format!("@{login} says #{} is done: {}", card.number, card.title),
            &serde_json::json!({
                "login": login,
                "reason": reason,
                "repository": self.repository(card.repository_id)?.name,
            }),
        )?;
        self.bus.publish(Event::CardUpdated(self.card(card.id)?));
        Ok("Thanks, the owner will take a look.".into())
    }

    /// Writes `text` as a comment on the card's issue when the repository answers on the forge. A comment that cannot
    /// be written is reported with the forge's other errors; the command it answers has been done.
    pub(in crate::app) fn answer(&self, repository: &Repository, card_id: i64, text: &str) {
        if !repository.answer_on_forge {
            return;
        }
        let by = Actor::Script {
            column: "commands".into(),
        };
        if let Err(e) = self.comment_issue(&by, card_id, text) {
            self.bus.publish(Event::ForgeError {
                repository: repository.name.clone(),
                message: format!("the answer to a command: {e}"),
            });
        }
    }

    /// The open question of the card that its assignee said the issue is done.
    fn done_question(&self, card_id: i64) -> Result<InboxItem> {
        self.store
            .inbox_for_card(card_id)?
            .into_iter()
            .rev()
            .find(|i| i.kind == DONE_ASKED && i.resolved_at.is_none())
            .ok_or_else(|| {
                AppError::Conflict("nobody has said this issue is done, or it was answered".into())
            })
    }

    /// The owner answers the question an assignee's `/done` asked: `close` closes the issue, `give_back` keeps the
    /// assignee and clears the question, and `release` takes the assignee off the issue. Each is recorded as a
    /// `answer_done` decision and written on the forge when the repository answers there. Closing is refused while a
    /// run of the card is going.
    pub fn answer_done(
        &self,
        by: &Actor,
        card_id: i64,
        answer: &str,
        reason: &str,
    ) -> Result<Card> {
        only_a_person(by)?;
        let card = self.card(card_id)?;
        let item = self.done_question(card_id)?;
        let login = item.data["login"].as_str().unwrap_or_default().to_string();
        let repository = self.repository(card.repository_id)?;
        let why = reason.trim();
        let with_reason = |text: String| {
            if why.is_empty() {
                text
            } else {
                format!("{text} {why}")
            }
        };
        let reply = match answer {
            "close" => {
                self.close_issue(by, card_id, reason)?;
                format!("Closed, thanks @{login}.")
            }
            "give_back" => with_reason(format!("Back to @{login}: it is not done yet.")),
            "release" => {
                self.unassign_taken(&repository, &card, &login)?;
                with_reason("Released by the owner.".to_string())
            }
            other => {
                return Err(AppError::Invalid(format!(
                    "answer is \"close\", \"give_back\" or \"release\", not {other:?}"
                )));
            }
        };
        let context = BTreeMap::from([("answer".into(), answer.to_string())]);
        self.record_action(by, "answer_done", Some(card_id), None, context, reason)?;
        self.store
            .resolve_inbox_kind_for_card(card_id, DONE_ASKED)?;
        self.answer(&repository, card_id, &reply);
        self.bus.publish(Event::CardUpdated(self.card(card_id)?));
        self.card(card_id)
    }

    /// The owner takes the assignee off an issue that was taken with `/take`. Recorded as a `release_taken` decision
    /// and written on the forge when the repository answers there. Refused for an issue that was not taken by command.
    pub fn release_taken(&self, by: &Actor, card_id: i64, reason: &str) -> Result<Card> {
        only_a_person(by)?;
        let card = self.card(card_id)?;
        let Some(login) = self.store.command_taker(card_id)? else {
            return Err(AppError::Conflict(format!(
                "#{} was not taken with /take",
                card.number
            )));
        };
        let repository = self.repository(card.repository_id)?;
        self.unassign_taken(&repository, &card, &login)?;
        let context = BTreeMap::from([("login".into(), login)]);
        self.record_action(by, "release_taken", Some(card_id), None, context, reason)?;
        let why = reason.trim();
        self.answer(
            &repository,
            card_id,
            &if why.is_empty() {
                "Released by the owner.".to_string()
            } else {
                format!("Released by the owner. {why}")
            },
        );
        self.card(card_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(body: &str) -> Option<(Command, String)> {
        command_in(body)
    }

    #[test]
    fn a_command_is_the_first_word_of_a_line_in_any_case() {
        assert_eq!(read("/take"), Some((Command::Take, String::new())));
        assert_eq!(
            read("Hi!\n  /TAKE please"),
            Some((Command::Take, "please\nHi!".into()))
        );
        assert_eq!(read("/Release"), Some((Command::Release, String::new())));
        assert_eq!(
            read("/done it works now\nsee the pull request"),
            Some((Command::Done, "it works now\nsee the pull request".into()))
        );
        assert_eq!(read("can you /take this?"), None);
        assert_eq!(read("/taken"), None);
        assert_eq!(read("/takeover"), None);
    }

    #[test]
    fn quotes_and_code_are_not_commands() {
        assert_eq!(read("> /take"), None);
        assert_eq!(read(">/take"), None);
        assert_eq!(read("```\n/take\n```"), None);
        assert_eq!(read("~~~sh\n/done\n~~~"), None);
        assert_eq!(read("    /take"), None);
        assert_eq!(read("\t/take"), None);
        // A command after a closed block, or after a quote, still counts.
        assert_eq!(
            read("```\n/take\n```\n/release"),
            Some((Command::Release, "```\n/take\n```".into()))
        );
        assert_eq!(
            read("> /take\n\n/done"),
            Some((Command::Done, "> /take".into()))
        );
        // Only the first command of a comment counts.
        assert_eq!(read("/take\n/done").map(|c| c.0), Some(Command::Take));
    }

    #[test]
    fn the_reason_is_cut_to_a_length() {
        let (_, reason) = read(&format!("/done {}", "x".repeat(2000))).unwrap();
        assert_eq!(reason.chars().count(), REASON_LIMIT);
    }
}
