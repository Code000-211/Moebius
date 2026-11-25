//! `moebius check-script` as a person or an agent runs it: the built binary on a script file.

use std::path::Path;
use std::process::{Command, Output};

fn check(dir: &Path, script: &str, args: &[&str]) -> Output {
    let file = dir.join("ready.luau");
    std::fs::write(&file, script).unwrap();
    Command::new(env!("CARGO_BIN_EXE_moebius"))
        .arg("check-script")
        .arg(&file)
        .args(args)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_good_script_prints_what_it_did() {
    let dir = tempfile::tempdir().unwrap();
    let script = r#"
        function on_card(card)
          if card.author == "mallory" or string.find(card.title, "[hold]", 1, true) then
            moebius.wait("held back: " .. card.title)
          else
            moebius.move("In progress")
          end
        end
    "#;
    let output = check(dir.path(), script, &["--column", "Not started"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "moved the card to In progress\n");

    // The made-up card can be changed, to try the other paths of the script.
    let output = check(
        dir.path(),
        script,
        &["--title", "[hold] Rewrite", "--author", "leon"],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "waited: held back: [hold] Rewrite\n");

    let output = check(dir.path(), "function on_card(card) end", &[]);
    assert!(output.status.success());
    assert!(text(&output.stdout).starts_with("The script did nothing"));
}

#[test]
fn ready_is_accepted_as_the_name_of_the_not_started_column() {
    let dir = tempfile::tempdir().unwrap();
    let output = check(
        dir.path(),
        "function on_card(card) moebius.move('Ready') end",
        &["--column", "Ready"],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "moved the card to Not started\n");
}

#[test]
fn a_script_with_an_error_fails_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let output = check(
        dir.path(),
        "function on_card(card) moebius.move('Doing') end",
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = text(&output.stderr);
    assert!(error.starts_with("moebius: "), "{error}");
    assert!(error.contains("no column \"Doing\""), "{error}");

    let output = check(dir.path(), "function on_card(card", &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("syntax error"));

    let output = check(
        dir.path(),
        "function on_card(card) end",
        &["--column", "Nowhere"],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("no column \"Nowhere\""));

    let missing = Command::new(env!("CARGO_BIN_EXE_moebius"))
        .args(["check-script", "/nonexistent/ready.luau"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(text(&missing.stderr).contains("cannot read /nonexistent/ready.luau"));
}

const TESTING: &str =
    "[[column]]\nname = \"Ready for testing\"\nscript = \"testing.luau\"\nafter = \"In review\"\n";

#[test]
fn a_custom_column_is_known_from_a_kanban_file() {
    let dir = tempfile::tempdir().unwrap();
    let kanban = dir.path().join("kanban.toml");
    std::fs::write(&kanban, TESTING).unwrap();
    let script = "function on_card(card)\n  if card.column_class == 'In review' then moebius.move('Ready for testing') end\nend";
    let kanban = kanban.to_str().unwrap();
    let output = check(
        dir.path(),
        script,
        &["--column", "ready for testing", "--kanban", kanban],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout),
        "moved the card to Ready for testing\n"
    );

    // Without the file the column is not there.
    let output = check(dir.path(), script, &["--column", "Ready for testing"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("no column \"Ready for testing\""));

    // A file that is not a board is refused with what is wrong.
    let bad = dir.path().join("bad.toml");
    std::fs::write(&bad, "[[column]]\nname = \"Doing\"\n").unwrap();
    let output = check(dir.path(), script, &["--kanban", bad.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text(&output.stderr).contains("after"),
        "{}",
        text(&output.stderr)
    );
}

#[test]
fn a_custom_column_is_known_from_a_project_of_the_harness_repository() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("data/harness/projects/Web");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("kanban.toml"), TESTING).unwrap();
    let config = dir.path().join("moebius.toml");
    std::fs::write(&config, "data_dir = \"data\"\n").unwrap();
    let script = "function on_card(card) moebius.move('Ready to merge') end";
    let run = |project: &str| {
        let file = dir.path().join("testing.luau");
        std::fs::write(&file, script).unwrap();
        Command::new(env!("CARGO_BIN_EXE_moebius"))
            .arg("--config")
            .arg(&config)
            .arg("check-script")
            .arg(&file)
            .args(["--column", "Ready for testing", "--project", project])
            .output()
            .unwrap()
    };
    let output = run("Web");
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout), "moved the card to Ready to merge\n");
    // A project without the file has the built-in columns; a name that is no folder is refused.
    let output = run("Other");
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("no column"));
    let output = run("../Web");
    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("cannot be the name of a project"));
}
