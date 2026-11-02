//! The launcher of `wrap_launcher.rs` as a plugin in its own process. Started with `--plugin`, it is the plugin: it speaks
//! JSON Lines on its standard input and output with [`serve`], as Moebius starts it from a `[[plugins]]` entry with
//! `launches = true`. Started without, `main` starts itself as a plugin process the way the server does, asks it for a
//! wrapped launch, an unchanged one and leftovers through a [`Host`] and asserts the answers.
//!
//! Run it with `cargo run -p moebius-plugin-api --example wrap_launcher_process`.

use std::collections::BTreeMap;
use std::time::Duration;

use moebius_plugin_api::{
    Call, CommandLine, Host, Kind, LaunchAnswer, LaunchRequest, LaunchRun, Leftovers,
    LeftoversRequest, Manifest, Plugin, PluginError, Process, ProcessSpec, State, Urls, UrlsInside,
    Wrapped, serve,
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
        read_only: vec!["/data/runs/7/agent".into()],
        read_write: vec!["/data/worktrees/run-7".into()],
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
    if std::env::args().any(|arg| arg == "--plugin") {
        serve(&BoxRun, std::io::stdin().lock(), std::io::stdout().lock())
            .expect("the standard input and output work");
        return;
    }

    let spec = ProcessSpec {
        name: "box-run".into(),
        program: std::env::current_exe().expect("this program has a path"),
        args: vec!["--plugin".into()],
        dir: std::env::current_dir().expect("there is a working directory"),
    };
    let process = Process::start(spec, Duration::from_secs(5), |line| {
        eprintln!("box-run: {line}")
    });
    assert_eq!(process.status().state, State::Running);
    let mut host = Host::new(Duration::from_secs(5));
    host.add_process(process);

    let LaunchAnswer::Wrapped(wrapped) = host.launch("box-run", &request(None)).unwrap() else {
        panic!("the launch was not wrapped")
    };
    assert_eq!(wrapped.program, "box-run");
    assert!(!wrapped.env.contains_key("PROVIDER_KEY"));
    assert_eq!(wrapped.remove, ["/tmp/box-profiles/run-7.profile"]);

    let unchanged = host.launch("box-run", &request(Some("gpu-box"))).unwrap();
    assert!(matches!(
        unchanged,
        LaunchAnswer::Unchanged { note: Some(_) }
    ));

    let left = host
        .leftovers("box-run", &LeftoversRequest::default())
        .unwrap();
    assert_eq!(left.remove, ["/tmp/box-profiles/run-0.profile"]);

    println!("box-run in its own process: launch wrapped, runner left unchanged, leftovers named");
}
