//! What a run of a card over several repositories (ADR 0019) is told of them: a `code` run or an attempt where each of
//! its worktrees is and which repositories it may ask for, and a review where the change in each is, so that it
//! reviews them together.

use super::super::lifecycle::RunningRun;
use super::review_notes;
use crate::app::App;
use crate::store::Card;

/// What a `code` run or an attempt of `card` is told of its repositories (ADR 0019): where the worktree of each is,
/// when the card works in several, and, for a `code` run, which of the project's repositories it may ask for, with a
/// line `REPOSITORY: <name>` of its summary, when there are any. Nothing for a card in one repository of a project that has
/// no other.
pub(in crate::harness::run) fn repositories_prompt(
    app: &App,
    card: &Card,
    run: &RunningRun,
) -> String {
    // An attempt is told where its worktrees are, but only a `code` run's ask for a repository is acted on.
    let may_ask = run.purpose == "code";
    let mut text = String::new();
    if !run.others.is_empty() {
        let own = card.repositories.first().cloned().unwrap_or_default();
        let mut places = vec![format!(
            "`{own}` at {} (your working directory)",
            run.worktree.display()
        )];
        places.extend(
            run.others
                .iter()
                .map(|other| format!("`{}` at {}", other.repository, other.worktree.display())),
        );
        text.push_str(&format!(
            "\n\n## Repositories\n\nThe card works in these repositories, each in a git worktree of its own: {}. \
             Make the change in each one the issue needs, and write nowhere else. Moebius commits what you leave in \
             each worktree, and each repository gets a change of its own.",
            places.join(", ")
        ));
    }
    let more: Vec<String> = may_ask
        .then(|| app.repositories_to_add(card).ok())
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|r| format!("`{}`", r.name))
        .collect();
    if !more.is_empty() {
        if text.is_empty() {
            text.push_str("\n\n## Repositories\n\n");
        } else {
            text.push(' ');
        }
        text.push_str(&format!(
            "If the work needs another repository of the project ({}), end your summary with a line \
             `REPOSITORY: <name>` for each: the card then comes back to you with a worktree of it.",
            more.join(", ")
        ));
    }
    text
}

/// What a review of a card over several repositories is told after its instructions: where the change in each
/// repository is and how to see it, as one change of the card reviewed together. The standards axis reviews each
/// repository against its own standards and review notes, the spec axis judges the issue over all of them, and a
/// single review does both. Nothing for a card in one repository.
pub(in crate::harness::run) fn review_repositories_prompt(
    app: &App,
    card: &Card,
    run: &RunningRun,
) -> String {
    if run.others.is_empty() {
        return String::new();
    }
    let repositories = app.card_repositories(card).unwrap_or_default();
    let base_of = |name: &str| {
        repositories
            .iter()
            .find(|repository| repository.name == name)
            .map_or_else(|| "main".to_string(), |repository| repository.base_ref())
    };
    let own = card.repositories.first().cloned().unwrap_or_default();
    let mut places = vec![format!(
        "`{own}` at {} (your working directory): `git diff {}...HEAD`",
        run.worktree.display(),
        base_of(&own)
    )];
    places.extend(run.others.iter().map(|other| {
        format!(
            "`{}` at {}: `git -C {} diff {}...HEAD`",
            other.repository,
            other.worktree.display(),
            other.worktree.display(),
            base_of(&other.repository)
        )
    }));
    let mut text = format!(
        "\n\n## Repositories\n\nThe card works in several repositories, each in a git worktree of its own on the \
         branch that holds its change, and its changes are reviewed together and merged together. See the change in \
         each with the command after it:\n\n{}\n\nAn empty diff means the card changes nothing there. Review every \
         one of them; name the repository in each finding.",
        places
            .iter()
            .map(|place| format!("- {place}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let standards = run.purpose != "review-spec";
    let spec = run.purpose != "review-standards";
    if standards {
        text.push_str(
            " Each repository has its own standards: read the CONTRIBUTING.md, CLAUDE.md and AGENTS.md in its \
             worktree, and its review notes below, and judge its change against those alone.",
        );
    }
    if spec {
        text.push_str(
            " Judge the issue over all the changes together: what it asks may be done in any of the repositories, and \
             it is done only when the changes together do it.",
        );
    }
    if standards {
        for repository in repositories.iter().skip(1) {
            if let Some(notes) = review_notes(app, repository) {
                text.push_str(&format!(
                    "\n\nThe review notes of `{}`:\n\n{}",
                    repository.name,
                    notes.trim()
                ));
            }
        }
    }
    text
}
