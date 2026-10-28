//! Replays (spec 7, ADR 0010): a tool call that a decider refused is run once in a throwaway copy of the worktree, and
//! what happened becomes the judgement's outcome, so that the decision can later be weighed by what it would have done.
//!
//! Only refused `tool_call` judgements are replayed, and only those that have no outcome and no replay yet, whose run's
//! repository still exists and whose call is reversible (see [`crate::guard::reversible`]): everything it does stays in
//! the worktree. Nothing else is ever run. The copy is a directory of its own under `<data_dir>/replays`, filled with
//! the files of the run's worktree if that still exists (never its `.git`, and symbolic links are left out), or else
//! with a checkout of the card's change branch, or of the repository's base. The file tools are applied by Moebius
//! itself; `bash`, `grep` and `find` run with an empty environment (`PATH` only, `HOME` the copy), no input, and a time
//! limit after which the process group is killed. Moebius lists the copy's files with their sizes and hashes before and
//! after, stores what the call printed and which files it created, changed and deleted, and removes the copy. The
//! original worktree and repository are never touched.
//!
//! The outcome is one sentence, `replayed: <how it ended>; <what happened to the files>`:
//!
//! - how it ended: `exit <code>`, `timed out` or `ended by a signal`;
//! - the files: `created N files`, `changed N files` and `deleted N files`, in that order, those that are not zero,
//!   joined by `, ` (`1 file` for one), or `no file changed`.
//!
//! A replay that could not be made reads `replayed: not run; <why>` and is stored all the same, so that it is not tried
//! again. The schedule asks [`on_schedule`] once a minute; inside the trial window, when no proposal is being tried, it
//! replays the oldest calls, at most [`PER_NIGHT`] per night. `POST /v1/judgements/{id}/replay` replays one at any hour.

use std::collections::BTreeMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::app::{App, AppError, Result, tail};
use crate::bus::AppEvent;
use crate::config::TimeOfDay;
use crate::guard;
use crate::store::{Judgement, Replay, Run, now};
use crate::trial::in_window;

/// How many calls the schedule replays in one night.
pub const PER_NIGHT: usize = 20;

/// How long a replayed command may run before it is killed.
pub const TIME_LIMIT: Duration = Duration::from_secs(60);

/// How old a refusal may be and still be replayed by the schedule (in seconds); older ones are left alone, so that the
/// calls that can never be replayed do not have to be looked at every minute for ever.
const MAX_AGE: i64 = 7 * 24 * 60 * 60;

/// The most replays one request returns.
pub const MAX_LISTED: i64 = 1000;

/// A refused call, read from its judgement.
struct Call {
    tool: String,
    input: Value,
    /// Where the run's worktree was (or is).
    worktree: PathBuf,
    run: Run,
    repository: crate::store::Repository,
}

/// The call the judgement refused, when it can be replayed; else why not.
fn call_of(app: &App, judgement: &Judgement) -> Result<Call> {
    let not = |why: &str| AppError::Invalid(format!("judgement {} {why}", judgement.id));
    if judgement.kind != "tool_call" || judgement.choice != "refuse" {
        return Err(not("is not a refused tool call"));
    }
    let run = judgement
        .run_id
        .and_then(|id| app.store.run(id).transpose())
        .transpose()?
        .ok_or_else(|| not("has no run"))?;
    let repository = match run.card_id {
        Some(card) => app
            .store
            .card(card)?
            .map(|card| card.repository_id)
            .or(run.repository_id),
        None => run.repository_id,
    };
    let repository = repository
        .and_then(|id| app.store.repository(id).transpose())
        .transpose()?
        .filter(|repository| Path::new(&repository.path).is_dir())
        .ok_or_else(|| not("is about a run whose repository is gone"))?;
    let context = judgement
        .context
        .first()
        .ok_or_else(|| not("has no context"))?;
    let context: BTreeMap<String, String> = serde_json::from_str(&app.store.blobs().get(context)?)
        .map_err(|_| not("has a context that cannot be read"))?;
    let tool = context.get("tool").ok_or_else(|| not("names no tool"))?;
    let input: Value = context
        .get("input")
        .and_then(|input| serde_json::from_str(input).ok())
        .ok_or_else(|| not("has no input to the tool"))?;
    let worktree = context
        .get("worktree")
        .ok_or_else(|| not("is about a run without a worktree"))?;
    guard::reversible(tool, &input, Path::new(worktree))
        .map_err(|why| not(&format!("is not reversible: {why}")))?;
    Ok(Call {
        tool: tool.clone(),
        input,
        worktree: PathBuf::from(worktree),
        run,
        repository,
    })
}

/// The files of a directory: path relative to it, size and SHA-256 of the content (of the target, for a link).
pub type Listing = BTreeMap<String, (u64, String)>;

/// What a call did to the files of the copy.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub created: Vec<String>,
    pub changed: Vec<String>,
    pub deleted: Vec<String>,
}

/// Lists the files under `dir`, without following links.
pub fn listing(dir: &Path) -> std::io::Result<Listing> {
    fn walk(root: &Path, dir: &Path, into: &mut Listing) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                walk(root, &path, into)?;
                continue;
            }
            let (size, bytes) = if kind.is_symlink() {
                let target = std::fs::read_link(&path)?;
                (0, target.to_string_lossy().into_owned().into_bytes())
            } else {
                let bytes = std::fs::read(&path)?;
                (bytes.len() as u64, bytes)
            };
            let name = path.strip_prefix(root).unwrap_or(&path);
            into.insert(
                name.to_string_lossy().into_owned(),
                (size, hex::encode(Sha256::digest(bytes))),
            );
        }
        Ok(())
    }
    let mut into = Listing::new();
    walk(dir, dir, &mut into)?;
    Ok(into)
}

/// What happened to the files between two listings.
pub fn compare(before: &Listing, after: &Listing) -> Changes {
    let mut changes = Changes::default();
    for (name, file) in after {
        match before.get(name) {
            None => changes.created.push(name.clone()),
            Some(was) if was != file => changes.changed.push(name.clone()),
            Some(_) => {}
        }
    }
    changes.deleted = before
        .keys()
        .filter(|name| !after.contains_key(*name))
        .cloned()
        .collect();
    changes
}

/// Copies the files of `from` into `to`: not `.git` and not links, which could lead out of the copy.
fn copy_files(from: &Path, to: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (path, name) = (entry.path(), entry.file_name());
        let kind = entry.file_type()?;
        if name == ".git" {
            continue;
        } else if kind.is_dir() {
            std::fs::create_dir_all(to.join(&name))?;
            copy_files(&path, &to.join(&name))?;
        } else if kind.is_file() {
            std::fs::copy(&path, to.join(&name))?;
        }
    }
    Ok(())
}

/// Fills `copy` with the files of the call's worktree, or else with a checkout of the card's change branch, or else of
/// the repository's base.
fn fill(app: &App, call: &Call, copy: &Path, tarball: &Path) -> std::result::Result<(), String> {
    if call.worktree.is_dir() {
        return copy_files(&call.worktree, copy)
            .map_err(|e| format!("cannot copy the worktree: {e}"));
    }
    let clone = Path::new(&call.repository.path);
    let branch = call.run.card_id.and_then(|card| {
        let changes = app.store.changes(call.repository.id).ok()?;
        changes
            .into_iter()
            .rfind(|change| change.card_id == Some(card))
            .map(|change| change.branch)
    });
    let base = call.repository.base_ref();
    let mut failure = String::new();
    for rev in branch.iter().chain([&base]) {
        let archived = crate::repos::git(
            clone,
            &[
                "archive",
                "--format=tar",
                "-o",
                &tarball.to_string_lossy(),
                rev,
            ],
        )
        .map_err(|e| e.to_string())
        .and_then(|_| {
            let status = Command::new("tar")
                .arg("-xf")
                .arg(tarball)
                .arg("-C")
                .arg(copy)
                .status()
                .map_err(|e| e.to_string())?;
            status.success().then_some(()).ok_or("tar failed".into())
        });
        match archived {
            Ok(()) => return Ok(()),
            Err(e) => failure = format!("cannot check out {rev}: {e}"),
        }
    }
    Err(failure)
}

/// Runs `program` in `dir` with an empty environment (`PATH` only, `HOME` is `dir`), no input and its output in `log`,
/// and kills its process group when it passes `limit`. Returns the exit code and whether it was killed for the time.
fn run_limited(
    program: &str,
    args: &[&str],
    dir: &Path,
    log: &Path,
    limit: Duration,
) -> std::io::Result<(Option<i32>, bool)> {
    let file = std::fs::File::create(log)?;
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env("PATH", path)
        .env("HOME", dir)
        .stdin(Stdio::null())
        .stdout(file.try_clone()?)
        .stderr(file)
        .process_group(0)
        .spawn()?;
    let group = format!("-{}", child.id());
    let kill = || {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &group])
            .stderr(Stdio::null())
            .status();
    };
    let deadline = Instant::now() + limit;
    loop {
        if let Some(status) = child.try_wait()? {
            // Whatever the command left running goes with it.
            kill();
            return Ok((status.code(), false));
        }
        if Instant::now() >= deadline {
            kill();
            child.wait()?;
            return Ok((None, true));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Applies a file tool to the copy, the way the tool would: the exit code and what it printed.
fn apply_file_tool(copy: &Path, tool: &str, input: &Value, relative: &Path) -> (i32, String) {
    let target = copy.join(relative);
    let text = |key: &str| input[key].as_str();
    let done = |result: std::io::Result<String>| match result {
        Ok(output) => (0, output),
        Err(e) => (1, e.to_string()),
    };
    let invalid = |why: &str| (1, why.to_string());
    match tool {
        "read" => done(std::fs::read_to_string(&target)),
        "write" => {
            let Some(content) = text("content") else {
                return invalid("the call has no content to write");
            };
            done((|| {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&target, content)?;
                Ok(format!("wrote {} bytes", content.len()))
            })())
        }
        "edit" => {
            fn pair(edit: &Value) -> Option<(&str, &str)> {
                Some((edit["oldText"].as_str()?, edit["newText"].as_str()?))
            }
            let mut edits: Vec<_> = input["edits"]
                .as_array()
                .map(|edits| edits.iter().filter_map(pair).collect())
                .unwrap_or_default();
            edits.extend(pair(input));
            done((|| {
                let mut content = std::fs::read_to_string(&target)?;
                for (old, new) in &edits {
                    if content.matches(old).count() != 1 {
                        return Err(std::io::Error::other(format!(
                            "{old:?} is not in the file exactly once"
                        )));
                    }
                    content = content.replacen(old, new, 1);
                }
                std::fs::write(&target, content)?;
                Ok(format!("made {} edits", edits.len()))
            })())
        }
        _ => unreachable!("only the file tools are applied"),
    }
}

/// Runs the call in the copy and returns how it ended; its output is in `log`.
fn run_in_copy(
    call: &Call,
    copy: &Path,
    log: &Path,
    limit: Duration,
) -> std::io::Result<(Option<i32>, bool)> {
    let input = &call.input;
    let relative = |key: &str, default: &str| {
        guard::relative_to(&call.worktree, input[key].as_str().unwrap_or(default))
            .unwrap_or_default()
    };
    match call.tool.as_str() {
        "bash" => {
            let command = input["command"].as_str().unwrap_or_default();
            run_limited("bash", &["-c", command], copy, log, limit)
        }
        "grep" | "find" => {
            let path = relative("path", ".");
            let path = if path.as_os_str().is_empty() {
                Path::new(".")
            } else {
                &path
            };
            let pattern = input["pattern"].as_str();
            let path = path.to_string_lossy().into_owned();
            let (program, args) = match (call.tool.as_str(), pattern) {
                ("grep", Some(pattern)) => {
                    ("grep", vec!["-rn", "-e", pattern, "--", path.as_str()])
                }
                ("find", Some(pattern)) => ("find", vec![path.as_str(), "-name", pattern]),
                ("find", None) => ("find", vec![path.as_str()]),
                _ => {
                    std::fs::write(log, "the call has no pattern")?;
                    return Ok((Some(1), false));
                }
            };
            run_limited(program, &args, copy, log, limit)
        }
        "ls" => {
            let dir = copy.join(relative("path", "."));
            let (exit, output) = match std::fs::read_dir(&dir) {
                Ok(entries) => {
                    let mut names: Vec<_> = entries
                        .flatten()
                        .map(|e| {
                            let slash = if e.path().is_dir() { "/" } else { "" };
                            format!("{}{slash}", e.file_name().to_string_lossy())
                        })
                        .collect();
                    names.sort();
                    (0, names.join("\n"))
                }
                Err(e) => (1, e.to_string()),
            };
            std::fs::write(log, output)?;
            Ok((Some(exit), false))
        }
        tool => {
            let (exit, output) = apply_file_tool(copy, tool, input, &relative("path", ""));
            std::fs::write(log, output)?;
            Ok((Some(exit), false))
        }
    }
}

/// `1 file`, `3 files`.
fn files(count: usize) -> String {
    match count {
        1 => "1 file".into(),
        n => format!("{n} files"),
    }
}

/// The sentence that is the judgement's outcome.
fn sentence(exit: Option<i32>, timed_out: bool, changes: &Changes) -> String {
    let ended = match (timed_out, exit) {
        (true, _) => "timed out".to_string(),
        (false, Some(code)) => format!("exit {code}"),
        (false, None) => "ended by a signal".to_string(),
    };
    let parts: Vec<String> = [
        ("created", changes.created.len()),
        ("changed", changes.changed.len()),
        ("deleted", changes.deleted.len()),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(what, count)| format!("{what} {}", files(count)))
    .collect();
    let what = if parts.is_empty() {
        "no file changed".to_string()
    } else {
        parts.join(", ")
    };
    format!("replayed: {ended}; {what}")
}

/// Makes the copy, runs the call in it and removes the copy: the replay to store and the outcome, or why it could not
/// be run.
fn try_replay(
    app: &App,
    call: &Call,
    id: i64,
    limit: Duration,
) -> std::result::Result<(Replay, String), String> {
    let directory = app.data_dir.join("replays");
    let (copy, log, tarball) = (
        directory.join(id.to_string()),
        directory.join(format!("{id}.log")),
        directory.join(format!("{id}.tar")),
    );
    let _ = std::fs::remove_dir_all(&copy);
    let made = std::fs::create_dir_all(&copy)
        .map_err(|e| format!("cannot make the copy: {e}"))
        .and_then(|()| fill(app, call, &copy, &tarball))
        .and_then(|()| {
            let before = listing(&copy).map_err(|e| format!("cannot list the copy: {e}"))?;
            let ended = run_in_copy(call, &copy, &log, limit)
                .map_err(|e| format!("cannot run the call: {e}"))?;
            let after = listing(&copy).map_err(|e| format!("cannot list the copy: {e}"))?;
            Ok((ended, compare(&before, &after)))
        });
    let output = tail(&log);
    let _ = std::fs::remove_dir_all(&copy);
    let _ = std::fs::remove_file(&log);
    let _ = std::fs::remove_file(&tarball);
    let ((exit, timed_out), changes) = made?;
    let outcome = sentence(exit, timed_out, &changes);
    let replay = Replay {
        id: 0,
        judgement: id,
        run: call.run.id,
        exit,
        timed_out,
        output,
        created: changes.created,
        changed: changes.changed,
        deleted: changes.deleted,
        at: 0,
    };
    Ok((replay, outcome))
}

/// Replays the judgement's call, stores the replay, sets the outcome and publishes `replayed`.
fn replay_judgement(app: &App, judgement: &Judgement, limit: Duration) -> Result<Replay> {
    let call = call_of(app, judgement)?;
    let (replay, outcome) = try_replay(app, &call, judgement.id, limit).unwrap_or_else(|why| {
        let why = crate::blobs::redact(&why.replace('\n', " "));
        let replay = Replay {
            id: 0,
            judgement: judgement.id,
            run: call.run.id,
            exit: None,
            timed_out: false,
            output: why.clone(),
            created: Vec::new(),
            changed: Vec::new(),
            deleted: Vec::new(),
            at: 0,
        };
        (replay, format!("replayed: not run; {why}"))
    });
    let replay = app.store.add_replay(replay)?;
    app.store.set_outcome(judgement.id, &outcome)?;
    app.bus.publish(AppEvent::new(
        "replayed",
        json!({ "judgement": judgement.id, "run": replay.run, "outcome": outcome }),
    ));
    Ok(replay)
}

/// Replays the call of the judgement `id` now, whatever the hour. It fails with `Invalid` when the judgement is not a
/// refused tool call or its call is not reversible, and with `Conflict` when it was replayed already or has an outcome.
pub fn replay(app: &App, id: i64, limit: Duration) -> Result<Replay> {
    let judgement = app
        .store
        .judgement(id)?
        .ok_or_else(|| AppError::NotFound(format!("judgement {id}")))?;
    call_of(app, &judgement)?;
    if !app.store.replays(Some(id), None, 1)?.is_empty() {
        return Err(AppError::Conflict(format!(
            "judgement {id} was replayed already"
        )));
    }
    if let Some(outcome) = &judgement.outcome {
        return Err(AppError::Conflict(format!(
            "judgement {id} has the outcome {outcome:?} already"
        )));
    }
    replay_judgement(app, &judgement, limit)
}

/// The refused calls the schedule would replay, at most `count`: the oldest of the last week that have no outcome and no
/// replay and can be replayed.
pub fn due(app: &App, count: usize) -> Result<Vec<Judgement>> {
    let mut due = Vec::new();
    for judgement in app.store.replay_candidates(now() - MAX_AGE)? {
        if due.len() == count {
            break;
        }
        if call_of(app, &judgement).is_ok() {
            due.push(judgement);
        }
    }
    Ok(due)
}

/// Replays up to `count` due calls, one after the other, and stops when a proposal is being tried, so that a replay
/// never runs beside a trial. Returns how many were replayed.
pub fn run_due(app: &App, count: usize, limit: Duration) -> Result<usize> {
    let mut replayed = 0;
    for judgement in due(app, count)? {
        if app
            .store
            .proposals(None)?
            .iter()
            .any(|p| p.status == "trying")
        {
            break;
        }
        replay_judgement(app, &judgement, limit)?;
        replayed += 1;
    }
    Ok(replayed)
}

/// The schedule's check, once a minute: inside the trial window, and while the night's [`PER_NIGHT`] are not used up,
/// replays the due calls.
pub fn on_schedule(app: &Arc<App>, window: (TimeOfDay, TimeOfDay)) -> Result<()> {
    let minutes = app.store.minutes_into_the_day()?;
    if !in_window(TimeOfDay(minutes), window) {
        return Ok(());
    }
    // The night began this many minutes ago (a minute more, as the clock is read twice).
    let elapsed = i64::from((minutes + 24 * 60 - window.0.0) % (24 * 60));
    let done = app.store.replays_since(now() - elapsed * 60 - 60)?;
    run_due(app, PER_NIGHT.saturating_sub(done), TIME_LIMIT).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn listed(files: &[(&str, &str)]) -> Listing {
        let dir = tempfile::tempdir().unwrap();
        for (name, text) in files {
            write_file(&dir.path().join(name), text);
        }
        listing(dir.path()).unwrap()
    }

    #[test]
    fn a_listing_has_every_file_with_its_size_and_hash_but_no_directories() {
        let list = listed(&[("a.txt", "abc"), ("src/b.txt", ""), ("src/deep/c.txt", "x")]);
        let names: Vec<_> = list.keys().map(String::as_str).collect();
        assert_eq!(names, ["a.txt", "src/b.txt", "src/deep/c.txt"]);
        assert_eq!(list["a.txt"].0, 3);
        assert_eq!(
            list["a.txt"].1,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(list["src/b.txt"].0, 0);
    }

    #[test]
    fn the_difference_of_two_listings_names_created_changed_and_deleted_files() {
        let before = listed(&[
            ("keep", "1"),
            ("edit", "1"),
            ("gone", "1"),
            ("src/gone", "2"),
        ]);
        let after = listed(&[("keep", "1"), ("edit", "2"), ("new", "1"), ("src/new", "2")]);
        assert_eq!(
            compare(&before, &after),
            Changes {
                created: vec!["new".into(), "src/new".into()],
                changed: vec!["edit".into()],
                deleted: vec!["gone".into(), "src/gone".into()],
            }
        );
        assert_eq!(compare(&before, &before), Changes::default());
    }

    #[test]
    fn a_file_with_the_same_size_and_other_content_is_changed_and_one_moved_is_created_and_deleted()
    {
        let before = listed(&[("a", "x"), ("b", "y")]);
        let after = listed(&[("a", "z"), ("c", "y")]);
        let changes = compare(&before, &after);
        assert_eq!(changes.changed, ["a"]);
        assert_eq!(
            (changes.created, changes.deleted),
            (vec!["c".to_string()], vec!["b".to_string()])
        );
    }

    #[test]
    fn the_outcome_says_how_the_call_ended_and_what_it_did_to_the_files() {
        let changes = |c: usize, m: usize, d: usize| Changes {
            created: vec!["f".into(); c],
            changed: vec!["f".into(); m],
            deleted: vec!["f".into(); d],
        };
        assert_eq!(
            sentence(Some(0), false, &changes(0, 0, 3)),
            "replayed: exit 0; deleted 3 files"
        );
        assert_eq!(
            sentence(Some(2), false, &changes(1, 1, 1)),
            "replayed: exit 2; created 1 file, changed 1 file, deleted 1 file"
        );
        assert_eq!(
            sentence(Some(0), false, &changes(0, 0, 0)),
            "replayed: exit 0; no file changed"
        );
        assert_eq!(
            sentence(None, true, &changes(0, 2, 0)),
            "replayed: timed out; changed 2 files"
        );
        assert_eq!(
            sentence(None, false, &changes(0, 0, 0)),
            "replayed: ended by a signal; no file changed"
        );
    }

    #[test]
    fn a_copy_has_the_files_of_the_worktree_but_not_its_git_or_its_links() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        write_file(&from.join("src/a.rs"), "a");
        write_file(&from.join(".git"), "gitdir: /elsewhere");
        write_file(&from.join("sub/.git/config"), "x");
        std::os::unix::fs::symlink("/etc", from.join("link")).unwrap();
        std::fs::create_dir_all(&to).unwrap();
        copy_files(&from, &to).unwrap();
        let copied: Vec<_> = listing(&to).unwrap().into_keys().collect();
        assert_eq!(copied, ["src/a.rs"]);
    }

    #[test]
    fn a_command_runs_with_an_empty_environment_and_is_killed_at_its_time_limit() {
        let dir = tempfile::tempdir().unwrap();
        let (copy, log) = (dir.path().join("copy"), dir.path().join("log"));
        std::fs::create_dir_all(&copy).unwrap();
        let run = |command: &str, seconds| {
            run_limited(
                "bash",
                &["-c", command],
                &copy,
                &log,
                Duration::from_secs(seconds),
            )
            .unwrap()
        };
        let variables = "env | cut -d= -f1 | grep -vx -e PWD -e SHLVL -e _ | sort | tr '\\n' ' '; echo $HOME; exit 3";
        assert_eq!(run(variables, 30), (Some(3), false));
        let printed = std::fs::read_to_string(&log).unwrap();
        assert_eq!(printed.trim(), format!("HOME PATH {}", copy.display()));
        assert_eq!(run("sleep 30", 1), (None, true));
    }
}
