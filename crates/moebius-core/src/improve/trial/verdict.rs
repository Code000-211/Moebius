//! The verdict of a trial: what the trial run and the card's first code run measured, whether the trial is positive
//! against it, and the status a proposal gets from all its trials.

use serde::Serialize;

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
/// happened, the checks passed or failed before as well, and the tokens are no more than `token_factor` times those of
/// the first code run.
pub fn compare(trial: &Measured, original: &Original, token_factor: f64) -> (bool, String) {
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
    if trial.tokens as f64 > original.tokens as f64 * token_factor {
        worse.push(format!(
            "it used {} tokens, more than {token_factor} times the first run's {}",
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

/// The status a proposal gets from the trials of its own cards and of its regression set: `positive` when every trial
/// was, `negative` when one of the regression set failed or fewer than half of its own trials that ended were positive,
/// and `mixed` when its regression set is clean, some of its own trials was negative and at least half of those that
/// ended were positive, which a person may still decide about. A trial ended when it was `positive` or `negative`, not
/// `skipped` or `failed`.
pub(super) fn status_of(own: &[&str], regression: &[&str]) -> &'static str {
    let all = || own.iter().chain(regression).copied();
    if !all().any(|outcome| outcome == "negative") {
        return if !own.is_empty() && all().all(|outcome| outcome == "positive") {
            "positive"
        } else {
            "untried"
        };
    }
    if regression.contains(&"negative") {
        return "negative";
    }
    let ended = own
        .iter()
        .filter(|outcome| **outcome == "positive" || **outcome == "negative")
        .count();
    let positive = own.iter().filter(|outcome| **outcome == "positive").count();
    if ended > 0 && positive * 2 >= ended {
        "mixed"
    } else {
        "negative"
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
        let (positive, reason) = compare(&measured(), &original(), 1.5);
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
        assert!(compare(&none, &original(), 1.5).0);
        let failing = Measured {
            checks: Some(false),
            ..measured()
        };
        let failed_before = Original {
            checks_failed: true,
            ..original()
        };
        assert!(compare(&failing, &failed_before, 1.5).0);
    }

    #[test]
    fn a_regression_trial_passes_where_the_card_passed_before() {
        let passed = Original {
            errors: vec![],
            ..original()
        };
        let (positive, reason) = compare(&measured(), &passed, 1.5);
        assert!(positive);
        assert_eq!(
            reason,
            "the run succeeded, the checks pass, and it used 150 tokens against 100"
        );
        let failing = Measured {
            checks: Some(false),
            ..measured()
        };
        assert!(!compare(&failing, &passed, 1.5).0);
    }

    #[test]
    fn a_trial_is_negative_for_each_thing_that_is_not_as_good() {
        let negative = |trial: Measured, original: Original| {
            let (positive, reason) = compare(&trial, &original, 1.5);
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
    fn a_trial_may_use_the_configured_factor_times_the_tokens_of_the_first_run() {
        let with = |tokens| Measured {
            tokens,
            ..measured()
        };
        assert!(compare(&with(150), &original(), 1.5).0);
        let (positive, reason) = compare(&with(151), &original(), 1.5);
        assert!(!positive);
        assert_eq!(
            reason,
            "it used 151 tokens, more than 1.5 times the first run's 100"
        );
        // A factor above 1.5 lets the same run through, and says which factor it used.
        assert!(compare(&with(190), &original(), 2.0).0);
        let (positive, reason) = compare(&with(201), &original(), 2.0);
        assert!(!positive);
        assert_eq!(
            reason,
            "it used 201 tokens, more than 2 times the first run's 100"
        );
        // A first run that used nothing allows nothing.
        let free = Original {
            tokens: 0,
            ..original()
        };
        assert!(compare(&with(0), &free, 1.5).0);
        assert!(!compare(&with(1), &free, 1.5).0);
    }

    #[test]
    fn the_trials_give_a_proposal_its_status() {
        assert_eq!(status_of(&["positive"], &[]), "positive");
        assert_eq!(status_of(&["positive", "positive"], &[]), "positive");
        assert_eq!(
            status_of(&["positive", "positive"], &["positive"]),
            "positive"
        );
        // A proposal that is partly positive with a clean regression set is a person's decision.
        assert_eq!(status_of(&["positive", "negative"], &[]), "mixed");
        assert_eq!(status_of(&["positive", "negative"], &["positive"]), "mixed");
        let mostly: Vec<&str> = std::iter::repeat_n("positive", 11)
            .chain(std::iter::repeat_n("negative", 6))
            .collect();
        assert_eq!(status_of(&mostly, &["positive"]), "mixed");
        // Fewer than half positive, or a regression card that fails, and no person is asked.
        assert_eq!(
            status_of(&["positive", "negative", "negative"], &[]),
            "negative"
        );
        assert_eq!(status_of(&["skipped", "negative"], &[]), "negative");
        assert_eq!(
            status_of(&["positive", "negative"], &["negative"]),
            "negative"
        );
        assert_eq!(status_of(&["positive", "skipped"], &[]), "untried");
        assert_eq!(status_of(&["failed"], &[]), "untried");
        assert_eq!(status_of(&[], &[]), "untried");
    }
}
