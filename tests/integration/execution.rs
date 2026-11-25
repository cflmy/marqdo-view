//! Execution controller tests: one explicit request, one Core subprocess, one
//! unique terminal state, SSE-ready events, dedupe, grants and cancellation.
//!
//! The fake Core is a real executable (never a mocked trait) so argv, process
//! lifecycle and the Observation v1 stdout/snapshot destinations are exercised at
//! the process boundary.

use crate::common::{sha256_hex, FakeCore};
use marqdo_view::execution::controller::{ExecutionConfig, ExecutionController, SseSubscription};
use marqdo_view::execution::events::RunState;
use marqdo_view::reader::assemble::DocumentService;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ANALYSIS: &str = "tests/golden/analysis-v3/control-flow.json";
const OBS_JSONL: &str = "tests/golden/observation-v1/control-flow.jsonl";
const OBS_SNAPSHOT: &str = "tests/golden/observation-v1/control-flow.snapshot.json";
const OBS_TRUNCATED_JSONL: &str = "tests/golden/observation-v1/control-flow.truncated.jsonl";
const OBS_TRUNCATED_SNAPSHOT: &str =
    "tests/golden/observation-v1/control-flow.truncated.snapshot.json";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn golden(name: &str) -> String {
    let path = repo_root().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {path:?}: {err}"))
}

fn analysis_digest(analysis: &str) -> String {
    format!("sha256:{}", sha256_hex(analysis.as_bytes()))
}

struct Fixture {
    core: FakeCore,
    controller: Arc<ExecutionController>,
    _service: Arc<DocumentService>,
}

fn fixture_with(
    name: &str,
    project_relative: &str,
    analysis: &str,
    observation_jsonl: &str,
    observation_snapshot: Option<&str>,
    config: ExecutionConfig,
) -> Fixture {
    let core = FakeCore::new(name);
    core.set_analysis(analysis);
    core.set_check_json(r#"{"command":"check","diagnostics":[],"errors":0,"success":true}"#);
    core.set_observation(0, observation_jsonl, observation_snapshot);
    let project = repo_root().join(project_relative);
    let service = Arc::new(
        DocumentService::open(&project, Some(core.script.as_path()))
            .expect("open document service"),
    );
    let controller = Arc::new(ExecutionController::new(Arc::clone(&service), config));
    Fixture {
        core,
        controller,
        _service: service,
    }
}

fn control_flow(name: &str) -> Fixture {
    fixture_with(
        name,
        "examples/control-flow",
        &golden(ANALYSIS),
        &golden(OBS_JSONL),
        Some(&golden(OBS_SNAPSHOT)),
        ExecutionConfig::default().with_timeout(10),
    )
}

fn request_body(
    analysis: &str,
    target: serde_json::Value,
    mode: &str,
    grants: serde_json::Value,
    client_request_id: &str,
) -> Vec<u8> {
    serde_json::json!({
        "document": "index.mq.md",
        "target": target,
        "analysis_digest": analysis_digest(analysis),
        "mode": mode,
        "grants": grants,
        "client_request_id": client_request_id,
    })
    .to_string()
    .into_bytes()
}

fn frame_data(frame: &str) -> Option<serde_json::Value> {
    frame
        .lines()
        .find(|line| line.starts_with("data: "))
        .and_then(|line| serde_json::from_str(line.trim_start_matches("data: ")).ok())
}

fn is_terminal_data(data: &serde_json::Value) -> bool {
    data["state"]
        .as_str()
        .map(|state| {
            matches!(
                state,
                "succeeded" | "failed" | "cancelled" | "timed_out" | "unavailable" | "incomplete"
            )
        })
        .unwrap_or(false)
}

fn terminal_events(subscription: SseSubscription) -> (Vec<serde_json::Value>, serde_json::Value) {
    let mut parsed = subscription
        .replay
        .iter()
        .filter_map(|frame| frame_data(frame))
        .collect::<Vec<serde_json::Value>>();
    let mut terminal = parsed
        .iter()
        .rev()
        .find(|data| is_terminal_data(data))
        .cloned();
    let deadline = Instant::now() + Duration::from_secs(20);
    while terminal.is_none() && Instant::now() < deadline {
        match subscription
            .receiver
            .recv_timeout(Duration::from_millis(250))
        {
            Ok(event) => {
                let data = event.data(&subscription.run_id);
                if event.state.is_terminal() {
                    terminal = Some(data.clone());
                }
                parsed.push(data);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if subscription.terminal {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    (parsed, terminal.expect("terminal event"))
}

fn wait_for_file(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if path.is_file() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {path:?}");
}

#[test]
fn explicit_entry_run_streams_accepted_running_and_verified_observation() {
    let fixture = control_flow("ctrl-success");
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-success",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    assert!(!accepted.reused);
    assert_eq!(accepted.run_id.len(), 32);
    assert_eq!(
        accepted.events_url,
        format!("/api/runs/{}/events", accepted.run_id)
    );

    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (events, terminal) = terminal_events(subscription);
    let seqs: Vec<u64> = events
        .iter()
        .map(|event| event["seq"].as_u64().unwrap())
        .collect();
    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "seqs: {seqs:?}"
    );
    let states: Vec<String> = events
        .iter()
        .map(|event| event["state"].as_str().unwrap().to_string())
        .collect();
    assert!(
        states.contains(&"accepted".to_string()),
        "states: {states:?}"
    );
    assert!(
        states.contains(&"running".to_string()),
        "states: {states:?}"
    );
    assert_eq!(terminal["state"], "succeeded");
    assert_eq!(terminal["result"]["success"], true);
    assert_eq!(terminal["result"]["observation_status"], "verified");
    let summary = terminal["observation_summary"]
        .as_object()
        .expect("observation summary");
    assert_eq!(summary["status"], "verified");
    assert!(!summary["events"].as_array().unwrap().is_empty());
    assert_eq!(terminal["result"]["stdout"], "0\n0\n2\n");
    let argv = fixture.core.last_args();
    assert!(argv.contains(&"--output".to_string()), "argv: {argv:?}");
    assert!(argv.contains(&"jsonl".to_string()), "argv: {argv:?}");
    assert!(
        argv.contains(&"--observation-file".to_string()),
        "argv: {argv:?}"
    );
}

#[test]
fn call_target_uses_the_call_subcommand_and_function_name() {
    let fixture = control_flow("ctrl-call");
    let analysis = golden(ANALYSIS);
    let function_id = "root:control-flow@0.1.0::index.mq.md::演示.main";
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "function", "function_id": function_id}),
        "pure",
        serde_json::json!({}),
        "req-call",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (_events, terminal) = terminal_events(subscription);
    assert_eq!(terminal["state"], "succeeded");
    let argv = fixture.core.last_args();
    assert!(argv.iter().any(|arg| arg == "call"), "argv: {argv:?}");
    assert!(
        argv.iter().any(|arg| arg.contains("main")),
        "argv: {argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| arg == "--trace-file"),
        "argv: {argv:?}"
    );
}

#[test]
fn same_client_request_id_is_idempotent_and_different_payload_conflicts() {
    let fixture = control_flow("ctrl-dedupe");
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-dedupe",
    );
    let first = fixture
        .controller
        .submit("session-a", &body)
        .expect("first accepted");
    let second = fixture
        .controller
        .submit("session-a", &body)
        .expect("second accepted");
    assert_eq!(first.run_id, second.run_id);
    assert!(second.reused);

    let mut changed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    changed["mode"] = serde_json::json!("project");
    let err = fixture
        .controller
        .submit("session-a", changed.to_string().as_bytes())
        .expect_err("conflicting payload must be rejected");
    assert_eq!(err.code(), "view.run_conflict");
}

#[test]
fn second_run_is_busy_while_one_is_active_and_cancel_is_idempotent() {
    let fixture = control_flow("ctrl-cancel");
    fixture.core.set_sleep_seconds(30);
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-cancel",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    wait_for_file(&fixture.core.dir.join("run-was-called"));

    let other_body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-cancel-other",
    );
    let busy = fixture
        .controller
        .submit("session-a", &other_body)
        .expect_err("second run must be busy");
    assert_eq!(busy.code(), "view.run_busy");

    let started = Instant::now();
    let state = fixture
        .controller
        .cancel("session-a", &accepted.run_id)
        .expect("cancel");
    assert!(matches!(
        state,
        RunState::CancelRequested | RunState::Cancelled
    ));
    let _ = fixture
        .controller
        .cancel("session-a", &accepted.run_id)
        .expect("second cancel");

    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (events, terminal) = terminal_events(subscription);
    assert_eq!(terminal["state"], "cancelled");
    let terminal_count = events
        .iter()
        .filter(|event| is_terminal_data(event))
        .count();
    assert_eq!(terminal_count, 1, "exactly one terminal state: {events:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn cross_session_observation_is_rejected() {
    let fixture = control_flow("ctrl-session");
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-session",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    assert_eq!(
        fixture
            .controller
            .subscribe("session-b", &accepted.run_id, None)
            .unwrap_err()
            .code(),
        "view.run_not_found"
    );
    assert_eq!(
        fixture
            .controller
            .cancel("session-b", &accepted.run_id)
            .unwrap_err()
            .code(),
        "view.run_not_found"
    );
}

#[test]
fn stale_request_digest_is_rejected_before_any_process_starts() {
    let fixture = control_flow("ctrl-stale-request");
    let analysis = golden(ANALYSIS);
    let mut body: serde_json::Value = serde_json::from_slice(&request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-stale",
    ))
    .unwrap();
    body["analysis_digest"] = serde_json::json!(format!("sha256:{}", "0".repeat(64)));
    let err = fixture
        .controller
        .submit("session-a", body.to_string().as_bytes())
        .expect_err("stale digest must be rejected");
    assert_eq!(err.code(), "view.run_invalid");
    assert!(!fixture.core.run_was_called());
}

#[test]
fn project_grants_are_mapped_to_explicit_cli_flags_and_pure_grants_are_rejected() {
    let fixture = control_flow("ctrl-grants");
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "project",
        serde_json::json!({"read": ["data/**"], "write": ["out/*.txt"]}),
        "req-project",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (_events, terminal) = terminal_events(subscription);
    assert_eq!(terminal["state"], "succeeded");
    let argv = fixture.core.last_args();
    assert!(
        argv.windows(2).any(|pair| pair == ["--mode", "project"]),
        "argv: {argv:?}"
    );
    assert!(
        argv.windows(2)
            .any(|pair| pair == ["--allow-read", "data/**"]),
        "argv: {argv:?}"
    );
    assert!(
        argv.windows(2)
            .any(|pair| pair == ["--allow-write", "out/*.txt"]),
        "argv: {argv:?}"
    );

    let pure_with_grants = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({"read": ["data/**"]}),
        "req-pure-grants",
    );
    let err = fixture
        .controller
        .submit("session-a", &pure_with_grants)
        .expect_err("pure grants must be rejected");
    assert_eq!(err.code(), "view.run_invalid");
}

#[test]
fn missing_observation_stream_is_incomplete_never_success() {
    let fixture = control_flow("ctrl-no-observation");
    fixture
        .core
        .set_observation(0, "", Some(&golden(OBS_SNAPSHOT)));
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-no-observation",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (_events, terminal) = terminal_events(subscription);
    assert_eq!(terminal["state"], "failed");
    assert_eq!(
        terminal["result"]["error"]["code"],
        "view.observation_invalid"
    );
    assert_ne!(terminal["result"]["success"], true);
}

#[test]
fn source_edit_during_a_run_marks_the_result_stale_and_drops_anchor_ids() {
    let project = crate::common::TempProject::new("ctrl-stale-source");
    project.marqdo_project(&golden("examples/control-flow/index.mq.md"));
    let analysis = golden(ANALYSIS);
    let core = FakeCore::new("ctrl-stale-source");
    core.set_analysis(&analysis);
    core.set_check_json(r#"{"command":"check","diagnostics":[],"errors":0,"success":true}"#);
    core.set_observation(0, &golden(OBS_JSONL), Some(&golden(OBS_SNAPSHOT)));
    core.set_sleep_seconds(1);
    let service = Arc::new(
        DocumentService::open(&project.dir, Some(core.script.as_path())).expect("open service"),
    );
    let controller = Arc::new(ExecutionController::new(
        Arc::clone(&service),
        ExecutionConfig::default().with_timeout(10),
    ));
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-stale-source",
    );
    let accepted = controller.submit("session-a", &body).expect("accepted");
    wait_for_file(&core.dir.join("run-was-called"));
    project.write(
        "index.mq.md",
        &format!(
            "{}\n\n<!-- changed -->\n",
            golden("examples/control-flow/index.mq.md")
        ),
    );
    let subscription = controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (_events, terminal) = terminal_events(subscription);
    assert_eq!(terminal["state"], "failed");
    assert_eq!(
        terminal["result"]["error"]["code"],
        "view.observation_stale"
    );
    assert_eq!(terminal["result"]["observation_status"], "stale");
    assert_ne!(terminal["result"]["success"], true);
    let summary = terminal["observation_summary"]
        .as_object()
        .expect("stale summary");
    assert_eq!(summary["status"], "stale");
    assert!(summary["events"].as_array().unwrap().is_empty());
}

#[test]
fn observation_with_a_mismatched_digest_is_rejected_as_stale() {
    let fixture = control_flow("ctrl-observation-mismatch");
    let mut lines: Vec<String> = golden(OBS_JSONL).lines().map(str::to_string).collect();
    for line in &mut lines {
        if line.contains("\"kind\":\"run.started\"") {
            *line = line.replace(
                &format!(
                    "\"analysis_digest\":\"sha256:{}\"",
                    sha256_hex(&golden(ANALYSIS).into_bytes())
                ),
                &format!("\"analysis_digest\":\"sha256:{}\"", "0".repeat(64)),
            );
        }
    }
    fixture.core.set_observation(
        0,
        &format!("{}\n", lines.join("\n")),
        Some(&golden(OBS_SNAPSHOT)),
    );
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-observation-mismatch",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (_events, terminal) = terminal_events(subscription);
    assert_eq!(terminal["state"], "failed");
    assert_eq!(
        terminal["result"]["error"]["code"],
        "view.observation_stale"
    );
    assert_ne!(terminal["result"]["observation_status"], "verified");
}

#[test]
fn truncated_observation_is_explicit_and_never_labelled_complete() {
    let fixture = fixture_with(
        "ctrl-truncated",
        "examples/control-flow",
        &golden(ANALYSIS),
        &golden(OBS_TRUNCATED_JSONL),
        Some(&golden(OBS_TRUNCATED_SNAPSHOT)),
        ExecutionConfig::default().with_timeout(10),
    );
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-truncated",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, Some(0))
        .expect("subscribe");
    let (_events, terminal) = terminal_events(subscription);
    assert_eq!(terminal["state"], "succeeded");
    assert_eq!(terminal["result"]["observation_status"], "truncated");
    assert_eq!(terminal["result"]["observation_truncated"], true);
    assert_eq!(terminal["result"]["output_complete"], false);
    let summary = terminal["observation_summary"]
        .as_object()
        .expect("observation summary");
    assert_eq!(summary["state"], "truncated");
    assert_eq!(summary["truncated"]["reason"], "event_budget");
    assert_eq!(summary["truncated"]["limit"], 5);
}

#[test]
fn event_frames_use_the_view_run_events_schema_and_numeric_ids() {
    let fixture = control_flow("ctrl-events");
    let analysis = golden(ANALYSIS);
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-events",
    );
    let accepted = fixture
        .controller
        .submit("session-a", &body)
        .expect("accepted");
    let subscription = fixture
        .controller
        .subscribe("session-a", &accepted.run_id, None)
        .expect("subscribe");
    let (events, terminal) = terminal_events(subscription);
    assert!(events.iter().all(|event| {
        event["schema"] == "marqdo.view-run-events" && event["schema_version"] == 1
    }));
    assert!(events
        .iter()
        .all(|event| event["run_id"] == accepted.run_id));
    assert!(events.iter().all(|event| event["seq"].as_u64().is_some()));
    let observation_frames = events
        .iter()
        .filter(|event| event.get("observation").is_some())
        .count();
    assert!(observation_frames > 0, "events: {events:?}");
    assert_eq!(terminal["state"], "succeeded");
}
