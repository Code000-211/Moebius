//! `plugin enable` and `plugin disable`: turn a plugin off or on at once, over `PUT /v1/plugins/{plugin}/enabled`
//! (see `docs/reference/desktop-api.md`).

use clap::Subcommand;
use serde_json::{Value, json};

use super::{Plan, Request, Show};

#[derive(Debug, Subcommand)]
pub enum PluginCommand {
    /// Turn a plugin on: it is started and asked again.
    Enable { name: String },
    /// Turn a plugin off: its entry and settings stay, but it is not started, asked or called.
    Disable {
        name: String,
        /// Confirm turning off the last launcher, after which runs start unwrapped.
        #[arg(long)]
        unwrap_runs: bool,
        /// Confirm turning off the guard, after which tool calls are allowed unchecked.
        #[arg(long)]
        allow_unchecked: bool,
    },
}

/// `plugin ...`.
pub fn plugin(command: &PluginCommand) -> Plan {
    let (name, enabled, unwrap_runs, allow_unchecked) = match command {
        PluginCommand::Enable { name } => (name, true, false, false),
        PluginCommand::Disable {
            name,
            unwrap_runs,
            allow_unchecked,
        } => (name, false, *unwrap_runs, *allow_unchecked),
    };
    let mut body = json!({ "enabled": enabled });
    if unwrap_runs {
        body["unwrap_runs"] = true.into();
    }
    if allow_unchecked {
        body["allow_unchecked"] = true.into();
    }
    Ok((
        Request::put(format!("/v1/plugins/{name}/enabled"), body),
        Show::Plugin,
    ))
}

/// The answer of the route as a sentence, with the note when runs now start unwrapped or tool calls are no longer checked.
pub fn show_plugin(value: &Value) -> String {
    let name = value["name"].as_str().unwrap_or("the plugin");
    let state = if value["enabled"] == true {
        "on"
    } else {
        "off"
    };
    let mut out = if value["changed"] == true {
        format!("{name} is {state} now")
    } else {
        format!("{name} was {state} already")
    };
    if let Some(note) = value["note"].as_str() {
        out.push_str(&format!("\n{note}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::tests::plan;
    use super::*;

    #[test]
    fn enable_and_disable_put_the_state_and_disable_may_confirm_unwrapping_runs() {
        let route = |args: &[&str]| {
            let (request, show) = plan(args).unwrap();
            assert_eq!(show, Show::Plugin);
            (request.method.to_string(), request.path, request.body)
        };
        assert_eq!(
            route(&["plugin", "enable", "box"]),
            (
                "PUT".into(),
                "/v1/plugins/box/enabled".into(),
                Some(json!({"enabled": true}))
            )
        );
        assert_eq!(
            route(&["plugin", "disable", "box"]).2,
            Some(json!({"enabled": false}))
        );
        assert_eq!(
            route(&["plugin", "disable", "box", "--unwrap-runs"]).2,
            Some(json!({"enabled": false, "unwrap_runs": true}))
        );
        assert_eq!(
            route(&["plugin", "disable", "guard", "--allow-unchecked"]).2,
            Some(json!({"enabled": false, "allow_unchecked": true}))
        );
        // Enabling confirms nothing: it takes no such option.
        assert!(super::super::tests::plan(&["plugin", "enable", "box", "--unwrap-runs"]).is_err());
        assert!(
            super::super::tests::plan(&["plugin", "enable", "guard", "--allow-unchecked"]).is_err()
        );
    }

    #[test]
    fn the_answer_says_the_state_and_that_runs_start_unwrapped() {
        let on = json!({"name": "box", "enabled": true, "changed": true, "unwrapped_runs": false, "note": null});
        assert_eq!(show_plugin(&on), "box is on now");
        let off = json!({"name": "box", "enabled": false, "changed": false, "unwrapped_runs": false, "note": null});
        assert_eq!(show_plugin(&off), "box was off already");
        let bare = json!({"name": "box", "enabled": false, "changed": true, "unwrapped_runs": true,
            "note": "runs now start unwrapped"});
        assert_eq!(
            show_plugin(&bare),
            "box is off now\nruns now start unwrapped"
        );
    }
}
