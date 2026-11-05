//! What a run of a card is told: the issue, the instructions of its purpose (`code`, the review axes, the choice between
//! attempts), the issue's comments, the files likely to be relevant, the lessons of earlier cards, the rule of red then green in a repository
//! that works test first, and the notes of the last round. The same for every harness.

use crate::app::App;
use crate::blobs::redact;
use crate::store::Card;

/// The most characters of an issue's comments a run is told; older comments are left out first.
const COMMENTS_LIMIT: usize = 8_000;

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
     CONTRIBUTING.md, CLAUDE.md and AGENTS.md, whichever of them exist, and in the review notes below when there \
     are any: read them first, and quote the rule each blocking finding breaks. Skip what the repository's tooling enforces. Besides those, look \
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

/// What a `code` run is told in a repository that works test first (see `app::reproduce`).
const TEST_FIRST_PROMPT: &str = "This repository works test first: red, then green. Before you change any code, write \
     a test that reproduces the issue, in a file of its own, and run it to see it fail. Then make the change and run \
     it again to see it pass. Name the test in your final summary, in the lines `REPRODUCE: <the command that runs only \
     that test>` and, one for each file that holds the test, `REPRODUCE-FILE: <its path from the repository's \
     root>`, in every run's summary. Moebius runs the command on the commit your change starts from with those \
     files added, where it must fail, and on your change, where it must pass; otherwise the card comes back to you. \
     If the issue cannot be tested this way, as for documentation or configuration only, write the single line \
     `REPRODUCE: untestable: <why>` instead; the reviewer judges the reason.";

/// The prompt for a run of `card` that the card's board starts: as [`prompt_for`], and a `code` run is also given the
/// files that are likely relevant (see `App::localize_hint`), when there is such a hint.
pub(crate) fn run_prompt(app: &App, card: &Card, purpose: &str, note: Option<&str>) -> String {
    let hint = matches!(purpose, "code" | "attempt")
        .then(|| app.localize_hint(card))
        .flatten();
    prompt_with(app, card, purpose, note, hint.as_deref())
}

/// The prompt for a run of `card`; `note` is added under a heading of its own. A `code` run in a repository that works
/// test first is told the rule of red, then green.
pub(crate) fn prompt_for(app: &App, card: &Card, purpose: &str, note: Option<&str>) -> String {
    prompt_with(app, card, purpose, note, None)
}

/// The text of the project's `review.md`, the repository's own notes for its reviews, when it has any.
fn review_notes(app: &App, card: &Card) -> Option<String> {
    let repository = app.store.repository(card.repository_id).ok().flatten()?;
    app.project_file(&repository, "review.md")
        .filter(|notes| !notes.trim().is_empty())
}

/// The comments of the card's issue, newest kept first when they are too long, with their authors, or none when it has
/// none. A run cannot look them up, so they come with the issue.
fn comments_section(app: &App, card: &Card) -> Option<String> {
    let comments = app.store.comments(card.id).ok()?;
    let mut kept = Vec::new();
    let mut length = 0;
    for comment in comments.iter().rev() {
        let author = match comment.author.strip_prefix("agent:") {
            Some(agent) => format!("the agent {agent}"),
            None if comment.author == "person" => "a person".to_string(),
            None => comment.author.clone(),
        };
        let text = format!("{author} wrote:\n{}\n", redact(comment.body.trim()));
        length += text.chars().count();
        if length > COMMENTS_LIMIT && !kept.is_empty() {
            break;
        }
        kept.push(text);
    }
    if kept.is_empty() {
        return None;
    }
    kept.reverse();
    let left_out = comments.len() - kept.len();
    let mut section = String::from("Comments on the issue, oldest first:\n");
    if left_out > 0 {
        section.push_str(&format!("({left_out} older comments are left out)\n"));
    }
    for text in kept {
        section.push('\n');
        section.push_str(&text);
    }
    Some(section)
}

/// Where the issue lives and what that means for a run: it has no way to look the issue up, so it is told not to try.
fn tracker_note(app: &App, card: &Card) -> String {
    let forge = app
        .store
        .repository(card.repository_id)
        .ok()
        .flatten()
        .map(|repository| repository.forge)
        .unwrap_or_default();
    let kept = match forge.as_str() {
        "" | "none" => {
            "The issues of this repository are kept by Moebius itself, not on a forge".to_string()
        }
        forge => format!("The issues of this repository are kept on {forge}"),
    };
    format!(
        "{kept}, and the issue above with its comments is all of it that you are given. Do not look for it with `gh` \
         or another tracker's command line: this run has no login for one. If something the issue leaves out matters, \
         say so in your summary."
    )
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
    if let Some(comments) = comments_section(app, card) {
        prompt.push('\n');
        prompt.push_str(&comments);
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
            if let Some(notes) = review_notes(app, card) {
                prompt.push_str("\n\nThe review notes of this repository's project:\n\n");
                prompt.push_str(notes.trim());
            }
        }
        "review-spec" => {
            prompt.push_str(&SPEC_PROMPT.replace("{base}", &base()));
            prompt.push_str(FINDINGS_FORMAT);
        }
        "review-select" => prompt.push_str(&SELECT_PROMPT.replace("{base}", &base())),
        _ => {
            prompt.push_str(CODE_PROMPT);
            prompt.push(' ');
            prompt.push_str(&tracker_note(app, card));
        }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;

    fn setup(forge: &str) -> (tempfile::TempDir, App, Card) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let repo = store
            .add_repository("demo", "/srv/demo", forge, "main")
            .unwrap();
        let card = store
            .add_card(repo.id, "Fix it", "The body.", "person", "Not started")
            .unwrap();
        let app = App::new(
            store,
            Arc::new(Host::new(Duration::from_secs(1))),
            dir.path(),
        );
        (dir, app, card)
    }

    #[test]
    fn a_run_is_told_the_comments_of_its_issue_with_their_authors() {
        let (_dir, app, card) = setup("none");
        for (author, body) in [
            ("person", "Use the old API."),
            ("agent:Moebius", "Noted."),
            ("sam", "Key sk-ant-abcdefghijklmnopqrstu is wrong."),
        ] {
            app.store
                .add_comment(card.id, author, body, None, None)
                .unwrap();
        }
        let prompt = prompt_for(&app, &card, "code", None);
        let at = |text: &str| {
            prompt
                .find(text)
                .unwrap_or_else(|| panic!("{text}: {prompt}"))
        };
        assert!(at("The body.") < at("Comments on the issue, oldest first:"));
        assert!(at("a person wrote:\nUse the old API.") < at("the agent Moebius wrote:\nNoted."));
        assert!(at("the agent Moebius wrote") < at("sam wrote:"));
        assert!(at("sam wrote:") < at("You are in a git worktree"));
        assert!(!prompt.contains("sk-ant-abcdefghijklmnopqrstu"), "{prompt}");
        // An issue without comments has no heading for them.
        let (_dir, app, card) = setup("none");
        assert!(!prompt_for(&app, &card, "code", None).contains("Comments on the issue"));
    }

    #[test]
    fn comments_too_long_to_tell_leave_the_oldest_out() {
        let (_dir, app, card) = setup("none");
        for n in 0..5 {
            let body = format!("comment {n} {}", "x".repeat(3_000));
            app.store
                .add_comment(card.id, "person", &body, None, None)
                .unwrap();
        }
        let prompt = prompt_for(&app, &card, "code", None);
        assert!(prompt.contains("comment 4 ") && !prompt.contains("comment 0 "));
        assert!(prompt.contains("older comments are left out"), "{prompt}");
    }

    #[test]
    fn a_code_run_is_told_where_its_issue_lives_and_not_to_look_for_it() {
        let (_dir, app, card) = setup("none");
        let prompt = prompt_for(&app, &card, "code", None);
        assert!(
            prompt.contains("kept by Moebius itself, not on a forge"),
            "{prompt}"
        );
        assert!(prompt.contains("Do not look for it with `gh`"), "{prompt}");
        let (_dir, app, card) = setup("forgejo");
        let prompt = prompt_for(&app, &card, "code", None);
        assert!(prompt.contains("kept on forgejo"), "{prompt}");
        // A review is not told: it reads the change, not the tracker.
        assert!(!prompt_for(&app, &card, "review", None).contains("Do not look for it"));
    }
}
