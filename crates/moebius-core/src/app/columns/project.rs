//! What the app does with a project's `kanban.toml`: the columns of the project's board, kept for as long as the file
//! reads the same, the class of a column, and the check a save of the file goes through (ADR 0030).
//!
//! Whenever the columns are read, the classes of the custom ones are written to the store's `column_classes` table too,
//! so that SQL can ask for them (`store::class_sql`). A file that is not valid leaves the columns last known in force;
//! a column the file lost while cards are in it stays known as an orphan, and its cards run the script of the class it
//! had, with a notice (`ProjectColumns::problem`), rather than failing at every tick.

use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the cards in the columns a file lost are taken to be where they were: a card moved out of such a column is
/// noticed after this long, and a file that changes is read at once.
const ORPHANS_KEPT: Duration = Duration::from_secs(5);

use super::{BACKLOG, ColumnInfo, KANBAN_FILE, ProblemKind, ProjectColumns, built_in, parse};
use crate::app::{App, AppError, Result};
use crate::store::Project;

/// What was read of a project's file, and what was made of it.
pub(crate) struct Cached {
    text: Option<String>,
    /// The columns of the last file that was valid (the six built-in ones before there is one).
    good: Arc<ProjectColumns>,
    columns: Arc<ProjectColumns>,
    /// When the orphans were last looked for.
    looked: Instant,
}

impl App {
    /// The columns of `project`'s board as its `kanban.toml` defines them; the six built-in ones without the file.
    pub fn project_columns(&self, project: &Project) -> Arc<ProjectColumns> {
        let text = self.project_file(project, KANBAN_FILE);
        self.project_columns_from(project, text)
    }

    /// [`App::project_columns`] for the project `id`.
    pub fn project_columns_of(&self, id: i64) -> Result<Arc<ProjectColumns>> {
        Ok(self.project_columns(&self.project(id)?))
    }

    /// The columns of `project` given the text of its `kanban.toml`, which the caller read (the engine reads each file
    /// once per tick).
    pub(crate) fn project_columns_from(
        &self,
        project: &Project,
        text: Option<String>,
    ) -> Arc<ProjectColumns> {
        let mut cache = self.column_cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = cache.get(&project.id)
            && found.text == text
            && (found.columns.orphans.is_empty() || found.looked.elapsed() < ORPHANS_KEPT)
        {
            return found.columns.clone();
        }
        let good = cache.get(&project.id).map(|c| c.good.clone());
        let (mut columns, good, invalid) = match text.as_deref().map(parse) {
            None => (
                ProjectColumns::default(),
                Arc::new(ProjectColumns::default()),
                None,
            ),
            Some(Ok(parsed)) => (parsed.clone(), Arc::new(parsed), None),
            Some(Err(error)) => {
                let good = good.unwrap_or_default_columns();
                ((*good).clone(), good, Some(error))
            }
        };
        // Columns the store knows that the file does not have, with cards in them.
        let known = self.store.column_classes(project.id).unwrap_or_default();
        for (name, class, _) in &known {
            if columns.column(name).is_some() || built_in(name).is_some() {
                continue;
            }
            let Some(class) = built_in(class) else {
                continue;
            };
            if self
                .store
                .cards_in_column(project.id, name)
                .is_ok_and(|cards| !cards.is_empty())
            {
                columns.orphans.push((name.clone(), class));
            }
        }
        let present: Vec<(String, String)> = columns
            .columns
            .iter()
            .filter(|c| c.custom)
            .map(|c| (c.name.clone(), c.counts_as.to_string()))
            .collect();
        let kept: Vec<(String, String)> = columns
            .orphans
            .iter()
            .map(|(n, c)| (n.clone(), (*c).to_string()))
            .collect();
        if let Err(error) = self.store.set_column_classes(project.id, &present, &kept) {
            eprintln!(
                "moebius: could not record the columns of {}: {error}",
                project.name
            );
        }
        columns.problem = match invalid {
            Some(error) => Some((ProblemKind::Invalid, error)),
            None if !columns.orphans.is_empty() => Some((
                ProblemKind::Orphaned,
                format!(
                    "cards are in columns that {KANBAN_FILE} of {} no longer has: {}; they run the script of the column they counted as",
                    project.name,
                    columns
                        .orphans
                        .iter()
                        .map(|(n, c)| format!("{n} (counted as {c})"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
            None => None,
        };
        // Said once for each change of what is wrong, not at every look.
        let said = cache
            .get(&project.id)
            .and_then(|c| c.columns.problem.clone());
        if columns.problem != said
            && let Some((_, problem)) = &columns.problem
        {
            eprintln!("moebius: {problem}");
        }
        let columns = Arc::new(columns);
        cache.insert(
            project.id,
            Cached {
                text,
                good,
                columns: columns.clone(),
                looked: Instant::now(),
            },
        );
        columns
    }

    /// The class of the column `column` of the project `project`: the built-in column it counts as. A column the
    /// project has never had counts as Backlog, where nothing starts for a card until a person moves it.
    pub fn column_class(&self, project: i64, column: &str) -> &'static str {
        self.project_columns_of(project)
            .ok()
            .and_then(|columns| columns.class_of(column))
            .or_else(|| built_in(column))
            .unwrap_or(BACKLOG)
    }

    /// Checks a save of the project's `kanban.toml` (`Some(text)`) or its removal (`None`) before it is written: the
    /// text must be a valid board, and no card may be in a column that the board it makes does not have.
    pub fn check_kanban_save(
        &self,
        project: &Project,
        new: Option<&str>,
    ) -> Result<ProjectColumns> {
        let columns = match new.map(parse).transpose() {
            Ok(columns) => columns.unwrap_or_default(),
            Err(error) => return Err(AppError::Invalid(error)),
        };
        let allowed: Vec<String> = columns.columns.iter().map(|c| c.name.clone()).collect();
        let stranded = self.store.cards_outside(project.id, &allowed)?;
        if let Some((column, numbers)) = stranded.first() {
            let numbers: Vec<String> = numbers.iter().take(10).map(|n| format!("#{n}")).collect();
            return Err(AppError::Conflict(format!(
                "{KANBAN_FILE} would remove the column {column:?} while cards are in it ({}{}); move them to another column first",
                numbers.join(", "),
                if stranded[0].1.len() > 10 {
                    ", …"
                } else {
                    ""
                }
            )));
        }
        Ok(columns)
    }
}

trait OrDefaultColumns {
    fn unwrap_or_default_columns(self) -> Arc<ProjectColumns>;
}

impl OrDefaultColumns for Option<Arc<ProjectColumns>> {
    fn unwrap_or_default_columns(self) -> Arc<ProjectColumns> {
        self.unwrap_or_else(|| Arc::new(ProjectColumns::default()))
    }
}

impl ColumnInfo {
    /// Whether this is a column the project defined.
    pub fn is_custom(&self) -> bool {
        self.custom
    }
}
