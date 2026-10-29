//! The runs started with a prompt and no person waiting: a run of a card (`code`, `review`, a trial) in a worktree of
//! the card's repository, and a run without a card (the Retro analyst, the Autoresearcher) in a detached worktree, with
//! what a run holds until it ends: what its agent left is committed when it does. Making the run, its worktree and its
//! token is the same for every harness (see `claude`), which then starts its own process.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use super::PiBolt;
use crate::app::App;
use crate::config::{AgentConfig, HarnessKind};
use crate::repos::{Worktree, git};
use crate::router::{Grant, Router};
use crate::store::{Card, Repository, Run};

impl PiBolt {
    /// Starts `agent` for `purpose`, with no card, on `prompt` in a fresh detached worktree of the clone `place` names
    /// (see [`detached_run`]), checked out at `base`.
    pub(super) fn start_detached(
        &self,
        app: &Arc<App>,
        (agent, purpose): (&str, &str),
        place: (&str, &Path, Option<i64>),
        base: &str,
        prompt: &str,
    ) -> Result<Run, String> {
        let config = self.agent(app, agent)?;
        let running = detached_run(app, &self.router, (agent, purpose), place, base, &config)?;
        self.launch_or_undo(app, None, &config, running, Some(prompt))
    }

    /// Starts a run of `card` by `agent`, as `config` has it, in a worktree `worktree` makes for the run (see
    /// [`card_run`]), and returns it.
    pub(super) fn start_card_run(
        &self,
        app: &Arc<App>,
        card: &Card,
        agent: &str,
        config: &AgentConfig,
        (purpose, prompt): (&str, &str),
        worktree: impl FnOnce(&Repository, &Path, i64) -> crate::repos::Result<Worktree>,
    ) -> Result<Run, String> {
        let running = card_run(app, &self.router, card, (agent, config), purpose, worktree)?;
        self.launch_or_undo(app, Some(card), config, running, Some(prompt))
    }
}

/// The run's token, which the router knows the run by. The router checks the tool calls of the answers to a harness
/// without a bridge, Claude Code.
pub(in crate::harness) fn issue_token(
    router: &Router,
    config: &AgentConfig,
    grant: Grant,
) -> String {
    match config.harness {
        HarnessKind::PiBolt => router.issue_token(grant),
        HarnessKind::ClaudeCode => router.issue_checked_token(grant),
    }
}

/// Records a run of `agent` (on the harness `config` names) for `purpose`, with no card, and makes it a fresh detached
/// worktree of the clone `place` names (the name its worktrees go under, its path, and the repository the run records,
/// if any), checked out at `base`, and its token. The run fails when that cannot be done.
pub(in crate::harness) fn detached_run(
    app: &App,
    router: &Router,
    (agent, purpose): (&str, &str),
    (name, clone, repository): (&str, &Path, Option<i64>),
    base: &str,
    config: &AgentConfig,
) -> Result<RunningRun, String> {
    let run = app
        .store
        .add_run(None, agent, config.harness.name(), purpose)
        .map_err(|e| e.to_string())?;
    let worktree = repository
        .map_or(Ok(()), |id| app.store.set_run_repository(run.id, id))
        .map_err(|e| e.to_string())
        .and_then(|()| {
            let worktree = format!("{purpose}-{}", run.id);
            app.repos
                .add_detached_worktree(name, clone, &worktree, base)
                .map_err(|e| e.to_string())
        })
        .map_err(|e| fail(app, run.id, e))?;
    let start = head(app, run.id, &worktree)?;
    let grant = Grant {
        run_id: run.id,
        card_id: None,
        budget: config.token_budget,
    };
    Ok(RunningRun {
        id: run.id,
        clone: Some(clone.to_path_buf()),
        start,
        worktree,
        token: issue_token(router, config, grant),
        purpose: purpose.into(),
        commit_message: String::new(),
    })
}

/// Records a run of `card` by `agent`, as `config` has it (on the harness it names), for `purpose`, with the worktree
/// `worktree` makes for the run and its token. The agent leaves its work in the worktree, which is committed when the
/// run ends. The run fails when that cannot be done.
pub(in crate::harness) fn card_run(
    app: &App,
    router: &Router,
    card: &Card,
    (agent, config): (&str, &AgentConfig),
    purpose: &str,
    worktree: impl FnOnce(&Repository, &Path, i64) -> crate::repos::Result<Worktree>,
) -> Result<RunningRun, String> {
    let repository = app
        .store
        .repository(card.repository_id)
        .map_err(|e| e.to_string())?
        .ok_or("the card's repository is gone")?;
    let run = app
        .store
        .add_run(Some(card.id), agent, config.harness.name(), purpose)
        .map_err(|e| e.to_string())?;
    let clone = PathBuf::from(&repository.path);
    let worktree =
        worktree(&repository, &clone, run.id).map_err(|e| fail(app, run.id, e.to_string()))?;
    let start = head(app, run.id, &worktree.path)?;
    let grant = Grant {
        run_id: run.id,
        card_id: Some(card.id),
        budget: config.token_budget,
    };
    Ok(RunningRun {
        id: run.id,
        clone: Some(clone),
        start,
        worktree: worktree.path,
        token: issue_token(router, config, grant),
        purpose: purpose.to_string(),
        commit_message: format!(
            "#{}: {}\n\nCommitted by Moebius for its run.",
            card.number, card.title
        ),
    })
}

/// What a started run holds until it ends, whatever its harness.
#[derive(Clone)]
pub(in crate::harness) struct RunningRun {
    pub(in crate::harness) id: i64,
    /// The repository's clone; none for a chat in an empty directory.
    pub(in crate::harness) clone: Option<PathBuf>,
    /// The commit the worktree was made at; what the run changed is everything after it.
    pub(in crate::harness) start: String,
    pub(in crate::harness) worktree: PathBuf,
    pub(in crate::harness) token: String,
    pub(in crate::harness) purpose: String,
    /// The message of the commit of what the agent leaves in the worktree.
    pub(in crate::harness) commit_message: String,
}

impl RunningRun {
    /// Commits what the agent left in the worktree of a repository; false when it changed nothing, that is, left
    /// the worktree clean and made no commit of its own. A chat without a repository has nothing to commit.
    pub(in crate::harness) fn commit(&self) -> crate::repos::Result<bool> {
        if self.clone.is_none() {
            return Ok(false);
        }
        let worktree = &self.worktree;
        if crate::repos::commit_all(worktree, &self.commit_message)? {
            return Ok(true);
        }
        Ok(git(worktree, &["rev-parse", "HEAD"])? != self.start)
    }

    /// Closes the terminals open on the run, then removes the worktree, or the chat's empty directory.
    pub(in crate::harness) fn remove_worktree(&self, app: &App) {
        app.terminals.close(self.id);
        match &self.clone {
            Some(clone) => drop(app.repos.remove_worktree(clone, &self.worktree)),
            None => drop(std::fs::remove_dir_all(&self.worktree)),
        }
    }
}

/// Fails a run that could not be started, with an event saying why; returns the message.
pub(in crate::harness) fn fail(app: &App, run_id: i64, message: String) -> String {
    let _ = app
        .store
        .append_event(run_id, "moebius_error", &json!({ "message": message }));
    let _ = app.store.set_run_status(run_id, "failed");
    message
}

/// What a `code` run is told after the issue.
const CODE_PROMPT: &str = "You are in a git worktree on the issue's own branch. Make the change, run what checks the \
     repository has, and stop when the issue is done. Moebius commits what you leave in the worktree.";

/// What a `review` run is told after the issue.
const REVIEW_PROMPT: &str = "You are the reviewer. You are in a git worktree on the branch that holds the change for \
     this issue. Review that change against the issue: is it correct, is it tested, and does it do what the issue \
     asks? Do not edit any files. Write your findings, then end your answer with one last line, either \
     `VERDICT: approve` or `VERDICT: changes`.";

/// The commit `worktree` of the run `run_id` is at, which fails the run when git cannot say.
pub(in crate::harness) fn head(app: &App, run_id: i64, worktree: &Path) -> Result<String, String> {
    git(worktree, &["rev-parse", "HEAD"]).map_err(|e| fail(app, run_id, e.to_string()))
}

/// The prompt for a run of `card`; `note` is added under a heading of its own.
pub(in crate::harness) fn prompt_for(card: &Card, purpose: &str, note: Option<&str>) -> String {
    let mut prompt = format!("Work on issue #{}: {}\n", card.number, card.title);
    if !card.body.trim().is_empty() {
        prompt.push('\n');
        prompt.push_str(&card.body);
        prompt.push('\n');
    }
    prompt.push('\n');
    prompt.push_str(if purpose == "review" {
        REVIEW_PROMPT
    } else {
        CODE_PROMPT
    });
    prompt.push('\n');
    if let Some(note) = note {
        prompt.push_str("\nNotes from the last round:\n\n");
        prompt.push_str(note);
        prompt.push('\n');
    }
    prompt
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::*;
    use crate::board::Runs;
    use crate::harness::pibolt::testing::{setup, wait_until_ended};

    #[test]
    fn a_review_run_starts_from_the_code_branch_and_commits_nothing() {
        let (_dir, app, pi, repo) = setup();
        {
            let mut set = pi.agents.write().unwrap();
            let coder = set.agents["Coder"].clone();
            set.agents.insert("Reviewer".into(), coder);
        }
        let card = app
            .create_issue(repo, "Write done.txt", "", "person")
            .unwrap();
        let code = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let code = wait_until_ended(&app, code.id);

        let review = pi
            .start(&app, &card, "Reviewer", "review", Some("Check the edge."))
            .unwrap();
        let review = wait_until_ended(&app, review.id);
        // Nothing was changed, and that is no failure for a review.
        assert_eq!(review.status, "succeeded");
        assert_eq!(
            review.summary.as_deref(),
            Some("Looks fine.\nVERDICT: approve")
        );
        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        assert_eq!(
            git(&clone, &["rev-parse", &review.branch()]).unwrap(),
            git(&clone, &["rev-parse", &code.branch()]).unwrap()
        );
        assert_eq!(app.store.code_branch(card.id).unwrap(), Some(code.branch()));

        let prompt_of = |run: &Run| {
            let file = format!("runs/{}/agent/prompt.txt", run.id);
            std::fs::read_to_string(app.data_dir.join(file)).unwrap()
        };
        let prompt = prompt_of(&review);
        assert!(prompt.contains("Work on issue #1: Write done.txt"));
        assert!(prompt.contains("VERDICT: approve"));
        assert!(prompt.contains("Notes from the last round:\n\nCheck the edge."));
        let prompt = prompt_of(&code);
        assert!(prompt.contains("Moebius commits what you leave"));
        assert!(!prompt.contains("Notes from the last round"));
    }

    #[test]
    fn a_run_can_be_stopped() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Please [hang]", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        // Wait until Pi has taken the prompt.
        let deadline = Instant::now() + Duration::from_secs(20);
        while app.store.events(run.id).unwrap().len() < 2 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(pi.stop(run.id));
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "stopped");
        let last = app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(
            (last.kind.as_str(), &last.data["aborted"]),
            ("agent_settled", &json!(true))
        );
    }

    #[test]
    fn a_run_that_changed_nothing_has_failed() {
        let (_dir, app, pi, repo) = setup();
        let card = app
            .create_issue(repo, "Do [nothing]", "", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "failed");
        let last = app.store.events(run.id).unwrap().pop().unwrap();
        assert_eq!(last.data["message"], "the agent changed nothing");
    }

    #[test]
    fn a_run_that_committed_its_own_work_has_succeeded() {
        let (_dir, app, pi, repo) = setup();
        let card = app.create_issue(repo, "Do [commit]", "", "person").unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "succeeded");
        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        let shown = git(&clone, &["show", &format!("{}:own.txt", run.branch())]);
        assert_eq!(shown.unwrap(), "own");
    }

    #[test]
    fn an_unknown_agent_or_a_missing_program_fails_cleanly() {
        let (_dir, app, mut pi, repo) = setup();
        let card = app.create_issue(repo, "Anything", "", "person").unwrap();
        assert!(pi.start(&app, &card, "Nobody", "code", None).is_err());
        pi.program = PathBuf::from("/nonexistent/pi-bolt");
        assert!(pi.start(&app, &card, "Coder", "code", None).is_err());
        let run = app.store.runs_for_card(card.id).unwrap().pop().unwrap();
        assert_eq!(run.status, "failed");
        assert!(!PathBuf::from(run.worktree.unwrap()).exists());
    }
}
