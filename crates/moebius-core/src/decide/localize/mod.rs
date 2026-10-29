//! The decision point `localize`: before a code run, which files is the card's change likely to touch? The answer is a
//! ranked list of at most ten paths, each with a short reason, which the run is given as a hint and not a limit.
//!
//! The question carries the card's issue (`title`, `body`), the `repository`, the `card`, how many `files` to name and
//! `candidates`, the files whose text best matches the issue, for a decider that has no index of its own. The answer's
//! choice is the list as JSON text (see [`encode`]) and the question has no options, since no list of options could hold
//! it; the fail-closed answer is the empty list, which gives the run no hint.
//!
//! The first decider, [`Localizer`], needs no model: BM25 over the path and contents of the files of the default branch
//! (`index`, `text`), boosted for what the issue names literally and for the files earlier cards with similar issues
//! changed (`rank`). When the card's change is known the judgement's outcome is [`Outcome`]: the files it touched with
//! the recall at ten and the precision of the list (see `App::record_localize_outcome`). The decision model shadows the
//! kind and compares its list with the touched files in the same way (see `decide::model::localize`).

mod index;
mod rank;
mod text;

use std::collections::BTreeSet;
use std::sync::Arc;

use moebius_plugin_api::{Call, DecideRequest, Decision, Kind, Manifest, Plugin, PluginError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use index::refresh;
pub use rank::{Issue, Past, candidates, past, rank};

use crate::config::MAX_LOCALIZE_FILES;
use crate::store::Store;

/// The kind of the decision, and the name of its baseline decider.
pub const KIND: &str = "localize";

/// The answer when there is nothing to hint, and the one taken when the decider fails.
pub const EMPTY: &str = "[]";

/// One file of a hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hint {
    /// From the repository's root, with `/` between directories.
    pub path: String,
    /// Why the file is named, in a few words.
    #[serde(default)]
    pub reason: String,
}

/// The choice that holds `hints`.
pub fn encode(hints: &[Hint]) -> String {
    serde_json::to_string(hints).unwrap_or_else(|_| EMPTY.to_string())
}

/// Reads a choice: a JSON array of at most [`MAX_LOCALIZE_FILES`] objects with a `path` of a file inside the repository,
/// each once, and optionally a `reason`.
pub fn parse(choice: &str) -> Result<Vec<Hint>, String> {
    let hints: Vec<Hint> = serde_json::from_str(choice)
        .map_err(|e| format!("the answer is not a list of paths with reasons ({e})"))?;
    if hints.len() > MAX_LOCALIZE_FILES {
        return Err(format!(
            "the answer names more than {MAX_LOCALIZE_FILES} files"
        ));
    }
    let mut seen = BTreeSet::new();
    for hint in &hints {
        let inside = !hint.path.is_empty()
            && !hint.path.starts_with('/')
            && !hint
                .path
                .split('/')
                .any(|part| part.is_empty() || part == "..");
        if !inside {
            return Err(format!(
                "{:?} is not a path inside the repository",
                hint.path
            ));
        }
        if !seen.insert(&hint.path) {
            return Err(format!("{:?} is named twice", hint.path));
        }
    }
    Ok(hints)
}

/// What came of a hint, stored as the judgement's outcome as JSON once the card's change is known.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// The files the change touched, in alphabetical order.
    #[serde(default)]
    pub touched: Vec<String>,
    /// How many files the hint named.
    #[serde(default)]
    pub hinted: usize,
    /// The share of the touched files among the first ten the hint named; none when the change touched none.
    #[serde(default)]
    pub recall: Option<f64>,
    /// The share of the files the hint named that the change touched; none when it named none.
    #[serde(default)]
    pub precision: Option<f64>,
    /// True when the change could not be read, so that nothing is known.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unknown: bool,
}

impl Outcome {
    /// The outcome of a hint that named `hinted`, in rank order, for a change that touched `touched`.
    pub fn of(hinted: &[String], touched: &[String]) -> Self {
        let touched: BTreeSet<&String> = touched.iter().collect();
        Self {
            touched: touched.iter().map(|path| (*path).clone()).collect(),
            hinted: hinted.len(),
            recall: recall(hinted, &touched),
            precision: (!hinted.is_empty()).then(|| {
                hinted.iter().filter(|path| touched.contains(path)).count() as f64
                    / hinted.len() as f64
            }),
            unknown: false,
        }
    }

    /// The outcome of a change that cannot be read.
    pub fn unknown() -> Self {
        Self {
            unknown: true,
            ..Self::default()
        }
    }

    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// How many of the files the hint named were touched.
    pub fn hits(&self, hinted: &[String]) -> usize {
        hinted
            .iter()
            .filter(|path| self.touched.contains(path))
            .count()
    }
}

/// The recall at ten: the share of `touched` among the first [`MAX_LOCALIZE_FILES`] of `hinted`; none when nothing was
/// touched.
pub fn recall(hinted: &[String], touched: &BTreeSet<&String>) -> Option<f64> {
    (!touched.is_empty()).then(|| {
        let found = hinted
            .iter()
            .take(MAX_LOCALIZE_FILES)
            .filter(|path| touched.contains(path))
            .count();
        found as f64 / touched.len() as f64
    })
}

/// The baseline decider of `localize`.
pub struct Localizer {
    store: Arc<Store>,
}

impl Localizer {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    fn answer(
        &self,
        context: &std::collections::BTreeMap<String, String>,
    ) -> Result<Decision, PluginError> {
        let failed = |e: crate::store::StoreError| PluginError::Failed(e.to_string());
        let name = context
            .get("repository")
            .ok_or_else(|| PluginError::BadParams("the context has no repository".into()))?;
        let repository = self
            .store
            .repositories()
            .map_err(failed)?
            .into_iter()
            .find(|repository| &repository.name == name)
            .ok_or_else(|| PluginError::Failed(format!("there is no repository {name}")))?;
        let limit = context
            .get("files")
            .and_then(|files| files.parse::<usize>().ok())
            .unwrap_or(MAX_LOCALIZE_FILES)
            .clamp(1, MAX_LOCALIZE_FILES);
        let issue = Issue {
            title: context.get("title").map_or("", String::as_str),
            body: context.get("body").map_or("", String::as_str),
        };
        let card = context
            .get("card")
            .and_then(|card| card.parse::<i64>().ok());
        let past = past(&self.store, name, card).map_err(failed)?;
        let hints = rank(&self.store, repository.id, &issue, &past, limit).map_err(failed)?;
        Ok(Decision {
            reason: format!(
                "{} files ranked by BM25 over the default branch",
                hints.len()
            ),
            choice: encode(&hints),
            by: None,
        })
    }
}

impl Plugin for Localizer {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: KIND.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        if request.kind != KIND {
            return Err(PluginError::BadParams(format!(
                "{} is not decided here",
                request.kind
            )));
        }
        let decision = self.answer(&request.context)?;
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use crate::repos::git;
    use crate::store::{NewJudgement, Repository};

    /// A repository with these files committed on `main`, in the store.
    fn repository(dir: &Path, store: &Store, files: &[(&str, &str)]) -> Repository {
        let clone = dir.join("clone");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "--quiet", "-b", "main"]).unwrap();
        commit(&clone, files);
        store
            .add_repository("demo", clone.to_str().unwrap(), "none", "main")
            .unwrap()
    }

    fn commit(clone: &Path, files: &[(&str, &str)]) {
        for (path, text) in files {
            let file = clone.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        git(clone, &["add", "."]).unwrap();
        git(clone, &["commit", "--quiet", "-m", "files"]).unwrap();
    }

    fn ask(localizer: &Localizer, title: &str, body: &str, card: Option<i64>) -> Vec<Hint> {
        let mut context = BTreeMap::from([
            ("repository".to_string(), "demo".to_string()),
            ("title".to_string(), title.to_string()),
            ("body".to_string(), body.to_string()),
        ]);
        if let Some(card) = card {
            context.insert("card".into(), card.to_string());
        }
        let decision = localizer.answer(&context).unwrap();
        parse(&decision.choice).unwrap()
    }

    fn paths(hints: &[Hint]) -> Vec<&str> {
        hints.iter().map(|hint| hint.path.as_str()).collect()
    }

    const FILES: &[(&str, &str)] = &[
        (
            "src/parser.rs",
            "fn parse_config(text: &str) { read_tokens(text) }\n",
        ),
        (
            "src/render.rs",
            "fn draw_frame() { paint_config_panel() }\n",
        ),
        (
            "src/server/routes.rs",
            "fn routes() { mount(\"/config\") }\n",
        ),
        (
            "docs/guide.md",
            "How to write a config file, step by step.\n",
        ),
        ("Cargo.lock", "parse_config parse_config parse_config\n"),
    ];

    #[test]
    fn the_baseline_ranks_a_file_the_issue_names_first() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let repository = repository(dir.path(), &store, FILES);
        assert_eq!(
            refresh(&store, &repository).unwrap(),
            4,
            "the lock file is left out"
        );
        let localizer = Localizer::new(store);

        // The issue talks about config everywhere, and names one path.
        let hints = ask(
            &localizer,
            "The config panel is wrong",
            "See src/render.rs: the config looks wrong when a config file is read.",
            None,
        );
        assert_eq!(hints[0].path, "src/render.rs");
        assert_eq!(
            hints[0].reason,
            "named in the issue; its text matches the issue"
        );
        assert!(hints.len() <= MAX_LOCALIZE_FILES);

        // A bare file name counts too, and so does an identifier written in snake_case.
        let hints = ask(&localizer, "Crash in parse_config", "", None);
        assert_eq!(hints[0].path, "src/parser.rs");
        assert!(
            hints[0].reason.contains("holds `parse_config`"),
            "{}",
            hints[0].reason
        );
        let hints = ask(&localizer, "routes.rs forgets a slash", "", None);
        assert_eq!(hints[0].path, "src/server/routes.rs");

        // Words of the path weigh more than words of the contents; nothing that matches gives an empty list.
        let hints = ask(&localizer, "The guide is out of date", "", None);
        assert_eq!(hints[0].path, "docs/guide.md");
        assert!(ask(&localizer, "Zebra giraffe", "", None).is_empty());
    }

    #[test]
    fn the_index_is_made_again_only_when_the_default_branch_has_moved() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let repository = repository(dir.path(), &store, FILES);
        refresh(&store, &repository).unwrap();
        let first = store.localize_index_commit(repository.id).unwrap().unwrap();
        refresh(&store, &repository).unwrap();
        assert_eq!(
            store.localize_index_commit(repository.id).unwrap().unwrap(),
            first
        );

        commit(
            Path::new(&repository.path),
            &[("src/new_tokenizer.rs", "fn tokenize() {}\n")],
        );
        assert_eq!(refresh(&store, &repository).unwrap(), 5);
        assert_ne!(
            store.localize_index_commit(repository.id).unwrap().unwrap(),
            first
        );
        let localizer = Localizer::new(store);
        assert_eq!(
            ask(&localizer, "tokenize is slow", "", None)[0].path,
            "src/new_tokenizer.rs"
        );
    }

    #[test]
    fn files_that_earlier_cards_with_similar_issues_changed_are_boosted() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let repository = repository(dir.path(), &store, FILES);
        refresh(&store, &repository).unwrap();
        let localizer = Localizer::new(store.clone());
        let first = store
            .add_card(repository.id, "First", "", "person", "Ready")
            .unwrap();
        let second = store
            .add_card(repository.id, "Second", "", "person", "Ready")
            .unwrap();
        let title = "Reading the settings is slow";
        let body = "Opening the settings takes seconds on a large machine.";
        // Nothing in the files talks about settings.
        assert!(ask(&localizer, title, body, Some(second.id)).is_empty());

        // An earlier card with the same issue changed the routes.
        let context = store
            .blobs()
            .put(
                &serde_json::to_string(&BTreeMap::from([
                    ("repository", "demo"),
                    ("title", title),
                    ("body", body),
                ]))
                .unwrap(),
            )
            .unwrap();
        let earlier = store
            .add_judgement(&NewJudgement {
                kind: KIND.into(),
                context: vec![context],
                choice: EMPTY.into(),
                decider: KIND.into(),
                card_id: Some(first.id),
                ..Default::default()
            })
            .unwrap();
        let outcome = Outcome::of(
            &[],
            &["src/server/routes.rs".to_string(), "gone.rs".to_string()],
        );
        store.set_outcome(earlier.id, &outcome.json()).unwrap();

        let hints = ask(&localizer, title, body, Some(second.id));
        assert_eq!(
            paths(&hints),
            ["src/server/routes.rs"],
            "a file that is gone is never named"
        );
        assert_eq!(
            hints[0].reason,
            format!("changed by card {}, whose issue is similar", first.id)
        );
        // The card's own earlier hints say nothing about it.
        assert!(ask(&localizer, title, body, Some(first.id)).is_empty());
    }

    #[test]
    fn the_outcome_has_the_files_touched_the_recall_at_ten_and_the_precision() {
        let list = |paths: &[&str]| paths.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        let outcome = Outcome::of(&list(&["a", "b", "c", "d"]), &list(&["c", "a", "z"]));
        assert_eq!(outcome.touched, ["a", "c", "z"]);
        assert_eq!(outcome.hinted, 4);
        assert_eq!(outcome.recall, Some(2.0 / 3.0));
        assert_eq!(outcome.precision, Some(0.5));
        assert_eq!(outcome.hits(&list(&["a", "b", "c", "d"])), 2);
        // Only the first ten files of a list count towards the recall.
        let eleven: Vec<String> = (0..11).map(|i| format!("f{i}")).collect();
        assert_eq!(Outcome::of(&eleven, &list(&["f10"])).recall, Some(0.0));
        // Nothing hinted, nothing touched.
        let nothing = Outcome::of(&[], &list(&["a"]));
        assert_eq!((nothing.recall, nothing.precision), (Some(0.0), None));
        let untouched = Outcome::of(&list(&["a"]), &[]);
        assert_eq!((untouched.recall, untouched.precision), (None, Some(0.0)));
        let json: Outcome = serde_json::from_str(&outcome.json()).unwrap();
        assert_eq!(json, outcome);
        assert!(Outcome::unknown().unknown);
    }
}
