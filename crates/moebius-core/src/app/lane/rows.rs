//! The rows of a measurement, read from what a step wrote: lines of its output that start with the step's prefix, or a
//! JSON file it left. What a step prints is data, since the code that printed it is the change's own: a name or a unit
//! that is not a plain word is dropped, a value must be a finite number, and a run has at most [`ROWS_AT_MOST`] rows: more
//! is an error and not a cut (which rows were the real ones is not known), as is a name that is given twice.

use serde_json::Value;

/// How many rows one run of a step may give.
pub const ROWS_AT_MOST: usize = 500;

/// One number a run measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

fn word(text: &str, longest: usize) -> bool {
    !text.is_empty()
        && text.chars().count() <= longest
        && text.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(
                    c,
                    '_' | '.' | '/' | ':' | '+' | '-' | '%' | '@' | '[' | ']' | '(' | ')'
                )
        })
}

fn row(name: &str, value: f64, unit: &str) -> Option<Row> {
    (word(name, 80) && value.is_finite() && (unit.is_empty() || word(unit, 20))).then(|| Row {
        name: name.to_string(),
        value,
        unit: unit.to_string(),
    })
}

/// The rows in the lines of output that start with `prefix`: `<prefix><name> <value> [unit]` or
/// `<prefix><name>=<value> [unit]`. More than [`ROWS_AT_MOST`] is an error.
pub fn from_lines(lines: &[String], prefix: &str) -> Result<Vec<Row>, String> {
    let rows: Vec<Row> = lines
        .iter()
        .filter_map(|line| line.strip_prefix(prefix))
        .filter_map(|rest| {
            let rest = rest.trim();
            let (name, tail) = match rest.split_once('=') {
                Some((name, tail)) if !name.contains(char::is_whitespace) => (name, tail),
                _ => rest.split_once(char::is_whitespace)?,
            };
            let mut parts = tail.split_whitespace();
            let value: f64 = parts.next()?.parse().ok()?;
            row(
                name.trim_end_matches(':'),
                value,
                parts.next().unwrap_or(""),
            )
        })
        .collect();
    too_many(rows)
}

fn too_many(rows: Vec<Row>) -> Result<Vec<Row>, String> {
    if rows.len() > ROWS_AT_MOST {
        return Err(format!(
            "it gave more than {ROWS_AT_MOST} rows in one run, which is not a measurement"
        ));
    }
    Ok(rows)
}

/// Whether no name is given twice: the second of a name would be the code under test's word against the harness's.
pub fn names_are_unique(rows: &[Row]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    match rows.iter().find(|row| !seen.insert(row.name.as_str())) {
        Some(row) => Err(format!("the row {} was given twice in one run", row.name)),
        None => Ok(()),
    }
}

/// The rows of a JSON file: `{"rows": [{"name", "value", "unit"}]}`, or a flat object of names and numbers.
pub fn from_json(text: &str) -> Result<Vec<Row>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let mut rows = Vec::new();
    if let Some(list) = value.get("rows").and_then(Value::as_array) {
        for item in list {
            let (Some(name), Some(number)) = (
                item.get("name").and_then(Value::as_str),
                item.get("value").and_then(Value::as_f64),
            ) else {
                continue;
            };
            let unit = item.get("unit").and_then(Value::as_str).unwrap_or("");
            rows.extend(row(name, number, unit));
        }
    } else if let Some(object) = value.as_object() {
        for (name, number) in object {
            if let Some(number) = number.as_f64() {
                rows.extend(row(name, number, ""));
            }
        }
    } else {
        return Err("expected an object with `rows`, or an object of names and numbers".into());
    }
    too_many(rows)
}

/// The median of `values`, which must not be empty.
pub fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    #[test]
    fn named_lines_of_output_are_rows() {
        let rows = from_lines(
            &lines(
                "noise\nMEASURE decode_tps 41.5 tok/s\nMEASURE prefill=1200\nMEASURE bad nan\nMEASURE\nMEASURE two words 3\n",
            ),
            "MEASURE ",
        )
        .unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(
            rows[0],
            Row {
                name: "decode_tps".into(),
                value: 41.5,
                unit: "tok/s".into()
            }
        );
        assert_eq!(rows[1].name, "prefill");
        assert_eq!(rows[1].value, 1200.0);
    }

    #[test]
    fn a_name_that_is_not_a_plain_word_is_not_a_row() {
        let rows = from_lines(&lines("M x<script> 1\nM ok 2 `rm`\nM fine 3"), "M ").unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].name, "fine");
    }

    #[test]
    fn a_json_file_gives_rows_in_either_form() {
        let listed = from_json(
            r#"{"rows":[{"name":"a","value":1.5,"unit":"ms"},{"name":"b","value":"x"}]}"#,
        )
        .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].unit, "ms");
        let flat = from_json(r#"{"a": 1, "b": 2.5, "c": "no"}"#).unwrap();
        assert_eq!(flat.len(), 2);
        assert!(from_json("[1]").is_err());
        assert!(from_json("nope").is_err());
    }

    #[test]
    fn more_rows_than_a_run_may_give_are_an_error_and_not_the_first_of_them() {
        let many = lines(&"M a 1\n".repeat(ROWS_AT_MOST + 1));
        assert!(from_lines(&many, "M ").unwrap_err().contains("more than"));
        assert_eq!(from_lines(&many[1..], "M ").unwrap().len(), ROWS_AT_MOST);
        let rows: Vec<String> = (0..=ROWS_AT_MOST)
            .map(|n| format!("{{\"name\": \"r{n}\", \"value\": 1}}"))
            .collect();
        let json = format!("{{\"rows\": [{}]}}", rows.join(","));
        assert!(from_json(&json).is_err());
    }

    #[test]
    fn a_name_given_twice_is_an_error() {
        let rows = from_lines(&lines("M tps 99\nM tps 41"), "M ").unwrap();
        assert!(names_are_unique(&rows).unwrap_err().contains("tps"));
        assert!(names_are_unique(&from_lines(&lines("M a 1\nM b 1"), "M ").unwrap()).is_ok());
    }

    #[test]
    fn the_median_is_of_the_middle_values() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    }
}
