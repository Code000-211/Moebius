//! Lessons (issue 116): what the Retro learned from earlier cards, so that a run does not repeat what went wrong on
//! similar ones. The Retro analyst ends its answer with a `lessons` block, one lesson for a failed run, a review
//! rejection or a card that merged at once (`store_written`). The lessons are kept per repository with their source
//! cards (`store::lessons`); at the start of a code or review run the best matches for the issue's text are put in
//! the prompt under "Lessons from earlier cards" (`for_prompt`), and the Pi tool `lessons` (see `tool`) finds more.
//!
//! A lesson is retired when it was not retrieved for `UNUSED_DAYS` days, which the schedule checks (`on_schedule`),
//! and when a trial run it was given to was negative (`retire_harmful`). A person edits and deletes lessons on the
//! Retro page. This module holds the text of lessons, how they are written and chosen; `tool` is the route of the tool.

pub mod tool;

use serde::Deserialize;

use crate::app::App;
use crate::blobs::redact;
use crate::store::{Card, Lesson, NewLesson, Store, now};

/// How many lessons a run's prompt holds at most.
pub const PROMPT_LESSONS: usize = 5;

/// How many lessons the tool answers at most.
pub const TOOL_LESSONS: usize = 8;

/// Days without a retrieval after which a lesson is retired.
pub const UNUSED_DAYS: i64 = 60;

/// How many characters each part of a lesson keeps.
const PART_LIMIT: usize = 500;

/// The kinds a lesson has.
const KINDS: [&str; 3] = ["failure", "rejection", "success"];

/// The heading of the prompt's section.
const HEADING: &str = "Lessons from earlier cards";

/// What the analyst is told about lessons, after the report.
pub const BRIEF: &str = "Besides proposals, write lessons: short notes for the agents that work on this repository's \
    later cards, from the failed runs, the review rejections and the cards that merged at once (`merged` in the \
    report). End your final answer with one fenced block, ```lessons, holding a JSON array; each entry is \
    {\"card\": <the card of the report>, \"kind\": \"failure\" | \"rejection\" | \"success\", \"situation\": <the kind \
    of task or step it is about>, \"lesson\": <what went wrong, or what worked>, \"advice\": <what to do>}. Each text \
    is one or two sentences; write what holds for similar cards, not the story of this one, and leave out anything \
    secret. Write no lesson the traces do not support.\n";

/// One lesson as the analyst writes it.
#[derive(Debug, Deserialize)]
struct Written {
    card: i64,
    #[serde(default)]
    kind: String,
    situation: String,
    lesson: String,
    advice: String,
}

fn shorten(text: &str) -> String {
    let text = redact(text.trim());
    match text.char_indices().nth(PART_LIMIT) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

/// The entries of the last `lessons` block of the analyst's answer that holds a JSON array, or why there are none to
/// read. An answer without such a block (one that only mentions the format, say) holds no lessons, which is no problem.
fn parse(answer: &str) -> Result<Vec<Written>, String> {
    let body = answer.rsplit("```lessons").find_map(|rest| {
        let body = rest.split("```").next().unwrap_or_default().trim();
        body.starts_with('[').then_some(body)
    });
    match body {
        None => Ok(Vec::new()),
        Some(body) => serde_json::from_str(body)
            .map_err(|e| format!("the lessons block is not readable: {e}")),
    }
}

/// Stores the lessons in the final answer of the analyst's run of `day`; answers what it left out and why. A lesson
/// of a card that does not exist, and one with an empty part, is left out.
pub fn store_written(app: &App, day: &str, answer: &str) -> Vec<String> {
    let written = match parse(answer) {
        Ok(written) => written,
        Err(why) => return vec![why],
    };
    let mut warnings = Vec::new();
    for entry in written {
        let card = app.store.card(entry.card).ok().flatten();
        let (situation, lesson, advice) = (
            shorten(&entry.situation),
            shorten(&entry.lesson),
            shorten(&entry.advice),
        );
        let Some(card) = card else {
            warnings.push(format!("a lesson names card {}, which is gone", entry.card));
            continue;
        };
        if [&situation, &lesson, &advice].iter().any(|t| t.is_empty()) {
            warnings.push(format!("a lesson of card {} has an empty part", entry.card));
            continue;
        }
        let kind = if KINDS.contains(&entry.kind.as_str()) {
            entry.kind
        } else {
            "failure".to_string()
        };
        let new = NewLesson {
            repository_id: card.repository_id,
            kind,
            situation,
            lesson,
            advice,
            cards: vec![card.id],
            retro_date: Some(day.to_string()),
        };
        if let Err(e) = app.store.add_lesson(&new) {
            warnings.push(format!("cannot store a lesson of card {}: {e}", card.id));
        }
    }
    warnings
}

/// The text of `lessons` as a prompt holds them.
pub fn section(lessons: &[Lesson]) -> String {
    let mut text = format!(
        "{HEADING} (notes from this repository's past cards; use what fits this task and ignore the rest):\n\n"
    );
    for lesson in lessons {
        text.push_str(&format!(
            "- When: {}\n  What happened: {}\n  Do: {}\n",
            lesson.situation, lesson.lesson, lesson.advice
        ));
    }
    text
}

/// The best lessons of the card's repository for its issue, at most [`PROMPT_LESSONS`], noted as used.
fn best(store: &Store, card: &Card) -> Vec<Lesson> {
    let text = format!("{} {}", card.title, card.body);
    let lessons = store
        .search_lessons(card.repository_id, &text, PROMPT_LESSONS)
        .unwrap_or_default();
    let ids: Vec<i64> = lessons.iter().map(|l| l.id).collect();
    if !ids.is_empty() {
        let _ = store.note_lessons_used(&ids, Some(card.id));
    }
    lessons
}

/// The prompt section for a run of `card` with this purpose: the lessons that best match the issue's text, for a code
/// or review run; none for any other, and none when nothing matches or anything fails.
pub fn for_prompt(app: &App, card: &Card, purpose: &str) -> Option<String> {
    if purpose != "code" && !purpose.starts_with("review") {
        return None;
    }
    let lessons = best(&app.store, card);
    (!lessons.is_empty()).then(|| section(&lessons))
}

/// Retires the lessons that were put in the prompts of `card`'s runs since `since`, because a trial run of the card
/// with them was negative. Answers their ids.
pub fn retire_harmful(app: &App, card: i64, since: i64, reason: &str) -> Vec<i64> {
    let ids = app
        .store
        .lessons_used_since(card, since)
        .unwrap_or_default();
    let why = format!("harmful: a trial of card {card} was negative with it ({reason})");
    ids.into_iter()
        .filter(|id| app.store.retire_lesson(*id, &why).unwrap_or(false))
        .collect()
}

/// The schedule's check: retires the lessons not retrieved for [`UNUSED_DAYS`] days.
pub fn on_schedule(app: &App) {
    let before = now() - UNUSED_DAYS * 86_400;
    let reason = format!("not retrieved for {UNUSED_DAYS} days");
    let _ = app.store.retire_unused_lessons(before, &reason);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use moebius_plugin_api::Host;

    use super::*;
    use crate::store::Store;

    fn app() -> (tempfile::TempDir, Arc<App>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(dir.path()).unwrap());
        let app = App::new(
            store,
            Arc::new(Host::new(std::time::Duration::from_secs(5))),
            dir.path(),
        );
        (dir, Arc::new(app))
    }

    #[test]
    fn the_analysts_answer_gives_lessons_and_warnings() {
        let (_dir, app) = app();
        let repo = app
            .store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = app
            .store
            .add_card(repo.id, "Parser", "", "leon", "Done")
            .unwrap();
        let answer = format!(
            "Done.\n\n```lessons\n[\
             {{\"card\": {}, \"kind\": \"rejection\", \"situation\": \"Changing the parser\", \
               \"lesson\": \"The reviewer wanted a test\", \"advice\": \"Add a parser test token=sk-ant-abcdefghijklmnopqrstuvwxyz\"}},\
             {{\"card\": 999, \"situation\": \"x\", \"lesson\": \"y\", \"advice\": \"z\"}},\
             {{\"card\": {}, \"kind\": \"odd\", \"situation\": \" \", \"lesson\": \"y\", \"advice\": \"z\"}}\
             ]\n```\n",
            card.id, card.id
        );
        let warnings = store_written(&app, "2026-03-10", &answer);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        let stored = app.store.lessons(Some(repo.id), false).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].kind, "rejection");
        assert_eq!(stored[0].cards, [card.id]);
        assert_eq!(stored[0].retro_date.as_deref(), Some("2026-03-10"));
        assert!(
            !stored[0]
                .advice
                .contains("sk-ant-abcdefghijklmnopqrstuvwxyz")
        );

        assert!(store_written(&app, "d", "no block at all").is_empty());
        let broken = store_written(&app, "d", "```lessons\n[not json\n```");
        assert!(store_written(&app, "d", "```lessons\nnot an array\n```").is_empty());
        assert!(broken[0].contains("not readable"), "{broken:?}");
    }

    #[test]
    fn a_code_or_review_prompt_gets_the_best_matching_lessons_and_they_count_as_used() {
        let (_dir, app) = app();
        let repo = app
            .store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = app
            .store
            .add_card(
                repo.id,
                "Fix the parser",
                "It breaks on a new token",
                "leon",
                "In progress",
            )
            .unwrap();
        for (situation, advice) in [
            ("Parser changes", "Run the tokenizer tests"),
            ("Database migrations", "Bump the schema version"),
        ] {
            app.store
                .add_lesson(&NewLesson {
                    repository_id: repo.id,
                    kind: "failure".into(),
                    situation: situation.into(),
                    lesson: "it went wrong".into(),
                    advice: advice.into(),
                    cards: vec![1],
                    retro_date: None,
                })
                .unwrap();
        }
        let text = for_prompt(&app, &card, "code").unwrap();
        assert!(text.starts_with("Lessons from earlier cards"), "{text}");
        assert!(text.contains("Run the tokenizer tests"));
        assert!(!text.contains("schema version"));
        assert!(for_prompt(&app, &card, "review-spec").is_some());
        assert!(for_prompt(&app, &card, "chat").is_none());
        let lessons = app.store.lessons(Some(repo.id), false).unwrap();
        let parser = lessons
            .iter()
            .find(|l| l.situation == "Parser changes")
            .unwrap();
        assert_eq!(parser.retrievals, 2);
        assert!(parser.last_retrieved_at.is_some());

        let other = app
            .store
            .add_card(repo.id, "Colours", "Paint it red", "leon", "In progress")
            .unwrap();
        assert!(for_prompt(&app, &other, "code").is_none());
    }

    #[test]
    fn lessons_are_retired_when_unused_for_sixty_days_or_harmful_in_a_trial() {
        let (_dir, app) = app();
        let repo = app
            .store
            .add_repository("demo", "/srv/demo", "none", "main")
            .unwrap();
        let card = app
            .store
            .add_card(repo.id, "Fix the parser", "token", "leon", "In progress")
            .unwrap();
        let add = |situation: &str| {
            app.store
                .add_lesson(&NewLesson {
                    repository_id: repo.id,
                    kind: "success".into(),
                    situation: situation.into(),
                    lesson: "parser token".into(),
                    advice: format!("advice {situation}"),
                    cards: vec![],
                    retro_date: None,
                })
                .unwrap()
                .unwrap()
        };
        let (old, fresh, harmful) = (add("old"), add("fresh"), add("harmful"));
        let day = 86_400;
        app.store.age_lesson(old.id, now() - 70 * day, None);
        app.store
            .age_lesson(fresh.id, now() - 70 * day, Some(now() - 10 * day));
        on_schedule(&app);
        assert_eq!(app.store.lessons(None, false).unwrap().len(), 2);
        assert_eq!(app.store.lesson(old.id).unwrap().unwrap().status, "retired");

        let since = now();
        app.store
            .note_lessons_used(&[harmful.id], Some(card.id))
            .unwrap();
        // Lessons given to the card before the trial started are not the trial's.
        assert!(retire_harmful(&app, card.id, since + 10, "still failing").is_empty());
        assert_eq!(
            retire_harmful(&app, card.id, since, "still failing"),
            [harmful.id]
        );
        let retired = app.store.lesson(harmful.id).unwrap().unwrap();
        assert!(
            retired
                .retired_reason
                .unwrap()
                .starts_with("harmful: a trial of card")
        );
        assert!(
            app.store
                .search_lessons(repo.id, "parser token", 5)
                .unwrap()
                .iter()
                .all(|l| l.id == fresh.id)
        );
    }
}
