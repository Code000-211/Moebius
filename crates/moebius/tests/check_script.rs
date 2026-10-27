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
    let output = check(dir.path(), script, &["--column", "Ready"]);
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
