//! Jira's users in the import: each becomes a person of Moebius who is disabled until an owner gives them a token, or
//! is mapped to a person that exists, or (when the import is asked to keep names) stays a name on what they wrote.

use super::read::User;
use super::{Import, SOURCE, Users};
use crate::app::{Result, person_author};
use crate::store::Role;

/// The longest handle a person has.
const HANDLE_LIMIT: usize = 39;

/// What a Jira user is in Moebius.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Who {
    /// The author of what they wrote: `person:<handle>`, or their name with `(Jira)` after it.
    pub author: String,
    /// Their person, who may be disabled; none for a user who stays a name.
    pub person: Option<(i64, String)>,
}

/// `text` as a handle: lowercase letters, digits, `-` and `_`, starting with a letter or a digit.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.to_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() || c == '_' {
            c
        } else {
            '-'
        };
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(c);
    }
    let out = out.trim_start_matches('_').trim_end_matches('-');
    out.chars().take(HANDLE_LIMIT).collect()
}

impl Import<'_> {
    /// The author a user is, made on the first ask: see [`Users`].
    pub(super) fn who_is(&mut self, user: &User) -> Result<Who> {
        if let Some(known) = self.users.get(&user.id) {
            return Ok(known.clone());
        }
        let who = self.resolve(user)?;
        self.users.insert(user.id.clone(), who.clone());
        Ok(who)
    }

    fn resolve(&mut self, user: &User) -> Result<Who> {
        let store = &self.app.store;
        let mapped = [
            Some(user.id.as_str()),
            user.email.as_deref(),
            Some(user.name.as_str()),
        ]
        .into_iter()
        .flatten()
        .find_map(|key| {
            self.options
                .user_map
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, handle)| handle.clone())
        });
        if let Some(handle) = mapped {
            let person = store
                .person_by_handle(handle.trim_start_matches('@'))?
                .expect("the handles of the map were checked");
            return Ok(Who {
                author: person.author(),
                person: Some((person.id, person.handle)),
            });
        }
        if let Some(made) = store.imported(self.project_id, SOURCE, "user", &user.id)?
            && let Some(person) = store.person(made.local_id)?
        {
            self.report.people.unchanged += 1;
            return Ok(Who {
                author: person.author(),
                person: Some((person.id, person.handle)),
            });
        }
        if self.options.users == Users::Names {
            return Ok(Who {
                author: format!("{} (Jira)", user.name),
                person: None,
            });
        }
        let base = user
            .email
            .as_deref()
            .and_then(|e| e.split('@').next())
            .map(slug)
            .filter(|s| !s.is_empty())
            .or_else(|| Some(slug(&user.name)).filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "jira-user".into());
        let mut handle = base.clone();
        let mut n = 1;
        while store.person_by_handle(&handle)?.is_some() {
            n += 1;
            let suffix = format!("-{n}");
            let room = HANDLE_LIMIT - suffix.len();
            handle = format!("{}{suffix}", base.chars().take(room).collect::<String>());
        }
        let person = store.add_person(&handle, &super::name_of(&user.name, 80), Role::Member)?;
        // Assigned to cards before it is disabled, which the app refuses to do the other way round.
        store.set_person_disabled(person.id, true)?;
        store.set_imported(
            self.project_id,
            SOURCE,
            "user",
            &user.id,
            person.id,
            &user.name,
        )?;
        self.report.people.created += 1;
        Ok(Who {
            author: person_author(&person.handle),
            person: Some((person.id, person.handle)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn a_handle_is_made_of_an_email_or_a_name() {
        assert_eq!(slug("ada.lovelace"), "ada-lovelace");
        assert_eq!(slug("Ada Lovelace"), "ada-lovelace");
        assert_eq!(slug("  -Ünï "), "n");
        assert_eq!(slug("__x"), "x");
        assert_eq!(slug("!!!"), "");
        assert!(slug(&"a".repeat(100)).len() <= 39);
    }
}
