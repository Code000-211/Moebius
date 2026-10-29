//! Compressing a tool's output by what it is: escape codes, progress bars and repeated lines removed from every
//! output (`clean`, `fold_repeats`), and for a well-known command the lines a model never needs, such as the tests
//! that passed (`for_command`). A compressor only removes lines it knows to be noise; a line it does not know is
//! kept, so that no error message and no failing test's output is ever compressed away.

use std::sync::LazyLock;

use regex::Regex;

/// What a compressor made of an output.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Compressed {
    /// The compressor's name, as the `tool_output` run event names it, such as `cargo-test`.
    pub(super) name: &'static str,
    pub(super) lines: Vec<String>,
    /// How many lines it left out.
    pub(super) left_out: usize,
    /// What the lines it left out were, such as "passing tests".
    pub(super) what: &'static str,
    /// What it counted of what it left out, such as how many tests passed.
    pub(super) summary: Option<String>,
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("pattern compiles")
}

/// The text without terminal escape codes, and each line as a terminal would end up showing it: what follows its
/// last carriage return, and no line that is only a progress bar.
pub(super) fn clean(text: &str) -> String {
    static ESCAPE: LazyLock<Regex> = LazyLock::new(|| {
        regex(r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]")
    });
    let text = ESCAPE.replace_all(text, "");
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            line.rsplit('\r')
                .find(|part| !part.trim().is_empty())
                .unwrap_or("")
        })
        .filter(|line| !progress_bar(line))
        .collect();
    lines.join("\n")
}

/// Whether `line` is a progress bar: a bar of at least eight marks between brackets or bars, and a percentage or a
/// count, and nothing that reads as an error.
fn progress_bar(line: &str) -> bool {
    static BAR: LazyLock<Regex> = LazyLock::new(|| {
        regex(r"[\[|][=#>\-. █▉▊▋▌▍▎▏░▒▓■□]{8,}[\]|].*?\d+(?:\.\d+)?\s?(?:%|/\d+)")
    });
    BAR.is_match(line) && !line.to_lowercase().contains("error")
}

/// Runs of the same line folded into one, followed by how often it came, such as `(×37)`. True when anything was
/// folded.
pub(super) fn fold_repeats(lines: &mut Vec<String>) -> bool {
    let mut folded: Vec<String> = Vec::with_capacity(lines.len());
    let mut any = false;
    let mut iter = std::mem::take(lines).into_iter().peekable();
    while let Some(line) = iter.next() {
        let mut count = 1;
        while iter.peek() == Some(&line) {
            iter.next();
            count += 1;
        }
        any |= count > 1;
        // A run of empty lines is one empty line.
        if count > 1 && !line.trim().is_empty() {
            folded.push(format!("{line} (×{count})"));
        } else {
            folded.push(line);
        }
    }
    *lines = folded;
    any
}

/// A compressor: an output's lines in, what is left of them out.
pub(super) type Compressor = fn(Vec<String>) -> Compressed;

/// The compressor for the command line `command`, if it runs a command whose output one knows: a test runner
/// (`cargo test`, `pytest`, `go test`, `jest` and `vitest`, also through `npm test` and the like) or a cargo build.
pub(super) fn for_command(command: &str) -> Option<Compressor> {
    static CARGO_TEST: LazyLock<Regex> =
        LazyLock::new(|| regex(r"\bcargo\s+(?:\+\S+\s+)?(?:test|t)\b"));
    static CARGO_BUILD: LazyLock<Regex> = LazyLock::new(|| {
        regex(r"\bcargo\s+(?:\+\S+\s+)?(?:build|b|check|c|clippy|doc|run|r|install)\b")
    });
    static PYTEST: LazyLock<Regex> = LazyLock::new(|| regex(r"\b(?:pytest|py\.test)\b"));
    static GO_TEST: LazyLock<Regex> = LazyLock::new(|| regex(r"\bgo\s+test\b"));
    static JS_TEST: LazyLock<Regex> =
        LazyLock::new(|| regex(r"\b(?:npm|pnpm|yarn|bun)\s+(?:run\s+)?test\b|\b(?:jest|vitest)\b"));
    let compressors: [(&Regex, Compressor); 5] = [
        (&CARGO_TEST, cargo_test),
        (&PYTEST, pytest),
        (&GO_TEST, go_test),
        (&JS_TEST, js_test),
        (&CARGO_BUILD, cargo_build),
    ];
    compressors
        .into_iter()
        .find(|(pattern, _)| pattern.is_match(command))
        .map(|(_, compressor)| compressor)
}

/// The lines without those `noise` matches, and how many those were.
fn without(lines: Vec<String>, noise: &Regex) -> (Vec<String>, usize) {
    let before = lines.len();
    let kept: Vec<String> = lines
        .into_iter()
        .filter(|line| !noise.is_match(line))
        .collect();
    let left_out = before - kept.len();
    (kept, left_out)
}

/// What cargo prints while it builds: the crates it compiles, downloads and locks.
static CARGO_PROGRESS: &str = r"^\s*(?:Compiling|Checking|Downloaded|Downloading|Fresh|Updating|Locking|Adding|Blocking waiting for file lock|Documenting|Unpacking|Packaging|Verifying|Building \[)\b";

/// A cargo build: its progress left out and each diagnostic that repeats (a warning of a crate built twice) kept once,
/// with how often it came.
fn cargo_build(lines: Vec<String>) -> Compressed {
    static PROGRESS: LazyLock<Regex> = LazyLock::new(|| regex(CARGO_PROGRESS));
    let (lines, progress) = without(lines, &PROGRESS);
    let (lines, repeated) = fold_diagnostics(lines);
    Compressed {
        name: "cargo-build",
        left_out: progress + repeated,
        lines,
        what: "build progress and repeated diagnostics",
        summary: None,
    }
}

/// Each diagnostic (a block from a line that starts `warning` or `error` to the next empty line) that comes again
/// later is kept only where it first came, its first line ending with how often it came. Returns the lines and how
/// many were left out.
fn fold_diagnostics(lines: Vec<String>) -> (Vec<String>, usize) {
    let mut blocks: Vec<Vec<String>> = vec![];
    let mut in_diagnostic = false;
    for line in lines {
        let starts = line.starts_with("warning") || line.starts_with("error");
        if starts || !in_diagnostic || blocks.is_empty() {
            blocks.push(vec![]);
            in_diagnostic = starts;
        }
        if line.trim().is_empty() {
            in_diagnostic = false;
        }
        blocks.last_mut().expect("a block was pushed").push(line);
    }
    let mut kept: Vec<(Vec<String>, usize)> = vec![];
    let mut left_out = 0;
    for block in blocks {
        let diagnostic = block[0].starts_with("warning") || block[0].starts_with("error");
        let first = diagnostic
            .then(|| kept.iter_mut().find(|(seen, _)| *seen == block))
            .flatten();
        match first {
            Some((_, count)) => {
                *count += 1;
                left_out += block.len();
            }
            None => kept.push((block, 1)),
        }
    }
    let lines = kept
        .into_iter()
        .flat_map(|(mut block, count)| {
            if count > 1 {
                block[0] = format!("{} (×{count})", block[0]);
            }
            block
        })
        .collect();
    (lines, left_out)
}

/// `cargo test`: the build as in `cargo_build`; the tests that passed and the test binaries in which every test
/// passed left out, with how many passed. A binary with a failure keeps all but its passing tests, and the result
/// line of each binary that failed stays.
fn cargo_test(lines: Vec<String>) -> Compressed {
    static PASSED: LazyLock<Regex> =
        LazyLock::new(|| regex(r"^test .+ \.\.\. (?:ok|ignored(?:, .*)?)$|^running \d+ tests?$"));
    static START: LazyLock<Regex> = LazyLock::new(|| regex(r"^\s*(?:Running|Doc-tests) "));
    static RESULT: LazyLock<Regex> =
        LazyLock::new(|| regex(r"^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored"));
    let mut built = cargo_build(lines);
    let mut sections: Vec<Vec<String>> = vec![vec![]];
    // A test binary's section runs from its `Running` line to its result line.
    for line in std::mem::take(&mut built.lines) {
        if START.is_match(&line) {
            sections.push(vec![]);
        }
        let ends = line.starts_with("test result: ");
        sections.last_mut().expect("there is a section").push(line);
        if ends {
            sections.push(vec![]);
        }
    }
    let (mut binaries, mut passed, mut ignored) = (0, 0, 0);
    let mut left_out = built.left_out;
    let mut lines = vec![];
    for (index, section) in sections.into_iter().enumerate() {
        let result = section.iter().find_map(|line| RESULT.captures(line));
        let failed = section.iter().any(|line| {
            let lower = line.to_lowercase();
            !PASSED.is_match(line)
                && !RESULT.is_match(line)
                && (lower.contains("fail") || lower.contains("panic") || lower.contains("error"))
        });
        if index > 0
            && let Some(result) = result.filter(|result| &result[2] == "0" && !failed)
        {
            binaries += 1;
            passed += result[1].parse::<usize>().unwrap_or(0);
            ignored += result[3].parse::<usize>().unwrap_or(0);
            left_out += section.len();
            continue;
        }
        let (kept, noise) = without(section, &PASSED);
        left_out += noise;
        lines.extend(kept);
    }
    // The empty lines that followed what was left out.
    let mut tidy: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        if line.trim().is_empty() && tidy.last().is_none_or(|last| last.trim().is_empty()) {
            left_out += 1;
            continue;
        }
        tidy.push(line);
    }
    Compressed {
        name: "cargo-test",
        lines: tidy,
        left_out,
        what: "build progress and passing tests",
        summary: (binaries > 0).then(|| {
            format!("test binaries passed whole: {binaries} ({passed} passed, {ignored} ignored)")
        }),
    }
}

/// `pytest`: its header, the tests that passed (`-v`) and the progress lines of files whose tests all passed or were
/// skipped left out; failures and the summary stay.
fn pytest(lines: Vec<String>) -> Compressed {
    static NOISE: LazyLock<Regex> = LazyLock::new(|| {
        regex(
            r"^(?:platform |cachedir: |rootdir: |configfile: |plugins: |collecting |collected \d+ items?)|^=+ test session starts =+$|^\S+::\S+.* PASSED(?:\s|$)|^\S+\.py [.sxX]+\s*(?:\[\s*\d+%\])?$",
        )
    });
    let (lines, left_out) = without(lines, &NOISE);
    Compressed {
        name: "pytest",
        lines,
        left_out,
        what: "the header and passing tests",
        summary: None,
    }
}

/// `go test`: the `=== RUN` lines and the tests and packages that passed left out; failures stay.
fn go_test(lines: Vec<String>) -> Compressed {
    static NOISE: LazyLock<Regex> = LazyLock::new(|| {
        regex(
            r"^=== (?:RUN|PAUSE|CONT|NAME)\s|^\s*--- (?:PASS|SKIP): |^PASS$|^ok\s+\S+\s|^\?\s+\S+\s+\[no test files\]",
        )
    });
    static OK: LazyLock<Regex> = LazyLock::new(|| regex(r"^ok\s+\S+\s"));
    let packages = lines.iter().filter(|line| OK.is_match(line)).count();
    let (lines, left_out) = without(lines, &NOISE);
    Compressed {
        name: "go-test",
        lines,
        left_out,
        what: "passing tests",
        summary: Some(format!("packages passed: {packages}")),
    }
}

/// `jest` and `vitest` (also as `npm test`): the files and tests that passed left out; failures and the summary stay.
fn js_test(lines: Vec<String>) -> Compressed {
    static NOISE: LazyLock<Regex> = LazyLock::new(|| regex(r"^\s*PASS\s|^\s*[✓√]\s"));
    let (lines, left_out) = without(lines, &NOISE);
    Compressed {
        name: "js-test",
        lines,
        left_out,
        what: "passing tests",
        summary: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(String::from).collect()
    }

    fn compress(command: &str, text: &str) -> Compressed {
        for_command(command).expect("a compressor")(lines(text))
    }

    #[test]
    fn escape_codes_carriage_returns_and_progress_bars_are_removed() {
        let text = "\x1b[1m\x1b[32m   Compiling\x1b[0m foo\nstep 1\rstep 2\rdone\n\
                    [========>       ] 45%\n[#########] 9/10\nerror [==========] 3/4\nend\r\n";
        assert_eq!(
            clean(text),
            "   Compiling foo\ndone\nerror [==========] 3/4\nend\n"
        );
    }

    #[test]
    fn repeated_lines_are_folded_with_their_count() {
        let mut text = lines("a\nretrying\nretrying\nretrying\nb\n\n\nc");
        assert!(fold_repeats(&mut text));
        assert_eq!(text, ["a", "retrying (×3)", "b", "", "c"]);
        let mut once = lines("a\nb");
        assert!(!fold_repeats(&mut once));
        assert_eq!(once, ["a", "b"]);
    }

    #[test]
    fn commands_are_known_by_their_runner() {
        let name = |command: &str| for_command(command).map(|c| c(vec![]).name);
        assert_eq!(name("cargo test --workspace"), Some("cargo-test"));
        assert_eq!(
            name("cd x && cargo +nightly test 2>&1 | tail"),
            Some("cargo-test")
        );
        assert_eq!(name("cargo clippy -- -D warnings"), Some("cargo-build"));
        assert_eq!(name("cargo build && cargo test"), Some("cargo-test"));
        assert_eq!(name("python -m pytest -q"), Some("pytest"));
        assert_eq!(name("go test ./..."), Some("go-test"));
        assert_eq!(name("npm test"), Some("js-test"));
        assert_eq!(name("npx vitest run"), Some("js-test"));
        assert_eq!(name("ls -la"), None);
        assert_eq!(name("echo testing"), None);
    }

    const CARGO_TEST: &str = "   Compiling demo v0.1.0 (/w)
warning: unused variable: `x`
 --> src/lib.rs:2:9
  |
2 |     let x = 1;
  |         ^ help: if this is intentional, prefix it with an underscore: `_x`

warning: unused variable: `x`
 --> src/lib.rs:2:9
  |
2 |     let x = 1;
  |         ^ help: if this is intentional, prefix it with an underscore: `_x`

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.50s
     Running unittests src/lib.rs (target/debug/deps/demo-1)

running 3 tests
test tests::adds ... ok
test tests::splits ... FAILED
test tests::slow ... ignored

failures:

---- tests::splits stdout ----
thread 'tests::splits' panicked at src/lib.rs:20:9:
assertion `left == right` failed
  left: 1
 right: 2

failures:
    tests::splits

test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/api.rs (target/debug/deps/api-2)

running 2 tests
test a ... ok
test b ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests demo

running 1 test
test src/lib.rs - f (line 3) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

error: test failed, to rerun pass `--lib`
";

    #[test]
    fn cargo_test_keeps_the_failing_tests_with_their_output_and_the_summary() {
        let compressed = compress("cargo test", CARGO_TEST);
        let kept = compressed.lines.join("\n");
        assert_eq!(
            kept,
            "warning: unused variable: `x` (×2)
 --> src/lib.rs:2:9
  |
2 |     let x = 1;
  |         ^ help: if this is intentional, prefix it with an underscore: `_x`

    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.50s
     Running unittests src/lib.rs (target/debug/deps/demo-1)

test tests::splits ... FAILED

failures:

---- tests::splits stdout ----
thread 'tests::splits' panicked at src/lib.rs:20:9:
assertion `left == right` failed
  left: 1
 right: 2

failures:
    tests::splits

test result: FAILED. 1 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `--lib`"
        );
        assert_eq!(
            compressed.left_out,
            lines(CARGO_TEST).len() - compressed.lines.len()
        );
        assert_eq!(
            compressed.summary.as_deref(),
            Some("test binaries passed whole: 2 (3 passed, 0 ignored)")
        );
    }

    #[test]
    fn a_cargo_build_keeps_every_error() {
        let text = "    Updating crates.io index\n   Compiling a v1.0.0\n   Compiling b v1.0.0\n\
                    error[E0425]: cannot find value `y` in this scope\n --> src/main.rs:3:5\n\n\
                    error: could not compile `b` (bin \"b\") due to 1 previous error\n";
        let compressed = compress("cargo build", text);
        assert_eq!(
            compressed.lines,
            [
                "error[E0425]: cannot find value `y` in this scope",
                " --> src/main.rs:3:5",
                "",
                "error: could not compile `b` (bin \"b\") due to 1 previous error"
            ]
        );
        assert_eq!(compressed.left_out, 3);
    }

    #[test]
    fn pytest_keeps_the_failures_and_the_summary() {
        let text =
            "============================= test session starts ==============================
platform linux -- Python 3.12.0, pytest-8.0.0
rootdir: /w
collected 5 items

tests/test_a.py ....                                                     [ 80%]
tests/test_b.py F                                                        [100%]

=================================== FAILURES ===================================
_________________________________ test_split __________________________________

    def test_split():
>       assert split('a') == ['b']
E       AssertionError: assert ['a'] == ['b']

tests/test_b.py:3: AssertionError
=========================== short test summary info ============================
FAILED tests/test_b.py::test_split - AssertionError: assert ['a'] == ['b']
========================= 1 failed, 4 passed in 0.05s ==========================";
        let compressed = compress("pytest", text);
        let kept = compressed.lines.join("\n");
        assert!(!kept.contains("platform") && !kept.contains("test_a.py ...."));
        for needed in [
            "tests/test_b.py F",
            "E       AssertionError",
            "FAILED tests/test_b.py::test_split",
            "1 failed, 4 passed",
        ] {
            assert!(kept.contains(needed), "{needed} in {kept}");
        }
        assert_eq!(compressed.left_out, 5);
    }

    #[test]
    fn go_test_keeps_the_failures() {
        let text = "=== RUN   TestAdd\n--- PASS: TestAdd (0.00s)\n=== RUN   TestSplit\n    split_test.go:9: got 1, want 2\n\
                    --- FAIL: TestSplit (0.00s)\nFAIL\nFAIL\texample.com/a\t0.01s\nok  \texample.com/b\t0.02s\n\
                    ?   \texample.com/c\t[no test files]\nFAIL";
        let compressed = compress("go test ./...", text);
        assert_eq!(
            compressed.lines,
            [
                "    split_test.go:9: got 1, want 2",
                "--- FAIL: TestSplit (0.00s)",
                "FAIL",
                "FAIL\texample.com/a\t0.01s",
                "FAIL"
            ]
        );
        assert_eq!(compressed.summary.as_deref(), Some("packages passed: 1"));
    }

    #[test]
    fn jest_keeps_the_failures_and_the_summary() {
        let text = "PASS src/a.test.js\n  ✓ adds (3 ms)\nFAIL src/b.test.js\n  ✕ splits (2 ms)\n\n  ● splits\n\n    \
                    expect(received).toBe(expected)\n\nTests:       1 failed, 1 passed, 2 total";
        let compressed = compress("npm test", text);
        assert_eq!(compressed.left_out, 2);
        assert!(compressed.lines.iter().any(|l| l.contains("✕ splits")));
        assert!(
            compressed
                .lines
                .iter()
                .any(|l| l.contains("toBe(expected)"))
        );
        assert!(compressed.lines.iter().any(|l| l.starts_with("Tests:")));
    }
}
