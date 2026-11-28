//! A project's job definition, `projects/<project>/lane.toml` of the harness configuration repository (ADR 0035): the
//! steps a lane runs for a change, in order, and which of them measure.
//!
//! ```toml
//! [[step]]
//! name = "build"
//! command = "./build.sh"          # run by `sh -c` in the checkout
//! timeout_seconds = 1800          # default 900
//! on = "both"                     # `head` (the default), or `both`: also at the merge base, for a measurement's `before`
//!
//! [[step]]
//! name = "measure"
//! command = "./bench.sh"
//! [step.measure]                  # makes the step a measurement
//! runs = 3                        # per side, 3 or more; before and after take turns
//! prefix = "MEASURE "             # lines `MEASURE <name> <value> [unit]` of its output are rows
//! json = "out/bench.json"         # and/or a file it writes: `{"rows": [{"name", "value", "unit"}]}` or `{"name": value}`
//! parity_file = "out/result.bin"  # a file whose bytes must be the same before and after
//! ```
//!
//! `harness = ["bench/", "tools/rad-kbench"]` at the top of the file names the paths that measure: the scripts and tools a
//! measuring step runs. They are taken from the merge base for the head too, and a change that touches one is not measured
//! by the lane but looked at by a person (the paths count as locked paths).
//!
//! `in_progress = "label"` (the default) puts the label [`IN_PROGRESS_LABEL`] on the card's issue while a job of the change
//! is queued or running; `in_progress = "none"` leaves the issue alone.
//!
//! A definition with any mistake is refused as a whole, with every mistake it has.

use serde::Deserialize;

/// The most steps a definition has.
const STEPS_AT_MOST: usize = 30;
/// The longest a step may be given, in seconds.
pub const STEP_TIMEOUT_AT_MOST: u64 = 14_400;
/// The default time limit of a step, in seconds.
const STEP_TIMEOUT: u64 = 900;
/// How many runs of each side a measurement needs at least.
pub const RUNS_AT_LEAST: usize = 3;
/// How many runs of each side it may ask for.
const RUNS_AT_MOST: usize = 20;
/// The file in a project's settings that holds the definition.
pub const LANE_FILE: &str = "lane.toml";

/// The label on the card's issue while a job of its change is queued or running.
pub const IN_PROGRESS_LABEL: &str = "measuring";

/// What shows on the card's issue that a job is queued or running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InProgress {
    /// The label [`IN_PROGRESS_LABEL`], taken off when the job ends, errs or is cancelled.
    #[default]
    Label,
    /// Nothing.
    None,
}

/// Where a step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum On {
    /// In the checkout of the change's head.
    Head,
    /// In the checkout of the head, and in the checkout of the merge base.
    Both,
}

/// How a measuring step gives its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Measure {
    pub runs: usize,
    /// The start of a line of output that is a row, such as `MEASURE `; none when only a file is read.
    pub prefix: Option<String>,
    /// A file the step writes, relative to the checkout.
    pub json: Option<String>,
    /// A file whose content must be the same before and after, relative to the checkout.
    pub parity_file: Option<String>,
}

/// One step of a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: String,
    pub command: String,
    pub timeout_seconds: u64,
    pub on: On,
    pub measure: Option<Measure>,
}

/// The steps of a project's lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub steps: Vec<Step>,
    /// Paths of the checkout (files or directories) that measure: from the merge base on both sides.
    pub harness: Vec<String>,
    /// What shows on the issue while a job is queued or running.
    pub in_progress: InProgress,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    harness: Vec<String>,
    #[serde(default)]
    in_progress: InProgress,
    #[serde(default)]
    step: Vec<RawStep>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    name: String,
    command: String,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    on: Option<On>,
    #[serde(default)]
    measure: Option<RawMeasure>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMeasure {
    #[serde(default)]
    runs: Option<usize>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    parity_file: Option<String>,
}

/// A path inside the checkout: relative, with no `..`, quote or line break.
fn is_checkout_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\'', '"', '\n', '\r', '\0'])
        && !path.split('/').any(|part| part == "..")
}

/// The definition `text` (a `lane.toml`) is, or what is wrong with it, one mistake after another.
pub fn parse(text: &str) -> Result<Definition, String> {
    let file: File = toml::from_str(text).map_err(|e| format!("{LANE_FILE}: {e}"))?;
    let mut problems: Vec<String> = Vec::new();
    if file.step.is_empty() {
        problems.push("there is no [[step]]".into());
    }
    if file.step.len() > STEPS_AT_MOST {
        problems.push(format!("there are at most {STEPS_AT_MOST} steps"));
    }
    for path in &file.harness {
        if !is_checkout_path(path.trim_start_matches('/')) || path.trim_matches('/').is_empty() {
            problems.push(format!(
                "harness: {path:?} is a path inside the checkout, with no `..`"
            ));
        }
    }
    if file.harness.len() > 50 {
        problems.push("harness has at most 50 paths".into());
    }
    let mut steps: Vec<Step> = Vec::new();
    for (at, raw) in file.step.iter().enumerate() {
        let name = raw.name.trim();
        let label = format!("step {} ({name:?})", at + 1);
        if name.is_empty()
            || name.chars().count() > 40
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'))
        {
            problems.push(format!(
                "{label}: a name is 1 to 40 letters, digits, spaces, dots, hyphens or underscores"
            ));
        }
        if steps.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
            problems.push(format!("{label}: the name is used twice"));
        }
        if raw.command.trim().is_empty() || raw.command.contains('\0') {
            problems.push(format!("{label}: the command is empty"));
        }
        let timeout = raw.timeout_seconds.unwrap_or(STEP_TIMEOUT);
        if !(1..=STEP_TIMEOUT_AT_MOST).contains(&timeout) {
            problems.push(format!(
                "{label}: timeout_seconds is from 1 to {STEP_TIMEOUT_AT_MOST}"
            ));
        }
        let measure = raw.measure.as_ref().map(|m| {
            let runs = m.runs.unwrap_or(RUNS_AT_LEAST);
            if !(RUNS_AT_LEAST..=RUNS_AT_MOST).contains(&runs) {
                problems.push(format!(
                    "{label}: measure.runs is from {RUNS_AT_LEAST} to {RUNS_AT_MOST}: a measurement is at least {RUNS_AT_LEAST} runs of each side"
                ));
            }
            if m.prefix.is_none() && m.json.is_none() {
                problems.push(format!(
                    "{label}: a measuring step says where its rows are, with measure.prefix or measure.json"
                ));
            }
            if m.prefix.as_deref().is_some_and(|p| p.trim().is_empty() || p.contains('\n')) {
                problems.push(format!("{label}: measure.prefix is the start of a line"));
            }
            for (key, path) in [("json", &m.json), ("parity_file", &m.parity_file)] {
                if path.as_deref().is_some_and(|p| !is_checkout_path(p)) {
                    problems.push(format!(
                        "{label}: measure.{key} is a path inside the checkout, with no `..`"
                    ));
                }
            }
            Measure {
                runs,
                prefix: m.prefix.clone(),
                json: m.json.clone(),
                parity_file: m.parity_file.clone(),
            }
        });
        steps.push(Step {
            name: name.to_string(),
            command: raw.command.clone(),
            timeout_seconds: timeout,
            // A measuring step runs on both sides whatever it says.
            on: if measure.is_some() {
                On::Both
            } else {
                raw.on.unwrap_or(On::Head)
            },
            measure,
        });
    }
    if problems.is_empty() {
        Ok(Definition {
            steps,
            harness: file.harness,
            in_progress: file.in_progress,
        })
    } else {
        Err(format!("{LANE_FILE}: {}", problems.join("; ")))
    }
}

impl Definition {
    /// Whether any step measures.
    pub fn measures(&self) -> bool {
        self.steps.iter().any(|s| s.measure.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_definition_lists_steps_with_their_limits() {
        let found = parse(
            "[[step]]\nname = \"build\"\ncommand = \"./build.sh\"\ntimeout_seconds = 1800\non = \"both\"\n\
             [[step]]\nname = \"measure\"\ncommand = \"./bench.sh\"\n[step.measure]\nprefix = \"MEASURE \"\n",
        )
        .unwrap();
        assert_eq!(found.steps.len(), 2);
        assert_eq!(found.steps[0].timeout_seconds, 1800);
        assert_eq!(found.steps[0].on, On::Both);
        assert_eq!(found.steps[1].timeout_seconds, STEP_TIMEOUT);
        let measure = found.steps[1].measure.as_ref().unwrap();
        assert_eq!(measure.runs, RUNS_AT_LEAST);
        assert_eq!(
            found.steps[1].on,
            On::Both,
            "a measurement runs on both sides"
        );
        assert!(found.measures());
    }

    #[test]
    fn the_harness_is_a_list_of_paths_inside_the_checkout() {
        let found = parse("harness = [\"bench/\", \"/tools/rad-kbench\"]\n[[step]]\nname = \"a\"\ncommand = \"x\"\n").unwrap();
        assert_eq!(found.harness, ["bench/", "/tools/rad-kbench"]);
        assert!(
            parse("[[step]]\nname = \"a\"\ncommand = \"x\"\n")
                .unwrap()
                .harness
                .is_empty()
        );
        for bad in ["\"../x\"", "\"\"", "\"/\""] {
            let wrong = parse(&format!(
                "harness = [{bad}]\n[[step]]\nname = \"a\"\ncommand = \"x\"\n"
            ))
            .unwrap_err();
            assert!(wrong.contains("harness"), "{wrong}");
        }
    }

    #[test]
    fn the_issue_is_labelled_unless_the_file_says_none() {
        let step = "[[step]]\nname = \"a\"\ncommand = \"x\"\n";
        assert_eq!(parse(step).unwrap().in_progress, InProgress::Label);
        let quiet = parse(&format!("in_progress = \"none\"\n{step}")).unwrap();
        assert_eq!(quiet.in_progress, InProgress::None);
        let label = parse(&format!("in_progress = \"label\"\n{step}")).unwrap();
        assert_eq!(label.in_progress, InProgress::Label);
        assert!(parse(&format!("in_progress = \"loud\"\n{step}")).is_err());
    }

    #[test]
    fn every_mistake_is_said_at_once() {
        let wrong = parse(
            "[[step]]\nname = \"a\"\ncommand = \"\"\n[[step]]\nname = \"A\"\ncommand = \"x\"\ntimeout_seconds = 0\n\
             [[step]]\nname = \"m\"\ncommand = \"x\"\n[step.measure]\nruns = 2\n",
        )
        .unwrap_err();
        for part in [
            "command is empty",
            "used twice",
            "timeout_seconds",
            "measure.runs",
            "measure.prefix or measure.json",
        ] {
            assert!(wrong.contains(part), "{wrong}");
        }
        assert!(parse("").unwrap_err().contains("no [[step]]"));
        assert!(parse("[[step]]\nname = \"a\"\ncommand = \"x\"\ntypo = 1\n").is_err());
        let path =
            parse("[[step]]\nname = \"m\"\ncommand = \"x\"\n[step.measure]\njson = \"../x\"\n")
                .unwrap_err();
        assert!(path.contains("measure.json"), "{path}");
    }
}
