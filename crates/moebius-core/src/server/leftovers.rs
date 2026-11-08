//! What a server that stopped left going, ended at the next start: its runs are failed, so the board starts them
//! again, and their worktrees removed; its Retros are failed, its research confirmed again and its trials reopened.

use std::path::Path;

use crate::app::App;
use crate::harness::remote::Runners;
use crate::repos::git;

/// Runs the last server left unended died with it: they are failed, so the board starts them again, and their
/// worktrees removed. What a run on a runner left there (its directory and branch) is removed too.
pub(super) fn end_runs_of_a_previous_server(app: &App, runners: &Runners) -> Result<(), String> {
    let runs = app
        .store
        .fail_unended_runs("the server stopped while the run was going")
        .map_err(|e| format!("cannot end the runs of the last start: {e}"))?;
    app.store
        .fail_analysing_retros("the server stopped while the analyst was going")
        .map_err(|e| format!("cannot end the Retros of the last start: {e}"))?;
    app.store
        .reconfirm_running_research()
        .map_err(|e| format!("cannot confirm the research of the last start again: {e}"))?;
    app.store
        .reopen_trying_proposals()
        .map_err(|e| format!("cannot reopen the proposals of the last start: {e}"))?;
    for run in &runs {
        if let (Some(runner), Some(card_id)) = (&run.runner, run.card_id)
            && let Ok(card) = app.card(card_id)
        {
            runners.clean_leftover(runner, run.id, card.repository_id);
        }
    }
    for run in runs {
        let Some(worktree) = run.worktree else {
            continue;
        };
        let repository_id = match run.card_id {
            Some(card_id) => app.card(card_id).ok().map(|card| card.repository_id),
            None => run.repository_id,
        };
        let repository = repository_id.and_then(|id| app.store.repository(id).ok().flatten());
        match repository {
            Some(repository) => {
                let _ = app
                    .repos
                    .remove_worktree(Path::new(&repository.path), Path::new(&worktree));
                if run.purpose == "trial" {
                    // The trial also had its branch, and a worktree of the proposal's branch in the harness clone.
                    let name = Path::new(&worktree).file_name().unwrap_or_default();
                    let _ = git(
                        Path::new(&repository.path),
                        &[
                            "branch",
                            "-D",
                            "--quiet",
                            &format!("moebius/{}", name.to_string_lossy()),
                        ],
                    );
                    let proposal = app.data_dir.join("worktrees").join("harness").join(name);
                    let _ = app
                        .repos
                        .remove_worktree(&app.data_dir.join("harness"), &proposal);
                }
            }
            // The Retro analyst worked in a worktree of the harness configuration clone.
            None if run.purpose == "retro" => {
                let _ = app
                    .repos
                    .remove_worktree(&app.data_dir.join("harness"), Path::new(&worktree));
            }
            // A chat without a repository worked in an empty directory.
            None if run.purpose == "chat" => {
                let _ = std::fs::remove_dir_all(&worktree);
            }
            None => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::server::Server;

    #[test]
    fn runs_left_going_by_a_previous_server_are_failed_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "first"],
        )
        .unwrap();

        // A server that dies while a run is going.
        let first = Server::new(config.clone(), "127.0.0.1:1".parse().unwrap()).unwrap();
        let repo = first
            .app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let card = first
            .app
            .create_issue(repo.id, "Card", "", crate::app::PERSON)
            .unwrap();
        let run = first
            .app
            .store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        let worktree = first
            .app
            .repos
            .add_worktree("demo", Path::new(&repo.path), run.id, "main")
            .unwrap();
        first
            .app
            .store
            .set_run_worktree(run.id, &worktree.path.to_string_lossy())
            .unwrap();
        first.app.store.set_run_status(run.id, "running").unwrap();
        let chat = first
            .app
            .store
            .add_run(None, "Coder", "pi-bolt", "chat")
            .unwrap();
        let empty = first.app.data_dir.join(format!("chats/{}", chat.id));
        std::fs::create_dir_all(&empty).unwrap();
        first
            .app
            .store
            .set_run_worktree(chat.id, &empty.to_string_lossy())
            .unwrap();
        first.app.store.set_run_status(chat.id, "waiting").unwrap();
        drop(first);

        let second = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let run = second.app.store.run(run.id).unwrap().unwrap();
        assert_eq!(run.status, "failed");
        assert!(second.app.store.active_runs().unwrap().is_empty());
        assert!(!worktree.path.exists());
        assert!(!empty.exists());
        assert_eq!(
            second.app.store.run(chat.id).unwrap().unwrap().status,
            "failed"
        );
    }

    /// A run that went on a runner leaves its directory and branch there when the server stops without cleaning up.
    #[cfg(unix)] // The fake `ssh` runs the command in `sh`.
    #[test]
    fn the_directory_and_branch_a_run_left_on_a_runner_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let remote = dir.path().join("remote");
        let fake_ssh = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-ssh");
        let config = Config::parse(
            &format!(
                "[[runners]]\nname = \"box\"\nhost = \"gpu.test\"\nworking_dir = \"{}\"\nssh = \"{}\"\n",
                remote.display(),
                fake_ssh.display()
            ),
            dir.path(),
            &dir.path().join("moebius.toml"),
        )
        .unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        git(
            &origin,
            &["commit", "--quiet", "--allow-empty", "-m", "first"],
        )
        .unwrap();

        let first = Server::new(config.clone(), "127.0.0.1:1".parse().unwrap()).unwrap();
        let repo = first
            .app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        let card = first
            .app
            .create_issue(repo.id, "Card", "", crate::app::PERSON)
            .unwrap();
        let run = first
            .app
            .store
            .add_run(Some(card.id), "Coder", "pi-bolt", "code")
            .unwrap();
        first.app.store.set_run_runner(run.id, "box").unwrap();
        first.app.store.set_run_status(run.id, "running").unwrap();
        // What the run left on the runner: its directory with a secret in it, and its branch in the bare clone.
        let run_dir = remote.join(format!("runs/{}", run.id));
        std::fs::create_dir_all(run_dir.join("work")).unwrap();
        std::fs::write(run_dir.join("env"), "export MOEBIUS_RUN_TOKEN=x\n").unwrap();
        let bare = remote.join(format!("repos/repo-{}.git", repo.id));
        std::fs::create_dir_all(&bare).unwrap();
        git(&bare, &["init", "--bare", "--quiet"]).unwrap();
        let branch = format!("refs/heads/moebius/run-{}", run.id);
        git(
            &origin,
            &[
                "push",
                "--quiet",
                bare.to_str().unwrap(),
                &format!("HEAD:{branch}"),
            ],
        )
        .unwrap();
        assert!(git(&bare, &["rev-parse", "--verify", &branch]).is_ok());
        drop(first);

        let second = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        assert_eq!(
            second.app.store.run(run.id).unwrap().unwrap().status,
            "failed"
        );
        assert!(!run_dir.exists());
        assert!(git(&bare, &["rev-parse", "--verify", &branch]).is_err());
    }

    /// A chat that answered and waits for the next message is open, not working. The server stopping ends it; it does
    /// not fail it, and nothing new is said in it.
    #[test]
    fn a_chat_that_waits_for_a_message_ends_rather_than_fails() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::parse("", dir.path(), &dir.path().join("moebius.toml")).unwrap();
        let first = Server::new(config.clone(), "127.0.0.1:1".parse().unwrap()).unwrap();
        let chat = first
            .app
            .store
            .add_run(None, "Moebius", "pi-bolt", "chat")
            .unwrap();
        first
            .app
            .store
            .append_event(chat.id, "agent_settled", &serde_json::json!({}))
            .unwrap();
        first.app.store.set_run_status(chat.id, "waiting").unwrap();
        drop(first);

        let second = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        let ended = second.app.store.run(chat.id).unwrap().unwrap();
        assert_eq!(ended.status, "stopped");
        assert!(ended.ended_at.is_some());
        let kinds: Vec<String> = second
            .app
            .store
            .events(chat.id)
            .unwrap()
            .into_iter()
            .map(|event| event.kind)
            .collect();
        assert_eq!(kinds, vec!["agent_settled".to_string()]);
    }
}

// Unix only: the launcher is a Python script started by its `#!` line.
#[cfg(all(test, unix))]
mod launcher_tests {
    use crate::config::Config;
    use crate::server::Server;

    fn config(dir: &std::path::Path, program: &str, args: &str) -> Config {
        Config::parse(
            &format!(
                "[[plugins]]\nname = \"box\"\npath = \"{program}\"\nargs = [{args}]\nlaunches = true\n"
            ),
            dir,
            &dir.join("moebius.toml"),
        )
        .unwrap()
    }

    #[test]
    fn the_launcher_is_asked_for_its_leftovers_when_the_server_starts() {
        let dir = tempfile::tempdir().unwrap();
        let launcher =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-launcher");
        let args = format!("\"{}\"", dir.path().display());
        std::fs::write(
            dir.path().join("leftover.txt"),
            "a profile of a run that died",
        )
        .unwrap();
        let config = config(dir.path(), launcher.to_str().unwrap(), &args);
        let server = Server::new(config, "127.0.0.1:1".parse().unwrap()).unwrap();
        assert!(!dir.path().join("leftover.txt").exists());
        assert!(server.harness.pi_bolt.launcher.is_set());
        let requests = std::fs::read_to_string(dir.path().join("requests.jsonl")).unwrap();
        assert!(requests.contains(r#""method": "leftovers""#), "{requests}");
    }

    #[test]
    fn a_server_whose_launcher_cannot_answer_does_not_start() {
        let dir = tempfile::tempdir().unwrap();
        // `false` exits at once: the plugin never sends a manifest.
        let config = config(dir.path(), "false", "");
        let error = Server::new(config, "127.0.0.1:1".parse().unwrap())
            .err()
            .expect("the server refuses to start");
        assert!(error.contains("launcher box"), "{error}");
    }
}
