//! Working with people on other servers through the forge (ADR 0027). Several people may run a server each on one
//! repository; nothing links the servers but the forge, so what a server wants the others to know is written there and
//! what it wants to know is read from there:
//!
//! - taking an issue (a claim, or a run the board starts for a person) makes the person's login on the forge an
//!   assignee of the issue ([`App::take_issue`]); a refused assignment is kept on the card and never fails what took it;
//! - an issue assigned to a login that is none of this server's people is *taken by* that login: no run starts for it
//!   here and a claim of it is refused ([`App::left_alone`]); a repository whose agents take "only labelled" issues also
//!   leaves alone the unlabelled ones nobody took here;
//! - a repository whose server marks ready issues keeps its label on the open issues that are approved and have no open
//!   blocker ([`App::mark_ready_issues`]);
//! - an open pull or merge request of someone who is not on this server, which closes an issue that has a card, is
//!   followed: the card is in review with it, as if it were Moebius's own change ([`App::follow_open_requests`]).

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::json;

use super::{App, AppError, Result, columns};
use crate::bus::Event;
use crate::repos::forge::Issue;
use crate::repos::git;
use crate::store::{Card, Change, Person, Repository};

/// A pull or merge request that is open on a forge, as the poll lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRequest {
    pub number: i64,
    pub title: String,
    pub body: String,
    /// The login of whoever opened it.
    pub author: String,
    /// The commit at its head.
    pub head: String,
    /// The branch it is against.
    pub base: String,
}

/// What taking an issue came to.
enum Took {
    /// This server's person is an assignee and no other server's login came before theirs (or there is nothing to
    /// assign on the forge).
    Held,
    /// Another server's login was already an assignee before ours: it took the issue first, and ours was taken off.
    Lost(String),
}

/// The local branch the head of the followed request `number` is kept on in the clone.
pub fn followed_branch(number: i64) -> String {
    format!("moebius/pr-{number}")
}

static CLOSING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\b:?[ \t]+(?:([\w.-]+/[\w.-]+))?#(\d+)\b",
    )
    .expect("the pattern is valid")
});

/// The numbers of the issues of `repository` (`owner/repo` on its forge) that `text` closes with a closing keyword,
/// such as `Fixes #12` or `closes owner/repo#12`; an issue of another repository is not one of them.
pub fn closing_references(text: &str, repository: &str) -> BTreeSet<i64> {
    CLOSING
        .captures_iter(text)
        .filter(|c| {
            c.get(1)
                .is_none_or(|named| named.as_str().eq_ignore_ascii_case(repository))
        })
        .filter_map(|c| c[2].parse().ok())
        .collect()
}

/// The label of `repository` as a name of the forge's own login for people: `GitHub`, `GitLab` or `Forgejo`.
pub fn forge_title(forge: &str) -> &'static str {
    match forge {
        "github" => "GitHub",
        "gitlab" => "GitLab",
        "forgejo" => "Forgejo",
        _ => "the forge",
    }
}

impl App {
    /// Records the logins the forge says the issue is assigned to, and tells the web app when who took the card changed.
    pub(in crate::app) fn sync_forge_assignees(&self, card: &Card, issue: &Issue) -> Result<()> {
        if self.store.set_forge_assignees(card.id, &issue.assignees)? {
            self.bus.publish(Event::CardUpdated(self.card(card.id)?));
        }
        Ok(())
    }

    /// The person whose login is assigned when `card` is taken: `by` when a person took it, else the first person the
    /// card is given to who has a login on the forge, else the one person of this server who has. Says why there is none
    /// as the problem the card shows.
    fn taker(
        &self,
        card: &Card,
        repository: &Repository,
        by: Option<&Person>,
    ) -> std::result::Result<(Person, String), String> {
        let title = forge_title(&repository.forge);
        let logins = self
            .store
            .logins_on(&repository.forge)
            .map_err(|e| e.to_string())?;
        let login_of = |handle: &str| {
            logins
                .iter()
                .find(|(h, _)| h.eq_ignore_ascii_case(handle))
                .map(|(_, login)| login.clone())
        };
        let person = match by {
            Some(person) => person.clone(),
            None => {
                let given = card
                    .assignees
                    .iter()
                    .find(|handle| login_of(handle).is_some());
                let handle = match (given, logins.as_slice()) {
                    (Some(handle), _) => handle.clone(),
                    (None, [(only, _)]) => only.clone(),
                    (None, []) => {
                        return Err(format!(
                            "Nobody has said their login on {title}, so taking the issue assigns no one there and \
                             other servers may take it too. Set it in Settings > You > Profile."
                        ));
                    }
                    (None, _) => {
                        return Err(format!(
                            "The issue is given to nobody who has said their login on {title}, so taking it assigns \
                             no one there. Give the card to a person who has."
                        ));
                    }
                };
                self.store
                    .person_by_handle(&handle)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("@{handle} is nobody of this server"))?
            }
        };
        match login_of(&person.handle) {
            Some(login) => Ok((person, login)),
            None => Err(format!(
                "@{} has not said their login on {title}, so taking the issue assigns no one there and other servers \
                 may take it too. Set it in Settings > You > Profile.",
                person.handle
            )),
        }
    }

    /// A person takes the card's issue: their login on the card's forge becomes an assignee of the issue, so that
    /// other servers leave it alone. `by` is the person who took it; without one it is the person the card is given to,
    /// or the one person of this server who has a login. Says whether this server may go on with the issue. It may when
    /// no login is set or the forge refuses (the reason is kept on the card as `assign_problem`, and nothing fails), and
    /// when nothing is assigned on the forge for lack of one. It may not when another server took the issue first: two
    /// servers that take at once both find themselves in the forge's list of assignees, and the one listed first keeps
    /// the issue; the other takes its own login off again and starts nothing. An issue of a repository without a forge
    /// has nothing to assign.
    pub fn take_issue(&self, card: &Card, by: Option<&Person>) -> bool {
        let outcome = self.try_take(card, by);
        let (problem, may) = match &outcome {
            Ok(Took::Held) => (None, true),
            Ok(Took::Lost(other)) => (
                Some(format!(
                    "@{other} took the issue first, on another server, so this server leaves it to them."
                )),
                false,
            ),
            Err(why) => (Some(why.clone()), true),
        };
        if let Ok(true) = self.store.set_assign_problem(card.id, problem.as_deref())
            && let Ok(card) = self.card(card.id)
        {
            self.bus.publish(Event::CardUpdated(card));
        }
        may
    }

    fn try_take(&self, card: &Card, by: Option<&Person>) -> std::result::Result<Took, String> {
        let repository = self
            .repository(card.repository_id)
            .map_err(|e| e.to_string())?;
        if repository.forge == "none" {
            return Ok(Took::Held);
        }
        let (person, login) = self.taker(card, &repository, by)?;
        let known = self
            .store
            .forge_assignees(card.id)
            .map_err(|e| e.to_string())?;
        if known.iter().any(|l| l.eq_ignore_ascii_case(&login)) {
            return Ok(Took::Held);
        }
        let forge = self.forge(&repository).map_err(|e| e.to_string())?;
        let after = forge
            .assign_issue(&repository, card.number, &login)
            .map_err(|e| {
                format!(
                    "{} could not be assigned to {} as @{}: {e}",
                    forge_title(&repository.forge),
                    card.number,
                    person.handle
                )
            })?;
        // The forge lists the assignees in the order they were added: when another server's login came before ours, it
        // took the issue first.
        let ours: BTreeSet<String> = self
            .store
            .logins_on(&repository.forge)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|(_, l)| l.to_lowercase())
            .collect();
        let mine = after.iter().position(|l| l.eq_ignore_ascii_case(&login));
        let first_other = after.iter().position(|l| !ours.contains(&l.to_lowercase()));
        if let (Some(other), Some(mine)) = (first_other, mine)
            && other < mine
        {
            let winner = after[other].clone();
            forge
                .unassign_issue(&repository, card.number, &login)
                .map_err(|e| {
                    format!(
                        "@{winner} took the issue first, but @{} could not be taken off it: {e}",
                        person.handle
                    )
                })?;
            let rest: Vec<String> = after
                .into_iter()
                .filter(|l| !l.eq_ignore_ascii_case(&login))
                .collect();
            self.store
                .set_forge_assignees(card.id, &rest)
                .map_err(|e| e.to_string())?;
            return Ok(Took::Lost(winner));
        }
        self.store
            .set_forge_assignees(card.id, &after)
            .map_err(|e| e.to_string())?;
        Ok(Took::Held)
    }

    /// Why no run may start for the card on this server: someone on another server took the issue, or the repository's
    /// agents take only labelled issues and this one is not, and nobody here took it either. None when it may. A run the
    /// card already has is no reason to start another when the label is gone.
    pub fn left_alone(&self, card: &Card) -> Result<Option<String>> {
        let card = self.card(card.id)?;
        if let Some(login) = &card.taken_by {
            return Ok(Some(format!("taken by @{login}")));
        }
        let repository = self.repository(card.repository_id)?;
        if repository.forge == "none" || repository.agents_take != "labelled" {
            return Ok(None);
        }
        let labelled = card
            .labels
            .iter()
            .any(|l| l.eq_ignore_ascii_case(&repository.ready_label));
        let ours = !self.store.forge_assignees(card.id)?.is_empty();
        let begun = !self.store.runs_for_card(card.id)?.is_empty();
        Ok((!labelled && !ours && !begun).then(|| {
            format!(
                "waiting for the label \"{}\" (this repository's agents take only labelled issues nobody took)",
                repository.ready_label
            )
        }))
    }

    /// Refuses the claim of a card another server's person took.
    pub(in crate::app) fn refuse_taken(&self, card: &Card) -> Result<()> {
        match &card.taken_by {
            Some(login) => Err(AppError::Conflict(format!(
                "#{} is taken by @{login} on {}",
                card.number,
                forge_title(&self.repository(card.repository_id)?.forge)
            ))),
            None => Ok(()),
        }
    }

    /// A person, or the Moebius agent, changes how the repository works with others: whether the server marks the issues
    /// that are ready for agents with a label (`mark_ready`), which label (`ready_label`), and which issues its agents
    /// take (`agents_take`, `all` or `labelled`); what is left out stays. Recorded as an `edit_repository` decision and
    /// announced as `repository_updated`; an edit that changes nothing does neither. A repository without a forge has
    /// no one to work with.
    pub fn edit_working_with_others(
        &self,
        by: &crate::decide::Actor,
        id: i64,
        mark_ready: Option<bool>,
        ready_label: Option<&str>,
        agents_take: Option<&str>,
        reason: &str,
    ) -> Result<Repository> {
        use std::collections::BTreeMap;
        let repository = self.repository(id)?;
        if repository.forge == "none" {
            return Err(AppError::Invalid(format!(
                "{} is on no forge, so there is no one to work with through it",
                repository.name
            )));
        }
        let label = ready_label.map_or(repository.ready_label.clone(), |l| l.trim().to_string());
        if label.is_empty() || label.len() > 50 || label.contains([',', '\n']) {
            return Err(AppError::Invalid(format!("{label:?} is not a label name")));
        }
        let takes = agents_take.map_or(repository.agents_take.clone(), str::to_string);
        if !["all", "labelled"].contains(&takes.as_str()) {
            return Err(AppError::Invalid(format!(
                "agents_take is \"all\" or \"labelled\", not {takes:?}"
            )));
        }
        let mark = mark_ready.unwrap_or(repository.mark_ready);
        if (mark, &label, &takes)
            == (
                repository.mark_ready,
                &repository.ready_label,
                &repository.agents_take,
            )
        {
            return Ok(repository);
        }
        let context = BTreeMap::from([
            ("repository".into(), repository.name.clone()),
            ("mark_ready".into(), mark.to_string()),
            ("ready_label".into(), label.clone()),
            ("agents_take".into(), takes.clone()),
        ]);
        self.record_action(by, "edit_repository", None, None, context, reason)?;
        self.store
            .set_repository_working_with_others(id, mark, &label, &takes)?;
        let repository = self.repository(id)?;
        self.bus
            .publish(Event::RepositoryUpdated(repository.clone()));
        Ok(repository)
    }

    /// Keeps the repository's label on its open issues that are approved and have no open blocker, and takes it off
    /// the others, as the poll has just read them. An issue about which the forge has not said whether anything blocks
    /// it is left as it is. A refusal of the forge (the label cannot be written) is reported with the repository's
    /// other forge errors and tried again later.
    pub fn mark_ready_issues(&self, repository: &Repository) -> Result<()> {
        if !repository.mark_ready || repository.forge == "none" {
            return Ok(());
        }
        let label = repository.ready_label.trim();
        if label.is_empty() {
            return Ok(());
        }
        for card in self.store.cards(repository.id)? {
            if card.closed || card.column == columns::DONE {
                continue;
            }
            let Some(wanted) = self.is_ready(&card)? else {
                continue;
            };
            let has = card.labels.iter().any(|l| l.eq_ignore_ascii_case(label));
            if wanted != has {
                self.set_labels_for_script(card.id, &[(label.to_string(), wanted)])?;
            }
        }
        Ok(())
    }

    /// Whether the card's issue is ready for agents: approved, with no open blocker and no sub-issue that is not done.
    /// None when it is not known (the forge has not said what blocks it).
    fn is_ready(&self, card: &Card) -> Result<Option<bool>> {
        if self
            .store
            .forge_seen(card.repository_id, card.number, "blockers")?
            .is_none()
        {
            return Ok(None);
        }
        let approved = card.confirmed || self.is_trusted(card) || card.column != columns::BACKLOG;
        let blocked = !self.blocked_by(card)?.is_empty()
            || self
                .children_of(card)?
                .iter()
                .any(|child| child.state == "open");
        Ok(Some(approved && !blocked))
    }

    /// The forges that have a repository of this server and nobody has said their login on: taking an issue there
    /// assigns no one, which the Get started page says.
    pub fn forges_without_logins(&self) -> Result<Vec<&'static str>> {
        let mut forges = Vec::new();
        for forge in crate::store::LOGIN_FORGES {
            let has_repository = self
                .store
                .repositories()?
                .iter()
                .any(|r| r.forge == forge && r.files_of.is_none());
            if has_repository && self.store.logins_on(forge)?.is_empty() {
                forges.push(forge);
            }
        }
        Ok(forges)
    }

    /// Whether the card follows a pull request someone else opened: such a card has no column script run, since the
    /// person who opened the request does the work.
    pub fn follows_outside_request(&self, card: &Card) -> Result<bool> {
        Ok(self
            .open_changes(card)?
            .iter()
            .any(|change| change.outside_author.is_some()))
    }

    /// Follows the open pull or merge requests `requests` of `repository` that someone who is not on this server opened
    /// for an issue that has a card: the card moves to In review with the request as its change, the checks the poll
    /// reads for it, and an `outside_change` inbox item for the owner, who may have it reviewed and merge it. A request
    /// of one of this server's people, or one that is a change of Moebius's own, is not followed; neither is an issue
    /// that is done, closed or already has an open change. The head of a request that is followed is kept in a local
    /// branch, so that an agent can read it.
    pub fn follow_open_requests(
        &self,
        repository: &Repository,
        requests: &[OpenRequest],
        writers: &BTreeSet<String>,
    ) -> Result<()> {
        for (request, card) in self.followable(repository, requests)? {
            let author = request.author.to_lowercase();
            // Anyone may open a pull request on a public repository: only one of someone who may write to it, or whom the
            // issue is assigned to, is taken for the work on the issue.
            let assigned = self
                .store
                .forge_assignees(card.id)?
                .iter()
                .any(|l| l.to_lowercase() == author);
            if writers.contains(&author) || assigned {
                self.follow(&card, repository, request)?;
            }
        }
        Ok(())
    }

    /// The logins of the authors of `requests` whose right to write to the repository decides whether their request is
    /// followed: the poll asks the forge about each once.
    pub fn request_authors_to_check(
        &self,
        repository: &Repository,
        requests: &[OpenRequest],
    ) -> Result<BTreeSet<String>> {
        let mut authors = BTreeSet::new();
        for (request, card) in self.followable(repository, requests)? {
            let author = request.author.to_lowercase();
            let assigned = self
                .store
                .forge_assignees(card.id)?
                .iter()
                .any(|l| l.to_lowercase() == author);
            if !assigned {
                authors.insert(request.author.clone());
            }
        }
        Ok(authors)
    }

    /// The requests of `requests`, each with the card it would be followed by, that pass everything but the right of
    /// its author to write: it is none of this server's people's and no change of Moebius's own that is known (a closed
    /// request of someone else that is open again counts as new); it is against the repository's default branch; it
    /// closes an issue whose card is not done and has no open change and no run going; and the issue is not taken by
    /// someone other than its author. One request is followed by one card.
    fn followable<'a>(
        &self,
        repository: &Repository,
        requests: &'a [OpenRequest],
    ) -> Result<Vec<(&'a OpenRequest, Card)>> {
        let Some(name) = repository.forge_name() else {
            return Ok(Vec::new());
        };
        let people: BTreeSet<String> = self
            .store
            .logins_on(&repository.forge)?
            .into_iter()
            .map(|(_, login)| login.to_lowercase())
            .collect();
        // A request that was followed and closed is followed again when it is open again; one of Moebius's own never is.
        let known: BTreeSet<i64> = self
            .store
            .changes(repository.id)?
            .iter()
            .filter(|c| c.outside_author.is_none() || c.state != "closed")
            .filter_map(|c| c.pull_request)
            .collect();
        let mut found = Vec::new();
        for request in requests {
            if known.contains(&request.number)
                || people.contains(&request.author.to_lowercase())
                || request.base != repository.default_branch
            {
                continue;
            }
            let text = format!("{}\n{}", request.title, request.body);
            for number in closing_references(&text, name) {
                let Some(card) = self.store.card_by_number(repository.id, number)? else {
                    continue;
                };
                let running = self
                    .store
                    .runs_for_card(card.id)?
                    .iter()
                    .any(|r| matches!(r.status.as_str(), "starting" | "running"));
                let taken_by_another = card
                    .taken_by
                    .as_deref()
                    .is_some_and(|t| !t.eq_ignore_ascii_case(&request.author));
                if card.closed
                    || card.column == columns::DONE
                    || running
                    || taken_by_another
                    || !self.open_changes(&card)?.is_empty()
                {
                    continue;
                }
                found.push((request, card));
                break;
            }
        }
        Ok(found)
    }

    fn follow(&self, card: &Card, repository: &Repository, request: &OpenRequest) -> Result<()> {
        let branch = followed_branch(request.number);
        // Without the branch the request is still followed; what needs the code says so when asked.
        if let Err(e) = self
            .forge(repository)?
            .fetch_request(repository, request.number, &branch)
        {
            self.bus.publish(Event::ForgeError {
                repository: repository.name.clone(),
                message: format!("pull request #{}: {e}", request.number),
            });
        }
        let change = self.store.add_change(
            repository.id,
            Some(card.id),
            &branch,
            Some(request.number),
            &request.head,
        )?;
        self.store
            .set_change_outside(change.id, &request.author, &card.column)?;
        let change = self.change(change.id)?;
        self.move_card(card.id, columns::IN_REVIEW, None)?;
        self.store.add_inbox_item(
            "outside_change",
            Some(card.id),
            &card.title,
            &json!({
                "change": change.id,
                "pull_request": request.number,
                "author": request.author,
                "repository": repository.name,
            }),
        )?;
        self.bus.publish(Event::ChangeOffered(change));
        Ok(())
    }

    /// A person has the Reviewer read the pull request someone else opened that the change follows: the request's head is
    /// fetched again and a `review` run of the card starts on it, whose findings are in the card's thread. Recorded as a
    /// `review_change` decision. The run is the card's, but starts no further work: nothing moves the card.
    pub fn review_outside_change(
        self: &std::sync::Arc<Self>,
        by: &crate::decide::Actor,
        runs: &dyn crate::board::CardRuns,
        change_id: i64,
        reason: &str,
    ) -> Result<crate::store::Run> {
        use std::collections::BTreeMap;
        let change = self.change(change_id)?;
        let card = match (change.outside_author.as_ref(), change.card_id) {
            (Some(_), Some(card_id)) if change.state == "open" => self.card(card_id)?,
            _ => {
                return Err(AppError::Invalid(format!(
                    "change {change_id} is not an open pull request of someone else that a card follows"
                )));
            }
        };
        if !runs.has_agent("Reviewer") {
            return Err(AppError::Invalid("there is no Reviewer agent".into()));
        }
        let repository = self.repository_of(&change)?;
        // The Reviewer reads what is fetched now, and the review counts for that commit only.
        let mut reviewed = change.head.clone();
        if let Some(number) = change.pull_request {
            self.forge(&repository)?
                .fetch_request(&repository, number, &change.branch)?;
            reviewed = Some(git(
                Path::new(&repository.path),
                &["rev-parse", &change.branch],
            )?);
        }
        if let Some(head) = &reviewed {
            self.store
                .set_change_head(change.id, head, crate::store::now())?;
            self.store.set_change_reviewed_head(change.id, head)?;
        }
        let context = BTreeMap::from([
            ("repository".into(), repository.name.clone()),
            (
                "pull_request".into(),
                change.pull_request.unwrap_or_default().to_string(),
            ),
        ]);
        self.record_action(by, "review_change", Some(card.id), None, context, reason)?;
        runs.start(
            self,
            &card,
            "Reviewer",
            ("review", None),
            crate::board::Variant::default(),
        )
        .map_err(|e| AppError::Conflict(e.to_string()))
    }

    /// Keeps the local branch of a followed request at its head when the request moved on.
    pub(crate) fn refresh_followed(&self, change: &Change, head: &str) -> Result<()> {
        if change.outside_author.is_none() || change.head.as_deref() == Some(head) {
            return Ok(());
        }
        let repository = self.repository_of(change)?;
        if let (Some(number), Ok(forge)) = (change.pull_request, self.forge(&repository)) {
            // The branch stays where it was when the fetch fails; the next poll tries again.
            if forge
                .fetch_request(&repository, number, &change.branch)
                .is_err()
            {
                return Ok(());
            }
        }
        self.store
            .set_change_head(change.id, head, crate::store::now())?;
        Ok(())
    }

    /// A followed request was closed without a merge: the change is closed, its inbox item resolved, and the card goes
    /// back to the column it was in, still taken until its assignee is removed.
    pub(crate) fn followed_closed(&self, change: &Change) -> Result<()> {
        self.store.set_change_state(change.id, "closed")?;
        self.bus.publish(Event::ChangeClosed { change: change.id });
        let Some(card_id) = change.card_id else {
            return Ok(());
        };
        self.store.resolve_inbox_for_card(card_id)?;
        let back = change
            .taken_from
            .as_deref()
            .filter(|column| *column != columns::IN_REVIEW && *column != columns::DONE)
            .unwrap_or(columns::NOT_STARTED);
        self.move_card_from(card_id, columns::IN_REVIEW, back, None, None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_keywords_name_the_issues_a_request_closes() {
        let numbers = |text: &str| {
            closing_references(text, "acme/widgets")
                .into_iter()
                .collect::<Vec<_>>()
        };
        assert_eq!(numbers("Fixes #12"), vec![12]);
        assert_eq!(numbers("closes: #3, and also resolved #4"), vec![3, 4]);
        assert_eq!(numbers("CLOSED #5\nfix #6"), vec![5, 6]);
        assert_eq!(numbers("Fixes acme/widgets#7"), vec![7]);
        assert_eq!(numbers("fixes ACME/Widgets#8"), vec![8]);
        // Another repository's issue, a plain mention and a word that merely contains a keyword are not references.
        assert!(numbers("Fixes other/repo#9").is_empty());
        assert!(numbers("See #10 and relates to #11").is_empty());
        assert!(numbers("prefixes #13, unfixed #14").is_empty());
    }
}
