//! The verdict of a trial: what the trial run and the card's first code run measured, whether the trial is positive
//! against it, and the status a proposal gets from all its trials.

use serde::Serialize;

/// How many times the tokens of the card's first code run a trial run may use and still be no worse.
const TOKENS_FACTOR: f64 = 1.5;

/// What the trial run of one card measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Measured {
    /// Whether the run succeeded: it ended with a commit of what the agent made.
    pub succeeded: bool,
    /// The kinds of the addressed errors that happened in the run, as the Retro's report has them.
    pub errors: Vec<String>,
    /// Whether the repository's own checks passed on what the run made; none when the repository has none, or when
    /// the run made nothing to check.
    pub checks: Option<bool>,
    /// The tokens the run used.
    pub tokens: i64,
    /// A trial is one round.
    pub rounds: usize,
}

/// What the card's first code run, the one the errors happened in, measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Original {
    /// The kinds of the errors the card had in the Retro's report.
    pub errors: Vec<String>,
    /// Whether the card was once sent back because the repository's own checks failed.
    pub checks_failed: bool,
    /// The code runs the card needed. A trial is one round, which is never more than that, so rounds never make a
    /// trial negative.
    pub rounds: usize,
    /// The tokens of the first code run.
    pub tokens: i64,
}

/// Whether a trial is positive, and why or why not: positive when the run succeeded, none of the addressed errors
/// happened, the checks passed or failed before as well, and the tokens are no more than `TOKENS_FACTOR` times those of
/// the first code run.
pub fn compare(trial: &Measured, original: &Original) -> (bool, String) {
    let mut worse = Vec::new();
    if !trial.succeeded {
        worse.push("the run did not succeed".to_string());
    }
    if !trial.errors.is_empty() {
        worse.push(format!("it still has {}", trial.errors.join(", ")));
    }
    if trial.checks == Some(false) && !original.checks_failed {
        worse.push("the repository's checks fail, which they did not before".to_string());
    }
    if trial.tokens as f64 > original.tokens as f64 * TOKENS_FACTOR {
        worse.push(format!(
            "it used {} tokens, more than {TOKENS_FACTOR} times the first run's {}",
            trial.tokens, original.tokens
        ));
    }
    if !worse.is_empty() {
        return (false, worse.join("; "));
    }
    let checks = match trial.checks {
        Some(true) => "the checks pass",
        Some(false) => "the checks fail as they did before",
        None => "there were no checks",
    };
    let gone = if original.errors.is_empty() {
        String::new()
    } else {
        format!(" {} did not happen again,", original.errors.join(", "))
    };
    (
        true,
        format!(
            "the run succeeded,{gone} {checks}, and it used {} tokens against {}",
            trial.tokens, original.tokens
        ),
    )
}

/// The status a proposal gets from its trials.
pub(super) fn status_of(outcomes: &[&str]) -> &'static str {
    if outcomes.contains(&"negative") {
        "negative"
    } else if !outcomes.is_empty() && outcomes.iter().all(|o| *o == "positive") {
        "positive"
    } else {
        "untried"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured() -> Measured {
        Measured {
            succeeded: true,
            errors: vec![],
            checks: Some(true),
            tokens: 150,
            rounds: 1,
        }
    }

    fn original() -> Original {
        Original {
            errors: vec!["run_failed".into()],
            checks_failed: false,
            rounds: 2,
            tokens: 100,
        }
    }

    #[test]
    fn a_trial_is_positive_when_the_error_is_gone_and_nothing_got_worse() {
        let (positive, reason) = compare(&measured(), &original());
        assert!(positive, "{reason}");
        assert!(
            reason.contains("run_failed did not happen again"),
            "{reason}"
        );
        // No checks, or checks that failed before as well, are no worse.
        let none = Measured {
            checks: None,
            ..measured()
        };
        assert!(compare(&none, &original()).0);
        let failing = Measured {
            checks: Some(false),
            ..measured()
        };
        let failed_before = Original {
            checks_failed: true,
            ..original()
        };
        assert!(compare(&failing, &failed_before).0);
    }

    #[test]
    fn a_regression_trial_passes_where_the_card_passed_before() {
        let passed = Original {
            errors: vec![],
            ..original()
        };
        let (positive, reason) = compare(&measured(), &passed);
        assert!(positive);
        assert_eq!(
            reason,
            "the run succeeded, the checks pass, and it used 150 tokens against 100"
        );
        let failing = Measured {
            checks: Some(false),
            ..measured()
        };
        assert!(!compare(&failing, &passed).0);
    }

    #[test]
    fn a_trial_is_negative_for_each_thing_that_is_not_as_good() {
        let negative = |trial: Measured, original: Original| {
            let (positive, reason) = compare(&trial, &original);
            assert!(!positive);
            reason
        };
        let reason = negative(
            Measured {
                succeeded: false,
                ..measured()
            },
            original(),
        );
        assert_eq!(reason, "the run did not succeed");
        let reason = negative(
            Measured {
                errors: vec!["run_failed".into(), "tool_failed".into()],
                ..measured()
            },
            original(),
        );
        assert_eq!(reason, "it still has run_failed, tool_failed");
        let reason = negative(
            Measured {
                checks: Some(false),
                ..measured()
            },
            original(),
        );
        assert_eq!(
            reason,
            "the repository's checks fail, which they did not before"
        );
        // Everything that is wrong is said.
        let reason = negative(
            Measured {
                succeeded: false,
                checks: Some(false),
                ..measured()
            },
            original(),
        );
        assert!(reason.starts_with("the run did not succeed; the repository's checks"));
    }

    #[test]
    fn a_trial_may_use_one_and_a_half_times_the_tokens_of_the_first_run() {
        let with = |tokens| Measured {
            tokens,
            ..measured()
        };
        assert!(compare(&with(150), &original()).0);
        let (positive, reason) = compare(&with(151), &original());
        assert!(!positive);
        assert_eq!(
            reason,
            "it used 151 tokens, more than 1.5 times the first run's 100"
        );
        // A first run that used nothing allows nothing.
        let free = Original {
            tokens: 0,
            ..original()
        };
        assert!(compare(&with(0), &free).0);
        assert!(!compare(&with(1), &free).0);
    }

    #[test]
    fn the_trials_give_a_proposal_its_status() {
        assert_eq!(status_of(&["positive"]), "positive");
        assert_eq!(status_of(&["positive", "positive"]), "positive");
        assert_eq!(status_of(&["positive", "negative"]), "negative");
        assert_eq!(status_of(&["skipped", "negative"]), "negative");
        assert_eq!(status_of(&["positive", "skipped"]), "untried");
        assert_eq!(status_of(&["failed"]), "untried");
        assert_eq!(status_of(&[]), "untried");
    }
}
