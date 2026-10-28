//! `moebius export` as a person runs it: the built binary on a data directory with judgements in it.

use std::path::Path;
use std::process::Command;

use moebius_core::store::{NewJudgement, Store};
use serde_json::{Value, json};

fn judge(store: &Store, kind: &str, context: Value, decider: &str) -> i64 {
    store
        .add_judgement(&NewJudgement {
            kind: kind.into(),
            context: vec![store.blobs().put(&context.to_string()).unwrap()],
            options: vec!["yes".into(), "no".into()],
            choice: "yes".into(),
            reason: "because".into(),
            decider: decider.into(),
            ..Default::default()
        })
        .unwrap()
        .id
}

fn export(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_moebius"))
        .arg("--config")
        .arg(dir.join("moebius.toml"))
        .arg("export")
        .args(args)
        .output()
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
fn the_command_writes_every_judgement_once_and_prints_the_counts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("moebius.toml"), "").unwrap();
    let data = dir.path().join(".moebius");
    let store = Store::open(&data).unwrap();
    let a = judge(&store, "merge", json!({"change": "1"}), "person");
    let b = judge(
        &store,
        "tool_call",
        json!({"tool": "bash"}),
        "guard/patterns",
    );
    let c = judge(&store, "merge", json!({"change": "2"}), "person");
    store.set_outcome(a, "merged").unwrap();
    drop(store);

    let out = dir.path().join("out");
    let output = export(dir.path(), &["--out", out.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(
        printed.contains("merge.jsonl: 2 judgements, 1 with an outcome, 2 by a person"),
        "{printed}"
    );
    assert!(printed.contains("tool_call.jsonl: 1 judgements, 0 with an outcome, 0 by a person"));

    let merge = lines(&out.join("merge.jsonl"));
    let ids: Vec<i64> = merge.iter().map(|l| l["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, [a, c]);
    assert_eq!(merge[0]["context"], json!({"change": "1"}));
    assert_eq!(merge[0]["outcome"], "merged");
    assert_eq!(lines(&out.join("tool_call.jsonl"))[0]["id"], b);

    // Only the kinds asked for; the store is as it was.
    let only = dir.path().join("only");
    let output = export(
        dir.path(),
        &["--out", only.to_str().unwrap(), "--kind", "tool_call"],
    );
    assert!(output.status.success());
    assert!(only.join("tool_call.jsonl").is_file() && !only.join("merge.jsonl").exists());
    let store = Store::open(&data).unwrap();
    assert_eq!(store.judgements("merge").unwrap().len(), 2);

    // A kind with nothing, and a directory without a store, fail with a message and write nothing.
    let nothing = dir.path().join("nothing");
    let output = export(
        dir.path(),
        &["--out", nothing.to_str().unwrap(), "--kind", "retry"],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no judgements of kind \"retry\""));
    let empty = tempfile::tempdir().unwrap();
    std::fs::write(empty.path().join("moebius.toml"), "").unwrap();
    let output = export(empty.path(), &["--out", nothing.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("no store in"));
    assert!(!empty.path().join(".moebius").exists() && !nothing.exists());
}
