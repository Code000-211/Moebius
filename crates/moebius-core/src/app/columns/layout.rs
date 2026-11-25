//! Reading and checking a project's `kanban.toml` (reference: `docs/reference/column-scripts.md`):
//!
//! ```toml
//! [[column]]
//! name = "Ready for testing"
//! script = "testing.luau"   # optional: the script of the class it counts as, when it is left out
//! after = "In review"       # a built-in or another custom column; Backlog stays first and Done last
//! counts_as = "In review"   # optional: the class of the column `after` names, when it is left out
//! lane = true               # optional, only for a column that counts as In review: a card entering it is queued
//!                           # for the project's test lane (ADR 0035)
//! ```
//!
//! The result is the whole board of the project, left to right: the six built-in columns with the custom ones among
//! them. A file with any mistake is refused as a whole, with every mistake it has said at once.

use serde::Deserialize;

use super::{BACKLOG, BUILT_IN, DONE, IN_REVIEW, built_in};

/// The file in a project's settings that defines its custom columns.
pub const KANBAN_FILE: &str = "kanban.toml";

/// The most characters a column's name has.
const LONGEST_NAME: usize = 40;

/// A column of a board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnInfo {
    pub name: String,
    /// The built-in column it counts as (for the built-in logic and reads); its own name for a built-in column.
    pub counts_as: &'static str,
    /// The file of its script in the project's settings (or built into the server, for a built-in one).
    pub script: String,
    pub custom: bool,
    /// Whether a card that enters the column is queued for the test lane (`lane = true` in `kanban.toml`).
    pub lane: bool,
}

/// What kind of problem a project's columns have, for the one who reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemKind {
    /// `kanban.toml` is not valid; the columns last known (or none) are used.
    Invalid,
    /// Cards are in columns the file no longer has; they run the script of their class.
    Orphaned,
}

/// The columns of a project's board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectColumns {
    pub columns: Vec<ColumnInfo>,
    /// Columns the file no longer has although cards are in them, with the class they counted as: their cards run
    /// the script of that class. Not part of [`ProjectColumns::columns`], so the board does not draw them.
    pub orphans: Vec<(String, &'static str)>,
    /// What is wrong with the project's `kanban.toml`, to show to people.
    pub problem: Option<(ProblemKind, String)>,
}

impl Default for ProjectColumns {
    /// The six built-in columns: the board of a project without a `kanban.toml`.
    fn default() -> Self {
        let scripts = [
            "backlog.luau",
            "ready.luau",
            "code.luau",
            "review.luau",
            "merge.luau",
            "done.luau",
        ];
        Self {
            columns: BUILT_IN
                .iter()
                .zip(scripts)
                .map(|(name, script)| ColumnInfo {
                    name: (*name).into(),
                    counts_as: name,
                    script: script.into(),
                    custom: false,
                    lane: false,
                })
                .collect(),
            orphans: Vec::new(),
            problem: None,
        }
    }
}

impl ProjectColumns {
    /// The column called `name` (in any case; "Ready" is "Not started"), if the board has it.
    pub fn column(&self, name: &str) -> Option<&ColumnInfo> {
        let name = if name.eq_ignore_ascii_case("Ready") {
            super::NOT_STARTED
        } else {
            name
        };
        self.columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// The class of the column `name`: the built-in column it counts as. A column the board no longer has but cards
    /// are in counts as it did; any other name counts as itself when it is built in, and as nothing when it is not.
    pub fn class_of(&self, name: &str) -> Option<&'static str> {
        self.column(name).map(|c| c.counts_as).or_else(|| {
            self.orphans
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, class)| *class)
        })
    }

    /// The names of the columns that count as `class`, in board order.
    pub fn names_counting_as(&self, class: &str) -> Vec<&str> {
        self.columns
            .iter()
            .filter(|c| c.counts_as == class)
            .map(|c| c.name.as_str())
            .collect()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    column: Vec<Raw>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    script: Option<String>,
    after: Option<String>,
    counts_as: Option<String>,
    #[serde(default)]
    lane: bool,
}

/// Whether `file` can name a column's script: a plain `.luau` file name of the project's folder, which is not
/// `kanban.toml`, starts with no dot and holds only letters, digits, `-`, `_`, `.` and spaces.
pub fn safe_script_name(file: &str) -> bool {
    let Some(stem) = file.strip_suffix(".luau") else {
        return false;
    };
    !stem.is_empty()
        && !stem.starts_with(['.', ' '])
        && !stem.ends_with(['.', ' '])
        && !stem.contains("..")
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
}

/// The board `text` (a `kanban.toml`) defines, or what is wrong with it, one problem per line.
pub fn parse(text: &str) -> Result<ProjectColumns, String> {
    let file: File = toml::from_str(text).map_err(|e| format!("{KANBAN_FILE}: {e}"))?;
    let mut problems: Vec<String> = Vec::new();
    let base = ProjectColumns::default();

    // Each custom column, checked on its own.
    struct Custom {
        name: String,
        script: Option<String>,
        after: Option<String>,
        counts_as: Option<&'static str>,
        lane: bool,
    }
    let mut customs: Vec<Custom> = Vec::new();
    for (at, raw) in file.column.iter().enumerate() {
        let name = raw.name.trim();
        let label = format!("column {} ({name:?})", at + 1);
        if name.is_empty() {
            problems.push(format!("column {} has no name", at + 1));
            continue;
        }
        if name.chars().count() > LONGEST_NAME || name.chars().any(char::is_control) {
            problems.push(format!(
                "{label}: a name has at most {LONGEST_NAME} characters and no control character"
            ));
        }
        if built_in(name).is_some() {
            problems.push(format!(
                "{label}: the name is taken by a built-in column (\"Ready\" is the alias of \"Not started\")"
            ));
        }
        if customs.iter().any(|c| c.name.eq_ignore_ascii_case(name)) {
            problems.push(format!(
                "{label}: the name is used twice (names are compared without case)"
            ));
        }
        if let Some(script) = &raw.script
            && !safe_script_name(script)
        {
            problems.push(format!(
                "{label}: script {script:?} must be a plain file name ending in .luau, without a folder"
            ));
        }
        let counts_as = match &raw.counts_as {
            None => None,
            Some(class) => match BUILT_IN.iter().find(|b| **b == class.as_str()) {
                Some(class) => Some(*class),
                None => {
                    problems.push(format!(
                        "{label}: counts_as {class:?} is none of {}",
                        BUILT_IN.join(", ")
                    ));
                    None
                }
            },
        };
        customs.push(Custom {
            name: name.to_string(),
            script: raw.script.clone(),
            after: raw.after.as_ref().map(|a| a.trim().to_string()),
            counts_as,
            lane: raw.lane,
        });
    }

    // Where each goes: `after` names a built-in (before Done) or another custom column.
    let mut parent: Vec<Option<Option<usize>>> = Vec::new(); // None: unresolved; Some(None): after a built-in
    let mut built_after: Vec<Option<&'static str>> = Vec::new();
    for c in &customs {
        let label = format!("column {:?}", c.name);
        let target = match (&c.after, c.counts_as) {
            (Some(after), _) => Some(after.clone()),
            (None, Some(class)) => Some(class.to_string()),
            (None, None) => {
                problems.push(format!(
                    "{label}: give `after` (the column it comes after) or `counts_as`"
                ));
                None
            }
        };
        let Some(target) = target else {
            parent.push(None);
            built_after.push(None);
            continue;
        };
        if let Some(b) = built_in(&target) {
            if b == DONE {
                problems.push(format!("{label}: nothing comes after Done"));
                parent.push(None);
                built_after.push(None);
            } else {
                parent.push(Some(None));
                built_after.push(Some(b));
            }
        } else if let Some(i) = customs
            .iter()
            .position(|o| o.name.eq_ignore_ascii_case(&target))
        {
            parent.push(Some(Some(i)));
            built_after.push(None);
        } else {
            problems.push(format!("{label}: after {target:?} names no column"));
            parent.push(None);
            built_after.push(None);
        }
    }

    // Order: each built-in, then the columns after it (file order), each followed by the columns after it.
    let mut order: Vec<usize> = Vec::new();
    fn place(at: usize, parent: &[Option<Option<usize>>], order: &mut Vec<usize>) {
        order.push(at);
        for (i, p) in parent.iter().enumerate() {
            if *p == Some(Some(at)) {
                place(i, parent, order);
            }
        }
    }
    let mut ordered: Vec<(Option<&'static str>, Vec<usize>)> = Vec::new();
    for built in BUILT_IN {
        let mut under = Vec::new();
        for (i, p) in parent.iter().enumerate() {
            if *p == Some(None) && built_after[i] == Some(built) {
                place(i, &parent, &mut under);
            }
        }
        ordered.push((Some(built), under));
    }
    order.extend(ordered.iter().flat_map(|(_, u)| u.iter().copied()));
    for (i, p) in parent.iter().enumerate() {
        if p.is_some() && !order.contains(&i) {
            problems.push(format!(
                "column {:?}: the `after` columns lead round in a circle",
                customs[i].name
            ));
        }
    }

    // Classes: explicit, or the class of the column it comes after.
    let mut class_of: Vec<Option<&'static str>> = vec![None; customs.len()];
    for &i in &order {
        class_of[i] = customs[i].counts_as.or(match parent[i] {
            Some(None) => built_after[i],
            Some(Some(p)) => class_of[p],
            None => None,
        });
    }

    for &i in &order {
        if customs[i].lane && class_of[i] != Some(IN_REVIEW) {
            problems.push(format!(
                "column {:?}: `lane = true` is for a column that counts as {IN_REVIEW}",
                customs[i].name
            ));
        }
    }

    if !problems.is_empty() {
        return Err(format!("{KANBAN_FILE}: {}", problems.join("; ")));
    }

    let mut columns = Vec::new();
    for (built, under) in ordered {
        let built = built.expect("each entry has its built-in column");
        let info = base.column(built).expect("built-in");
        columns.push(info.clone());
        for i in under {
            let class = class_of[i].expect("checked");
            let custom = &customs[i];
            columns.push(ColumnInfo {
                name: custom.name.clone(),
                counts_as: class,
                script: custom
                    .script
                    .clone()
                    .unwrap_or_else(|| base.column(class).expect("built-in").script.clone()),
                custom: true,
                lane: custom.lane,
            });
        }
    }
    debug_assert_eq!(columns.first().map(|c| c.name.as_str()), Some(BACKLOG));
    debug_assert_eq!(columns.last().map(|c| c.name.as_str()), Some(DONE));
    Ok(ProjectColumns {
        columns,
        orphans: Vec::new(),
        problem: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        parse(text)
            .unwrap()
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    #[test]
    fn no_custom_column_is_the_six_built_in_ones() {
        assert_eq!(parse("").unwrap(), ProjectColumns::default());
    }

    #[test]
    fn columns_come_after_what_they_name_and_ties_keep_file_order() {
        let text = r#"
[[column]]
name = "Ready for testing"
script = "testing.luau"
after = "In review"

[[column]]
name = "Being measured"
after = "In review"

[[column]]
name = "Waiting for GPU"
after = "Ready for testing"

[[column]]
name = "Parked"
after = "Backlog"
"#;
        assert_eq!(
            names(text),
            [
                "Backlog",
                "Parked",
                "Not started",
                "In progress",
                "In review",
                "Ready for testing",
                "Waiting for GPU",
                "Being measured",
                "Ready to merge",
                "Done"
            ]
        );
    }

    #[test]
    fn a_custom_column_counts_as_the_column_before_it_unless_it_says_otherwise() {
        let parsed = parse(
            r#"
[[column]]
name = "Ready for testing"
script = "testing.luau"
after = "In review"

[[column]]
name = "Being measured"
after = "Ready for testing"

[[column]]
name = "Shipped"
after = "Ready to merge"
counts_as = "Done"
"#,
        )
        .unwrap();
        let class = |n: &str| parsed.class_of(n).unwrap();
        assert_eq!(class("Ready for testing"), "In review");
        assert_eq!(class("Being measured"), "In review");
        assert_eq!(class("Shipped"), "Done");
        assert_eq!(class("Done"), "Done");
        assert_eq!(
            parsed.column("Being measured").unwrap().script,
            "review.luau"
        );
        assert_eq!(
            parsed.column("Ready for testing").unwrap().script,
            "testing.luau"
        );
        assert_eq!(parsed.names_counting_as("In review").len(), 3);
    }

    #[test]
    fn counts_as_alone_places_a_column_after_that_built_in_one() {
        assert_eq!(
            names("[[column]]\nname = \"Hold\"\ncounts_as = \"In progress\"\n")[3],
            "Hold"
        );
    }

    #[test]
    fn mistakes_are_all_reported() {
        let text = r#"
[[column]]
name = "done"
after = "In review"

[[column]]
name = "Ready"
after = "In review"

[[column]]
name = "A"
after = "B"

[[column]]
name = "B"
after = "A"

[[column]]
name = "c"
after = "Nowhere"
script = "../x.luau"

[[column]]
name = "C"
after = "Done"
counts_as = "Shipping"

[[column]]
name = "D"
"#;
        let error = parse(text).unwrap_err();
        for expected in [
            "taken by a built-in",
            "used twice",
            "circle",
            "names no column",
            "plain file name",
            "nothing comes after Done",
            "is none of",
            "give `after`",
        ] {
            assert!(error.contains(expected), "{expected} not in {error}");
        }
    }

    #[test]
    fn unknown_keys_and_bad_toml_are_refused() {
        assert!(parse("[[column]]\nname = \"A\"\nafter = \"Done\"\nbad = 1\n").is_err());
        assert!(parse("[[column").is_err());
    }

    #[test]
    fn script_names_must_be_safe() {
        for good in ["testing.luau", "my script-2.luau", "a.b.luau"] {
            assert!(safe_script_name(good), "{good}");
        }
        for bad in [
            "",
            ".luau",
            "a/b.luau",
            "a\\b.luau",
            ".hidden.luau",
            "x.lua",
            "kanban.toml",
            "a..b.luau",
            "é.luau",
        ] {
            assert!(!safe_script_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_column_marked_lane_must_count_as_in_review() {
        let board =
            parse("[[column]]\nname = \"Ready for testing\"\nafter = \"In review\"\nlane = true\n")
                .unwrap();
        let column = board.column("Ready for testing").unwrap();
        assert!(column.lane && column.counts_as == "In review");
        assert!(
            !board.column("In review").unwrap().lane,
            "a built-in column has no lane"
        );
        assert!(
            board
                .views()
                .iter()
                .any(|c| c.name == "Ready for testing" && c.lane)
        );
        // After a custom column that counts as In review, it counts as it too.
        assert!(parse("[[column]]\nname = \"A\"\nafter = \"In review\"\n[[column]]\nname = \"B\"\nafter = \"A\"\nlane = true\n").is_ok());
        let wrong =
            parse("[[column]]\nname = \"Shipped\"\nafter = \"Ready to merge\"\nlane = true\n")
                .unwrap_err();
        assert!(
            wrong.contains("`lane = true` is for a column that counts as In review"),
            "{wrong}"
        );
        assert!(
            parse("[[column]]\nname = \"X\"\nafter = \"In review\"\nlane = \"yes\"\n").is_err()
        );
    }
}
