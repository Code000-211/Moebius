//! The wiki of a whole server: how it starts with and without a `[wiki] url`, and how a person connects a git
//! repository to it while it runs.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::Changes;
use crate::app::AppError;
use crate::config::Config;
use crate::decide::Actor;
use crate::repos::git;
use crate::server::Server;
use crate::wiki::testing::{bare_origin, push_from_elsewhere};
use crate::wiki::{Author, State, Status};

fn ada() -> Author {
    Author {
        name: "Ada Lovelace".into(),
        handle: "ada".into(),
    }
}

/// A server over a file with `text`, which also holds `[router] listen` so that nothing binds a fixed port.
fn server(dir: &Path, text: &str) -> Server {
    let file = dir.join("moebius.toml");
    std::fs::write(&file, format!("[router]\nlisten = \"127.0.0.1:0\"\n{text}")).unwrap();
    Server::new(Config::load(&file).unwrap(), "127.0.0.1:1".parse().unwrap()).unwrap()
}

fn connect(server: &Server, url: &Path) -> Result<Status, AppError> {
    server
        .live
        .connect_wiki(&server.app, &Actor::first_owner(), &url.to_string_lossy())
}

fn file(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("moebius.toml")).unwrap()
}

#[test]
fn a_server_without_a_wiki_table_keeps_pages_on_this_server() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), "");
    let wiki = server.app.wiki().unwrap();
    assert_eq!(server.app.wiki_status().state, State::Local);
    assert!(wiki.pages().unwrap().is_empty());
    wiki.write_page(&server.app, &ada(), "Onboarding", "Welcome.", None)
        .unwrap();
    wiki.write_page(&server.app, &ada(), "Team/On call", "Who answers.", None)
        .unwrap();
    wiki.move_page(&server.app, &ada(), "Onboarding", "Start/Here")
        .unwrap();
    let paths: Vec<String> = wiki.pages().unwrap().into_iter().map(|p| p.path).collect();
    assert_eq!(paths, ["Start/Here", "Team/On call"]);
    let history = wiki.history("Start/Here").unwrap();
    assert_eq!(history.len(), 2, "{history:?}");
    assert_eq!(history[0].author, "Ada Lovelace");
    // A refresh with no origin follows nothing and fails nothing.
    wiki.refresh(&server.app).unwrap();
    assert_eq!(wiki.page("Start/Here", None).unwrap().text, "Welcome.\n");
}

#[test]
fn the_pages_kept_on_this_server_are_there_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    {
        let server = server(dir.path(), "");
        let wiki = server.app.wiki().unwrap();
        wiki.write_page(&server.app, &ada(), "Onboarding", "Welcome.", None)
            .unwrap();
    }
    let server = server(dir.path(), "");
    let wiki = server.app.wiki().unwrap();
    assert_eq!(wiki.pages().unwrap().len(), 1);
}

#[test]
fn a_wiki_url_that_cannot_be_cloned_leaves_the_server_running_without_a_wiki() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nowhere.git");
    let server = server(
        dir.path(),
        &format!("[wiki]\nurl = {:?}\n", missing.display().to_string()),
    );
    let Err(AppError::Conflict(why)) = server.app.wiki().map(|_| ()) else {
        panic!("the server has a wiki");
    };
    assert!(why.contains("cannot clone the wiki"), "{why}");
    let status = server.app.wiki_status();
    assert_eq!(status.state, State::Unavailable);
    assert!(status.problem.unwrap().contains("cannot clone the wiki"));
    // A person who connects a repository that works gets the wiki, with no restart.
    let origin = bare_origin(dir.path());
    let status = connect(&server, &origin).unwrap();
    assert_eq!(status.state, State::Remote);
    server.app.wiki().unwrap();
}

#[test]
fn what_the_server_says_about_a_wiki_it_could_not_open_has_no_password_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(
        dir.path(),
        "[wiki]\nurl = \"http://ada:hunter2@127.0.0.1:1/wiki.git\"\n",
    );
    let status = server.app.wiki_status();
    assert_eq!(status.state, State::Unavailable);
    let said = format!("{status:?}");
    assert!(said.contains("cannot clone the wiki"), "{said}");
    assert!(!said.contains("hunter2"), "{said}");
}

#[test]
fn connecting_an_empty_repository_pushes_the_history_and_later_edits_follow() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(
        dir.path(),
        "\n[board]\ntick_ms = 777\n\n# The wiki.\n# [wiki]\n# url = \"x\"\n",
    );
    let wiki = server.app.wiki().unwrap();
    wiki.write_page(&server.app, &ada(), "Onboarding", "Welcome.", None)
        .unwrap();
    let origin = bare_origin(dir.path());
    assert_eq!(git(&origin, &["branch", "--list"]).unwrap(), "");

    let status = connect(&server, &origin).unwrap();
    assert_eq!(status.state, State::Remote);
    assert_eq!(status.url.as_deref(), Some(&*origin.to_string_lossy()));
    let log = git(&origin, &["log", "--format=%an|%s", "main"]).unwrap();
    assert_eq!(log, "Ada Lovelace|Add Onboarding");

    // The file names it, the other tables stay what they were, and no restart is asked for.
    let text = file(dir.path());
    let config = Config::load(&dir.path().join("moebius.toml")).unwrap();
    assert_eq!(config.wiki_url.as_deref(), Some(&*origin.to_string_lossy()));
    assert_eq!(
        config.board_tick,
        std::time::Duration::from_millis(777),
        "{text}"
    );
    assert!(server.live.view().unwrap().restart_keys.is_empty());
    assert_eq!(server.live.get().wiki_url, config.wiki_url);

    wiki.write_page(&server.app, &ada(), "Team/On call", "Who answers.", None)
        .unwrap();
    let log = git(&origin, &["log", "--format=%s", "main"]).unwrap();
    assert_eq!(log, "Add Team/On call\nAdd Onboarding");
    // What someone else pushes is followed from now on.
    push_from_elsewhere(dir.path(), &origin, "Else.md", "From elsewhere.\n");
    wiki.refresh(&server.app).unwrap();
    assert!(wiki.page("Else", None).is_ok());

    // A second repository is refused: the wiki has one already.
    let other = dir.path().join("other.git");
    git(dir.path(), &["init", "--quiet", "--bare", "other.git"]).unwrap();
    assert!(matches!(
        connect(&server, &other),
        Err(AppError::Conflict(_))
    ));
}

#[test]
fn a_repository_with_pages_is_refused_while_the_wiki_has_pages_and_taken_while_it_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), "");
    let origin = bare_origin(dir.path());
    push_from_elsewhere(dir.path(), &origin, "Theirs.md", "Their page.\n");
    let wiki = server.app.wiki().unwrap();
    wiki.write_page(&server.app, &ada(), "Ours", "Our page.", None)
        .unwrap();
    let before = file(dir.path());

    let Err(AppError::Conflict(why)) = connect(&server, &origin) else {
        panic!("both have pages and the repository was connected");
    };
    assert!(why.contains("both"), "{why}");
    assert_eq!(file(dir.path()), before, "the file is as it was");
    assert_eq!(server.app.wiki_status().state, State::Local);
    assert_eq!(
        git(&origin, &["log", "--format=%s", "main"]).unwrap(),
        "Edit elsewhere"
    );
    assert!(wiki.page("Ours", None).is_ok());

    // A wiki with no page takes the pages of the repository.
    let fresh = tempfile::tempdir().unwrap();
    let second = self::server(fresh.path(), "");
    let status = connect(&second, &origin).unwrap();
    assert_eq!(status.state, State::Remote);
    let adopted = second.app.wiki().unwrap();
    assert_eq!(adopted.page("Theirs", None).unwrap().text, "Their page.\n");
    adopted
        .write_page(&second.app, &ada(), "Ours", "Our page.", None)
        .unwrap();
    assert_eq!(
        git(&origin, &["log", "--format=%s", "main"]).unwrap(),
        "Add Ours\nEdit elsewhere"
    );
}

#[test]
fn a_repository_that_cannot_be_reached_is_refused_and_the_file_stays() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), "");
    let before = file(dir.path());
    let missing = dir.path().join("nowhere.git");
    let Err(AppError::Invalid(why)) = connect(&server, &missing) else {
        panic!("a repository that is not there was connected");
    };
    assert!(why.contains("cannot be reached"), "{why}");
    assert_eq!(file(dir.path()), before);
    assert!(matches!(
        server
            .live
            .connect_wiki(&server.app, &Actor::first_owner(), "  "),
        Err(AppError::Invalid(_))
    ));
    assert_eq!(server.app.wiki_status().state, State::Local);
}

#[test]
fn a_url_in_the_file_for_a_wiki_kept_on_this_server_is_connected_at_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    {
        let server = server(dir.path(), "");
        let wiki = server.app.wiki().unwrap();
        wiki.write_page(&server.app, &ada(), "Onboarding", "Welcome.", None)
            .unwrap();
    }
    let origin = bare_origin(dir.path());
    let server = server(
        dir.path(),
        &format!("[wiki]\nurl = {:?}\n", origin.display().to_string()),
    );
    assert_eq!(server.app.wiki_status().state, State::Remote);
    assert_eq!(
        git(&origin, &["log", "--format=%s", "main"]).unwrap(),
        "Add Onboarding"
    );
}

#[test]
fn the_wiki_url_set_through_the_settings_is_used_after_a_restart_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), "");
    let changes = Changes {
        set: BTreeMap::from([("wiki.url".to_string(), json!("/srv/wiki.git"))]),
        ..Changes::default()
    };
    let saved = server
        .live
        .save(&server.app, &Actor::first_owner(), &changes)
        .unwrap();
    assert!(saved.restart_needed);
    assert_eq!(saved.restart_keys, ["wiki.url"]);
    let values = server.live.view().unwrap().values;
    assert_eq!(
        values["wiki.url"]["value"],
        Value::String("/srv/wiki.git".into())
    );
    assert_eq!(server.app.wiki_status().state, State::Local);
}

/// Connects as `connect_wiki` does, but the file cannot be written afterwards: it is made a directory once the wiki is
/// connected, which is what the write then fails on.
fn connect_but_fail_to_write(server: &Server, dir: &Path, url: &Path) -> AppError {
    let file = dir.join("moebius.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    let error = server
        .live
        .connect_wiki_with(
            &server.app,
            &Actor::first_owner(),
            &url.to_string_lossy(),
            |app, wiki_dir, url| {
                let connected = crate::wiki::connect(app, wiki_dir, url)?;
                std::fs::remove_file(&file).unwrap();
                std::fs::create_dir(&file).unwrap();
                Ok(connected)
            },
        )
        .expect_err("the file could not be written");
    std::fs::remove_dir(&file).unwrap();
    std::fs::write(&file, text).unwrap();
    error
}

#[test]
fn a_connection_whose_file_cannot_be_written_is_undone_and_the_same_request_succeeds_after() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), "");
    let wiki = server.app.wiki().unwrap();
    wiki.write_page(&server.app, &ada(), "Ours", "Our page.", None)
        .unwrap();
    let origin = bare_origin(dir.path());
    connect_but_fail_to_write(&server, dir.path(), &origin);
    assert_eq!(server.app.wiki_status().state, State::Local);
    assert_eq!(wiki.url(), None);
    assert_eq!(server.live.get().wiki_url, None);
    assert!(
        git(&dir.path().join(".moebius/wiki"), &["remote"])
            .unwrap()
            .is_empty(),
        "origin was left"
    );
    // The history was pushed before the file failed; the same repository is accepted again, since it holds this wiki.
    let status = connect(&server, &origin).unwrap();
    assert_eq!(status.state, State::Remote);
    wiki.write_page(&server.app, &ada(), "More", "More.", None)
        .unwrap();
    assert_eq!(
        git(&origin, &["log", "--format=%s", "main"]).unwrap(),
        "Add More\nAdd Ours"
    );
}

#[test]
fn an_adopted_repository_is_given_back_when_the_file_cannot_be_written() {
    let dir = tempfile::tempdir().unwrap();
    let server = server(dir.path(), "");
    let origin = bare_origin(dir.path());
    push_from_elsewhere(dir.path(), &origin, "Theirs.md", "Their page.\n");
    connect_but_fail_to_write(&server, dir.path(), &origin);
    assert_eq!(server.app.wiki_status().state, State::Local);
    let wiki = server.app.wiki().unwrap();
    assert!(wiki.pages().unwrap().is_empty(), "the pages were kept");
    assert!(!dir.path().join(".moebius/wiki.previous").exists());
    let status = connect(&server, &origin).unwrap();
    assert_eq!(status.state, State::Remote);
    assert_eq!(wiki.page("Theirs", None).unwrap().text, "Their page.\n");
    assert!(!dir.path().join(".moebius/wiki.previous").exists());
}

#[test]
fn a_wiki_opened_for_a_connection_that_fails_to_be_written_is_closed_again() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nowhere.git");
    let server = server(
        dir.path(),
        &format!("[wiki]\nurl = {:?}\n", missing.display().to_string()),
    );
    let origin = bare_origin(dir.path());
    connect_but_fail_to_write(&server, dir.path(), &origin);
    assert_eq!(server.app.wiki_status().state, State::Unavailable);
    assert_eq!(connect(&server, &origin).unwrap().state, State::Remote);
}

#[test]
fn a_repository_that_was_the_origin_before_the_url_was_removed_can_be_connected_again() {
    let dir = tempfile::tempdir().unwrap();
    let origin = bare_origin(dir.path());
    let url = format!("[wiki]\nurl = {:?}\n", origin.display().to_string());
    {
        let server = server(dir.path(), &url);
        let wiki = server.app.wiki().unwrap();
        wiki.write_page(&server.app, &ada(), "Ours", "Our page.", None)
            .unwrap();
    }
    // The person takes `[wiki] url` out: the clone keeps its origin, and is a wiki of its own again.
    let server = server(dir.path(), "");
    assert_eq!(server.app.wiki_status().state, State::Local);
    server
        .app
        .wiki()
        .unwrap()
        .write_page(&server.app, &ada(), "Later", "Later.", None)
        .unwrap();
    assert_eq!(connect(&server, &origin).unwrap().state, State::Remote);
    assert_eq!(
        git(&origin, &["log", "--format=%s", "main"]).unwrap(),
        "Add Later\nAdd Ours"
    );
    // A repository whose history this one does not extend is still refused.
    let other = dir.path().join("other.git");
    git(
        dir.path(),
        &["init", "--quiet", "--bare", "-b", "main", "other.git"],
    )
    .unwrap();
    push_from_elsewhere(dir.path(), &other, "Theirs.md", "Their page.\n");
    let second = tempfile::tempdir().unwrap();
    let local = self::server(second.path(), "");
    local
        .app
        .wiki()
        .unwrap()
        .write_page(&local.app, &ada(), "Ours", "Our page.", None)
        .unwrap();
    assert!(matches!(
        connect(&local, &other),
        Err(AppError::Conflict(_))
    ));
}

#[test]
fn a_server_without_a_wiki_answers_a_repository_with_pages_of_its_own_with_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    {
        let server = server(dir.path(), "");
        server
            .app
            .wiki()
            .unwrap()
            .write_page(&server.app, &ada(), "Ours", "Our page.", None)
            .unwrap();
    }
    let origin = bare_origin(dir.path());
    push_from_elsewhere(dir.path(), &origin, "Theirs.md", "Their page.\n");
    // The file names a repository whose pages this wiki does not have: the server starts without a wiki.
    let server = server(
        dir.path(),
        &format!("[wiki]\nurl = {:?}\n", origin.display().to_string()),
    );
    assert_eq!(server.app.wiki_status().state, State::Unavailable);
    let Err(error) = connect(&server, &origin) else {
        panic!("both have pages and the repository was connected");
    };
    assert!(matches!(error, AppError::Conflict(_)), "{error:?}");
}
