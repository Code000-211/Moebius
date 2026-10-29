//! Measuring: a brief's `measure` command run in a fresh worktree with a cleared environment and a time limit, the
//! last number it printed, and whether a lever's measurement beats the baseline by enough to be kept.

use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;

use super::MARGIN;
use crate::app::{App, tail};
use crate::improve::replay::run_apart;
use crate::store::Repository;

/// How long a measure may run before it is killed.
pub const MEASURE_LIMIT: Duration = Duration::from_secs(10 * 60);

/// The most of the end of a measure's output that is looked at for its number.
const OUTPUT_LOOKED_AT: usize = 64 * 1024;

/// The last number in what a measure printed.
pub fn parse_measurement(output: &str) -> Option<f64> {
    static NUMBER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?").unwrap());
    NUMBER
        .find_iter(output)
        .last()
        .and_then(|found| found.as_str().parse::<f64>().ok())
        .filter(|number| number.is_finite())
}

/// How much `value` differs from `baseline`, in percent of the baseline; none when the baseline is 0.
pub fn percent(baseline: f64, value: f64) -> Option<f64> {
    (baseline != 0.0).then(|| (value - baseline) / baseline.abs() * 100.0)
}

/// Whether a lever is kept, and why or why not: its measurement is better than the baseline in the brief's direction
/// by more than [`MARGIN`] of the baseline, and the checks passed or there are none.
pub fn judge(baseline: f64, value: f64, better: &str, checks: Option<bool>) -> (bool, String) {
    let gain = if better == "lower" {
        baseline - value
    } else {
        value - baseline
    };
    if gain <= MARGIN * baseline.abs() {
        let why = format!(
            "{value} against the baseline {baseline}: not better by more than {} %",
            MARGIN * 100.0
        );
        return (false, why);
    }
    let change = percent(baseline, value).map_or(String::new(), |p| format!(" ({p:+.1} %)"));
    if checks == Some(false) {
        let why = format!(
            "{value} against the baseline {baseline}{change} is better, but the repository's checks fail"
        );
        return (false, why);
    }
    (
        true,
        format!("{baseline} to {value}{change}, better than the baseline"),
    )
}

/// Whether the tokens used have reached the budget.
pub fn spent(tokens: i64, budget: i64) -> bool {
    tokens >= budget
}

/// Runs the measure `command` in a fresh worktree of `repository` at `rev`, named `name`, and returns the number it
/// printed or why the measurement failed.
pub(super) fn measure(
    app: &App,
    repository: &Repository,
    name: &str,
    rev: &str,
    command: &str,
    limit: Duration,
) -> std::result::Result<f64, String> {
    let clone = Path::new(&repository.path);
    let worktree = app
        .repos
        .add_detached_worktree(&repository.name, clone, name, rev)
        .map_err(|e| format!("cannot make a worktree of {rev}: {e}"))?;
    let (out, err) = (
        app.data_dir.join(format!("{name}.out")),
        app.data_dir.join(format!("{name}.err")),
    );
    let ended = std::fs::File::create(&out)
        .and_then(|stdout| Ok((stdout, std::fs::File::create(&err)?)))
        .and_then(|files| run_apart("sh", &["-c", command], &worktree, files, limit));
    let printed = std::fs::read(&out).unwrap_or_default();
    let printed = &printed[printed.len().saturating_sub(OUTPUT_LOOKED_AT)..];
    let printed = String::from_utf8_lossy(printed).into_owned();
    let complaint = tail(&err);
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&err);
    let _ = app.repos.remove_worktree(clone, &worktree);
    match ended {
        Err(e) => Err(format!("cannot run the measure: {e}")),
        Ok((_, true)) => Err(format!(
            "the measure was stopped after {} seconds",
            limit.as_secs()
        )),
        Ok((Some(0), false)) => {
            parse_measurement(&printed).ok_or_else(|| "the measure printed no number".to_string())
        }
        Ok((code, false)) => {
            let code = code.map_or("a signal".to_string(), |code| format!("exit status {code}"));
            Err(format!(
                "the measure ended with {code}: {}",
                complaint.trim()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repos::git;
    use crate::store::Store;
    use moebius_plugin_api::Host;
    use std::sync::Arc;

    /// An app with a repository `demo` whose `score` file holds 100.
    fn app_with_repository() -> (tempfile::TempDir, App, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]).unwrap();
        std::fs::write(origin.join("score"), "100\n").unwrap();
        git(&origin, &["add", "."]).unwrap();
        git(&origin, &["commit", "--quiet", "-m", "first"]).unwrap();
        let store = Arc::new(Store::in_memory(&dir.path().join("blobs")).unwrap());
        let host = Arc::new(Host::new(Duration::from_secs(1)));
        let app = App::new(store, host, &dir.path().join("data"));
        let repository = app
            .add_repository("demo", origin.to_str().unwrap())
            .unwrap();
        (dir, app, repository)
    }

    #[test]
    fn a_measure_runs_in_a_worktree_with_a_clean_environment_and_a_time_limit() {
        let (_dir, app, repository) = app_with_repository();
        let run = |command: &str, limit: Duration| {
            measure(&app, &repository, "m", "main", command, limit)
        };
        let limit = Duration::from_secs(30);
        assert_eq!(run("cat score", limit), Ok(100.0));
        // Only what was printed on standard output counts, and only the last number.
        assert_eq!(run("echo 7 >&2; echo 1; echo 2.5", limit), Ok(2.5));
        // The environment holds PATH and HOME, and the command runs where the files of the branch are.
        let variables = "env | cut -d= -f1 | grep -vx -e PWD -e SHLVL -e _ | wc -l";
        assert_eq!(run(variables, limit), Ok(2.0));
        let failed = run("echo 1; echo broke >&2; exit 4", limit).unwrap_err();
        assert!(
            failed.contains("exit status 4") && failed.contains("broke"),
            "{failed}"
        );
        assert_eq!(
            run("echo nothing", limit).unwrap_err(),
            "the measure printed no number"
        );
        let slow = run("echo 1; sleep 30", Duration::from_secs(1)).unwrap_err();
        assert!(slow.contains("stopped after 1 seconds"), "{slow}");
        // The worktrees are gone.
        let worktrees = git(Path::new(&repository.path), &["worktree", "list"]).unwrap();
        assert_eq!(worktrees.lines().count(), 1, "{worktrees}");
    }

    #[test]
    fn a_measurement_is_the_last_number_printed() {
        assert_eq!(parse_measurement("80\n"), Some(80.0));
        assert_eq!(parse_measurement("took 12 ms\nscore: 80.5\n"), Some(80.5));
        assert_eq!(parse_measurement("delta -3.25"), Some(-3.25));
        assert_eq!(parse_measurement("1 2 3"), Some(3.0));
        assert_eq!(parse_measurement("x = .5"), Some(0.5));
        assert_eq!(parse_measurement("2.5e-3 s"), Some(0.0025));
        assert_eq!(parse_measurement("+7"), Some(7.0));
        assert_eq!(parse_measurement("12 then done"), Some(12.0));
    }

    #[test]
    fn output_without_a_number_has_no_measurement() {
        assert_eq!(parse_measurement(""), None);
        assert_eq!(parse_measurement("all good\n"), None);
        assert_eq!(parse_measurement("- . e"), None);
    }

    #[test]
    fn a_lower_measurement_is_kept_when_it_beats_the_baseline_by_more_than_the_margin() {
        assert!(judge(100.0, 80.0, "lower", None).0);
        assert!(judge(100.0, 98.9, "lower", None).0);
        // Exactly the margin is not more than it; worse is not better.
        assert!(!judge(100.0, 99.0, "lower", None).0);
        assert!(!judge(100.0, 100.0, "lower", None).0);
        assert!(!judge(100.0, 120.0, "lower", None).0);
    }

    #[test]
    fn a_higher_measurement_is_kept_when_it_beats_the_baseline_by_more_than_the_margin() {
        assert!(judge(100.0, 120.0, "higher", None).0);
        assert!(judge(100.0, 101.1, "higher", None).0);
        assert!(!judge(100.0, 101.0, "higher", None).0);
        assert!(!judge(100.0, 80.0, "higher", None).0);
        // The margin is of the size of the baseline, whatever its sign; from 0 any gain counts.
        assert!(judge(-100.0, -80.0, "higher", None).0);
        assert!(!judge(-100.0, -99.5, "higher", None).0);
        assert!(judge(0.0, 0.1, "higher", None).0);
        assert!(!judge(0.0, 0.0, "higher", None).0);
    }

    #[test]
    fn a_lever_whose_checks_fail_is_not_kept_however_good_it_measures() {
        assert!(judge(100.0, 80.0, "lower", Some(true)).0);
        let (kept, why) = judge(100.0, 80.0, "lower", Some(false));
        assert!(!kept);
        assert!(why.contains("checks fail"), "{why}");
    }

    #[test]
    fn the_budget_is_spent_when_the_tokens_reach_it() {
        assert!(!spent(0, 1000));
        assert!(!spent(999, 1000));
        assert!(spent(1000, 1000));
        assert!(spent(5000, 1000));
    }

    #[test]
    fn the_change_is_shown_in_percent_of_the_baseline() {
        assert_eq!(percent(100.0, 80.0), Some(-20.0));
        assert_eq!(percent(-50.0, -25.0), Some(50.0));
        assert_eq!(percent(0.0, 1.0), None);
    }
}
