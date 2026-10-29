//! What a server that stopped left going, ended at the next start: its runs are failed, so the board starts them
//! again, and their worktrees removed; its Retros are failed, its research confirmed again and its trials reopened.

use std::path::Path;

use crate::app::App;
use crate::repos::git;

/// Runs the last server left unended died with it: they are failed, so the board starts them again, and their
/// worktrees removed.
pub(super) fn end_runs_of_a_previous_server(app: &App) -> Result<(), String> {
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
}
