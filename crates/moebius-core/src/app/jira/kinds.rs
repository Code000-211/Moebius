//! Jira's issue types as kinds, and the custom fields their issues have as the kinds' fields. A kind is the
//! organisation's, so one that has the name of an issue type already is used and given the fields it lacks; a field it
//! has with another type is left alone, and the values for it are not imported.

use std::collections::{BTreeMap, BTreeSet};

use super::read::{Custom, CustomField, Read};
use super::{Import, name_of};
use crate::app::Result;
use crate::app::kinds::{FIELD_LIMIT, NAME_LIMIT};
use crate::store::{FieldDef, FieldType};

/// What the kinds are after the import: the kind each issue type became, and the name each custom field has in them.
#[derive(Debug, Default)]
pub(super) struct Kinds {
    pub of_type: BTreeMap<String, String>,
    pub field_names: BTreeMap<String, String>,
}

fn def(field: &KindFieldOf) -> FieldDef {
    FieldDef {
        name: field.name.clone(),
        field_type: field.field_type,
        options: field.options.clone(),
    }
}

/// A field of a kind, as the store or the import has it.
struct KindFieldOf {
    name: String,
    field_type: FieldType,
    options: Vec<String>,
}

impl Import<'_> {
    /// Makes or completes the kind of each issue type, and has the project offer them. Epics are no cards, so they
    /// have no kind.
    pub(super) fn import_kinds(&mut self, read: &Read) -> Result<Kinds> {
        let app = self.app;
        let mut kinds = Kinds::default();
        // The names of the custom fields in Moebius: Jira's own, told apart by their id when two fields share one.
        let mut taken = BTreeSet::new();
        for field in &read.custom_fields {
            let mut name = name_of(&field.name, NAME_LIMIT);
            if name.is_empty() || !taken.insert(name.to_lowercase()) {
                name = name_of(&format!("{name} ({})", field.id), NAME_LIMIT);
                taken.insert(name.to_lowercase());
            }
            kinds.field_names.insert(field.name.clone(), name);
        }
        let types: BTreeSet<&str> = read
            .issues
            .iter()
            .filter(|i| !i.is_epic)
            .map(|i| i.issue_type.as_str())
            .collect();
        let mut offered: Vec<String> = Vec::new();
        for issue_type in types {
            let name = name_of(issue_type, NAME_LIMIT);
            let wanted = self.wanted_fields(read, issue_type, &kinds);
            let existing = app.store.kind_by_name(&name)?;
            let mut defs: Vec<FieldDef> = existing
                .iter()
                .flat_map(|k| &k.fields)
                .map(|f| {
                    def(&KindFieldOf {
                        name: f.name.clone(),
                        field_type: f.field_type,
                        options: f.options.clone(),
                    })
                })
                .collect();
            let before = defs.clone();
            for want in wanted {
                let at = defs
                    .iter()
                    .position(|d| d.name.eq_ignore_ascii_case(&want.name));
                match at {
                    Some(i) if defs[i].field_type == want.field_type => {
                        for option in want.options {
                            if !defs[i].options.iter().any(|o| o.eq_ignore_ascii_case(&option)) {
                                defs[i].options.push(option);
                            }
                        }
                    }
                    Some(i) => self.warn(format!(
                        "the kind {name} has a {} field {} where Jira has a {}: its values are not imported",
                        defs[i].field_type.as_str(),
                        defs[i].name,
                        want.field_type.as_str()
                    )),
                    None if defs.len() >= FIELD_LIMIT => self.warn(format!(
                        "the kind {name} has {FIELD_LIMIT} fields already: {} is left out",
                        want.name
                    )),
                    None => defs.push(want),
                }
            }
            match existing {
                None => {
                    app.add_kind(&name, &defs)?;
                    self.report.kinds.created += 1;
                }
                Some(kind) if defs != before => {
                    app.update_kind(kind.id, &kind.name, &defs)?;
                    self.report.kinds.updated += 1;
                }
                Some(_) => self.report.kinds.unchanged += 1,
            }
            let stored = app.kind_named(&name)?;
            kinds
                .of_type
                .insert(issue_type.to_string(), stored.name.clone());
            offered.push(stored.name);
        }
        let mut names: Vec<String> = app
            .store
            .project_kinds(self.project_id)?
            .into_iter()
            .filter_map(|id| app.store.kind(id).ok().flatten())
            .map(|k| k.name)
            .collect();
        let before = names.len();
        for name in offered {
            if !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                names.push(name);
            }
        }
        if names.len() != before {
            app.set_project_kinds(self.project_id, &names)?;
        }
        Ok(kinds)
    }

    /// The fields the issues of `issue_type` have a value for, with the options their values need.
    fn wanted_fields(&self, read: &Read, issue_type: &str, kinds: &Kinds) -> Vec<FieldDef> {
        let mut wanted = Vec::new();
        for field in &read.custom_fields {
            let Some(name) = kinds.field_names.get(&field.name) else {
                continue;
            };
            let values: Vec<&Custom> = read
                .issues
                .iter()
                .filter(|i| !i.is_epic && i.issue_type == issue_type)
                .filter_map(|i| i.custom.get(&field.name))
                .collect();
            if values.is_empty() {
                continue;
            }
            wanted.push(FieldDef {
                name: name.clone(),
                field_type: field.field_type,
                options: options_of(field, &values),
            });
        }
        wanted
    }
}

/// The options a choice needs for `values`: each distinct one that is a name, in the order they were read.
fn options_of(field: &CustomField, values: &[&Custom]) -> Vec<String> {
    if field.field_type != FieldType::Choice {
        return Vec::new();
    }
    let mut options: Vec<String> = Vec::new();
    for value in values {
        if let Custom::Choice(text) = value {
            let option = name_of(text, NAME_LIMIT);
            if !option.is_empty() && !options.iter().any(|o| o.eq_ignore_ascii_case(&option)) {
                options.push(option);
            }
        }
    }
    options
}
