//! The runs started with a prompt and no person waiting: a run of a card (`code`, `review`, a trial) in a worktree of
//! the card's repository, and a run without a card (the Retro analyst, the Autoresearcher) in a detached worktree, with
//! what a run holds until it ends: what its agent left is committed when it does. Making the run, its worktree and its
//! token is the same for every harness (see `claude`), which then starts its own process.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;

use super::PiBolt;
use crate::app::App;
use crate::config::{AgentConfig, HarnessKind};
use crate::harness::remote::{RemoteRun, Runners};
use crate::repos::{Worktree, git};
use crate::router::{Grant, Router};
use crate::store::{Card, Repository, Run};

impl PiBolt {
    /// Starts `agent` for `purpose`, with no card, on `prompt` in a fresh detached worktree of the clone `place` names
    /// (see [`detached_run`]), checked out at `base`, with `files` written into it.
    pub(super) fn start_detached(
        &self,
        app: &Arc<App>,
        (agent, purpose): (&str, &str),
        place: (&str, &Path, Option<i64>),
        base: &str,
        (prompt, files): (&str, &BTreeMap<String, String>),
    ) -> Result<Run, String> {
        let config = self.agent(app, agent)?;
        let running = detached_run(
            app,
            &self.router,
            (agent, purpose),
            place,
            (base, files),
            &config,
        )?;
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
        let running = card_run(
            app,
            (&self.router, &self.runners),
            card,
            (agent, config),
            purpose,
            worktree,
        )?;
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
/// if any), checked out at `base`, with `files` written into it (see `write_untracked`), and its token. The run fails
/// when that cannot be done.
pub(in crate::harness) fn detached_run(
    app: &App,
    router: &Router,
    (agent, purpose): (&str, &str),
    (name, clone, repository): (&str, &Path, Option<i64>),
    (base, files): (&str, &BTreeMap<String, String>),
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
        .and_then(|worktree| match write_untracked(&worktree, files) {
            Ok(()) => Ok(worktree),
            Err(e) => {
                let _ = app.repos.remove_worktree(clone, &worktree);
                Err(format!("cannot write the files of the run: {e}"))
            }
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
        remote: None,
    })
}

/// Records a run of `card` by `agent`, as `config` has it (on the harness it names), for `purpose`, with the worktree
/// `worktree` makes for the run and its token. The agent leaves its work in the worktree, which is committed when the
/// run ends. The commit the worktree starts at is recorded with the run (`Store::set_run_base`), which the trials of the
/// regression set start from. The run fails when that cannot be done. When the agent names a runner, the run holds one
/// of its runs from here on (see `remote`), and no run is made when the runner is full.
pub(in crate::harness) fn card_run(
    app: &App,
    (router, runners): (&Router, &Arc<Runners>),
    card: &Card,
    (agent, config): (&str, &AgentConfig),
    purpose: &str,
    worktree: impl FnOnce(&Repository, &Path, i64) -> crate::repos::Result<Worktree>,
) -> Result<RunningRun, String> {
    let slot = config
        .runner
        .as_deref()
        .map(|name| runners.claim(name))
        .transpose()?;
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
    app.store
        .set_run_base(run.id, &start)
        .map_err(|e| fail(app, run.id, e.to_string()))?;
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
        remote: slot.map(|slot| Arc::new(RemoteRun::new(slot, run.id))),
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
    /// The runner the harness goes on, if the run is not on the server.
    pub(in crate::harness) remote: Option<Arc<RemoteRun>>,
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

    /// Brings what the agent did on the runner into the worktree: committed there, fetched, and the worktree set to
    /// it. A run on the server has nothing to bring.
    pub(in crate::harness) fn bring_back(&self) -> Result<(), String> {
        match &self.remote {
            Some(remote) => remote.bring_back(&self.worktree, &self.commit_message),
            None => Ok(()),
        }
    }

    /// Closes the terminals open on the run, then removes the worktree, or the chat's empty directory, and what the
    /// run left on its runner.
    pub(in crate::harness) fn remove_worktree(&self, app: &App) {
        app.terminals.close(self.id);
        if let Some(remote) = &self.remote {
            remote.cleanup();
        }
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

/// How a review of one axis reports, after what it checks.
const FINDINGS_FORMAT: &str = "Your run covers this one axis only, so do not use the report headings of the code-review \
     skill; instead report each finding as a line `FINDING blocking: <what is wrong>` (the change must \
     not be merged as it is) or `FINDING judgement: <what is doubtful>` (a person should look), and on the next line \
     `EVIDENCE: <the lines of the diff, or the line of the issue, that show it>`. A finding without evidence is \
     dropped. Do not edit any files. End your answer with one last line, either `VERDICT: approve` or \
     `VERDICT: changes`.";

/// What a `review-standards` run is told after the issue: the change against the repository's standards.
const STANDARDS_PROMPT: &str = "You are the reviewer of the standards. You are in a git worktree on the branch that \
     holds the change for this issue. Look only at whether the change follows the repository's standards, not at \
     whether it does what the issue asks. See the change with `git diff {base}...HEAD`. The standards are written in \
     CONTRIBUTING.md, CLAUDE.md, AGENTS.md and .moebius/review.md, whichever of them exist: read them first, and \
     quote the rule each blocking finding breaks. Skip what the repository's tooling enforces. Besides those, look \
     for these smells, each only ever a judgement call that begins \"Possible <smell>\" and is dropped when a \
     repository standard allows it: Mysterious Name (a name that does not say what it holds: rename it); Duplicated \
     Code (the same logic in more than one hunk: extract it); Feature Envy (a method that uses another object's data \
     more than its own: move it); Data Clumps (the same few fields travelling together: make them a type); \
     Primitive Obsession (a string or number standing for a domain concept: give it a type); Repeated Switches (the \
     same cascade on one type recurring: use one map or polymorphism); Shotgun Surgery (one change forcing edits in \
     many files: gather what changes together); Divergent Change (one module edited for several reasons: split it); \
     Speculative Generality (hooks for needs nobody has: delete them); Message Chains (long a.b().c().d() walks: \
     hide them); Middle Man (a function that only delegates: call the target); Refused Bequest (an implementer that \
     ignores most of what it inherits: compose). ";

/// What a `review-spec` run is told after the issue: the change against the issue.
const SPEC_PROMPT: &str = "You are the reviewer of the spec. You are in a git worktree on the branch that holds the \
     change for this issue. Look only at whether the change does what the issue asks, not at style. See the change \
     with `git diff {base}...HEAD`. Report what the issue asked for that is missing or only partly done, what the \
     change does that the issue did not ask for, and what looks implemented but wrong. For each, the evidence is \
     the line of the issue it rests on or the hunk that shows it. ";

/// What a `review-select` run is told after the issue: which of several attempts that all passed to keep.
const SELECT_PROMPT: &str = "Several attempts were made at this issue, each on a branch of its own, and every one \
     passed the repository's tests and checks, which are listed below the issue. You are in a git worktree; choose \
     the attempt to keep. See an attempt with `git diff {base}...<its branch>`. Compare them on whether each does what \
     the issue asks and nothing else, how small and clear the change is, whether it follows CONTRIBUTING.md, \
     CLAUDE.md and AGENTS.md, whichever of them exist, and whether it leaves a case unhandled. Give your reasons for \
     each attempt in a few lines. Do not edit any files. End your answer with one last line `PICK: <run id>` naming \
     exactly one of the attempts by the run id the list gives.";

/// Writes `files` (paths relative to `worktree`, and their text) into the worktree and keeps them out of git: the first
/// directory of each path is added to the clone's `info/exclude`, which its worktrees share.
fn write_untracked(worktree: &Path, files: &BTreeMap<String, String>) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }
    let mut excluded = BTreeSet::new();
    for (path, text) in files {
        let relative = Path::new(path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(format!("{path} is not a path inside the worktree"));
        }
        let file = worktree.join(relative);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, text).map_err(|e| e.to_string())?;
        if let Some(first) = relative.components().next() {
            excluded.insert(format!("/{}", first.as_os_str().to_string_lossy()));
        }
    }
    let exclude =
        git(worktree, &["rev-parse", "--git-path", "info/exclude"]).map_err(|e| e.to_string())?;
    let exclude = worktree.join(exclude.trim());
    let mut text = std::fs::read_to_string(&exclude).unwrap_or_default();
    let missing: Vec<_> = excluded
        .iter()
        .filter(|pattern| !text.lines().any(|line| line == pattern.as_str()))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    for pattern in missing {
        text.push_str(pattern);
        text.push('\n');
    }
    if let Some(parent) = exclude.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&exclude, text).map_err(|e| e.to_string())
}

/// What a `code` run is told in a repository that works test first (see `app::reproduce`).
const TEST_FIRST_PROMPT: &str = "This repository works test first: red, then green. Before you change any code, write \
     a test that reproduces the issue, in a file of its own, and run it to see it fail. Then make the change and run \
     it again to see it pass. Write the file `.moebius/reproduce`: its first line is the command that runs only that \
     test, and each further line names a file that holds the test. Moebius runs the command on the commit your change \
     starts from with those files added, where it must fail, and on your change, where it must pass; otherwise the \
     card comes back to you. If the issue cannot be tested this way, as for documentation or configuration only, \
     write the single line `untestable: <why>` in that file instead; the reviewer judges the reason.";

/// The commit `worktree` of the run `run_id` is at, which fails the run when git cannot say.
pub(in crate::harness) fn head(app: &App, run_id: i64, worktree: &Path) -> Result<String, String> {
    git(worktree, &["rev-parse", "HEAD"]).map_err(|e| fail(app, run_id, e.to_string()))
}

/// The prompt for a run of `card` that the card's board starts: as [`prompt_for`], and a `code` run is also given the
/// files that are likely relevant (see `App::localize_hint`), when there is such a hint.
pub(in crate::harness) fn run_prompt(
    app: &App,
    card: &Card,
    purpose: &str,
    note: Option<&str>,
) -> String {
    let hint = matches!(purpose, "code" | "attempt")
        .then(|| app.localize_hint(card))
        .flatten();
    prompt_with(app, card, purpose, note, hint.as_deref())
}

/// The prompt for a run of `card`; `note` is added under a heading of its own. A `code` run in a repository that works
/// test first is told the rule of red, then green.
pub(in crate::harness) fn prompt_for(
    app: &App,
    card: &Card,
    purpose: &str,
    note: Option<&str>,
) -> String {
    prompt_with(app, card, purpose, note, None)
}

fn prompt_with(
    app: &App,
    card: &Card,
    purpose: &str,
    note: Option<&str>,
    hint: Option<&str>,
) -> String {
    let mut prompt = format!("Work on issue #{}: {}\n", card.number, card.title);
    if !card.body.trim().is_empty() {
        prompt.push('\n');
        prompt.push_str(&card.body);
        prompt.push('\n');
    }
    prompt.push('\n');
    let base = || {
        app.store
            .repository(card.repository_id)
            .ok()
            .flatten()
            .map(|repository| repository.base_ref())
            .unwrap_or_else(|| "main".into())
    };
    match purpose {
        "review" => prompt.push_str(REVIEW_PROMPT),
        "review-standards" => {
            prompt.push_str(&STANDARDS_PROMPT.replace("{base}", &base()));
            prompt.push_str(FINDINGS_FORMAT);
        }
        "review-spec" => {
            prompt.push_str(&SPEC_PROMPT.replace("{base}", &base()));
            prompt.push_str(FINDINGS_FORMAT);
        }
        "review-select" => prompt.push_str(&SELECT_PROMPT.replace("{base}", &base())),
        _ => prompt.push_str(CODE_PROMPT),
    }
    prompt.push('\n');
    if let Some(hint) = hint {
        prompt.push('\n');
        prompt.push_str(hint);
    }
    if let Some(lessons) = crate::improve::lessons::for_prompt(app, card, purpose) {
        prompt.push('\n');
        prompt.push_str(&lessons);
    }
    let test_first = || {
        app.store
            .repository(card.repository_id)
            .ok()
            .flatten()
            .is_some_and(|repository| app.test_first(&repository))
    };
    if !purpose.starts_with("review") && test_first() {
        prompt.push('\n');
        prompt.push_str(TEST_FIRST_PROMPT);
        prompt.push('\n');
    }
    if let Some(note) = note {
        let heading = match purpose {
            "review-select" => "The attempts:",
            _ => "Notes from the last round:",
        };
        prompt.push_str(&format!("\n{heading}\n\n"));
        prompt.push_str(note);
        prompt.push('\n');
    }
    prompt
}

// Unix only: the fake Pi is a Python script started by its `#!` line.
#[cfg(all(test, unix))]
mod tests {
    use std::time::{Duration, Instant};

    use serde_json::json;

    use super::*;
    use crate::board::Runs;
    use crate::harness::pibolt::testing::{setup, setup_localizing, wait_until_ended};

    #[test]
    fn files_written_into_a_worktree_stay_out_of_git_and_inside_it() {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "--quiet"]).unwrap();
        let files = BTreeMap::from([
            (".retro/run-1.events.jsonl".to_string(), "{}\n".to_string()),
            (".retro/run-1.diff".to_string(), "diff\n".to_string()),
        ]);
        write_untracked(dir.path(), &files).unwrap();
        write_untracked(dir.path(), &files).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".retro/run-1.diff")).unwrap(),
            "diff\n"
        );
        assert_eq!(git(dir.path(), &["status", "--porcelain"]).unwrap(), "");
        let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches("/.retro\n").count(), 1, "{exclude}");
        let outside = BTreeMap::from([("../x".to_string(), String::new())]);
        assert!(write_untracked(dir.path(), &outside).is_err());
    }

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
    fn the_review_axes_are_told_their_rules_and_how_to_report_findings() {
        let (_dir, app, _pi, repo) = setup();
        let card = app.create_issue(repo, "Fix a bug", "", "person").unwrap();
        let standards = prompt_for(&app, &card, "review-standards", None);
        assert!(standards.contains("You are the reviewer"), "{standards}");
        for word in [
            ".moebius/review.md",
            "CONTRIBUTING.md",
            "Possible <smell>",
            "Feature Envy",
            "EVIDENCE:",
            "git diff main...HEAD",
        ] {
            assert!(standards.contains(word), "{word}: {standards}");
        }
        let spec = prompt_for(&app, &card, "review-spec", None);
        assert!(spec.contains("what the issue asks"), "{spec}");
        assert!(spec.contains("FINDING blocking"), "{spec}");
        assert!(!spec.contains("Feature Envy"), "{spec}");
    }

    #[test]
    fn a_code_run_in_a_repository_that_works_test_first_is_told_red_then_green() {
        let (_dir, app, _pi, repo) = setup();
        let card = app.create_issue(repo, "Fix a bug", "", "person").unwrap();
        assert!(!prompt_for(&app, &card, "code", None).contains("red, then green"));

        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        std::fs::create_dir_all(clone.join(".moebius")).unwrap();
        std::fs::write(clone.join(".moebius/test-first"), "").unwrap();
        git(&clone, &["add", "."]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", "test first"]).unwrap();
        let prompt = prompt_for(&app, &card, "code", None);
        assert!(prompt.contains("red, then green"), "{prompt}");
        assert!(prompt.contains("`.moebius/reproduce`"), "{prompt}");
        assert!(!prompt_for(&app, &card, "review", None).contains("red, then green"));
    }

    #[test]
    fn a_code_or_review_run_is_given_the_lessons_that_match_its_issue() {
        let (_dir, app, _pi, repo) = setup();
        let card = app
            .create_issue(repo, "Fix the tokenizer", "It breaks on emoji", "person")
            .unwrap();
        assert!(!prompt_for(&app, &card, "code", None).contains("Lessons from earlier cards"));
        app.store
            .add_lesson(&crate::store::NewLesson {
                repository_id: repo,
                kind: "rejection".into(),
                situation: "Changing the tokenizer".into(),
                lesson: "The reviewer found no test for emoji".into(),
                advice: "Add a test with a multi-byte character".into(),
                cards: vec![card.id],
                retro_date: None,
            })
            .unwrap();
        let code = prompt_for(&app, &card, "code", None);
        assert!(code.contains("Lessons from earlier cards"), "{code}");
        assert!(code.contains("Do: Add a test with a multi-byte character"));
        // After the issue and the instructions, before the notes of the last round.
        let note = prompt_for(&app, &card, "code", Some("Fix it."));
        assert!(
            note.find("Lessons from").unwrap() < note.find("Notes from the last round").unwrap()
        );
        assert!(
            prompt_for(&app, &card, "review-spec", None).contains("Lessons from earlier cards")
        );
        let other = app
            .create_issue(repo, "Paint the button", "Make it red", "person")
            .unwrap();
        assert!(!prompt_for(&app, &other, "code", None).contains("Lessons from earlier cards"));
    }

    /// Commits `files` (path and text) in the clone of the repository `repo`.
    fn commit_files(app: &App, repo: i64, files: &[(&str, &str)]) -> PathBuf {
        let clone = PathBuf::from(&app.store.repository(repo).unwrap().unwrap().path);
        for (path, text) in files {
            let file = clone.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        git(&clone, &["add", "."]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", "files"]).unwrap();
        clone
    }

    #[test]
    fn a_code_run_is_given_the_likely_files_and_the_files_its_change_touched_become_the_outcome() {
        let (_dir, app, pi, repo) = setup_localizing();
        commit_files(
            &app,
            repo,
            &[
                ("done.txt", "old\n"),
                ("src/render.rs", "fn draw() {}\n"),
                ("src/parser.rs", "fn parse() {}\n"),
            ],
        );
        let card = app
            .create_issue(repo, "Write done.txt", "Its text is wrong.", "person")
            .unwrap();
        let run = pi.start(&app, &card, "Coder", "code", None).unwrap();
        let run = wait_until_ended(&app, run.id);
        assert_eq!(run.status, "succeeded");

        let prompt = std::fs::read_to_string(
            app.data_dir
                .join(format!("runs/{}/agent/prompt.txt", run.id)),
        )
        .unwrap();
        assert!(
            prompt.contains("Files that are likely relevant (a hint, not a limit):\n\n- done.txt: named in the issue"),
            "{prompt}"
        );
        assert!(
            prompt.contains("Verify them before you rely on them"),
            "{prompt}"
        );
        assert!(
            prompt.find("Files that are likely").unwrap()
                < prompt.find("Moebius commits").unwrap() + 400
        );

        let hints = app.store.judgements("localize").unwrap();
        assert_eq!(hints.len(), 1);
        assert_eq!(
            (hints[0].decider.as_str(), hints[0].card_id),
            ("localize", Some(card.id))
        );
        assert!(
            hints[0].choice.starts_with("[{\"path\":\"done.txt\""),
            "{}",
            hints[0].choice
        );
        let outcome: crate::decide::localize::Outcome =
            serde_json::from_str(hints[0].outcome.as_deref().unwrap()).unwrap();
        assert_eq!(outcome.touched, ["done.txt"]);
        assert_eq!((outcome.recall, outcome.precision), (Some(1.0), Some(1.0)));
        // The card's other judgements did not get the hint's outcome from a script.
        assert_eq!(
            app.store.set_missing_outcomes(card.id, "merged").unwrap(),
            0
        );
    }

    #[test]
    fn a_repository_can_turn_the_hint_off_for_its_own_workflow() {
        let (_dir, app, _pi, repo) = setup_localizing();
        let card = app
            .create_issue(repo, "Change README.md", "", "person")
            .unwrap();
        assert!(run_prompt(&app, &card, "code", None).contains("- README.md"));
        // Only a code run asks.
        assert!(!run_prompt(&app, &card, "review", None).contains("likely relevant"));
        commit_files(
            &app,
            repo,
            &[(".moebius/localize", "# nothing here\noff\n")],
        );
        assert!(!run_prompt(&app, &card, "code", None).contains("likely relevant"));
        commit_files(&app, repo, &[(".moebius/localize", "on\nfiles 1\n")]);
        let prompt = run_prompt(&app, &card, "code", None);
        assert_eq!(prompt.matches("\n- ").count(), 1, "{prompt}");
        // Two hints were asked for and recorded, and the one that was not asked for left no judgement.
        assert_eq!(app.store.judgements("localize").unwrap().len(), 2);
    }

    #[test]
    fn a_decider_that_fails_leaves_the_run_without_a_hint() {
        let (_dir, app, _pi, repo) = setup();
        app.decisions.route(crate::decide::localize::KIND, "nobody");
        let card = app
            .create_issue(repo, "Change README.md", "", "person")
            .unwrap();
        let prompt = run_prompt(&app, &card, "code", None);
        assert!(!prompt.contains("likely relevant"), "{prompt}");
        assert!(prompt.contains("Moebius commits what you leave"));
        // The failure is recorded as the decision point records any: the fail-closed answer, and why.
        let hints = app.store.judgements("localize").unwrap();
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].choice, "[]");
        assert!(
            hints[0].reason.starts_with("refused: "),
            "{}",
            hints[0].reason
        );
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
