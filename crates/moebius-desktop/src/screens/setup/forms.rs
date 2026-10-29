//! The forms of the Models and Settings pages made from what the server said: the upstreams with their API keys,
//! the teacher and the decision model that may be left out, and what a save of either page sends.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use crate::client::ServerSettings;

use super::fields::{DECISION_MODEL, Field, Form, GROUPS, Spec, TABLES, TEACHER};
use super::{KeyMode, Owner, Plan};

/// An API key to store by name.
pub(super) type Stored = (String, String);

/// Where the key of an upstream or of the decision model comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct KeyChoice {
    pub(super) mode: KeyMode,
    /// The environment variable.
    pub(super) env: String,
    /// The name of the key the server stores, if it has one for this.
    pub(super) stored: Option<String>,
    /// The key being typed; never read back from the server.
    pub(super) typed: String,
}

impl KeyChoice {
    pub(super) fn read(env: Option<&Value>, secret: Option<&Value>) -> Self {
        let name = |value: Option<&Value>| value.and_then(Value::as_str).map(str::to_string);
        let (env, stored) = (name(env), name(secret));
        KeyChoice {
            mode: match (&env, &stored) {
                (Some(_), _) => KeyMode::Environment,
                (None, Some(_)) => KeyMode::Typed,
                (None, None) => KeyMode::None,
            },
            env: env.unwrap_or_default(),
            stored,
            typed: String::new(),
        }
    }

    /// The key to send: the environment variable or the name of the stored key, which is `fresh` when none is stored
    /// yet; the key to store under that name, if one was typed.
    pub(super) fn plan(
        &self,
        what: &str,
        fresh: impl FnOnce() -> String,
    ) -> Result<(Map<String, Value>, Option<Stored>), String> {
        let mut keys = Map::new();
        let mut store = None;
        match self.mode {
            KeyMode::None => {}
            KeyMode::Environment => {
                if self.env.trim().is_empty() {
                    return Err(format!(
                        "Enter the environment variable of the key of {what}."
                    ));
                }
                keys.insert("api_key_env".into(), json!(self.env.trim()));
            }
            KeyMode::Typed => {
                let typed = self.typed.trim();
                if typed.is_empty() && self.stored.is_none() {
                    return Err(format!("Type the key of {what}."));
                }
                let name = self.stored.clone().unwrap_or_else(fresh);
                if !typed.is_empty() {
                    store = Some((name.clone(), typed.to_string()));
                }
                keys.insert("api_key_secret".into(), json!(name));
            }
        }
        Ok((keys, store))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Upstream {
    /// The place in the list the file had it, none for one added here.
    pub(super) origin: Option<usize>,
    pub(super) protocol: String,
    pub(super) base_url: String,
    pub(super) key: KeyChoice,
}

impl Upstream {
    pub(super) fn read(value: &Value, origin: usize) -> Self {
        let text = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Upstream {
            origin: Some(origin),
            protocol: text("protocol"),
            base_url: text("base_url"),
            key: KeyChoice::read(value.get("api_key_env"), value.get("api_key_secret")),
        }
    }
}

/// The teacher or the decision model: a table of the file that can be turned off.
#[derive(Debug, Clone)]
pub(super) struct Optional {
    pub(super) was_on: bool,
    pub(super) on: bool,
    pub(super) form: Form,
    /// Only the decision model has a key.
    pub(super) key: Option<(KeyChoice, KeyChoice)>,
}

impl Optional {
    /// Adds what the person changed in the table `table` to `plan`.
    pub(super) fn changes(
        &self,
        table: &str,
        plan: &mut Plan,
        fresh: impl FnOnce() -> String,
    ) -> Result<(), String> {
        if self.was_on && !self.on {
            plan.remove.push(table.to_string());
            return Ok(());
        }
        if !self.on {
            return Ok(());
        }
        let new = !self.was_on;
        let key_changed = self
            .key
            .as_ref()
            .is_some_and(|(now, before)| new || now != before || !now.typed.is_empty());
        if !new && !self.form.dirty() && !key_changed {
            return Ok(());
        }
        let model = self.form.field(&format!("{table}.model"));
        if model.is_some_and(|field| field.text.trim().is_empty()) {
            return Err(format!(
                "Enter the model of the {}.",
                table.replace('_', " ")
            ));
        }
        if new {
            self.form.all(&mut plan.set)?;
        } else {
            self.form.changes(&mut plan.set, &mut plan.remove)?;
        }
        if let (Some((now, _)), true) = (&self.key, key_changed) {
            let (keys, store) = now.plan("the decision model", fresh)?;
            plan.store.extend(store);
            for name in ["api_key_env", "api_key_secret"] {
                match keys.get(name) {
                    Some(value) => {
                        plan.set.insert(format!("{table}.{name}"), value.clone());
                    }
                    None if !new => plan.remove.push(format!("{table}.{name}")),
                    None => {}
                }
            }
        }
        Ok(())
    }
}

/// The forms of the Models and Settings pages, made from what the server told.
#[derive(Debug, Clone, Default)]
pub(super) struct Forms {
    pub(super) settings: Vec<(&'static str, Form)>,
    pub(super) upstreams: Vec<Upstream>,
    pub(super) original_upstreams: Vec<Upstream>,
    pub(super) teacher: Option<Optional>,
    pub(super) decision_model: Option<Optional>,
}

impl Forms {
    pub(super) fn read(server: &ServerSettings) -> Self {
        let upstreams: Vec<Upstream> = server
            .value("router.upstreams")
            .as_array()
            .map(|list| {
                list.iter()
                    .enumerate()
                    .map(|(n, u)| Upstream::read(u, n))
                    .collect()
            })
            .unwrap_or_default();
        let optional = |specs: &[Spec], keyed: bool| {
            let was_on = specs.iter().any(|(key, _, _)| server.is_set(key));
            let key = keyed.then(|| {
                let choice = KeyChoice::read(
                    server
                        .is_set("decision_model.api_key_env")
                        .then(|| server.value("decision_model.api_key_env")),
                    server
                        .is_set("decision_model.api_key_secret")
                        .then(|| server.value("decision_model.api_key_secret")),
                );
                (choice.clone(), choice)
            });
            Optional {
                was_on,
                on: was_on,
                form: Form::read(specs, server),
                key,
            }
        };
        Forms {
            settings: GROUPS
                .iter()
                .map(|(title, specs)| (*title, Form::read(specs, server)))
                .collect(),
            original_upstreams: upstreams.clone(),
            upstreams,
            teacher: Some(optional(TEACHER, false)),
            decision_model: Some(optional(DECISION_MODEL, true)),
        }
    }

    pub(super) fn field_mut(&mut self, key: &str) -> Option<&mut Field> {
        let optional = self
            .teacher
            .iter_mut()
            .chain(self.decision_model.iter_mut())
            .map(|optional| &mut optional.form);
        let settings = self.settings.iter_mut().map(|(_, form)| form);
        for form in settings.chain(optional) {
            if let Some(field) = form.fields.iter_mut().find(|field| field.key == key) {
                return Some(field);
            }
        }
        None
    }

    pub(super) fn settings_dirty(&self) -> bool {
        self.settings.iter().any(|(_, form)| form.dirty())
    }

    /// The keys the Settings page changed.
    pub(super) fn settings_changes(&self) -> Result<(Map<String, Value>, Vec<String>), String> {
        let (mut set, mut remove) = (Map::new(), Vec::new());
        for (_, form) in &self.settings {
            form.changes(&mut set, &mut remove)?;
        }
        // A table whose every input is blank is taken out of the file as a whole.
        for (table, keys) in TABLES {
            let fields: Vec<&Field> = self
                .settings
                .iter()
                .flat_map(|(_, form)| &form.fields)
                .filter(|field| keys.contains(&field.key))
                .collect();
            if fields.iter().all(|field| field.text.trim().is_empty())
                && fields.iter().any(|field| field.was_set())
            {
                remove.retain(|key| !keys.contains(&key.as_str()));
                remove.push((*table).to_string());
            }
        }
        Ok((set, remove))
    }

    /// What the Models page changed: the keys to store, to set and to remove.
    pub(super) fn models_changes(&self, secrets: &[String]) -> Result<Plan, String> {
        let mut plan = Plan::default();
        let mut taken: BTreeSet<String> = secrets.iter().cloned().collect();
        taken.extend(self.upstreams.iter().filter_map(|u| u.key.stored.clone()));
        let mut fresh = |base: &str| {
            let mut n = 1;
            loop {
                let name = if base == "decision-model" && n == 1 {
                    base.to_string()
                } else {
                    format!("{base}-{n}")
                };
                if taken.insert(name.clone()) {
                    return name;
                }
                n += 1;
            }
        };

        // Rows that are gone are removed from the last to the first, so that the places of the others hold; the rows
        // that changed, and the ones added, are set at the place they have after that.
        let kept: BTreeSet<usize> = self.upstreams.iter().filter_map(|u| u.origin).collect();
        for gone in (0..self.original_upstreams.len())
            .rev()
            .filter(|n| !kept.contains(n))
        {
            plan.remove.push(format!("router.upstreams.{gone}"));
        }
        for (place, upstream) in self.upstreams.iter().enumerate() {
            let changed = upstream
                .origin
                .and_then(|origin| self.original_upstreams.get(origin))
                .is_none_or(|original| original != upstream);
            if !changed {
                continue;
            }
            if upstream.base_url.trim().is_empty() {
                return Err("Enter the base URL of each upstream.".into());
            }
            let (keys, store) = upstream
                .key
                .plan(&format!("the upstream {}", place + 1), || fresh("upstream"))?;
            plan.store.extend(store);
            let mut table = Map::new();
            table.insert("protocol".into(), json!(upstream.protocol));
            table.insert("base_url".into(), json!(upstream.base_url.trim()));
            table.extend(keys);
            plan.set
                .insert(format!("router.upstreams.{place}"), Value::Object(table));
        }

        if let Some(teacher) = &self.teacher {
            teacher.changes("teacher", &mut plan, String::new)?;
        }
        if let Some(model) = &self.decision_model {
            model.changes("decision_model", &mut plan, || fresh("decision-model"))?;
        }
        Ok(plan)
    }

    pub(super) fn models_dirty(&self) -> bool {
        self.upstreams != self.original_upstreams
            || [&self.teacher, &self.decision_model]
                .into_iter()
                .flatten()
                .any(|optional| {
                    optional.was_on != optional.on
                        || optional.form.dirty()
                        || optional
                            .key
                            .as_ref()
                            .is_some_and(|(now, before)| now != before)
                })
    }

    pub(super) fn key_mut(&mut self, owner: Owner) -> Option<&mut KeyChoice> {
        match owner {
            Owner::Upstream(place) => self.upstreams.get_mut(place).map(|u| &mut u.key),
            Owner::DecisionModel => self
                .decision_model
                .as_mut()
                .and_then(|model| model.key.as_mut())
                .map(|(now, _)| now),
        }
    }
}
