//! The training-set export (ADR 0010): judgements with their full contexts and outcomes, written as JSON Lines, one
//! file per kind, for fine-tuning the decision model. Judgements leave the server only here, and only when a person
//! starts an export: `moebius export` on the command line or `POST /v1/exports` from the app. The format is
//! documented in `docs/reference/training-export.md`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::app::{App, PERSON};
use crate::blobs::redact_json;
use crate::decide::Question;
use crate::store::{self, Judgement, Store, StoreError};

/// How many judgements are read from the store at a time.
const PAGE: i64 = 1000;

/// An odd constant that spreads consecutive ids over the ten parts of [`held_out`].
const SPREAD: u64 = 0x9E37_79B9_7F4A_7C15;

/// The file in an export's directory that says when it was made and what it holds.
const RECORD: &str = "export.json";

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("cannot write the export: {0}")]
    Io(#[from] std::io::Error),
    #[error("no judgements of kind {0:?}")]
    UnknownKind(String),
    #[error("judgement {0}: its context cannot be read: {1}")]
    Context(i64, String),
}

/// Whether the judgement belongs to the tenth that is kept back from training. Shadowing measures a decision model on
/// these. The rule: the id as an unsigned 64-bit number is multiplied, wrapping around, by `0x9E3779B97F4A7C15`, and
/// the judgement is held out when the upper 32 bits of the product leave remainder 0 when divided by 10. It involves no randomness, so the
/// same judgement is held out in every export, on every server.
pub fn held_out(id: i64) -> bool {
    ((id as u64).wrapping_mul(SPREAD) >> 32).is_multiple_of(10)
}

/// What a kind of judgement has: how many, and how many of them carry an outcome, were decided by a person and are
/// held out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindCounts {
    pub kind: String,
    pub count: i64,
    pub with_outcome: i64,
    pub by_person: i64,
    pub held_out: i64,
}

impl KindCounts {
    fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            count: 0,
            with_outcome: 0,
            by_person: 0,
            held_out: 0,
        }
    }

    fn add(&mut self, id: i64, has_outcome: bool, decider: &str) {
        self.count += 1;
        self.with_outcome += i64::from(has_outcome);
        self.by_person += i64::from(decider == PERSON);
        self.held_out += i64::from(held_out(id));
    }
}

/// A kind's file and what is in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub file: PathBuf,
    pub counts: KindCounts,
}

/// What an export wrote, kinds in alphabetical order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub kinds: Vec<Written>,
}

/// The counts of the judgements stored now, per kind in alphabetical order.
pub fn counts(store: &Store) -> Result<Vec<KindCounts>, ExportError> {
    let mut by_kind: BTreeMap<String, KindCounts> = BTreeMap::new();
    store.each_judgement(|kind, id, has_outcome, decider| {
        by_kind
            .entry(kind.to_string())
            .or_insert_with(|| KindCounts::new(kind))
            .add(id, has_outcome, decider);
    })?;
    Ok(by_kind.into_values().collect())
}

/// Whether `kind` can be a file name: letters, digits, `_`, `-` and `.`, not starting with a dot.
fn file_safe(kind: &str) -> bool {
    !kind.is_empty()
        && !kind.starts_with('.')
        && kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// One line of a kind's file.
fn line(store: &Store, j: &Judgement) -> Result<String, ExportError> {
    let mut context = serde_json::Map::new();
    for blob in &j.context {
        let text = store
            .blobs()
            .get(blob)
            .map_err(|e| ExportError::Context(j.id, e.to_string()))?;
        match serde_json::from_str(&text) {
            Ok(Value::Object(fields)) => context.extend(fields),
            Ok(_) => {
                return Err(ExportError::Context(
                    j.id,
                    "a blob is not a JSON object".into(),
                ));
            }
            Err(e) => return Err(ExportError::Context(j.id, e.to_string())),
        }
    }
    let row = json!({
        "id": j.id,
        "kind": j.kind,
        "context": context,
        "options": j.options,
        "choice": j.choice,
        "reason": j.reason,
        "decider": j.decider,
        "run": j.run_id,
        "card": j.card_id,
        "outcome": j.outcome,
        "at": j.at,
        "split": if held_out(j.id) { "held_out" } else { "train" },
    });
    Ok(redact_json(&row).to_string())
}

/// Writes the judgements of one kind to `<dir>/<kind>.jsonl`, through a temporary name that is renamed at the end.
fn write_kind(store: &Store, dir: &Path, kind: &str) -> Result<Written, ExportError> {
    let file = dir.join(format!("{kind}.jsonl"));
    let temporary = dir.join(format!("{kind}.jsonl.tmp"));
    let mut counts = KindCounts::new(kind);
    let mut out = std::io::BufWriter::new(std::fs::File::create(&temporary)?);
    let mut after = 0;
    loop {
        let page = store.judgements_of_kind_after(kind, after, PAGE)?;
        let Some(last) = page.last() else { break };
        after = last.id;
        for j in &page {
            writeln!(out, "{}", line(store, j)?)?;
            counts.add(j.id, j.outcome.is_some(), &j.decider);
        }
    }
    out.flush()?;
    drop(out);
    std::fs::rename(&temporary, &file)?;
    Ok(Written { file, counts })
}

/// Writes one file per kind of judgement to `dir`, or only of the given kinds, oldest judgement first. A file only
/// appears under its name once it is complete; one that fails leaves no file of its own.
pub fn export(store: &Store, dir: &Path, kinds: Option<&[String]>) -> Result<Summary, ExportError> {
    let stored = counts(store)?;
    let wanted: Vec<String> = match kinds {
        None => stored.into_iter().map(|c| c.kind).collect(),
        Some(kinds) => {
            let mut wanted = Vec::new();
            for kind in kinds {
                if !stored.iter().any(|c| &c.kind == kind) || !file_safe(kind) {
                    return Err(ExportError::UnknownKind(kind.clone()));
                }
                if !wanted.contains(kind) {
                    wanted.push(kind.clone());
                }
            }
            wanted.sort();
            wanted
        }
    };
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    for kind in wanted.iter().filter(|kind| file_safe(kind)) {
        match write_kind(store, dir, kind) {
            Ok(w) => written.push(w),
            Err(e) => {
                let _ = std::fs::remove_file(dir.join(format!("{kind}.jsonl.tmp")));
                return Err(e);
            }
        }
    }
    Ok(Summary { kinds: written })
}

/// An export a person started through the API: kept in `<data_dir>/exports/<id>/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Export {
    pub id: i64,
    pub at: i64,
    pub kinds: Vec<KindCounts>,
}

fn exports_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("exports")
}

/// Makes the next export directory, numbered one above the highest there is.
fn new_directory(data_dir: &Path) -> Result<(i64, PathBuf), ExportError> {
    let parent = exports_dir(data_dir);
    std::fs::create_dir_all(&parent)?;
    let mut id = 1 + ids(data_dir).into_iter().max().unwrap_or(0);
    loop {
        let dir = parent.join(id.to_string());
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok((id, dir)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => id += 1,
            Err(e) => return Err(e.into()),
        }
    }
}

fn ids(data_dir: &Path) -> Vec<i64> {
    std::fs::read_dir(exports_dir(data_dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect()
}

/// Exports for a person who asked in the app: writes the files, keeps the record and records the person's decision.
/// The export does not contain the judgement of its own decision.
pub fn create(app: &App, kinds: Option<&[String]>) -> Result<Export, ExportError> {
    let (id, dir) = new_directory(&app.data_dir)?;
    let summary = match export(&app.store, &dir, kinds) {
        Ok(summary) => summary,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let export = Export {
        id,
        at: store::now(),
        kinds: summary.kinds.into_iter().map(|w| w.counts).collect(),
    };
    let temporary = dir.join(format!("{RECORD}.tmp"));
    std::fs::write(
        &temporary,
        serde_json::to_vec(&export).map_err(StoreError::from)?,
    )?;
    std::fs::rename(&temporary, dir.join(RECORD))?;
    let names: Vec<&str> = export.kinds.iter().map(|c| c.kind.as_str()).collect();
    let judgements: i64 = export.kinds.iter().map(|c| c.count).sum();
    app.decisions.record_person(
        &Question {
            kind: "export".into(),
            context: BTreeMap::from([
                ("export".to_string(), id.to_string()),
                ("kinds".to_string(), names.join(",")),
                (
                    "counts".to_string(),
                    serde_json::to_string(&export.kinds).map_err(StoreError::from)?,
                ),
            ]),
            options: vec!["export".into()],
            ..Default::default()
        },
        "export",
        &format!("exported {judgements} judgements"),
    )?;
    Ok(export)
}

/// The exports made through the API, newest first.
pub fn list(data_dir: &Path) -> Vec<Export> {
    let mut found: Vec<Export> = ids(data_dir)
        .into_iter()
        .filter_map(|id| {
            let text =
                std::fs::read(exports_dir(data_dir).join(id.to_string()).join(RECORD)).ok()?;
            serde_json::from_slice(&text).ok()
        })
        .collect();
    found.sort_by_key(|e| std::cmp::Reverse(e.id));
    found
}

/// The file of `kind` in export `id`, if there is one.
pub fn file(data_dir: &Path, id: i64, kind: &str) -> Option<PathBuf> {
    if !file_safe(kind) {
        return None;
    }
    let path = exports_dir(data_dir)
        .join(id.to_string())
        .join(format!("{kind}.jsonl"));
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::NewJudgement;

    fn setup() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::in_memory(&dir.path().join("blobs")).unwrap();
        (dir, store)
    }

    /// Records a judgement whose context is `context`, put into the blob store as it is.
    fn judge(store: &Store, kind: &str, context: &str, decider: &str) -> Judgement {
        store
            .add_judgement(&NewJudgement {
                kind: kind.into(),
                context: vec![store.blobs().put(context).unwrap()],
                options: vec!["yes".into(), "no".into()],
                choice: "yes".into(),
                reason: "because".into(),
                decider: decider.into(),
                ..Default::default()
            })
            .unwrap()
    }

    fn lines(file: &Path) -> Vec<Value> {
        std::fs::read_to_string(file)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn held_out_is_stable_and_about_a_tenth() {
        let held: Vec<i64> = (1..=10_000).filter(|id| held_out(*id)).collect();
        assert!(
            (900..=1100).contains(&held.len()),
            "{} of 10000 are held out",
            held.len()
        );
        assert_eq!(held[..5], [22, 26, 27, 31, 58][..], "the rule changed");
        assert!(held.iter().all(|id| held_out(*id)));
        // Neighbouring ids are not all held out together.
        assert!(
            (1..=10_000)
                .filter(|id| held_out(*id) && held_out(id + 1))
                .count()
                < 300
        );
    }

    #[test]
    fn one_line_per_judgement_oldest_first_with_the_context_from_its_blob() {
        let (dir, store) = setup();
        let a = judge(
            &store,
            "merge",
            r#"{"change":"7","title":"Fix it"}"#,
            PERSON,
        );
        judge(&store, "retry", r#"{"card":"3"}"#, "none");
        let b = judge(&store, "merge", r#"{"change":"8"}"#, "guard");
        store.set_outcome(a.id, "merged").unwrap();

        let out = dir.path().join("out");
        let summary = export(&store, &out, None).unwrap();
        let merge = lines(&out.join("merge.jsonl"));
        assert_eq!(merge.len(), 2);
        assert_eq!(
            merge[0],
            json!({
                "id": a.id, "kind": "merge",
                "context": {"change": "7", "title": "Fix it"},
                "options": ["yes", "no"], "choice": "yes", "reason": "because",
                "decider": "person", "run": null, "card": null, "outcome": "merged",
                "at": a.at, "split": if held_out(a.id) { "held_out" } else { "train" },
            })
        );
        assert_eq!(merge[1]["id"], b.id);
        assert_eq!(merge[1]["outcome"], Value::Null);
        assert_eq!(lines(&out.join("retry.jsonl")).len(), 1);

        let held = [a.id, b.id].into_iter().filter(|id| held_out(*id)).count() as i64;
        assert_eq!(
            summary.kinds[0],
            Written {
                file: out.join("merge.jsonl"),
                counts: KindCounts {
                    kind: "merge".into(),
                    count: 2,
                    with_outcome: 1,
                    by_person: 1,
                    held_out: held,
                },
            }
        );
        assert_eq!(summary.kinds.len(), 2);
    }

    #[test]
    fn a_secret_in_a_blob_does_not_come_out() {
        let (dir, store) = setup();
        let key = format!("sk-ant-api03-{}", "x".repeat(40));
        let blob = format!(r#"{{"command":"curl -H {key}"}}"#);
        // Put straight on the disk, as a blob from before redaction existed would be.
        let hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(blob.as_bytes()));
        let folder = dir.path().join("blobs").join(&hash[..2]);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(&hash[2..]), &blob).unwrap();
        store
            .add_judgement(&NewJudgement {
                kind: "tool_call".into(),
                context: vec![crate::blobs::BlobId(hash)],
                choice: "allow".into(),
                reason: format!("saw {key}"),
                decider: "guard".into(),
                ..Default::default()
            })
            .unwrap();

        let out = dir.path().join("out");
        export(&store, &out, Some(&["tool_call".to_string()])).unwrap();
        let text = std::fs::read_to_string(out.join("tool_call.jsonl")).unwrap();
        assert!(!text.contains("sk-ant-"), "{text}");
        assert!(text.contains("[redacted:anthropic-key]"));
    }

    #[test]
    fn only_the_kinds_asked_for_are_written() {
        let (dir, store) = setup();
        judge(&store, "merge", "{}", PERSON);
        judge(&store, "retry", "{}", PERSON);
        let out = dir.path().join("out");
        let summary = export(&store, &out, Some(&["retry".to_string()])).unwrap();
        assert_eq!(summary.kinds.len(), 1);
        assert!(out.join("retry.jsonl").is_file() && !out.join("merge.jsonl").exists());
        for bad in ["nothing", "../merge"] {
            let error = export(&store, &out, Some(&[bad.to_string()])).unwrap_err();
            assert!(matches!(error, ExportError::UnknownKind(_)), "{error}");
        }
    }

    #[test]
    fn a_failed_export_leaves_no_file_under_the_final_name() {
        let (dir, store) = setup();
        judge(&store, "merge", "{}", PERSON);
        let broken = store
            .add_judgement(&NewJudgement {
                kind: "retry".into(),
                context: vec![store.blobs().put("[1]").unwrap()],
                ..Default::default()
            })
            .unwrap();
        let out = dir.path().join("out");
        let error = export(&store, &out, None).unwrap_err();
        assert!(
            matches!(error, ExportError::Context(id, _) if id == broken.id),
            "{error}"
        );
        let names: Vec<String> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["merge.jsonl"],
            "the file that failed is not there, not even half"
        );
    }

    #[test]
    fn more_judgements_than_a_page_are_all_written() {
        let (dir, store) = setup();
        for _ in 0..PAGE + 5 {
            judge(&store, "merge", "{}", PERSON);
        }
        let out = dir.path().join("out");
        let summary = export(&store, &out, None).unwrap();
        assert_eq!(summary.kinds[0].counts.count, PAGE + 5);
        let ids: Vec<i64> = lines(&out.join("merge.jsonl"))
            .iter()
            .map(|l| l["id"].as_i64().unwrap())
            .collect();
        assert_eq!(ids, (1..=PAGE + 5).collect::<Vec<_>>());
        assert_eq!(
            counts(&store).unwrap(),
            vec![summary.kinds[0].counts.clone()]
        );
    }
}
