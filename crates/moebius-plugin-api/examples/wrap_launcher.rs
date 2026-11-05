//! A launcher plugin that wraps every launch of a harness in a made-up sandbox program, `box-run`. `main` registers it
//! with a [`Host`], asks it the way the core does before a harness process starts, and asserts the answers: a
//! wrapped launch with only the environment the run needs, the addresses as seen from inside, a stop and a cleanup
//! command and the file to remove; "unchanged" for a run on a runner; leftovers; and, for a launcher that fails, the
//! refusal that makes the core fail the run.
//!
//! Run it with `cargo run -p moebius-plugin-api --example wrap_launcher`.

use std::collections::BTreeMap;
use std::time::Duration;

use moebius_plugin_api::{
    Call, CommandLine, Host, Kind, LaunchAnswer, LaunchRequest, LaunchRun, Leftovers,
    LeftoversRequest, Manifest, Plugin, PluginError, Refusal, Urls, UrlsInside, Wrapped,
};
use serde_json::Value;

/// The variables a run needs; the rest of its environment is not passed on.
const KEPT: [&str; 3] = ["PATH", "HOME", "LANG"];

/// Where the profile of a run is written.
const PROFILES: &str = "/tmp/box-profiles";

struct BoxRun;

impl BoxRun {
    fn profile(run: i64) -> String {
        format!("{PROFILES}/run-{run}.profile")
    }

    fn launch(request: LaunchRequest) -> LaunchAnswer {
        // A run on another machine is out of reach of a sandbox on this one.
        if request.runner.is_some() {
            return LaunchAnswer::Unchanged {
                note: Some("the run is on a runner, out of reach of a sandbox here".into()),
            };
        }
        let profile = Self::profile(request.run.id);
        // The profile (written by a real launcher to `profile`) allows the paths the run needs and the router.
        let mut args = vec!["--profile".to_string(), profile.clone(), "--".to_string()];
        args.push(request.program);
        args.extend(request.args);
        let mut env: BTreeMap<String, String> = request
            .env
            .into_iter()
            .filter(|(name, _)| KEPT.contains(&name.as_str()) || name.starts_with("MOEBIUS_"))
            .collect();
        // Inside, the router is at the sandbox's own address.
        env.insert("MOEBIUS_CHECK_URL".into(), "http://10.0.0.1:7172".into());
        let command = |what: &str| CommandLine {
            program: "box-run".into(),
            args: vec![what.into(), "--run".into(), request.run.id.to_string()],
            env: BTreeMap::new(),
        };
        LaunchAnswer::Wrapped(Box::new(Wrapped {
            program: "box-run".into(),
            args,
            env,
            urls: UrlsInside {
                router: Some("http://10.0.0.1:7172".into()),
                gateway: Some("http://10.0.0.1:7172/mcp".into()),
                check: Some("http://10.0.0.1:7172".into()),
            },
            stop: Some(command("stop")),
            cleanup: Some(command("cleanup")),
            remove: vec![profile],
            note: None,
        }))
    }
}

impl Plugin for BoxRun {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "box-run".into(),
            version: "0.1.0".into(),
            kinds: vec![Kind::Launcher],
        }
    }

    fn call(&self, call: Call) -> Result<Value, PluginError> {
        match call.method.as_str() {
            "launch" => {
                let request: LaunchRequest = serde_json::from_value(call.params)
                    .map_err(|e| PluginError::BadParams(e.to_string()))?;
                serde_json::to_value(Self::launch(request))
            }
            "leftovers" => {
                let request: LeftoversRequest = serde_json::from_value(call.params)
                    .map_err(|e| PluginError::BadParams(e.to_string()))?;
                // The profiles of runs that no longer exist; a real launcher lists its directory and keeps those of
                // `request.running`.
                serde_json::to_value(Leftovers {
                    cleanup: vec![],
                    remove: vec![Self::profile(0)]
                        .into_iter()
                        .filter(|_| !request.running.contains(&0))
                        .collect(),
                })
            }
            other => return Err(PluginError::Failed(format!("no method {other:?}"))),
        }
        .map_err(|e| PluginError::Failed(e.to_string()))
    }
}

/// A launcher that cannot sandbox anything: the core fails the run it was asked about.
struct Broken;

impl Plugin for Broken {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "broken".into(),
            version: "0.1.0".into(),
            kinds: vec![Kind::Launcher],
        }
    }

    fn call(&self, _: Call) -> Result<Value, PluginError> {
        Err(PluginError::Failed("the sandbox is not available".into()))
    }
}

fn request(runner: Option<&str>) -> LaunchRequest {
    LaunchRequest {
        run: LaunchRun {
            id: 7,
            purpose: "code".into(),
            agent: "Coder".into(),
            chat: false,
            card: Some(3),
        },
        program: "pi-bolt".into(),
        args: vec!["--mode".into(), "rpc".into()],
        env: BTreeMap::from([
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("MOEBIUS_RUN_TOKEN".to_string(), "moebius-run-x".to_string()),
            ("PROVIDER_KEY".to_string(), "secret".to_string()),
        ]),
        dir: "/data/worktrees/run-7".into(),
        read_only: vec!["/data/runs/7/moebius-bridge.ts".into()],
        read_write: vec!["/data/worktrees/run-7".into(), "/data/runs/7/agent".into()],
        urls: Urls {
            router: "http://127.0.0.1:7172".into(),
            gateway: "http://127.0.0.1:7172/mcp".into(),
            check: "http://127.0.0.1:7172".into(),
        },
        clone: Some("/data/repos/demo".into()),
        runner: runner.map(String::from),
    }
}

fn main() {
    let mut host = Host::new(Duration::from_secs(5));
    host.add(BoxRun).add(Broken);

    let LaunchAnswer::Wrapped(wrapped) = host.launch("box-run", &request(None)).unwrap() else {
        panic!("the launch was not wrapped")
    };
    assert_eq!(wrapped.program, "box-run");
    assert_eq!(
        wrapped.args,
        [
            "--profile",
            "/tmp/box-profiles/run-7.profile",
            "--",
            "pi-bolt",
            "--mode",
            "rpc"
        ]
    );
    // The provider key is not passed on, the run's token is.
    assert!(!wrapped.env.contains_key("PROVIDER_KEY"));
    assert_eq!(wrapped.env["MOEBIUS_RUN_TOKEN"], "moebius-run-x");
    assert_eq!(wrapped.urls.router.as_deref(), Some("http://10.0.0.1:7172"));
    assert_eq!(wrapped.stop.unwrap().args, ["stop", "--run", "7"]);
    assert_eq!(wrapped.cleanup.unwrap().args, ["cleanup", "--run", "7"]);
    assert_eq!(wrapped.remove, ["/tmp/box-profiles/run-7.profile"]);

    // A run on a runner is left as it is.
    let unchanged = host.launch("box-run", &request(Some("gpu-box"))).unwrap();
    assert!(matches!(
        unchanged,
        LaunchAnswer::Unchanged { note: Some(_) }
    ));

    let left = host
        .leftovers("box-run", &LeftoversRequest::default())
        .unwrap();
    assert_eq!(left.remove, ["/tmp/box-profiles/run-0.profile"]);

    // A launcher that fails is a refusal; the core fails the run and never starts it unwrapped.
    let broken = host.launch("broken", &request(None));
    assert!(matches!(broken, Err(Refusal::Failed { .. })));

    println!(
        "box-run: launch wrapped, runner left unchanged, leftovers named, broken launcher refused"
    );
}
