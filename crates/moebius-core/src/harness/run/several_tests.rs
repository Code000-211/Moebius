//! The tests of a card over several repositories (ADR 0019): a run gets a worktree of each and commits what it left in
//! each, each repository gets its own change, and a run that asks for another repository sends the card back with it.
//! On the fake `pi-bolt` of `testdata`.

use std::path::{Path, PathBuf};

use super::testing::{Setup, prompt_of, setup, wait_until_ended};
use crate::board::{CardRuns, Variant};
use crate::decide::Actor;
use crate::repos::git;
use crate::store::{Card, Repository, Run};

/// A git repository with one commit on `main` at `dir/name`, added to Moebius as `name` and to the project of the
/// setup's repository.
fn second_repository(setup: &Setup, name: &str) -> Repository {
    let origin = setup
        .app
        .data_dir
        .parent()
        .unwrap()
        .join(format!("origin-{name}"));
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
    std::fs::write(origin.join("README.md"), format!("{name}\n")).unwrap();
    git(&origin, &["add", "."]).unwrap();
    git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
    let repository = setup
        .app
        .add_repository(name, origin.to_str().unwrap())
        .unwrap();
    let project = setup
        .app
        .store
        .repository(setup.repo)
        .unwrap()
        .unwrap()
        .home_project_id;
    setup
        .app
        .store
        .link_repository(project, repository.id)
        .unwrap();
    repository
}

/// Starts a `code` run of `card` on Pi-Bolt and waits for it to end.
fn code_run(setup: &Setup, card: &Card) -> Run {
    round(setup, card, None)
}

/// Starts a `code` run of `card` on Pi-Bolt, which also writes `text` to `attempt.txt` when there is one, so that a
/// later round changes something, and waits for it to end.
fn round(setup: &Setup, card: &Card, text: Option<&str>) -> Run {
    let model = text.map(|text| format!("qwen-local:{text}"));
    let variant = Variant {
        model: model.as_deref(),
        runner: None,
    };
    let run = setup
        .harnesses
        .pi_bolt
        .start(&setup.app, card, "Coder", ("code", None), variant)
        .unwrap();
    wait_until_ended(&setup.app, run.id)
}

fn done_on(clone: &str, branch: &str) -> String {
    git(Path::new(clone), &["show", &format!("{branch}:done.txt")]).unwrap()
}

#[test]
fn a_run_of_a_card_with_two_repositories_has_both_worktrees_and_makes_two_changes() {
    let setup = setup();
    let web = second_repository(&setup, "web");
    let card = setup
        .app
        .create_issue(setup.repo, "Change both [everywhere]", "", "person")
        .unwrap();
    let card = setup
        .app
        .set_card_repositories(&Actor::first_owner(), card.id, &["web".into()], "both")
        .unwrap();
    assert_eq!(card.repositories, ["demo", "web"]);

    let run = code_run(&setup, &card);
    assert_eq!(run.status, "succeeded");
    let worktrees = setup.app.store.run_worktrees(run.id).unwrap();
    assert_eq!(
        worktrees
            .iter()
            .map(|w| w.repository_id)
            .collect::<Vec<_>>(),
        [setup.repo, web.id]
    );
    // The run was told where both are, and both are gone once it ended.
    let prompt = prompt_of(&setup.app, &run);
    for worktree in &worktrees {
        assert!(prompt.contains(&worktree.path), "{prompt}");
        assert!(!PathBuf::from(&worktree.path).exists());
    }
    // What it left in each is committed on its branch of each repository.
    let demo = setup.app.store.repository(setup.repo).unwrap().unwrap();
    assert_eq!(done_on(&demo.path, &run.branch()), "done");
    assert_eq!(done_on(&web.path, &run.branch()), "done");

    let changes = setup
        .app
        .publish_card_changes(card.id, false, None)
        .unwrap();
    assert_eq!(
        changes
            .iter()
            .map(|c| (c.repository_id, c.branch.as_str()))
            .collect::<Vec<_>>(),
        [
            (Some(setup.repo), "moebius/card-1"),
            (
                Some(web.id),
                format!("moebius/card-{}-1", card.project_key).as_str()
            ),
        ]
    );
    assert_eq!(setup.app.open_changes(&card).unwrap(), changes);
    assert_eq!(done_on(&web.path, &changes[1].branch), "done");

    // The next round starts in each repository from where the card's work there stands.
    let next = round(&setup, &setup.app.card(card.id).unwrap(), Some("two"));
    assert_eq!(next.status, "succeeded");
    let started = setup.app.store.run_worktrees(next.id).unwrap();
    let web_head = git(Path::new(&web.path), &["rev-parse", &run.branch()]).unwrap();
    assert_eq!(started[1].start, web_head);
}

#[test]
fn a_repository_with_nothing_of_the_card_gets_no_change() {
    let setup = setup();
    second_repository(&setup, "web");
    let card = setup
        .app
        .create_issue(setup.repo, "Change the API only", "", "person")
        .unwrap();
    let card = setup
        .app
        .set_card_repositories(&Actor::first_owner(), card.id, &["web".into()], "")
        .unwrap();
    assert_eq!(code_run(&setup, &card).status, "succeeded");
    let changes = setup
        .app
        .publish_card_changes(card.id, false, None)
        .unwrap();
    assert_eq!(
        changes.iter().map(|c| c.repository_id).collect::<Vec<_>>(),
        [Some(setup.repo)]
    );
}

#[test]
fn a_result_asking_for_a_third_repository_sends_the_card_back_with_it_added() {
    let setup = setup();
    let web = second_repository(&setup, "web");
    let docs = second_repository(&setup, "docs");
    let card = setup
        .app
        .create_issue(setup.repo, "Change all three [ask docs]", "", "person")
        .unwrap();
    let card = setup
        .app
        .set_card_repositories(&Actor::first_owner(), card.id, &["web".into()], "")
        .unwrap();
    let run = code_run(&setup, &card);
    // The run was told which repositories it may ask for.
    let prompt = prompt_of(&setup.app, &run);
    assert!(prompt.contains("`REPOSITORY: <name>`"), "{prompt}");
    assert!(prompt.contains("`docs`"), "{prompt}");

    assert_eq!(run.status, "succeeded");
    let card = setup.app.card(card.id).unwrap();
    assert_eq!(card.repositories, ["demo", "web", "docs"]);
    let note = card.note.clone().expect("the card goes back to coding");
    assert!(note.contains("You asked for docs"), "{note}");

    // The next round has a worktree of the third, and asks for nothing more.
    let next = round(&setup, &card, Some("two"));
    assert_eq!(next.status, "succeeded");
    let repositories: Vec<i64> = setup
        .app
        .store
        .run_worktrees(next.id)
        .unwrap()
        .iter()
        .map(|w| w.repository_id)
        .collect();
    assert_eq!(repositories, [setup.repo, web.id, docs.id]);
    assert_eq!(setup.app.card(card.id).unwrap().note, None);
}

#[test]
fn a_result_asking_for_no_repository_of_the_project_says_which_it_may_ask_for() {
    let setup = setup();
    second_repository(&setup, "web");
    let card = setup
        .app
        .create_issue(setup.repo, "Change it [ask billing]", "", "person")
        .unwrap();
    let run = code_run(&setup, &card);
    assert_eq!(run.status, "succeeded");
    let card = setup.app.card(card.id).unwrap();
    assert_eq!(card.repositories, ["demo"]);
    let note = card.note.expect("the card goes back to coding");
    assert!(note.contains("billing, which is no repository"), "{note}");
    assert!(note.contains("you may ask for web"), "{note}");
}

#[test]
fn only_the_project_s_repositories_can_be_named_and_the_card_s_own_stays_first() {
    let setup = setup();
    second_repository(&setup, "web");
    let card = setup
        .app
        .create_issue(setup.repo, "Change", "", "person")
        .unwrap();
    let by = Actor::first_owner();
    // A repository of no project of the card's is refused.
    let outsider = setup.app.data_dir.parent().unwrap().join("outsider");
    std::fs::create_dir_all(&outsider).unwrap();
    git(&outsider, &["init", "--quiet", "-b", "main"]).unwrap();
    git(
        &outsider,
        &["commit", "--quiet", "--allow-empty", "-m", "first"],
    )
    .unwrap();
    setup
        .app
        .add_repository("outsider", outsider.to_str().unwrap())
        .unwrap();
    let refused = setup
        .app
        .set_card_repositories(&by, card.id, &["outsider".into()], "")
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("not a repository of the project"),
        "{refused}"
    );
    let card = setup
        .app
        .set_card_repositories(
            &by,
            card.id,
            &["WEB".into(), "demo".into(), "web".into()],
            "",
        )
        .unwrap();
    assert_eq!(card.repositories, ["demo", "web"]);
    let judgement = setup
        .app
        .store
        .judgements_for_card(card.id)
        .unwrap()
        .into_iter()
        .find(|j| j.kind == "set_repositories")
        .expect("the naming is recorded");
    assert_eq!(judgement.decider, by.decider());
    let card = setup
        .app
        .set_card_repositories(&by, card.id, &[], "")
        .unwrap();
    assert_eq!(card.repositories, ["demo"]);
}

#[test]
fn a_run_over_several_repositories_is_not_sent_to_a_runner() {
    let setup = setup();
    second_repository(&setup, "web");
    let card = setup
        .app
        .create_issue(setup.repo, "Change both", "", "person")
        .unwrap();
    let card = setup
        .app
        .set_card_repositories(&Actor::first_owner(), card.id, &["web".into()], "")
        .unwrap();
    let variant = Variant {
        model: None,
        runner: Some("far"),
    };
    let refused = setup
        .harnesses
        .pi_bolt
        .start(&setup.app, &card, "Coder", ("code", None), variant)
        .unwrap_err();
    assert!(
        refused.to_string().contains("runs on the server"),
        "{refused}"
    );
    assert!(setup.app.store.runs_for_card(card.id).unwrap().is_empty());
}

#[test]
fn the_pull_request_of_another_repository_names_the_card_and_closes_no_issue() {
    let setup = setup();
    let web = second_repository(&setup, "web");
    let card = setup
        .app
        .create_issue(setup.repo, "Change both [everywhere]", "", "person")
        .unwrap();
    let card = setup
        .app
        .set_card_repositories(&Actor::first_owner(), card.id, &["web".into()], "")
        .unwrap();
    assert_eq!(code_run(&setup, &card).status, "succeeded");
    let demo = setup.app.store.repository(setup.repo).unwrap().unwrap();
    let own = setup
        .app
        .pull_request_body(&card, &demo, "moebius/card-1", None);
    assert!(own.starts_with("Closes #1\n"), "{own}");
    let branch = format!("moebius/card-{}-1", card.project_key);
    setup
        .app
        .publish_card_changes(card.id, false, None)
        .unwrap();
    let other = setup.app.pull_request_body(&card, &web, &branch, None);
    assert!(
        other.starts_with(&format!(
            "Part of {}-1 (issue #1 in demo)\n",
            card.project_key
        )),
        "{other}"
    );
    assert!(!other.contains("Closes"), "{other}");
}

#[test]
fn a_run_that_only_asks_for_a_repository_succeeds_and_sends_the_card_back_with_it() {
    let setup = setup();
    second_repository(&setup, "web");
    let card = setup
        .app
        .create_issue(
            setup.repo,
            "Change the web [nothing] [ask web]",
            "",
            "person",
        )
        .unwrap();
    let run = code_run(&setup, &card);
    assert_eq!(run.status, "succeeded");
    let card = setup.app.card(card.id).unwrap();
    assert_eq!(card.repositories, ["demo", "web"]);
    assert!(
        card.note
            .as_deref()
            .is_some_and(|note| note.contains("You asked for web"))
    );
    // A run that changed nothing and asked for nothing new has failed, as before.
    let again = code_run(&setup, &card);
    assert_eq!(again.status, "failed");
}

#[test]
fn a_card_is_done_once_its_last_change_is_merged_and_keeps_a_repository_with_an_open_change() {
    let setup = setup();
    let web = second_repository(&setup, "web");
    let by = Actor::first_owner();
    let card = setup
        .app
        .create_issue(setup.repo, "Change both [everywhere]", "", "person")
        .unwrap();
    setup
        .app
        .set_card_repositories(&by, card.id, &["web".into()], "")
        .unwrap();
    assert_eq!(code_run(&setup, &card).status, "succeeded");
    let (changes, moved) = setup.app.offer_change_noted(card.id, None).unwrap();
    assert!(moved);
    assert_eq!(changes.len(), 2);
    let open_items = || {
        setup
            .app
            .store
            .inbox_for_card(card.id)
            .unwrap()
            .iter()
            .filter(|item| item.resolved_at.is_none())
            .count()
    };
    assert_eq!(open_items(), 2);
    // While the change in web is open, web cannot leave the card.
    let refused = setup
        .app
        .set_card_repositories(&by, card.id, &[], "")
        .unwrap_err();
    assert!(
        refused.to_string().contains("open change in web"),
        "{refused}"
    );

    setup.app.merge_change(&by, changes[0].id, "").unwrap();
    assert_eq!(setup.app.card(card.id).unwrap().column, "Ready to merge");
    assert_eq!(open_items(), 1);
    setup.app.merge_change(&by, changes[1].id, "").unwrap();
    assert_eq!(setup.app.card(card.id).unwrap().column, "Done");
    assert_eq!(open_items(), 0);
    // The merge in web names the card, not an issue number of web's.
    let subject = git(Path::new(&web.path), &["log", "-1", "--format=%s", "main"]).unwrap();
    assert_eq!(
        subject,
        format!("Merge {}-1: Change both [everywhere]", card.project_key)
    );
}

#[test]
fn an_attempt_is_told_its_worktrees_but_not_that_it_may_ask_for_a_repository() {
    let setup = setup();
    second_repository(&setup, "web");
    second_repository(&setup, "docs");
    let card = setup
        .app
        .create_issue(setup.repo, "Change both", "", "person")
        .unwrap();
    let card = setup
        .app
        .set_card_repositories(&Actor::first_owner(), card.id, &["web".into()], "")
        .unwrap();
    let run = setup
        .harnesses
        .pi_bolt
        .start(
            &setup.app,
            &card,
            "Coder",
            ("attempt", None),
            Variant::default(),
        )
        .unwrap();
    let run = wait_until_ended(&setup.app, run.id);
    let prompt = prompt_of(&setup.app, &run);
    assert!(prompt.contains("`web` at "), "{prompt}");
    assert!(!prompt.contains("REPOSITORY:"), "{prompt}");
}
