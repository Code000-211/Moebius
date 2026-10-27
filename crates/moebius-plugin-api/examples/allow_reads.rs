//! A decider plugin that allows tool calls which only read and refuses every other tool. `main` registers it with a
//! [`Host`], asks it three questions the way the core does, and asserts the answers.
//!
//! Run it with `cargo run -p moebius-plugin-api --example allow_reads`.

use std::collections::BTreeMap;
use std::time::Duration;

use moebius_plugin_api::{
    Call, DecideRequest, Decision, Host, Kind, Manifest, Plugin, PluginError, Refusal,
};
use serde_json::Value;

/// The tools of Pi-Bolt that do not change anything.
const READ_ONLY: [&str; 4] = ["read", "grep", "find", "ls"];

struct AllowReads;

impl Plugin for AllowReads {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "allow-reads".into(),
            version: "0.1.0".into(),
            kinds: vec![Kind::Decider],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        if call.method != "decide" {
            return Err(PluginError::Failed(format!("no method {:?}", call.method)));
        }
        let request: DecideRequest = serde_json::from_value(call.params)
            .map_err(|e| PluginError::BadParams(e.to_string()))?;
        if request.kind != "tool_call" {
            return Err(PluginError::Failed(format!(
                "this plugin decides tool calls, not {:?}",
                request.kind
            )));
        }
        let tool = request.context.get("tool").map_or("", String::as_str);
        let (choice, reason) = if READ_ONLY.contains(&tool) {
            ("allow", format!("{tool} only reads"))
        } else {
            ("refuse", format!("only reads are allowed, not {tool}"))
        };
        let decision = Decision {
            choice: choice.into(),
            reason,
            by: None,
        };
        serde_json::to_value(decision).map_err(|e| PluginError::Failed(e.to_string()))
    }
}

fn request(kind: &str, tool: &str) -> DecideRequest {
    DecideRequest {
        kind: kind.into(),
        context: BTreeMap::from([("tool".to_string(), tool.to_string())]),
        options: vec!["allow".into(), "refuse".into()],
    }
}

fn main() {
    let mut host = Host::new(Duration::from_secs(5));
    host.add(AllowReads);

    let read = host
        .decide("allow-reads", &request("tool_call", "read"))
        .unwrap();
    assert_eq!(read.choice, "allow");

    let bash = host
        .decide("allow-reads", &request("tool_call", "bash"))
        .unwrap();
    assert_eq!(bash.choice, "refuse");
    assert_eq!(bash.reason, "only reads are allowed, not bash");

    // A question the plugin does not decide fails, and the host reports that as a refusal.
    let other = host.decide("allow-reads", &request("merge", "read"));
    assert!(matches!(other, Err(Refusal::Failed { .. })));

    println!("allow-reads: read allowed, bash refused, merge refused as a failure");
}
