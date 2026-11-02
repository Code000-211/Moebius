//! The package manager: `pkg search`, `info`, `install`, `update`, `remove` and `list` over the packages routes
//! (see `docs/reference/packages.md`).

use clap::Subcommand;
use serde_json::{Value, json};

use super::{Plan, Request, Show, cell, table};

#[derive(Debug, Subcommand)]
pub enum PkgCommand {
    /// Packages of the registries whose name, description or kind contains the text; all without one.
    Search { text: Option<String> },
    /// Every version of a package, and what is installed of it.
    Info { name: String },
    /// Install a package: a plugin at once, the files of a workflow, rules or skills package as a change to merge.
    Install {
        /// `<name>` for the newest version that works with this Moebius, or `<name>@<version>`.
        name: String,
        /// The repository a workflow or rules package is proposed to, by name; needed with several repositories.
        #[arg(long)]
        repository: Option<String>,
        /// The registry to install from; the first that has the package without it.
        #[arg(long)]
        registry: Option<String>,
    },
    /// Update a package to the newest version that works with this Moebius. Without a name, the one package that
    /// has an update waiting.
    Update { name: Option<String> },
    /// Remove a package: a plugin's entry and files go at once, the files of the others as a change to merge.
    Remove { name: String },
    /// What is installed.
    List,
}

/// `pkg ...`; `get` reads what `update` without a name needs.
pub fn pkg(command: &PkgCommand, get: &mut dyn FnMut(&str) -> Result<Value, String>) -> Plan {
    match command {
        PkgCommand::Search { text } => Ok((
            Request::get(format!(
                "/v1/packages/search?q={}",
                encode(text.as_deref().unwrap_or(""))
            )),
            Show::Packages,
        )),
        PkgCommand::Info { name } => {
            Ok((Request::get(format!("/v1/packages/{name}")), Show::Packages))
        }
        PkgCommand::List => Ok((Request::get("/v1/packages"), Show::Packages)),
        PkgCommand::Remove { name } => Ok((
            Request::delete(format!("/v1/packages/{name}")),
            Show::Packages,
        )),
        PkgCommand::Install {
            name,
            repository,
            registry,
        } => {
            let (name, version) = match name.rsplit_once('@') {
                Some((name, version)) => (name, Some(version)),
                None => (name.as_str(), None),
            };
            let mut body = json!({ "name": name });
            for (key, value) in [
                ("version", version),
                ("repository", repository.as_deref()),
                ("registry", registry.as_deref()),
            ] {
                if let Some(value) = value {
                    body[key] = value.into();
                }
            }
            Ok((Request::post("/v1/packages", body), Show::Packages))
        }
        PkgCommand::Update { name } => {
            let name = match name {
                Some(name) => name.clone(),
                None => {
                    let installed = get("/v1/packages")?;
                    let waiting: Vec<&str> = installed
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|p| p["update_available"].is_string())
                        .filter_map(|p| p["name"].as_str())
                        .collect();
                    match waiting.as_slice() {
                        [] => {
                            return Ok((
                                Request::get("/v1/packages"),
                                Show::Done("no package has an update waiting".into()),
                            ));
                        }
                        [one] => one.to_string(),
                        several => {
                            return Err(format!(
                                "several packages have an update waiting ({}); name one",
                                several.join(", ")
                            ));
                        }
                    }
                }
            };
            Ok((
                Request::post(format!("/v1/packages/{name}/update"), json!({})),
                Show::Packages,
            ))
        }
    }
}

/// `text` as a query value.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// An answer of the package routes as plain text.
pub fn show_packages(value: &Value) -> String {
    if let Some(found) = value["packages"].as_array() {
        let mut out = vec![table(
            &Value::Array(found.clone()),
            &[
                "name",
                "kind",
                "version",
                "registry",
                "installed",
                "description",
            ],
        )];
        for problem in value["problems"].as_array().into_iter().flatten() {
            out.push(format!(
                "problem  {}: {}",
                cell(&problem["registry"]),
                cell(&problem["message"])
            ));
        }
        return out.join("\n");
    }
    if value.is_array() {
        return table(
            value,
            &["name", "version", "kind", "registry", "update_available"],
        );
    }
    if let Some(versions) = value["versions"].as_array() {
        let mut out = vec![format!(
            "{} (registry {}, installed {})",
            cell(&value["name"]),
            cell(&value["registry"]),
            value["installed"]["version"].as_str().unwrap_or("-")
        )];
        if let Some(description) = versions.first().and_then(|v| v["description"].as_str()) {
            out.push(description.to_string());
        }
        out.push(table(
            &Value::Array(versions.clone()),
            &["version", "compatible", "moebius", "kind"],
        ));
        return out.join("\n");
    }
    let mut out = format!(
        "{} {} {}",
        cell(&value["action"]),
        cell(&value["name"]),
        cell(&value["version"])
    );
    if let Some(change) = value["change"].as_i64() {
        out.push_str(&format!(
            "\nchange {change} waits in the inbox to be merged"
        ));
    }
    if let Some(error) = value["plugin"]["error"].as_str() {
        out.push_str(&format!("\nthe plugin does not run: {error}"));
    }
    if value["restart_needed"] == true {
        out.push_str("\nrestart the server for it to take effect");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::plan;
    use super::*;

    #[test]
    fn the_package_commands_map_to_their_routes() {
        let route = |args: &[&str]| {
            let (request, _) = plan(args).unwrap();
            (request.method.to_string(), request.path, request.body)
        };
        assert_eq!(
            route(&["pkg", "search", "docker box"]).1,
            "/v1/packages/search?q=docker%20box"
        );
        assert_eq!(route(&["pkg", "search"]).1, "/v1/packages/search?q=");
        assert_eq!(route(&["pkg", "info", "gate"]).1, "/v1/packages/gate");
        assert_eq!(route(&["pkg", "list"]).1, "/v1/packages");
        assert_eq!(route(&["pkg", "remove", "gate"]).0, "DELETE");
        assert_eq!(
            route(&[
                "pkg",
                "install",
                "gate@1.2.0",
                "--repository",
                "demo",
                "--registry",
                "acme"
            ]),
            (
                "POST".into(),
                "/v1/packages".into(),
                Some(
                    json!({"name": "gate", "version": "1.2.0", "repository": "demo", "registry": "acme"})
                )
            )
        );
        assert_eq!(
            route(&["pkg", "install", "gate"]).2,
            Some(json!({"name": "gate"}))
        );
        assert_eq!(
            route(&["pkg", "update", "gate"]).1,
            "/v1/packages/gate/update"
        );
    }

    #[test]
    fn update_without_a_name_takes_the_one_package_with_an_update_waiting() {
        let with = |installed: Value, args: &[&str]| {
            use clap::Parser;
            let probe = super::super::tests::Probe::try_parse_from(
                std::iter::once("moebius").chain(args.iter().copied()),
            )
            .unwrap();
            probe.command.plan(&mut |_| Ok(installed.clone()))
        };
        let one = with(
            json!([{"name": "a", "update_available": null}, {"name": "b", "update_available": "1.1.0"}]),
            &["pkg", "update"],
        )
        .unwrap();
        assert_eq!(one.0.path, "/v1/packages/b/update");
        let none = with(
            json!([{"name": "a", "update_available": null}]),
            &["pkg", "update"],
        )
        .unwrap();
        assert!(matches!(none.1, Show::Done(_)));
        let several = with(
            json!([{"name": "a", "update_available": "2"}, {"name": "b", "update_available": "1.1.0"}]),
            &["pkg", "update"],
        )
        .unwrap_err();
        assert!(several.contains("a, b"), "{several}");
    }

    #[test]
    fn answers_are_written_as_plain_text() {
        let found = json!({"packages": [{"name": "gate", "kind": "plugin", "version": "1.0.0",
            "registry": "r", "installed": null, "description": "Holds cards."}],
            "problems": [{"registry": "broken", "message": "no index"}]});
        let text = show_packages(&found);
        assert!(
            text.contains("gate") && text.contains("problem  broken: no index"),
            "{text}"
        );
        let done = json!({"action": "installed", "name": "box", "version": "0.1.0", "restart_needed": true,
            "change": 4, "plugin": {"state": "restarting", "error": "boom"}});
        let text = show_packages(&done);
        assert!(text.starts_with("installed box 0.1.0"), "{text}");
        assert!(
            text.contains("change 4")
                && text.contains("boom")
                && text.contains("restart the server")
        );
    }
}
