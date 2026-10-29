//! The Improve pages against a real server: the usage stored, the Retro and its proposals, research, the judgements
//! exported, and the decision model's shadowing and takeovers.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use moebius_core::repos::git;
use moebius_core::store::Usage;
use moebius_desktop::client::{Client, ClientError, StreamEvent, events};
use serde_json::json;

use crate::fixtures::{add_demo, harness_origin, next, start, start_with};

#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_usage_the_server_stored() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with(dir.path(), "[budgets]\nday_tokens = 10000\n").await;
    let store = &server.app.store;
    let run = store.add_run(None, "Coder", "test", "code").unwrap();
    for (model, input, output, cached, duration_ms, tools) in [
        ("big", 1000, 100, 600, 4000, vec!["bash", "read"]),
        ("big", 500, 50, 0, 2000, vec!["bash"]),
        ("small", 200, 20, 100, 1000, vec![]),
    ] {
        store
            .add_usage(&Usage {
                run_id: Some(run.id),
                model: model.into(),
                input_tokens: input,
                output_tokens: output,
                cached_tokens: cached,
                duration_ms,
                tools: tools.into_iter().map(String::from).collect(),
            })
            .unwrap();
    }
    let report = Client::new(settings).usage(1).await.unwrap();
    assert_eq!(report.days_in_window, 1);
    assert_eq!(report.totals.tokens, 1870);
    assert_eq!(report.totals.calls, 3);
    assert_eq!(report.totals.model_ms, 7000);
    assert_eq!(report.totals.day_budget, Some(10_000));
    assert!((report.totals.cached_share - 700.0 / 1700.0).abs() < 1e-9);
    assert_eq!(report.days.len(), 1);
    assert_eq!(
        (report.days[0].tokens, report.days[0].model.as_str()),
        (1870, "big")
    );
    assert_eq!(report.models[0].model, "big");
    assert_eq!((report.models[0].tokens, report.models[0].calls), (1650, 2));
    assert_eq!(report.agents[0].agent, "Coder");
    assert_eq!(report.agents[0].model_ms, 7000);
    let activities: Vec<_> = report
        .activities
        .iter()
        .map(|a| (a.activity.as_str(), a.model_ms, a.calls, a.output_tokens))
        .collect();
    assert_eq!(
        activities,
        [
            ("bash", 4000, 2, 100),
            ("read", 2000, 1, 50),
            ("answering", 1000, 1, 20)
        ]
    );
}

/// A day with a failed run becomes a Retro through the client, the fake Pi analyst proposes a branch for the Coder, and
/// the app's screens read the report and the proposal.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_runs_the_retro_and_reads_its_report_and_proposals() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    // The nightly Retro is set to the day's last minute, so that the server does not run one of its own while the
    // test runs (the test's day is read once, so it may not cross midnight anyway).
    let more = format!(
        "retro_at = \"23:59\"\n[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        origin.display().to_string()
    );
    let (settings, server) = start_with(dir.path(), &more).await;
    let day = server.app.store.today().unwrap();
    let branch = format!("proposal/{day}-careful");
    let store = &server.app.store;
    let failed = store.add_run(None, "Coder", "test", "code").unwrap();
    let why = format!("stalled [propose {branch} agents/Coder.md]");
    store
        .append_event(failed.id, "moebius_error", &json!({ "message": why }))
        .unwrap();
    store.set_run_status(failed.id, "failed").unwrap();

    let client = Client::new(settings.clone());
    assert!(client.retros().await.unwrap().is_empty());
    let mut stream = Box::pin(events(settings.clone()));
    assert_eq!(next(&mut stream).await, StreamEvent::Connected);
    let started = client.start_retro().await.unwrap();
    assert_eq!(started.date, day);
    assert_eq!(started.report.errors[0].kind, "run_failed");
    assert_eq!(started.report.errors[0].agent.as_deref(), Some("Coder"));
    loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "retro_done"
        {
            assert_eq!(event.data["status"], "proposed");
            break;
        }
    }
    let retro = client.retro(&day).await.unwrap();
    assert_eq!(retro.status, "proposed");
    assert_eq!(retro.proposals.len(), 1);
    assert_eq!(retro.proposals[0].branch, branch);
    assert_eq!(retro.proposals[0].agents, ["Coder"]);
    let list = client.retros().await.unwrap();
    assert_eq!((list[0].errors, list[0].proposals), (1, 1));
    assert_eq!(client.proposals().await.unwrap(), retro.proposals);
    // A day has one Retro: the server says so, and the client passes the message on.
    let again = client.start_retro().await.unwrap_err();
    assert!(
        matches!(again, ClientError::Server { status: 409, .. }),
        "{again:?}"
    );
}

/// A proposal without cards is tried (and so offered as it is) by the server; the client lists it with its trial and
/// merges it, and the inbox item goes.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_lists_proposals_with_their_trials_and_merges_one() {
    let dir = tempfile::tempdir().unwrap();
    let origin = harness_origin(dir.path());
    let fake_pi = Path::new(env!("CARGO_MANIFEST_DIR")).join("../moebius-core/testdata/fake-pi");
    let more = format!(
        "[pi_bolt]\nprogram = {:?}\n[harness]\nurl = {:?}\n",
        fake_pi.display().to_string(),
        origin.display().to_string()
    );
    let (settings, server) = start_with(dir.path(), &more).await;
    let store = &server.app.store;
    let day = "2026-10-01";
    store
        .add_retro(day, &json!({ "date": day, "errors": [] }), "proposed")
        .unwrap();
    let clone = dir.path().join(".moebius/harness");
    let branch = format!("proposal/{day}-plain");
    git(&clone, &["checkout", "--quiet", "-b", &branch]).unwrap();
    std::fs::write(
        clone.join("agents/Coder.md"),
        "Keep changes small.\nWrite plainly.\n",
    )
    .unwrap();
    git(&clone, &["commit", "--quiet", "-am", "Write plainly"]).unwrap();
    git(&clone, &["push", "--quiet", "origin", &branch]).unwrap();
    git(&clone, &["checkout", "--quiet", "main"]).unwrap();
    store
        .add_proposal(
            day,
            &branch,
            "Write plainly",
            "",
            &["Coder".to_string()],
            &[],
        )
        .unwrap();

    let runs: Arc<dyn moebius_core::board::Runs> = server.harness.clone();
    let repo = server.harness_repo.clone().unwrap();
    let app = server.app.clone();
    tokio::task::spawn_blocking(move || moebius_core::improve::trial::start(&app, &runs, &repo, 1))
        .await
        .unwrap()
        .unwrap();
    let client = Client::new(settings.clone());
    let proposal = loop {
        let proposals = client.proposals().await.unwrap();
        if proposals[0].status != "trying" {
            break proposals.into_iter().next().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(proposal.status, "untried");
    assert_eq!(proposal.trials.len(), 1);
    assert_eq!(proposal.trials[0].outcome, "skipped");
    let snapshot = client.snapshot().await.unwrap();
    assert_eq!(snapshot.inbox[0].proposal(), Some(1));

    client.merge_proposal(1).await.unwrap();
    assert_eq!(client.proposals().await.unwrap()[0].status, "merged");
    assert!(client.snapshot().await.unwrap().inbox.is_empty());
    let text = git(&origin, &["show", "main:agents/Coder.md"]).unwrap();
    assert!(text.contains("Write plainly."), "{text}");
    // Merged is final.
    let again = client.merge_proposal(1).await.unwrap_err();
    assert!(
        matches!(again, ClientError::Server { status: 409, .. }),
        "{again:?}"
    );
}

/// A brief becomes a research through the client: stored as a draft, confirmed, started and, as its measure fails,
/// ended as failed, which the stream reports; the list shows it.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_stores_confirms_and_starts_a_research() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _) = start_with(dir.path(), "[agents.Coder]\nmodel = \"m\"\n").await;
    let repository = add_demo(&settings, dir.path()).await;
    let repository = repository["id"].as_i64().unwrap();
    let client = Client::new(settings.clone());
    assert!(client.researches().await.unwrap().is_empty());

    let draft = client
        .create_research(repository, "Be faster", "exit 1", "lower", 1000)
        .await
        .unwrap();
    assert_eq!((draft.status.as_str(), draft.baseline), ("draft", None));
    let refused = client
        .create_research(repository, "Be faster", "exit 1", "smaller", 1000)
        .await
        .unwrap_err();
    assert!(matches!(refused, ClientError::Server { status: 422, .. }));

    let mut stream = Box::pin(events(settings.clone()));
    assert_eq!(next(&mut stream).await, StreamEvent::Connected);
    let confirmed = client.act_on_research(draft.id, "confirm").await.unwrap();
    assert_eq!(confirmed.status, "confirmed");
    client.act_on_research(draft.id, "start").await.unwrap();
    loop {
        if let StreamEvent::Event(event) = next(&mut stream).await
            && event.kind == "research_status"
            && event.data["status"] == "failed"
        {
            break;
        }
    }
    let list = client.researches().await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].status, "failed");
    assert!(
        list[0].reason.contains("the baseline failed"),
        "{}",
        list[0].reason
    );
    assert_eq!(list[0].token_budget, 1000);
}

/// The client counts the judgements the server stored, makes an export of them and lists it.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_counts_judgements_and_starts_an_export() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start(dir.path()).await;
    let client = Client::new(settings);
    assert!(client.judgement_kinds().await.unwrap().is_empty());
    assert!(client.exports().await.unwrap().is_empty());

    let question = moebius_core::decide::Question {
        kind: "merge".into(),
        context: std::collections::BTreeMap::from([("change".to_string(), "1".to_string())]),
        options: vec!["merge".into()],
        ..Default::default()
    };
    let id = server
        .app
        .decisions
        .record_by(
            &moebius_core::decide::Actor::Person,
            &question,
            "merge",
            "fine",
        )
        .unwrap()
        .judgement_id;
    server.app.store.set_outcome(id, "merged").unwrap();

    let kinds = client.judgement_kinds().await.unwrap();
    assert_eq!(kinds.len(), 1);
    assert_eq!(
        (
            kinds[0].kind.as_str(),
            kinds[0].count,
            kinds[0].with_outcome,
            kinds[0].by_person
        ),
        ("merge", 1, 1, 1)
    );
    let made = client.start_export().await.unwrap();
    assert_eq!((made.id, made.kinds.clone()), (1, kinds));
    assert_eq!(client.exports().await.unwrap(), vec![made]);
    // The person's decision to export is now a kind of its own.
    let kinds = client.judgement_kinds().await.unwrap();
    assert_eq!(
        kinds.iter().map(|k| k.kind.as_str()).collect::<Vec<_>>(),
        ["export", "merge"]
    );
}

/// Without a `[decision_model]` the client reads that none is configured; with one it reads an empty shadowing, and a
/// takeover is merged and released through the client.
#[tokio::test(flavor = "multi_thread")]
async fn the_client_reads_the_shadowing_and_merges_and_releases_a_takeover() {
    let dir = tempfile::tempdir().unwrap();
    let (settings, _server) = start(dir.path()).await;
    let shadowing = Client::new(settings).shadowing().await.unwrap();
    assert!(!shadowing.configured && shadowing.kinds.is_empty() && shadowing.model.is_none());

    let dir = tempfile::tempdir().unwrap();
    let (settings, server) = start_with(
        dir.path(),
        "[decision_model]\nurl = \"http://127.0.0.1:1/v1\"\nmodel = \"decider\"\nmin_judgements = 7\n",
    )
    .await;
    let client = Client::new(settings);
    let shadowing = client.shadowing().await.unwrap();
    assert!(shadowing.configured && shadowing.kinds.is_empty());
    assert_eq!(
        (
            shadowing.model.as_deref(),
            shadowing.agreement,
            shadowing.min_judgements
        ),
        (Some("decider"), Some(0.95), Some(7))
    );

    let store = &server.app.store;
    let proposed = store.add_takeover("tool_call", 0.97, 120).unwrap();
    store
        .add_inbox_item(
            "takeover",
            None,
            "The decision model can take over tool_call",
            &serde_json::json!({"takeover": proposed.id, "kind": "tool_call", "agreement": 0.97, "count": 120}),
        )
        .unwrap();
    let snapshot = client.snapshot().await.unwrap();
    assert_eq!(snapshot.inbox[0].takeover(), Some(proposed.id));

    client.merge_takeover(proposed.id).await.unwrap();
    assert!(server.app.decisions.routed().contains("tool_call"));
    assert!(client.snapshot().await.unwrap().inbox.is_empty());
    let shadowing = client.shadowing().await.unwrap();
    assert_eq!(shadowing.takeovers[0].status, "merged");
    assert_eq!(shadowing.kinds[0].kind, "tool_call");
    assert!(shadowing.kinds[0].routed);

    client.release_takeover(proposed.id).await.unwrap();
    assert!(!server.app.decisions.routed().contains("tool_call"));
    assert_eq!(
        client.shadowing().await.unwrap().takeovers[0].status,
        "released"
    );
    // A second release is refused with the server's message.
    assert!(matches!(
        client.release_takeover(proposed.id).await.unwrap_err(),
        ClientError::Server { status: 409, .. }
    ));

    let second = store.add_takeover("merge", 0.99, 90).unwrap();
    client.dismiss_takeover(second.id).await.unwrap();
    assert_eq!(
        client.shadowing().await.unwrap().takeovers[1].status,
        "dismissed"
    );
}
