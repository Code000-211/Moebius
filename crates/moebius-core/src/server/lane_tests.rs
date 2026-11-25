//! The test lane (ADR 0035), against the fake GitHub, a fake `ssh` whose "machine" is a directory of this one and a fake
//! container runtime that records the arguments it is given: a job for a change at its pinned head, in a container or
//! directly, what comes of it, and everything that must stop one.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;

use super::fake_github::Fixture;
use super::reviews_tests::{ready, said};
use super::taking_tests::push_to_the_pull_request;
use crate::app::WorkingWithOthers;
use crate::app::factory::FactoryEdit;
use crate::config::{Isolation, LaneConfig, RunnerConfig};
use crate::decide::Actor;
use crate::harness::repo::testing::{put, use_harness};
use crate::repos::git;
use crate::store::{Card, LaneJob};

const KANBAN: &str =
    "[[column]]\nname = \"Ready for testing\"\nafter = \"In review\"\nlane = true\n";

fn owner() -> Actor {
    Actor::first_owner()
}

/// A pull request that was reviewed up to the code, with a lane.
struct Lane {
    f: Fixture,
    card: Card,
    head: String,
    base: String,
    /// The fake container runtime, and its log.
    podman: PathBuf,
    /// The directory of the "machine".
    machine: PathBuf,
}

impl Fixture {
    fn lane_edit_factory(&self, edit: FactoryEdit) {
        self.server
            .app
            .edit_working_with_others(
                &owner(),
                self.repository.id,
                &WorkingWithOthers {
                    factory: edit,
                    ..Default::default()
                },
                "test",
            )
            .unwrap();
    }
}

fn lane_config(isolation: Isolation, runtime: &str) -> LaneConfig {
    LaneConfig {
        name: "gpu1".into(),
        runner: Some("box".into()),
        work_dir: None,
        isolation,
        runtime: runtime.into(),
        image: Some("rocm/dev-ubuntu:6.2".into()),
        devices: vec!["/dev/kfd".into(), "/dev/dri".into()],
        groups: vec!["video".into(), "render".into()],
        user: "65534:65534".into(),
        timeout_seconds: 120,
        memory: "16g".into(),
        cpus: None,
        shared_disk: false,
        disk: "1m".into(),
        gpus: Some("2x MI300X".into()),
        power_cap: Some("750 W".into()),
        rocm: Some("6.2.1".into()),
        driver: None,
    }
}

impl Lane {
    /// A lane with `isolation`, the project's job `definition`, the pull request ready for its review and the lane
    /// assigned to the repository.
    async fn new(isolation: Isolation, definition: &str) -> Lane {
        let (f, card, head) = ready().await;
        let dir = f._dir.path().to_path_buf();
        let base = git(
            std::path::Path::new(&f.repository.path),
            &["merge-base", &f.repository.base_ref(), &head],
        )
        .unwrap();
        use_harness(&f.server.app, &dir);
        put(&f.server.app, "widgets", "kanban.toml", KANBAN);
        let definition = definition
            .replace("@HEAD@", &head)
            .replace("@BASE@", &base)
            .replace("@DIR@", dir.to_str().unwrap());
        put(&f.server.app, "widgets", "lane.toml", &definition);
        let testdata = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let podman = dir.join("podman");
        std::fs::copy(testdata.join("fake-podman"), &podman).unwrap();
        let machine = dir.join("remote");
        std::fs::create_dir_all(&machine).unwrap();
        let key = dir.join("gpu.key");
        std::fs::write(&key, "not a key").unwrap();
        let runner = RunnerConfig {
            name: "box".into(),
            host: "gpu.test".into(),
            user: Some("lane".into()),
            port: Some(2222),
            key_file: Some(key),
            working_dir: machine.to_string_lossy().into_owned(),
            max_runs: 1,
            bin_dir: None,
            ssh: testdata.join("fake-ssh"),
        };
        let lane = Lane {
            f,
            card,
            head,
            base,
            podman,
            machine,
        };
        lane.f.server.app.lanes.configure(
            vec![lane_config(isolation, lane.podman.to_str().unwrap())],
            vec![runner],
        );
        lane.f
            .server
            .app
            .store
            .set_repository_lane(lane.f.repository.id, Some("gpu1"))
            .unwrap();
        lane
    }

    fn app(&self) -> std::sync::Arc<crate::app::App> {
        self.f.server.app.clone()
    }

    /// The Reviewer approves the code and asks for a measurement: the card goes to Ready for testing.
    async fn code_approved(&self) {
        self.f.author_says_done().await;
        let run = self.f.reviews_of(&self.card).remove(0);
        self.f
            .reviewer_ends(
                &self.card,
                &run,
                &said("approve_code", "The code is right.", json!([])),
            )
            .await;
    }

    fn jobs(&self) -> Vec<LaneJob> {
        let change = self
            .f
            .server
            .app
            .followed_change(&self.card)
            .unwrap()
            .unwrap();
        self.f
            .server
            .app
            .store
            .lane_jobs_of_change(change.id)
            .unwrap()
    }

    /// One pass of the lane, and every job it starts run to its end.
    async fn pass(&self) -> Vec<i64> {
        let app = self.app();
        tokio::task::spawn_blocking(move || {
            let started = app.lane_pass();
            for id in &started {
                app.run_lane_job(*id);
            }
            started
        })
        .await
        .unwrap()
    }

    /// The calls the fake container runtime got, one list of arguments each.
    fn container_calls(&self) -> Vec<Vec<String>> {
        let log = format!("{}.log", self.podman.display());
        std::fs::read_to_string(log)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// The containers of the commands of jobs (not the cleanup of a job's directory, which has no name).
    /// The containers of the commands of jobs (not the cleanup of a job's directory, which has no name).
    fn runs_in_containers(&self) -> Vec<Vec<String>> {
        self.container_calls()
            .into_iter()
            .filter(|c| {
                c.first().map(String::as_str) == Some("run")
                    && c.iter().any(|a| a == "--name")
                    // Not the probe of the limits, nor the cleanup.
                    && !c.iter().any(|a| a.ends_with("-probe") || a.contains("-clean-"))
            })
            .collect()
    }

    fn the_job(&self) -> LaneJob {
        self.jobs().pop().expect("a job")
    }

    /// What was written on the pull request about the lane: not the template's notice of what the description lacks.
    fn replies_on_the_pull_request(&self) -> Vec<String> {
        self.f
            .on_the_pull_request()
            .into_iter()
            .filter(|text| !text.starts_with("## Missing"))
            .collect()
    }
}

/// A job that runs `command` as its one step.
fn one_step(command: &str) -> String {
    format!("[[step]]\nname = \"check\"\ncommand = '''{command}'''\n")
}

/// The step that passes only in the pinned checkouts: the head's on the `after` side and the merge base's on `before`.
const PIN: &str = "[[step]]\nname = \"pin\"\non = \"both\"\ncommand = '''\n\
if [ \"$LANE_SIDE\" = before ]; then want=@BASE@; else want=@HEAD@; fi\n\
test \"$(git rev-parse HEAD)\" = \"$want\" && test -z \"$(git status --porcelain)\"\n\
if [ \"$LANE_SIDE\" = after ]; then test -f src/lib.rs; else test ! -f src/lib.rs; fi\n'''\n";

// ---------------------------------------------------------------------------------------------------------------------
// Triggering: nothing before the review, and a column that is marked.

#[tokio::test(flavor = "multi_thread")]
async fn nothing_runs_before_the_reviewer_has_given_a_verdict() {
    let lane = Lane::new(Isolation::Always, PIN).await;
    let asked = lane.app().request_lane_job(lane.card.id).unwrap();
    assert!(asked.job.is_none());
    assert!(
        asked
            .why
            .as_deref()
            .unwrap()
            .contains("Reviewer has not approved"),
        "{asked:?}"
    );
    // Not even a card a person drags into the lane's column gets one.
    lane.app()
        .move_card(lane.card.id, "Ready for testing", Some(&owner()))
        .unwrap();
    assert!(lane.jobs().is_empty());
    assert!(lane.pass().await.is_empty());
    assert!(lane.container_calls().is_empty());
    // A review that asked for changes is no approval either.
    lane.app()
        .move_card(lane.card.id, "In review", Some(&owner()))
        .unwrap();
    lane.f.author_says_done().await;
    let run = lane.f.reviews_of(&lane.card).remove(0);
    lane.f
        .reviewer_ends(&lane.card, &run, &said("changes", "No.", json!([])))
        .await;
    assert!(
        lane.app()
            .request_lane_job(lane.card.id)
            .unwrap()
            .job
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_card_entering_the_column_that_is_marked_queues_a_job_at_the_pinned_head() {
    let lane = Lane::new(Isolation::Always, PIN).await;
    lane.code_approved().await;
    assert_eq!(lane.f.card(1).unwrap().column, "Ready for testing");
    let job = lane.the_job();
    assert_eq!(
        (job.state.as_str(), job.lane.as_str(), job.external),
        ("queued", "gpu1", true)
    );
    assert_eq!(
        (job.head.as_str(), job.base.as_str()),
        (lane.head.as_str(), lane.base.as_str())
    );
    // Asking again does not queue a second.
    let again = lane.app().request_lane_job(lane.card.id).unwrap();
    assert_eq!(again.job.unwrap().id, job.id);
    assert!(again.why.unwrap().contains("there already"));
    assert_eq!(lane.jobs().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_column_that_is_not_marked_queues_nothing() {
    let lane = Lane::new(Isolation::Always, PIN).await;
    put(
        &lane.f.server.app,
        "widgets",
        "kanban.toml",
        "[[column]]\nname = \"Ready for testing\"\nafter = \"In review\"\n",
    );
    lane.code_approved().await;
    assert_eq!(lane.f.card(1).unwrap().column, "Ready for testing");
    assert!(lane.jobs().is_empty());
}

// ---------------------------------------------------------------------------------------------------------------------
// Running: pinned commits, containers, isolation.

#[tokio::test(flavor = "multi_thread")]
async fn a_job_checks_out_exactly_the_pinned_commits_and_runs_each_command_in_a_container() {
    let lane = Lane::new(Isolation::Always, PIN).await;
    lane.code_approved().await;
    // The default branch moves on while the job waits: the job is of the merge base it was queued with.
    let started = lane.pass().await;
    assert_eq!(started.len(), 1);
    let job = lane.the_job();
    assert_eq!(job.state, "passed", "{job:?}");
    assert_eq!(job.report["machine"]["isolation"], "always");
    assert_eq!(job.report["steps"][0]["state"], "passed");

    // Two commands (before and after) ran, each in a fresh container with no network and nothing else of the machine.
    let runs = lane.runs_in_containers();
    assert_eq!(runs.len(), 2, "{runs:?}");
    for call in &runs {
        let joined = call.join(" ");
        for part in [
            "--network none",
            "--read-only",
            "--cap-drop ALL",
            "--security-opt no-new-privileges",
            "--user 65534:65534",
            "--device /dev/kfd --device /dev/dri",
            "--group-add video --group-add render",
            "--label moebius-lane=",
            "--label moebius-lane-job=",
            "--memory 16g --memory-swap 16g",
            "--oom-score-adj 1000",
            "--ulimit fsize=",
            "rocm/dev-ubuntu:6.2 sh -c",
        ] {
            assert!(joined.contains(part), "{part} in {joined}");
        }
        assert!(!joined.contains("--privileged"));
        assert!(
            !joined.contains("MOEBIUS") && !joined.contains("TOKEN"),
            "{joined}"
        );
    }
    let names: Vec<&String> = runs
        .iter()
        .map(|c| &c[c.iter().position(|a| a == "--name").unwrap() + 1])
        .collect();
    assert_ne!(
        names[0], names[1],
        "a container of its own for each command"
    );
    // Each container sees the checkout of its own side and nothing else of the job: never both, never the directory.
    let mounted: Vec<String> = runs
        .iter()
        .map(|c| c[c.iter().position(|a| a == "-v").unwrap() + 1].clone())
        .collect();
    assert!(mounted[0].ends_with("/before:/work:rw"), "{mounted:?}");
    assert!(mounted[1].ends_with("/after:/work:rw"), "{mounted:?}");
    for (call, mount) in runs.iter().zip(&mounted) {
        let other = if mount.contains("/before:") {
            "after"
        } else {
            "before"
        };
        assert!(
            !call.iter().any(|a| a.contains(&format!("/{other}"))),
            "{call:?}"
        );
    }
    // Every call of ssh, the checkout and the push included, forwards no agent and nothing else.
    let key = lane.f._dir.path().join("gpu.key.log");
    let calls = std::fs::read_to_string(&key).unwrap();
    assert!(calls.lines().count() > 3);
    for call in calls.lines() {
        assert!(call.contains("ForwardAgent=no"), "{call}");
        assert!(call.contains("ClearAllForwardings=yes"), "{call}");
    }
    // The job's directory is gone from the machine afterwards.
    let left = std::fs::read_dir(lane.machine.join("lanes/gpu1/jobs"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(left, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn external_only_contains_a_pull_request_and_runs_the_servers_own_change_directly() {
    let lane = Lane::new(
        Isolation::ExternalOnly,
        &one_step("touch @DIR@/ran-$LANE_JOB"),
    )
    .await;
    lane.code_approved().await;
    lane.pass().await;
    let first = lane.the_job();
    assert_eq!(first.state, "passed", "{first:?}");
    assert_eq!(
        lane.runs_in_containers().len(),
        1,
        "the pull request ran in a container"
    );
    assert_eq!(first.report["machine"]["contained"], true);

    // A job for a change of the server's own, at the same commit, goes directly to the machine.
    let app = lane.app();
    let own = app
        .store
        .add_change(
            lane.f.repository.id,
            Some(lane.card.id),
            "moebius/own",
            None,
            &lane.head,
        )
        .unwrap();
    let id = app
        .store
        .add_lane_job(
            lane.f.repository.id,
            lane.card.id,
            own.id,
            "gpu1",
            &lane.head,
            &lane.base,
            false,
            0,
        )
        .unwrap()
        .unwrap();
    assert!(app.store.claim_lane_job(id).unwrap());
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || app2.run_lane_job(id))
        .await
        .unwrap();
    let second = app.store.lane_job(id).unwrap().unwrap();
    assert_eq!(second.state, "passed", "{second:?}");
    assert_eq!(second.report["machine"]["contained"], false);
    assert_eq!(
        lane.runs_in_containers().len(),
        1,
        "the container was not used again"
    );
    assert!(lane.f._dir.path().join(format!("ran-{id}")).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn off_runs_everything_on_the_machine_and_the_container_is_never_asked() {
    let lane = Lane::new(Isolation::Off, &one_step("echo ran > @DIR@/off-ran")).await;
    lane.code_approved().await;
    lane.pass().await;
    assert_eq!(lane.the_job().state, "passed", "{:?}", lane.the_job());
    assert!(lane.f._dir.path().join("off-ran").exists());
    assert!(lane.container_calls().is_empty());
    assert_eq!(lane.the_job().report["machine"]["contained"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_container_runtime_refuses_the_job_and_never_runs_it_directly() {
    let lane = Lane::new(Isolation::Always, &one_step("echo ran > @DIR@/leaked")).await;
    lane.app().lanes.configure(
        vec![lane_config(Isolation::Always, "/nonexistent/podman")],
        vec![RunnerConfig {
            name: "box".into(),
            host: "gpu.test".into(),
            user: None,
            port: None,
            key_file: None,
            working_dir: lane.machine.to_string_lossy().into_owned(),
            max_runs: 1,
            bin_dir: None,
            ssh: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-ssh"),
        }],
    );
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "errored", "{job:?}");
    assert!(job.summary.contains("container runtime"), "{}", job.summary);
    assert!(
        job.summary
            .contains("never runs a job that must be isolated"),
        "{}",
        job.summary
    );
    assert!(
        !lane.f._dir.path().join("leaked").exists(),
        "nothing ran without the container"
    );
    // An error of the lane is not the author's: no reply on the pull request, and the card stays.
    assert!(lane.replies_on_the_pull_request().is_empty());
    assert_eq!(lane.f.card(1).unwrap().column, "Ready for testing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_runtime_that_cannot_start_the_container_is_an_error_of_the_lane() {
    let lane = Lane::new(Isolation::Always, &one_step("true")).await;
    std::fs::write(format!("{}.fail", lane.podman.display()), "").unwrap();
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "errored", "{job:?}");
    assert!(
        job.summary.contains("could not start the job's container"),
        "{}",
        job.summary
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Steps, timeouts, rows.

#[tokio::test(flavor = "multi_thread")]
async fn steps_run_in_order_and_a_step_past_its_limit_stops_the_job() {
    let definition = "[[step]]\nname = \"build\"\ncommand = \"echo built > @DIR@/built\"\n\
                      [[step]]\nname = \"slow\"\ncommand = \"sleep 30\"\ntimeout_seconds = 1\n\
                      [[step]]\nname = \"never\"\ncommand = \"touch @DIR@/never\"\n";
    let lane = Lane::new(Isolation::Off, definition).await;
    lane.code_approved().await;
    let began = Instant::now();
    lane.pass().await;
    assert!(began.elapsed() < Duration::from_secs(25));
    let job = lane.the_job();
    assert_eq!(job.state, "failed", "{job:?}");
    assert_eq!(job.failed_step.as_deref(), Some("slow"));
    assert!(
        job.summary.contains("limit of 1 seconds"),
        "{}",
        job.summary
    );
    assert!(lane.f._dir.path().join("built").exists());
    assert!(!lane.f._dir.path().join("never").exists());
    let steps = job.report["steps"].as_array().unwrap();
    assert_eq!(
        steps
            .iter()
            .map(|s| s["state"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["passed", "failed"]
    );
}

const MEASURE: &str = "[[step]]\nname = \"measure\"\ncommand = '''\n\
echo \"$LANE_SIDE $LANE_RUN\" >> @DIR@/order\n\
if [ \"$LANE_SIDE\" = before ]; then v=$((100 + LANE_RUN)); else v=$((110 + LANE_RUN)); fi\n\
echo noise\n\
echo \"MEASURE decode_tps $v tok/s\"\n\
echo '{\"rows\":[{\"name\":\"mem\",\"value\":3.5,\"unit\":\"GB\"}]}' > out.json\n\
echo same > out.bin\n'''\n\
[step.measure]\nprefix = \"MEASURE \"\njson = \"out.json\"\nparity_file = \"out.bin\"\n";

#[tokio::test(flavor = "multi_thread")]
async fn a_measuring_step_runs_before_and_after_by_turns_and_its_rows_are_read() {
    let lane = Lane::new(Isolation::Off, MEASURE).await;
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "measured", "{job:?}");
    let order = std::fs::read_to_string(lane.f._dir.path().join("order")).unwrap();
    assert_eq!(
        order.lines().collect::<Vec<_>>(),
        [
            "before 1", "after 1", "before 2", "after 2", "before 3", "after 3"
        ]
    );
    let rows = &job.report["measurements"][0]["rows"];
    let decode = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "decode_tps")
        .unwrap();
    assert_eq!(decode["before"], json!([101.0, 102.0, 103.0]));
    assert_eq!(decode["after"], json!([111.0, 112.0, 113.0]));
    assert_eq!(decode["unit"], "tok/s");
    assert_eq!(decode["median_after"], 112.0);
    let mem = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "mem")
        .unwrap();
    assert_eq!(mem["after"], json!([3.5, 3.5, 3.5]));
    assert!(
        job.report["measurements"][0]["parity"]
            .as_str()
            .unwrap()
            .starts_with("identical")
    );
    // The machine is in the report, with what the configuration says of it.
    assert_eq!(job.report["machine"]["gpus"], "2x MI300X");
    assert_eq!(job.report["machine"]["power_cap"], "750 W");
    assert_eq!(job.report["machine"]["rocm"], "6.2.1");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_gives_no_rows_or_changes_its_output_between_sides_is_told_so() {
    let silent = "[[step]]\nname = \"measure\"\ncommand = \"echo nothing\"\n[step.measure]\nprefix = \"MEASURE \"\n";
    let lane = Lane::new(Isolation::Off, silent).await;
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(
        job.summary.contains("no measurement rows"),
        "{}",
        job.summary
    );

    let differs = MEASURE.replace("echo same > out.bin", "echo $LANE_SIDE > out.bin");
    let lane = Lane::new(Isolation::Off, &differs).await;
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "measured");
    assert!(
        job.report["measurements"][0]["parity"]
            .as_str()
            .unwrap()
            .starts_with("differs")
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Results.

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_step_is_answered_once_on_the_pull_request_and_the_card_goes_back_to_the_author()
{
    let command = "i=0; while [ $i -lt 80 ]; do echo \"early line $i\"; i=$((i+1)); done\n\
                   echo 'token ghp_abcdefghijklmnopqrstuvwxyz0123456789 leaked'\n\
                   echo 'error: kernel not found'\nexit 3";
    let definition = format!("[[step]]\nname = \"build\"\ncommand = '''{command}'''\n");
    let lane = Lane::new(Isolation::Off, &definition).await;
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(
        (job.state.as_str(), job.failed_step.as_deref()),
        ("failed", Some("build"))
    );
    let replies = lane.replies_on_the_pull_request();
    assert_eq!(replies.len(), 1, "{replies:?}");
    let reply = &replies[0];
    assert!(
        reply.contains("**build**") && reply.contains("exited with code 3"),
        "{reply}"
    );
    assert!(reply.contains("error: kernel not found"), "{reply}");
    assert!(
        !reply.contains("early line 0\n"),
        "only the last lines: {reply}"
    );
    assert!(
        !reply.contains("ghp_abcdefghijklmnopqrstuvwxyz"),
        "redacted: {reply}"
    );
    assert_eq!(
        lane.f.card(1).unwrap().column,
        "In review",
        "the author's turn"
    );
    // It is handed on once, whatever the lane does next.
    lane.pass().await;
    lane.pass().await;
    assert_eq!(lane.replies_on_the_pull_request().len(), 1);
    // And the same head is not run again.
    assert!(
        lane.app()
            .request_lane_job(lane.card.id)
            .unwrap()
            .job
            .unwrap()
            .state
            == "failed"
    );
    assert_eq!(lane.jobs().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_measurement_starts_the_next_round_and_an_approval_merges_through_the_gate() {
    let lane = Lane::new(Isolation::Off, MEASURE).await;
    lane.f.lane_edit_factory(FactoryEdit {
        require_measurement: Some(true),
        ..Default::default()
    });
    lane.code_approved().await;
    let change = lane
        .f
        .server
        .app
        .followed_change(&lane.card)
        .unwrap()
        .unwrap();
    lane.pass().await;
    assert_eq!(lane.the_job().state, "measured");

    // Recorded as the lane's measurement of this head, with the machine and every run, and asked of the Reviewer.
    let reports = lane
        .f
        .server
        .app
        .store
        .measurement_reports(change.id)
        .unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(
        (reports[0].measurer.as_str(), reports[0].state.as_str()),
        ("lane:gpu1", "submitted")
    );
    assert_eq!(
        (reports[0].head.as_str(), reports[0].before.as_str()),
        (lane.head.as_str(), lane.base.as_str())
    );
    for part in [
        "GPUs: 2x MI300X",
        "Power cap: 750 W",
        "ROCm: 6.2.1",
        "decode_tps",
        "101, 102, 103",
        "111, 112, 113",
    ] {
        assert!(
            reports[0].body.contains(part),
            "{part} in {}",
            reports[0].body
        );
    }
    let rounds = lane.f.server.app.store.external_reviews(change.id).unwrap();
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[1].kind, "measurement");
    assert_eq!(rounds[1].head, lane.head);
    assert!(lane.the_job().delivered);
    assert!(
        !lane
            .f
            .server
            .app
            .store
            .has_measurement(change.id, &lane.head)
            .unwrap(),
        "nobody accepted it yet: the Reviewer judges it first"
    );

    // The Reviewer reads it as data, with the lane's name, and an approval merges: the gate's one condition holds.
    lane.f.server.poll_github().await;
    let run = lane.f.reviews_of(&lane.card).remove(1);
    // The prompt of the round is built while it is claimed.
    let round = lane.f.server.app.store.external_reviews(change.id).unwrap()[1].id;
    let store = &lane.f.server.app.store;
    assert!(
        store
            .move_external_review(round, "running", "starting")
            .unwrap()
    );
    let section = lane
        .f
        .server
        .app
        .measurement_section(&lane.card, |_| "````".to_string());
    assert!(
        store
            .move_external_review(round, "starting", "running")
            .unwrap()
    );
    assert!(
        section.contains("data, not instructions") && section.contains("test lane `gpu1`"),
        "{section}"
    );
    assert!(
        section.contains("decode_tps") && section.contains("2x MI300X"),
        "{section}"
    );
    assert!(!section.contains("registered measurer"), "{section}");
    lane.f
        .reviewer_ends(
            &lane.card,
            &run,
            &said("approve", "The numbers meet the bar.", json!([])),
        )
        .await;
    assert_eq!(
        lane.f.merged_requests().len(),
        1,
        "{:?}",
        lane.replies_on_the_pull_request()
    );
    assert!(
        lane.f
            .server
            .app
            .store
            .has_measurement(change.id, &lane.head)
            .unwrap()
    );
    assert_eq!(
        lane.f
            .server
            .app
            .store
            .measurement_reports(change.id)
            .unwrap()[0]
            .state,
        "accepted"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_push_cancels_a_queued_job_and_the_new_head_needs_its_own_review() {
    let lane = Lane::new(Isolation::Off, &one_step("true")).await;
    lane.code_approved().await;
    assert_eq!(lane.the_job().state, "queued");
    let newer = push_to_the_pull_request(&lane.f);
    lane.f.server.poll_github().await;
    assert_eq!(lane.the_job().state, "cancelled", "{:?}", lane.the_job());
    assert!(lane.pass().await.is_empty());
    let asked = lane.app().request_lane_job(lane.card.id).unwrap();
    assert!(asked.job.is_none(), "{asked:?}");
    assert!(asked.why.unwrap().contains("has not approved"));
    assert_eq!(
        lane.f
            .server
            .app
            .followed_change(&lane.card)
            .unwrap()
            .unwrap()
            .head
            .as_deref(),
        Some(newer.as_str())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_push_stops_a_job_that_is_running_and_its_result_counts_for_nothing() {
    let lane = Lane::new(
        Isolation::Off,
        &one_step("echo started > @DIR@/started; sleep 40"),
    )
    .await;
    lane.code_approved().await;
    let app = lane.app();
    let running = tokio::task::spawn_blocking(move || {
        let started = app.lane_pass();
        for id in &started {
            app.run_lane_job(*id);
        }
    });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !lane.f._dir.path().join("started").exists() {
        assert!(Instant::now() < deadline, "the job did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(lane.the_job().state, "running");
    let began = Instant::now();
    push_to_the_pull_request(&lane.f);
    lane.f.server.poll_github().await;
    running.await.unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(20),
        "the job was stopped, not waited for"
    );
    let job = lane.the_job();
    assert_eq!(job.state, "cancelled", "{job:?}");
    assert!(lane.replies_on_the_pull_request().is_empty());
    assert!(lane.f.server.app.store.lane_active().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_result_that_waits_for_a_hold_is_dropped_when_the_head_moves() {
    let lane = Lane::new(Isolation::Off, MEASURE).await;
    lane.code_approved().await;
    // The card is held once the job is going; its result is not handed on while the hold stands.
    let app = lane.app();
    let started = {
        let app = app.clone();
        tokio::task::spawn_blocking(move || app.lane_pass())
            .await
            .unwrap()
    };
    assert_eq!(started.len(), 1);
    app.store
        .hold_card(lane.card.id, "person", Some("a look"))
        .unwrap();
    {
        let app = app.clone();
        let id = started[0];
        tokio::task::spawn_blocking(move || app.run_lane_job(id))
            .await
            .unwrap();
    }
    let job = lane.the_job();
    assert_eq!((job.state.as_str(), job.delivered), ("measured", false));
    let change = app.followed_change(&lane.card).unwrap().unwrap();
    assert!(app.store.measurement_reports(change.id).unwrap().is_empty());
    push_to_the_pull_request(&lane.f);
    lane.f.server.poll_github().await;
    assert!(
        lane.the_job().delivered,
        "handed on to nobody: it counts for nothing"
    );
    app.store.release_card(lane.card.id).unwrap();
    lane.pass().await;
    assert!(app.store.measurement_reports(change.id).unwrap().is_empty());
}

// ---------------------------------------------------------------------------------------------------------------------
// What stops a job from starting.

#[tokio::test(flavor = "multi_thread")]
async fn a_freeze_a_hold_a_pause_and_a_locked_path_start_nothing() {
    // A locked path: no job is queued, and the answer says which.
    let lane = Lane::new(Isolation::Off, &one_step("touch @DIR@/ran")).await;
    lane.f.lane_edit_factory(FactoryEdit {
        locked_paths: Some(vec!["src/**".into()]),
        ..Default::default()
    });
    lane.code_approved().await;
    assert!(lane.jobs().is_empty());
    let asked = lane.app().request_lane_job(lane.card.id).unwrap();
    assert!(asked.why.unwrap().contains("locked by src/**"));
    assert!(lane.pass().await.is_empty());
    lane.f.lane_edit_factory(FactoryEdit {
        locked_paths: Some(vec![]),
        ..Default::default()
    });
    assert!(
        lane.app()
            .request_lane_job(lane.card.id)
            .unwrap()
            .job
            .is_some()
    );
    // Each hold stops the start of the job that is queued, and lifting it lets it go.
    let app = lane.app();
    let repository = lane.f.repository.id;
    type Hold = Box<dyn Fn()>;
    let holds: Vec<(&str, Hold, Hold)> = vec![
        (
            "frozen",
            Box::new({
                let app = app.clone();
                move || {
                    drop(
                        app.freeze_repository(&owner(), repository, "a look")
                            .unwrap(),
                    )
                }
            }),
            Box::new({
                let app = app.clone();
                move || {
                    drop(
                        app.unfreeze_repository(&owner(), repository, "done")
                            .unwrap(),
                    )
                }
            }),
        ),
        (
            "held",
            Box::new({
                let app = app.clone();
                let card = lane.card.id;
                move || assert!(app.store.hold_card(card, "person", None).unwrap())
            }),
            Box::new({
                let app = app.clone();
                let card = lane.card.id;
                move || assert!(app.store.release_card(card).unwrap())
            }),
        ),
        (
            "paused",
            Box::new({
                let app = app.clone();
                move || assert!(app.store.pause_runs("person").unwrap())
            }),
            Box::new({
                let app = app.clone();
                move || assert!(app.store.resume_runs().unwrap())
            }),
        ),
    ];
    for (name, hold, lift) in holds {
        hold();
        assert!(app.lane_pass().is_empty(), "{name}: nothing starts");
        assert_eq!(lane.the_job().state, "queued", "{name}");
        let asked = app.request_lane_job(lane.card.id).unwrap();
        assert!(asked.why.is_some() || asked.job.is_some(), "{name}");
        lift();
    }
    assert!(!lane.f._dir.path().join("ran").exists());
    assert_eq!(lane.pass().await.len(), 1);
    assert_eq!(lane.the_job().state, "passed");
    assert!(lane.f._dir.path().join("ran").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lane_runs_one_job_at_a_time() {
    let lane = Lane::new(Isolation::Off, &one_step("true")).await;
    lane.code_approved().await;
    let app = lane.app();
    let change = app.followed_change(&lane.card).unwrap().unwrap();
    // A second job of the lane, for a change of another card, waits behind the first.
    let other = app
        .store
        .add_card(lane.f.repository.id, "Other", "", "person", "In review")
        .unwrap();
    let other_change = app
        .store
        .add_change(
            lane.f.repository.id,
            Some(other.id),
            "other",
            None,
            "aaaaaaa",
        )
        .unwrap();
    let second = app
        .store
        .add_lane_job(
            lane.f.repository.id,
            other.id,
            other_change.id,
            "gpu1",
            "aaaaaaa",
            "b",
            false,
            0,
        )
        .unwrap()
        .unwrap();
    let first = lane.the_job().id;
    assert_eq!(app.start_lane_jobs().unwrap(), vec![first]);
    assert!(
        app.start_lane_jobs().unwrap().is_empty(),
        "the lane is locked while the first runs"
    );
    assert_eq!(app.store.lane_job(second).unwrap().unwrap().state, "queued");
    let queue = app.store.lane_queue("gpu1").unwrap();
    assert_eq!(
        queue.iter().map(|j| j.state.as_str()).collect::<Vec<_>>(),
        ["running", "queued"]
    );
    let _ = change;
}

#[tokio::test(flavor = "multi_thread")]
async fn three_jobs_in_a_row_that_fail_for_the_lane_pause_it_and_tell_the_owner() {
    let lane = Lane::new(Isolation::Always, &one_step("true")).await;
    lane.app().lanes.configure(
        vec![lane_config(Isolation::Always, "/nonexistent/podman")],
        vec![RunnerConfig {
            name: "box".into(),
            host: "gpu.test".into(),
            user: None,
            port: None,
            key_file: None,
            working_dir: lane.machine.to_string_lossy().into_owned(),
            max_runs: 1,
            bin_dir: None,
            ssh: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/fake-ssh"),
        }],
    );
    let app = lane.app();
    lane.code_approved().await;
    let change = app.followed_change(&lane.card).unwrap().unwrap();
    let mut ids = vec![lane.the_job().id];
    for head in ["1111111", "2222222"] {
        ids.push(
            app.store
                .add_lane_job(
                    lane.f.repository.id,
                    lane.card.id,
                    change.id,
                    "gpu1",
                    head,
                    &lane.base,
                    true,
                    0,
                )
                .unwrap()
                .unwrap(),
        );
    }
    for (n, id) in ids.iter().enumerate() {
        assert!(
            app.store.lane_pause("gpu1").unwrap().is_none(),
            "before job {n}"
        );
        assert!(app.store.claim_lane_job(*id).unwrap());
        let app2 = app.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || app2.run_lane_job(id))
            .await
            .unwrap();
        assert_eq!(app.store.lane_job(id).unwrap().unwrap().state, "errored");
    }
    let pause = app
        .store
        .lane_pause("gpu1")
        .unwrap()
        .expect("the lane paused");
    assert!(pause.reason.contains("3 jobs in a row"), "{}", pause.reason);
    let item = app
        .store
        .inbox()
        .unwrap()
        .into_iter()
        .find(|i| i.kind == "lane_paused")
        .expect("the owner is told");
    assert!(item.title.contains("gpu1"));
    // A job that is queued waits, however long.
    app.store
        .add_lane_job(
            lane.f.repository.id,
            lane.card.id,
            change.id,
            "gpu1",
            "3333333",
            &lane.base,
            true,
            0,
        )
        .unwrap();
    assert!(app.start_lane_jobs().unwrap().is_empty());
    // Only a person lifts it, and the count starts over.
    assert!(
        app.resume_lane(&Actor::Script { column: "x".into() }, "gpu1", "")
            .is_err()
    );
    assert!(
        app.resume_lane(&owner(), "gpu1", "fixed the runtime")
            .unwrap()
    );
    assert!(app.store.lane_pause("gpu1").unwrap().is_none());
    assert_eq!(app.store.lane_infrastructure_streak("gpu1").unwrap(), 0);
    assert!(
        app.store
            .inbox()
            .unwrap()
            .iter()
            .all(|i| i.kind != "lane_paused")
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// The digest, the settings and the script call.

#[tokio::test(flavor = "multi_thread")]
async fn the_digest_warns_while_a_lane_is_not_isolated_everywhere_and_tells_what_the_lane_did() {
    for (isolation, warned) in [
        (Isolation::Off, true),
        (Isolation::ExternalOnly, true),
        (Isolation::Always, false),
    ] {
        let lane = Lane::new(isolation, &one_step("exit 2")).await;
        lane.code_approved().await;
        lane.pass().await;
        lane.f
            .server
            .app
            .store
            .set_digest_epoch(crate::store::now() - 3 * 86_400)
            .unwrap();
        let app = lane.app();
        let item = tokio::task::spawn_blocking(move || app.make_digest("2099-01-02", None))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let text = item.data["text"].as_str().unwrap().to_string();
        assert_eq!(
            text.contains("runs with isolation"),
            warned,
            "{isolation:?}: {text}"
        );
        assert!(
            text.contains("test lane gpu1") && text.contains("check"),
            "{text}"
        );
        if warned {
            assert!(text.contains(isolation.as_str()), "{text}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_definition_that_is_not_valid_is_refused_where_it_is_saved() {
    let lane = Lane::new(Isolation::Off, &one_step("true")).await;
    let app = lane.app();
    let repo = app.harness_repo().unwrap();
    let save = |text: &str| {
        let named = || Ok("widgets".to_string());
        repo.save_project_file(
            &app,
            &owner(),
            (&named, "lane.toml"),
            Some(text),
            ("edit_project_file", "Put a file"),
            Default::default(),
        )
    };
    let wrong = save(
        "[[step]]\nname = \"m\"\ncommand = \"x\"\n[step.measure]\nruns = 1\nprefix = \"M \"\n",
    )
    .unwrap_err();
    assert!(wrong.to_string().contains("measure.runs"), "{wrong}");
    save("[[step]]\nname = \"m\"\ncommand = \"x\"\n").unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_assigns_the_lane_and_nobody_else_does() {
    let lane = Lane::new(Isolation::Off, &one_step("true")).await;
    let app = lane.app();
    let repo = lane.f.repository.id;
    let agent = Actor::Script {
        column: "review".into(),
    };
    assert!(app.edit_repository_lane(&agent, repo, None, "").is_err());
    assert!(
        app.edit_repository_lane(&owner(), repo, Some("nowhere".into()), "")
            .is_err()
    );
    let view = app
        .edit_repository_lane(&owner(), repo, None, "off")
        .unwrap();
    assert!(view.lane.is_none() && view.current.is_none());
    assert_eq!(view.lanes.len(), 1);
    assert!(
        app.request_lane_job(lane.card.id)
            .unwrap()
            .why
            .unwrap()
            .contains("uses no test lane")
    );
    let view = app
        .edit_repository_lane(&owner(), repo, Some("gpu1".into()), "on")
        .unwrap();
    let current = view.current.unwrap();
    assert_eq!(current.isolation, "off");
    assert!(current.warning.unwrap().contains("directly on the machine"));
    assert!(view.definition.present && view.definition.problem.is_none());
    assert_eq!(view.definition.steps, ["check"]);
}

// ---------------------------------------------------------------------------------------------------------------------
// What the code of a pull request must not be able to do to the machine, or to the numbers.

/// Gives pull request 7 a new head that adds `files`.
fn pull_writes(f: &Fixture, files: &[(&str, &str)]) -> String {
    let work = f._dir.path().join("pull-work");
    for (path, text) in files {
        std::fs::create_dir_all(work.join(path).parent().unwrap()).unwrap();
        // `->target` makes the path a link to `target`, in place of what was there.
        if let Some(target) = text.strip_prefix("->") {
            let _ = std::fs::remove_dir_all(work.join(path));
            std::os::unix::fs::symlink(target, work.join(path)).unwrap();
        } else {
            std::fs::write(work.join(path), text).unwrap();
        }
    }
    git(&work, &["add", "-A"]).unwrap();
    git(
        &work,
        &[
            "-c",
            "user.name=alice",
            "-c",
            "user.email=alice@example.com",
            "commit",
            "--quiet",
            "-m",
            "More",
        ],
    )
    .unwrap();
    git(
        &work,
        &["push", "--quiet", "origin", "HEAD:refs/pull/7/head"],
    )
    .unwrap();
    let head = git(&work, &["rev-parse", "HEAD"]).unwrap();
    f.fake.set_head(&head);
    f.fake.listed_pulls.lock().unwrap()[0]["head"]["sha"] = json!(head);
    head
}

/// The pid file a step writes, once it is there.
async fn pid_of(file: &std::path::Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(text) = std::fs::read_to_string(file)
            && !text.trim().is_empty()
        {
            return text.trim().to_string();
        }
        assert!(Instant::now() < deadline, "the step did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn alive(pid: &str) -> bool {
    // A process that was killed and not yet reaped (nothing here may be init) is a zombie: it is not running.
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .map(str::trim_start)
            .is_some_and(|rest| !rest.starts_with('Z')),
        Err(_) => false,
    }
}

/// Runs the pass on a thread of its own, as the server does, so that the test can act while a job goes.
fn pass_in_the_background(lane: &Lane) -> tokio::task::JoinHandle<()> {
    let app = lane.app();
    tokio::task::spawn_blocking(move || {
        for id in app.lane_pass() {
            app.run_lane_job(id);
        }
    })
}

impl Lane {
    /// The runner of the lane as `Lane::new` made it.
    fn runner(&self) -> RunnerConfig {
        let testdata = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        RunnerConfig {
            name: "box".into(),
            host: "gpu.test".into(),
            user: Some("lane".into()),
            port: Some(2222),
            key_file: Some(self.f._dir.path().join("gpu.key")),
            working_dir: self.machine.to_string_lossy().into_owned(),
            max_runs: 1,
            bin_dir: None,
            ssh: testdata.join("fake-ssh"),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_checkout_runs_no_filter_hook_or_configuration_of_the_account() {
    let lane = Lane::new(Isolation::Off, &one_step("true")).await;
    let dir = lane.f._dir.path().to_path_buf();
    // An account with a global filter and hooks, as a machine with git-lfs or a template directory has.
    let hooks = dir.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for hook in ["post-checkout", "post-merge", "pre-commit"] {
        let path = hooks.join(hook);
        std::fs::write(
            &path,
            format!("#!/bin/sh\ntouch {}/hook-ran\n", dir.display()),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }
    let config = dir.join("account.gitconfig");
    std::fs::write(
        &config,
        format!(
            "[filter \"evil\"]\n\tsmudge = sh -c 'touch {d}/filter-ran; cat'\n\tclean = cat\n[core]\n\thooksPath = {d}/hooks\n[init]\n\ttemplateDir = {d}/hooks\n",
            d = dir.display()
        ),
    )
    .unwrap();
    lane.app().lanes.set_test_environment(vec![
        ("GIT_CONFIG_GLOBAL".into(), config.display().to_string()),
        ("GIT_CONFIG_SYSTEM".into(), config.display().to_string()),
    ]);
    // The pull request brings the attributes that name that filter.
    let head = pull_writes(
        &lane.f,
        &[
            (".gitattributes", "* filter=evil\n"),
            ("src/more.rs", "more\n"),
        ],
    );
    lane.f.server.poll_github().await;
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.head, head);
    assert_eq!(job.state, "passed", "{job:?}");
    assert!(
        !dir.join("filter-ran").exists(),
        "the filter of the account ran on the pull request's tree"
    );
    assert!(
        !dir.join("hook-ran").exists(),
        "a hook of the account ran on the pull request's tree"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_job_has_its_container_removed_before_the_lane_is_free() {
    let lane = Lane::new(Isolation::Always, &one_step("sleep 40")).await;
    lane.code_approved().await;
    let app = lane.app();
    let id = lane.the_job().id;
    let running = pass_in_the_background(&lane);
    let name = format!("moebius-lane-{id}-0");
    let there = format!("{}.running.{name}", lane.podman.display());
    let deadline = Instant::now() + Duration::from_secs(20);
    while !std::path::Path::new(&there).exists() {
        assert!(Instant::now() < deadline, "the container did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    push_to_the_pull_request(&lane.f);
    lane.f.server.poll_github().await;
    running.await.unwrap();
    assert_eq!(lane.the_job().state, "cancelled");
    // The runtime was told to remove it by name, and it is gone, before the job counted as over.
    let removed = lane
        .container_calls()
        .iter()
        .any(|c| c.first().map(String::as_str) == Some("rm") && c.contains(&name));
    assert!(removed, "{:?}", lane.container_calls());
    assert!(!std::path::Path::new(&there).exists());
    assert!(app.store.lane_pause("gpu1").unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_job_leaves_nothing_of_a_direct_step_running() {
    let lane = Lane::new(
        Isolation::Off,
        &one_step("echo $$ > @DIR@/step.pid; sleep 60"),
    )
    .await;
    lane.code_approved().await;
    let running = pass_in_the_background(&lane);
    let pid = pid_of(&lane.f._dir.path().join("step.pid")).await;
    assert!(alive(&pid));
    push_to_the_pull_request(&lane.f);
    lane.f.server.poll_github().await;
    running.await.unwrap();
    assert_eq!(lane.the_job().state, "cancelled");
    assert!(!alive(&pid), "the step is still running on the machine");
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_step_left_running_in_the_background_ends_with_the_step() {
    let lane = Lane::new(
        Isolation::Off,
        &one_step("(sleep 120 & echo $! > @DIR@/leftover.pid); echo done"),
    )
    .await;
    lane.code_approved().await;
    let began = Instant::now();
    lane.pass().await;
    assert!(
        began.elapsed() < Duration::from_secs(60),
        "the job did not wait for it"
    );
    assert_eq!(lane.the_job().state, "passed", "{:?}", lane.the_job());
    let pid = pid_of(&lane.f._dir.path().join("leftover.pid")).await;
    assert!(!alive(&pid), "the background process is still there");
}

const LINK: &str = "[[step]]\nname = \"measure\"\ncommand = '''\necho \"MEASURE decode_tps 1\"\n@PLANT@\n'''\n[step.measure]\nprefix = \"MEASURE \"\njson = \"@JSON@\"\n";

#[tokio::test(flavor = "multi_thread")]
async fn a_symbolic_link_the_job_planted_is_not_followed_out_of_its_checkout() {
    let secret = "{\"rows\":[{\"name\":\"fast\",\"value\":1}]}";
    for (plant, json) in [
        ("ln -sf @DIR@/secret.json out.json", "out.json"),
        ("rm -rf sub; ln -s @DIR@ sub", "sub/secret.json"),
    ] {
        let definition = LINK.replace("@PLANT@", plant).replace("@JSON@", json);
        let lane = Lane::new(Isolation::Off, &definition).await;
        std::fs::write(lane.f._dir.path().join("secret.json"), secret).unwrap();
        lane.code_approved().await;
        lane.pass().await;
        let job = lane.the_job();
        assert_eq!(job.state, "failed", "{plant}: {job:?}");
        assert!(
            job.summary.contains("no regular file"),
            "{plant}: {}",
            job.summary
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_exit_codes_the_runtime_and_the_shell_use_are_the_changes_own_when_its_command_gives_them()
 {
    for (isolation, code) in [
        (Isolation::Off, 124),
        (Isolation::Off, 255),
        (Isolation::Always, 125),
        (Isolation::Always, 124),
        (Isolation::Always, 255),
    ] {
        let lane = Lane::new(isolation, &one_step(&format!("echo trouble; exit {code}"))).await;
        lane.code_approved().await;
        lane.pass().await;
        let job = lane.the_job();
        assert_eq!(job.state, "failed", "{isolation:?} {code}: {job:?}");
        assert!(
            job.summary.contains(&format!("exited with code {code}")),
            "{}",
            job.summary
        );
        let replies = lane.replies_on_the_pull_request();
        assert_eq!(replies.len(), 1, "{isolation:?} {code}: {replies:?}");
        assert_eq!(lane.f.card(1).unwrap().column, "In review");
        // Nothing of the lane's: no streak of errors, so a pull request cannot pause the lane.
        assert_eq!(
            lane.app().store.lane_infrastructure_streak("gpu1").unwrap(),
            0
        );
    }
}

/// A lane whose jobs are given `seconds` altogether.
async fn short_lane(definition: &str, seconds: u64) -> Lane {
    let lane = Lane::new(Isolation::Off, definition).await;
    let mut config = lane_config(Isolation::Off, "podman");
    config.timeout_seconds = seconds;
    lane.app()
        .lanes
        .configure(vec![config], vec![lane.runner()]);
    lane
}

#[tokio::test(flavor = "multi_thread")]
async fn a_head_that_runs_out_of_the_jobs_total_time_fails_and_names_that_limit() {
    let lane = short_lane(&one_step("sleep 60"), 8).await;
    lane.code_approved().await;
    let began = Instant::now();
    lane.pass().await;
    assert!(began.elapsed() < Duration::from_secs(40));
    // The head took all the time it was given: that is the change's, so it is a failure with the limit in it, and it is
    // not an error of the lane, which would count towards pausing it.
    let job = lane.the_job();
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("total time"), "{}", job.summary);
    assert!(job.summary.contains("8 seconds"), "{}", job.summary);
    assert!(
        !job.summary.contains("900"),
        "the step's own limit did not run out: {}",
        job.summary
    );
    assert_eq!(
        lane.app().store.lane_infrastructure_streak("gpu1").unwrap(),
        0
    );
    let replies = lane.replies_on_the_pull_request();
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert!(replies[0].contains("8 seconds"), "{replies:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merge_base_that_runs_out_of_the_jobs_total_time_is_the_lanes() {
    let both = "[[step]]\nname = \"check\"\non = \"both\"\ncommand = \"sleep 60\"\n";
    let lane = short_lane(both, 8).await;
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "errored", "{job:?}");
    assert!(
        job.summary.contains("total time of 8 seconds"),
        "{}",
        job.summary
    );
    // A pull request of someone else's that ran out of the time through the lane's fault is not counted towards pausing it.
    assert_eq!(job.failed_step.as_deref(), Some("total time"));
    assert_eq!(
        lane.app().store.lane_infrastructure_streak("gpu1").unwrap(),
        0
    );
    // The change is not blamed: no reply, and the card stays where it was.
    assert!(lane.replies_on_the_pull_request().is_empty());
    assert_eq!(lane.f.card(1).unwrap().column, "Ready for testing");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_step_a_job_is_at_is_published_as_it_goes() {
    let lane = Lane::new(Isolation::Off, &one_step("sleep 3")).await;
    lane.code_approved().await;
    let id = lane.the_job().id;
    let mut events = lane.app().bus.subscribe();
    let running = pass_in_the_background(&lane);
    let mut seen_at_a_step = false;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !running.is_finished() && Instant::now() < deadline {
        let published = events.try_recv().is_ok();
        let step = lane.app().store.lane_job(id).unwrap().unwrap().step;
        if published && step.is_some() {
            seen_at_a_step = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    running.await.unwrap();
    assert!(
        seen_at_a_step,
        "the step was written and the card published"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_measurement_that_justified_a_merge_stays_on_the_card_after_it_merged() {
    let lane = Lane::new(Isolation::Off, MEASURE).await;
    lane.f.lane_edit_factory(FactoryEdit {
        require_measurement: Some(true),
        ..Default::default()
    });
    lane.code_approved().await;
    lane.pass().await;
    lane.f.server.poll_github().await;
    let run = lane.f.reviews_of(&lane.card).remove(1);
    lane.f
        .reviewer_ends(
            &lane.card,
            &run,
            &said("approve", "Meets the bar.", json!([])),
        )
        .await;
    assert_eq!(lane.f.merged_requests().len(), 1);
    let report = lane.app().card_lane(lane.card.id).unwrap();
    assert_eq!(
        report.head.as_deref(),
        Some(lane.head.as_str()),
        "the head the change ended with"
    );
    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].head, lane.head);
    assert_eq!(report.jobs[0].state, "measured");
}

#[tokio::test(flavor = "multi_thread")]
async fn another_repositorys_running_job_is_seen_only_by_those_who_see_its_card() {
    let lane = Lane::new(Isolation::Off, &one_step("sleep 3")).await;
    lane.code_approved().await;
    let app = lane.app();
    assert_eq!(app.start_lane_jobs().unwrap().len(), 1);
    let card = app.card(lane.card.id).unwrap();
    // Whoever may see the card reads the job: its card, its commit and the step.
    let open = app.lane_views(&crate::app::Sight::everything()).unwrap();
    assert!(open[0].busy);
    assert_eq!(open[0].running.as_ref().unwrap().card_id, card.id);
    // Someone who may not sees that the lane is busy and how long the queue is, and nothing else.
    let hidden = crate::app::Sight::hiding(&[card.project_id]);
    let views = app.lane_views(&hidden).unwrap();
    assert!(views[0].busy && views[0].running.is_none());
    assert_eq!(views[0].queued, open[0].queued);
    let settings = app
        .repository_lane_view(lane.f.repository.id, &hidden)
        .unwrap();
    assert!(settings.jobs.is_empty(), "{:?}", settings.jobs);
    assert!(settings.lanes[0].running.is_none());
    let json = serde_json::to_string(&settings).unwrap();
    assert!(!json.contains(&lane.head), "{json}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_to_the_measuring_harness_is_not_measured_and_the_owner_is_told() {
    let harness = format!("harness = [\"src/\"]\n{}", one_step("true"));
    let lane = Lane::new(Isolation::Off, &harness).await;
    lane.code_approved().await;
    // The pull request adds src/lib.rs, which is the harness: no job, a reason, and one item for the owner.
    assert!(lane.jobs().is_empty());
    for _ in 0..2 {
        let asked = lane.app().request_lane_job(lane.card.id).unwrap();
        assert!(asked.job.is_none());
        assert!(asked.why.unwrap().contains("measuring harness"));
    }
    let items = lane
        .app()
        .store
        .inbox_for_card(lane.card.id)
        .unwrap()
        .into_iter()
        .filter(|i| i.kind == "lane_needs_a_person" && i.resolved_at.is_none())
        .count();
    assert_eq!(items, 1);
    assert!(lane.pass().await.is_empty());
    // The paths count as locked paths for the merge of a script or an agent too.
    let patterns = lane
        .app()
        .lane_harness_patterns(&lane.app().card(lane.card.id).unwrap());
    assert_eq!(patterns, ["/src/"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_harness_of_the_head_is_the_merge_bases_whatever_the_head_has_there() {
    // The head adds src/lib.rs; the merge base has no src/. A job that was queued before the harness was named (or by a
    // race) still measures with the base's harness: the head's file is gone from the checkout of the head.
    let definition = "harness = [\"src/\"]\n[[step]]\nname = \"base harness\"\non = \"both\"\ncommand = \"test ! -e src/lib.rs\"\n";
    let lane = Lane::new(Isolation::Off, definition).await;
    lane.code_approved().await;
    let app = lane.app();
    let own = app
        .store
        .add_change(
            lane.f.repository.id,
            Some(lane.card.id),
            "moebius/own",
            None,
            &lane.head,
        )
        .unwrap();
    let id = app
        .store
        .add_lane_job(
            lane.f.repository.id,
            lane.card.id,
            own.id,
            "gpu1",
            &lane.head,
            &lane.base,
            false,
            0,
        )
        .unwrap()
        .unwrap();
    assert!(app.store.claim_lane_job(id).unwrap());
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || app2.run_lane_job(id))
        .await
        .unwrap();
    let job = app.store.lane_job(id).unwrap().unwrap();
    assert_eq!(job.state, "passed", "{job:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn only_a_person_changes_what_runs_on_the_owners_machine() {
    let lane = Lane::new(Isolation::Off, &one_step("true")).await;
    let app = lane.app();
    let repo = app.harness_repo().unwrap();
    let named = || Ok("widgets".to_string());
    let save = |actor: &Actor, text: Option<&str>| {
        repo.save_project_file(
            &app,
            actor,
            (&named, "lane.toml"),
            text,
            ("edit_project_file", "Put a file"),
            Default::default(),
        )
    };
    let script = Actor::Script {
        column: "review".into(),
    };
    for text in [Some("[[step]]\nname = \"a\"\ncommand = \"id\"\n"), None] {
        let refused = save(&script, text).unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("only a person changes lane.toml"),
            "{refused}"
        );
    }
    save(&owner(), Some("[[step]]\nname = \"a\"\ncommand = \"id\"\n")).unwrap();
    // The Retro proposes rules and the scripts of columns, and no other file.
    assert!(
        !crate::improve::own::files_of(&app, None)
            .iter()
            .any(|f| f.contains("lane"))
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// The measuring harness cannot be changed by what it measures, and its rows cannot be forged.

/// A commit on `parent` in the server's clone of the repository that adds `files`, made as a person would.
fn commit_files(f: &Fixture, parent: &str, files: &[(&str, &str)]) -> String {
    let clone = std::path::Path::new(&f.repository.path);
    let work = f
        ._dir
        .path()
        .join(format!("work-{}", files.len() + parent.len()));
    let _ = std::fs::remove_dir_all(&work);
    git(
        f._dir.path(),
        &[
            "clone",
            "--quiet",
            &f.repository.path,
            work.to_str().unwrap(),
        ],
    )
    .unwrap();
    git(&work, &["checkout", "--quiet", "--detach", parent]).unwrap();
    for (path, text) in files {
        std::fs::create_dir_all(work.join(path).parent().unwrap()).unwrap();
        // `->target` makes the path a link to `target`, in place of what was there.
        if let Some(target) = text.strip_prefix("->") {
            let _ = std::fs::remove_dir_all(work.join(path));
            std::os::unix::fs::symlink(target, work.join(path)).unwrap();
        } else {
            std::fs::write(work.join(path), text).unwrap();
        }
    }
    git(&work, &["add", "-A"]).unwrap();
    git(
        &work,
        &[
            "-c",
            "user.name=a",
            "-c",
            "user.email=a@example.com",
            "commit",
            "--quiet",
            "-m",
            "x",
        ],
    )
    .unwrap();
    let commit = git(&work, &["rev-parse", "HEAD"]).unwrap();
    git(
        clone,
        &[
            "fetch",
            "--quiet",
            work.to_str().unwrap(),
            &format!("+HEAD:refs/test/{commit}"),
        ],
    )
    .unwrap();
    commit
}

impl Lane {
    /// Queues a job of the card's change for `head` on `base` by hand and runs it to its end.
    async fn run_job_for(&self, head: &str, base: &str, external: bool) -> LaneJob {
        let app = self.app();
        let change = app
            .store
            .add_change(
                self.f.repository.id,
                Some(self.card.id),
                &format!("moebius/own-{head}"),
                None,
                head,
            )
            .unwrap();
        let id = app
            .store
            .add_lane_job(
                self.f.repository.id,
                self.card.id,
                change.id,
                "gpu1",
                head,
                base,
                external,
                0,
            )
            .unwrap()
            .unwrap();
        assert!(app.store.claim_lane_job(id).unwrap());
        let app2 = app.clone();
        tokio::task::spawn_blocking(move || app2.run_lane_job(id))
            .await
            .unwrap();
        app.store.lane_job(id).unwrap().unwrap()
    }
}

/// The step that measures with the harness `bench/run.sh`, and the one that builds, which overwrites that harness.
const OVERWRITES: &str = "harness = [\"bench/\"]\n\
[[step]]\nname = \"build\"\ncommand = '''\n\
echo 'echo \"MEASURE tps 999\"' > bench/run.sh || true\n\
echo 'echo \"MEASURE tps 999\"' > bench/more.sh || true\n'''\n\
[[step]]\nname = \"measure\"\ncommand = 'sh bench/run.sh'\n\
[step.measure]\nruns = 3\nprefix = \"MEASURE \"\n";

fn tps(job: &LaneJob, side: &str) -> serde_json::Value {
    job.report["measurements"][0]["rows"][0][side].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_of_the_head_cannot_overwrite_the_harness_that_measures_it_in_a_container() {
    let lane = Lane::new(Isolation::Always, OVERWRITES).await;
    let base = commit_files(
        &lane.f,
        &lane.base,
        &[("bench/run.sh", "echo \"MEASURE tps 100\"\n")],
    );
    let head = commit_files(&lane.f, &base, &[("app.txt", "x\n")]);
    let job = lane.run_job_for(&head, &base, false).await;
    assert_eq!(job.state, "measured", "{job:?}");
    // The build step wrote its own bench/run.sh; the measurement ran the merge base's.
    assert_eq!(tps(&job, "after"), json!([100.0, 100.0, 100.0]));
    assert_eq!(tps(&job, "before"), json!([100.0, 100.0, 100.0]));
    // The harness is laid over the checkout of both sides, read-only, after the checkout's own mount.
    for call in lane.runs_in_containers() {
        let mounts: Vec<&String> = call
            .iter()
            .enumerate()
            .filter(|(at, _)| *at > 0 && call[*at - 1] == "-v")
            .map(|(_, a)| a)
            .collect();
        assert!(mounts[0].ends_with(":/work:rw"), "{mounts:?}");
        assert!(
            mounts[1].ends_with("/harness/bench:/work/bench:ro"),
            "{mounts:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_of_the_head_cannot_overwrite_the_harness_that_measures_it_on_the_machine() {
    let lane = Lane::new(Isolation::Off, OVERWRITES).await;
    let base = commit_files(
        &lane.f,
        &lane.base,
        &[("bench/run.sh", "echo \"MEASURE tps 100\"\n")],
    );
    let head = commit_files(&lane.f, &base, &[("app.txt", "x\n")]);
    let job = lane.run_job_for(&head, &base, false).await;
    assert_eq!(job.state, "measured", "{job:?}");
    // The step overwrote the file in the checkout; it was put back from the job's own copy before the measuring run.
    assert_eq!(tps(&job, "after"), json!([100.0, 100.0, 100.0]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_the_head_put_where_the_harness_is_does_not_let_the_harness_be_written_outside() {
    let plant = "harness = [\"bench/\"]\n\
[[step]]\nname = \"build\"\ncommand = '''\nrm -rf bench; ln -s @DIR@/outside bench\n'''\n\
[[step]]\nname = \"measure\"\ncommand = 'echo \"MEASURE tps 1\"'\n\
[step.measure]\nruns = 3\nprefix = \"MEASURE \"\n";
    let lane = Lane::new(Isolation::Off, plant).await;
    std::fs::create_dir_all(lane.f._dir.path().join("outside")).unwrap();
    let base = commit_files(
        &lane.f,
        &lane.base,
        &[("bench/run.sh", "echo \"MEASURE tps 100\"\n")],
    );
    let head = commit_files(&lane.f, &base, &[("app.txt", "x\n")]);
    let job = lane.run_job_for(&head, &base, false).await;
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("harness"), "{}", job.summary);
    assert!(
        std::fs::read_dir(lane.f._dir.path().join("outside"))
            .unwrap()
            .next()
            .is_none(),
        "the harness was written through the link"
    );
}

async fn measured_with(script: &str) -> LaneJob {
    let definition = format!(
        "[[step]]\nname = \"measure\"\ncommand = '''\n{script}'''\n[step.measure]\nruns = 3\nprefix = \"MEASURE \"\n"
    );
    let lane = Lane::new(Isolation::Off, &definition).await;
    lane.code_approved().await;
    lane.pass().await;
    lane.the_job()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forged_row_followed_by_more_lines_than_are_read_fails_the_step() {
    // The code under test prints a row, then more lines than are kept, then the row the harness would print.
    let job = measured_with(
        "echo 'MEASURE tps 999'\ni=0; while [ $i -lt 20100 ]; do echo \"MEASURE junk$i 1\"; i=$((i+1)); done\necho 'MEASURE tps 41'\n",
    )
    .await;
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("more lines"), "{}", job.summary);
    assert!(job.report["measurements"].as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn more_rows_than_a_run_may_give_fail_the_step_and_are_not_cut() {
    let job =
        measured_with("i=0; while [ $i -lt 600 ]; do echo \"MEASURE row$i 1\"; i=$((i+1)); done\n")
            .await;
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("more than 500"), "{}", job.summary);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_row_name_that_is_given_twice_fails_the_step() {
    let job = measured_with("echo 'MEASURE tps 999'\necho 'MEASURE tps 41'\n").await;
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(
        job.summary.contains("tps was given twice"),
        "{}",
        job.summary
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// What a job leaves behind, and what it may use up.

/// The `PATH` of the machine with the `rm` of a user who does not own what a container's user wrote first.
fn denying_rm(lane: &Lane) {
    let shim = lane.f._dir.path().join("denying-rm");
    std::fs::create_dir_all(&shim).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/denying-rm/rm");
    std::fs::copy(source, shim.join("rm")).unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    lane.app()
        .lanes
        .set_test_environment(vec![("PATH".into(), format!("{}:{path}", shim.display()))]);
}

const FILL: &str =
    "mkdir keep ..keep && echo x > keep/file && echo y > ..keep/file && chmod 000 keep";

#[tokio::test(flavor = "multi_thread")]
async fn what_a_container_made_that_the_machine_cannot_remove_is_removed_by_the_container() {
    let lane = Lane::new(Isolation::Always, &one_step(FILL)).await;
    denying_rm(&lane);
    lane.code_approved().await;
    lane.pass().await;
    assert_eq!(lane.the_job().state, "passed", "{:?}", lane.the_job());
    // A directory of mode 000 and a name of two dots: the directory of the job is gone, and nothing went wrong.
    let left = std::fs::read_dir(lane.machine.join("lanes/gpu1/jobs"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(left, 0);
    assert!(lane.app().store.lane_pause("gpu1").unwrap().is_none());
    let cleanup: Vec<Vec<String>> = lane
        .container_calls()
        .into_iter()
        .filter(|c| c.iter().any(|a| a == "--rm"))
        .collect();
    assert!(!cleanup.is_empty());
    let text = cleanup[0].join(" ");
    assert!(text.contains("chmod -R u+rwX /work"), "{text}");
    assert!(text.contains("find /work -mindepth 1 -delete"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_directory_that_cannot_be_removed_is_an_error_of_the_lane_that_pauses_it() {
    let lane = Lane::new(Isolation::Always, &one_step(FILL)).await;
    denying_rm(&lane);
    // The cleanup container cannot be started either.
    std::fs::write(format!("{}.fail-cleanup", lane.podman.display()), "").unwrap();
    lane.code_approved().await;
    lane.pass().await;
    let left = std::fs::read_dir(lane.machine.join("lanes/gpu1/jobs"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(left, 1, "the directory survived");
    let pause = lane
        .app()
        .store
        .lane_pause("gpu1")
        .unwrap()
        .expect("a pause");
    assert!(
        pause.reason.contains("could not be removed"),
        "{}",
        pause.reason
    );
    let id = lane.the_job().id;
    assert!(
        pause.reason.contains(&format!("job {id}")),
        "{}",
        pause.reason
    );
    // Nothing starts on the lane until a person has looked.
    assert!(lane.app().start_lane_jobs().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_does_not_start_where_the_disk_has_less_free_than_the_lane_allows() {
    let lane = Lane::new(Isolation::Always, &one_step("echo ran > @DIR@/ran")).await;
    let mut config = lane_config(Isolation::Always, lane.podman.to_str().unwrap());
    config.disk = "1000t".into();
    lane.app()
        .lanes
        .configure(vec![config], vec![lane.runner()]);
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "errored", "{job:?}");
    assert!(job.summary.contains("free"), "{}", job.summary);
    assert!(job.summary.contains("job was not run"), "{}", job.summary);
    assert!(!lane.f._dir.path().join("ran").exists());
    assert!(lane.runs_in_containers().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_starts_while_a_job_runs_ends_what_it_left_and_queues_the_job_again() {
    use std::os::unix::process::CommandExt;
    let lane = Lane::new(Isolation::Always, &one_step("true")).await;
    let app = lane.app();
    let change = lane
        .f
        .server
        .app
        .followed_change(&lane.card)
        .unwrap()
        .unwrap();
    let id = app
        .store
        .add_lane_job(
            lane.f.repository.id,
            lane.card.id,
            change.id,
            "gpu1",
            &lane.head,
            &lane.base,
            true,
            0,
        )
        .unwrap()
        .unwrap();
    assert!(app.store.claim_lane_job(id).unwrap());
    // What the server that stopped left: a container of the job and a process group of a direct command in its directory.
    let job_dir = lane.machine.join(format!("lanes/gpu1/jobs/{id}"));
    std::fs::create_dir_all(&job_dir).unwrap();
    let name = format!("moebius-lane-{id}-0");
    let mut container = std::process::Command::new(&lane.podman)
        .args([
            "run",
            "--name",
            &name,
            "--label",
            &format!("moebius-lane-job={id}"),
        ])
        .args([
            "-v",
            &format!("{}:/work:rw", job_dir.display()),
            "-w",
            "/work",
            "image",
            "sh",
            "-c",
            "sleep 300",
        ])
        .spawn()
        .unwrap();
    let mut direct = std::process::Command::new("sh")
        .args(["-c", "echo $$ > pid-1; exec sleep 300"])
        .current_dir(&job_dir)
        .process_group(0)
        .spawn()
        .unwrap();
    let there = format!("{}.running.{name}", lane.podman.display());
    let pid = pid_of(&job_dir.join("pid-1")).await;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !std::path::Path::new(&there).exists() {
        assert!(Instant::now() < deadline, "the container did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(alive(&pid));

    let app2 = app.clone();
    tokio::task::spawn_blocking(move || app2.recover_lane_jobs())
        .await
        .unwrap();
    container.wait().unwrap();
    direct.wait().unwrap();
    assert!(
        !std::path::Path::new(&there).exists(),
        "the container is still there"
    );
    assert!(!alive(&pid), "the direct command is still running");
    assert!(!job_dir.exists(), "the job's directory is still there");
    let asked = lane.container_calls().iter().any(|c| {
        c.first().map(String::as_str) == Some("ps")
            && c.join(" ")
                .contains(&format!("label=moebius-lane-job={id}"))
    });
    assert!(asked, "{:?}", lane.container_calls());
    assert_eq!(app.store.lane_job(id).unwrap().unwrap().state, "queued");
    assert!(app.store.lane_pause("gpu1").unwrap().is_none());

    // What cannot be ended pauses the lane: a directory the machine cannot remove.
    denying_rm(&lane);
    assert!(app.store.claim_lane_job(id).unwrap());
    std::fs::create_dir_all(job_dir.join("keep")).unwrap();
    std::fs::write(job_dir.join("keep/file"), "x").unwrap();
    std::fs::set_permissions(
        job_dir.join("keep"),
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .unwrap();
    let app3 = app.clone();
    tokio::task::spawn_blocking(move || app3.recover_lane_jobs())
        .await
        .unwrap();
    let pause = app.store.lane_pause("gpu1").unwrap().expect("a pause");
    assert!(
        pause.reason.contains("when the server stopped"),
        "{}",
        pause.reason
    );
    assert_eq!(app.store.lane_job(id).unwrap().unwrap().state, "queued");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_reason_a_lane_paused_names_a_card_only_to_those_who_see_it() {
    let lane = Lane::new(Isolation::Off, &one_step("true")).await;
    let app = lane.app();
    let secret = "3 jobs in a row ended because of the lane itself; the last: the step of SECRET-CARD said token";
    app.pause_lane_for_test("gpu1", secret, Some(lane.card.id));
    let card = app.card(lane.card.id).unwrap();
    let everyone = crate::app::Sight::everything();
    let hidden = crate::app::Sight::hiding(&[card.project_id]);
    let pause = |sight: &crate::app::Sight| {
        app.lane_views(sight)
            .unwrap()
            .remove(0)
            .paused
            .expect("paused")
    };
    assert_eq!(pause(&everyone).reason, secret);
    let seen = pause(&hidden);
    assert!(!seen.reason.contains("SECRET-CARD"), "{}", seen.reason);
    assert!(seen.reason.contains("stopped itself"), "{}", seen.reason);
    // What the API sends is that, and carries no card.
    let json = serde_json::to_string(&app.lane_views(&hidden).unwrap()).unwrap();
    assert!(
        !json.contains("SECRET-CARD") && !json.contains("card_id"),
        "{json}"
    );
    // The answer of a resume is the same list as seen by the one who resumes: a person who sees no card sees no reason.
    let listed = serde_json::to_string(
        &app.repository_lane_view(lane.f.repository.id, &hidden)
            .unwrap(),
    )
    .unwrap();
    assert!(!listed.contains("SECRET-CARD"), "{listed}");
    // A pause no card made is told as it is.
    app.resume_lane(&owner(), "gpu1", "").unwrap();
    app.pause_lane_for_test("gpu1", "the runner went away", None);
    assert_eq!(pause(&hidden).reason, "the runner went away");
}

const TOOLS: &str = "harness = [\"tools/kbench/\"]\n\
[[step]]\nname = \"measure\"\ncommand = \'\'\'\n\
if [ \"$LANE_SIDE\" = after ]; then mv tools t0; mkdir -p tools/kbench; echo 'echo \"MEASURE tps 999\"' > tools/kbench/run.sh; fi\n\
sh tools/kbench/run.sh\n\'\'\'\n\
[step.measure]\nruns = 3\nprefix = \"MEASURE \"\n";

#[tokio::test(flavor = "multi_thread")]
async fn a_harness_that_a_step_renamed_away_and_planted_again_is_not_a_measurement() {
    let lane = Lane::new(Isolation::Always, TOOLS).await;
    let base = commit_files(
        &lane.f,
        &lane.base,
        &[("tools/kbench/run.sh", "echo \"MEASURE tps 100\"\n")],
    );
    let head = commit_files(&lane.f, &base, &[("app.txt", "x\n")]);
    let job = lane.run_job_for(&head, &base, false).await;
    // In a container the rename of `tools` cannot be done at all (it is a mount point); and what was planted anyway is
    // found by the hash of the harness that is checked after every run.
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("harness"), "{}", job.summary);
    // The directory above the harness path is mounted over itself, before the read-only mount of the path.
    let call = lane.runs_in_containers().pop().expect("a container ran");
    let mounts: Vec<&String> = call
        .iter()
        .enumerate()
        .filter(|(at, _)| *at > 0 && call[*at - 1] == "-v")
        .map(|(_, a)| a)
        .collect();
    assert!(
        mounts[1].ends_with("/after/tools:/work/tools:rw"),
        "{mounts:?}"
    );
    assert!(
        mounts[2].ends_with("/harness/tools/kbench:/work/tools/kbench:ro"),
        "{mounts:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_link_where_a_directory_above_the_harness_is_makes_the_machine_touch_nothing_outside_the_job()
 {
    let definition = "harness = [\"ci/bench/\"]\n[[step]]\nname = \"check\"\ncommand = \"true\"\n";
    let lane = Lane::new(Isolation::Off, definition).await;
    let outside = lane.f._dir.path().join("outside");
    std::fs::create_dir_all(outside.join("bench")).unwrap();
    std::fs::write(outside.join("bench/victim.txt"), "keep me").unwrap();
    let base = commit_files(&lane.f, &lane.base, &[("ci/bench/run.sh", "true\n")]);
    let head = commit_files(
        &lane.f,
        &base,
        &[("ci", &format!("->{}", outside.display()))],
    );
    let job = lane.run_job_for(&head, &base, false).await;
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(
        job.summary
            .contains("a link or a file where the directory ci"),
        "{}",
        job.summary
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("bench/victim.txt")).unwrap(),
        "keep me",
        "the harness was removed or written through the link"
    );
    assert!(!outside.join("bench/run.sh").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_to_a_directory_above_the_harness_is_a_change_to_the_harness() {
    let harness = "harness = [\"src/kbench/\"]\n[[step]]\nname = \"check\"\ncommand = \"true\"\n";
    let lane = Lane::new(Isolation::Off, harness).await;
    // The pull request adds src/lib.rs: a file `src` would be the directory above src/kbench, here a file under it.
    let app = lane.app();
    let repository = app.repository(lane.f.repository.id).unwrap();
    let hit = |files: &[&str]| {
        let files: Vec<String> = files.iter().map(|f| f.to_string()).collect();
        crate::app::App::locked_hit_for_test(&repository, &files, &["/src/kbench/".to_string()])
    };
    assert!(
        hit(&["src"]).is_some(),
        "a file in the place of the directory above"
    );
    assert!(hit(&["src/other.rs"]).is_none());
    assert!(hit(&["src/kbench/run.sh"]).is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_row_joined_to_a_partial_line_or_lost_in_a_long_one_fails_the_step() {
    let joined = measured_with("echo 'MEASURE a 1 x'\nprintf X\necho 'MEASURE a 2 x'\n").await;
    assert_eq!(joined.state, "failed", "{joined:?}");
    assert!(joined.summary.contains("middle"), "{}", joined.summary);
    let long = measured_with("head -c 5000 /dev/zero | tr '\\0' y\necho 'MEASURE a 1 x'\n").await;
    assert_eq!(long.state, "failed", "{long:?}");
    assert!(long.summary.contains("too long"), "{}", long.summary);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_head_that_uses_the_total_time_up_is_blamed_when_the_merge_base_runs_out_after_it() {
    let measure = "[[step]]\nname = \"measure\"\ncommand = \'\'\'\n\
if [ \"$LANE_SIDE\" = after ]; then sleep 8; elif [ \"$LANE_RUN\" = 2 ]; then sleep 30; fi\n\
echo \"MEASURE tps 1\"\n\'\'\'\n[step.measure]\nruns = 3\nprefix = \"MEASURE \"\n";
    let lane = short_lane(measure, 12).await;
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("12 seconds"), "{}", job.summary);
    assert_eq!(
        lane.app().store.lane_infrastructure_streak("gpu1").unwrap(),
        0
    );
}

async fn fills_the_disk(isolation: Isolation) {
    let step = one_step(
        "i=0; while [ $i -lt 600 ]; do head -c 20000 /dev/zero > f$i; i=$((i+1)); done; sleep 40",
    );
    let lane = Lane::new(isolation, &step).await;
    let mut config = lane_config(isolation, lane.podman.to_str().unwrap());
    config.disk = "2m".into();
    lane.app()
        .lanes
        .configure(vec![config], vec![lane.runner()]);
    lane.code_approved().await;
    let began = Instant::now();
    lane.pass().await;
    // Many small files, none over a limit of its own: the watcher of the script stopped it long before the sleep ended.
    assert!(
        began.elapsed() < Duration::from_secs(35),
        "{:?}",
        began.elapsed()
    );
    let job = lane.the_job();
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("wrote more than"), "{}", job.summary);
    let left = std::fs::read_dir(lane.machine.join("lanes/gpu1/jobs"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(left, 0, "the files went with the job's directory");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_writes_many_small_files_is_stopped_in_a_container() {
    fills_the_disk(Isolation::Always).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_step_that_writes_many_small_files_is_stopped_on_the_machine() {
    fills_the_disk(Isolation::Off).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_runtime_that_ignores_the_limits_is_refused_before_anything_runs() {
    let lane = Lane::new(Isolation::Always, &one_step("echo ran > @DIR@/ran")).await;
    std::fs::write(format!("{}.nolimits", lane.podman.display()), "").unwrap();
    lane.code_approved().await;
    lane.pass().await;
    let job = lane.the_job();
    assert_eq!(job.state, "errored", "{job:?}");
    assert!(
        job.summary.contains("does not enforce the limits"),
        "{}",
        job.summary
    );
    assert!(job.summary.contains("memory.max"), "{}", job.summary);
    assert!(!lane.f._dir.path().join("ran").exists());
    // And with the limits in force the same job runs: the probe is told what the flags say.
    std::fs::remove_file(format!("{}.nolimits", lane.podman.display())).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn what_a_stopped_server_left_in_a_directory_of_a_side_is_removed_by_a_limited_container() {
    let lane = Lane::new(Isolation::Always, &one_step("true")).await;
    denying_rm(&lane);
    let app = lane.app();
    let change = app.followed_change(&lane.card).unwrap().unwrap();
    let id = app
        .store
        .add_lane_job(
            lane.f.repository.id,
            lane.card.id,
            change.id,
            "gpu1",
            &lane.head,
            &lane.base,
            true,
            0,
        )
        .unwrap()
        .unwrap();
    assert!(app.store.claim_lane_job(id).unwrap());
    let job_dir = lane.machine.join(format!("lanes/gpu1/jobs/{id}"));
    for hidden in ["after/keep", "after/..keep"] {
        std::fs::create_dir_all(job_dir.join(hidden)).unwrap();
        std::fs::write(job_dir.join(hidden).join("file"), "x").unwrap();
    }
    std::fs::set_permissions(
        job_dir.join("after/keep"),
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .unwrap();
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || app2.recover_lane_jobs())
        .await
        .unwrap();
    assert!(!job_dir.exists(), "the directory is still there");
    assert!(app.store.lane_pause("gpu1").unwrap().is_none());
    assert_eq!(app.store.lane_job(id).unwrap().unwrap().state, "queued");
    let clean = lane
        .container_calls()
        .into_iter()
        .find(|c| c.iter().any(|a| a.contains("-clean-after")))
        .expect("a cleanup container");
    let text = clean.join(" ");
    for part in [
        format!("--label moebius-lane-job={id}"),
        "--pids-limit 256".to_string(),
        "--memory 16g".to_string(),
        "--network none".to_string(),
        "--cap-drop ALL".to_string(),
    ] {
        assert!(text.contains(&part), "{part} in {text}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn what_the_head_left_on_the_disk_trips_the_next_command_of_the_merge_base_and_that_is_the_changes()
 {
    let steps = "[[step]]\nname = \"fill\"\ncommand = 'head -c 1500000 /dev/zero > big'\n\
[[step]]\nname = \"check\"\non = \"both\"\ncommand = 'head -c 1000000 /dev/zero > more; sleep 30'\n";
    let lane = Lane::new(Isolation::Off, steps).await;
    let mut config = lane_config(Isolation::Off, "podman");
    config.disk = "2m".into();
    lane.app()
        .lanes
        .configure(vec![config], vec![lane.runner()]);
    lane.code_approved().await;
    let began = Instant::now();
    lane.pass().await;
    assert!(
        began.elapsed() < Duration::from_secs(40),
        "{:?}",
        began.elapsed()
    );
    // The merge base's command wrote 1 MB beside the head's 1.5 MB, in the directory of one job.
    let job = lane.the_job();
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(
        job.summary.contains("left the job's disk full"),
        "{}",
        job.summary
    );
    assert_eq!(
        lane.app().store.lane_infrastructure_streak("gpu1").unwrap(),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_directory_the_watcher_cannot_measure_counts_as_over_the_limit() {
    let lane = Lane::new(Isolation::Off, &one_step("sleep 30")).await;
    // A `du` that fails, as one does on a directory it cannot read.
    let shim = lane.f._dir.path().join("failing-du");
    std::fs::create_dir_all(&shim).unwrap();
    std::fs::write(
        shim.join("du"),
        "#!/bin/sh\necho 'du: cannot read directory' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(
        shim.join("du"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let path = std::env::var("PATH").unwrap_or_default();
    lane.app()
        .lanes
        .set_test_environment(vec![("PATH".into(), format!("{}:{path}", shim.display()))]);
    lane.code_approved().await;
    let began = Instant::now();
    lane.pass().await;
    assert!(
        began.elapsed() < Duration::from_secs(25),
        "{:?}",
        began.elapsed()
    );
    let job = lane.the_job();
    assert_eq!(job.state, "failed", "{job:?}");
    assert!(job.summary.contains("wrote more than"), "{}", job.summary);
}
