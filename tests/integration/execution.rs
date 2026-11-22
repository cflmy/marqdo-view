//! Execution controller tests: one explicit request, one Core subprocess, one
//! unique terminal state, SSE-ready events, dedupe, grants and cancellation.
//!
//! The fake Core is a real executable (never a mocked trait) so argv, process
//! lifecycle and trace files are exercised at the process boundary.

use crate::common::{sha256_hex, FakeCore};
use marqdo_view::execution::controller::{ExecutionConfig, ExecutionController, SseSubscription};
use marqdo_view::execution::events::RunState;
use marqdo_view::reader::assemble::DocumentService;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn golden(name: &str) -> String {
    let path = repo_root().join("tests/golden/analysis-v2").join(name);
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
    run_json: &str,
    trace: Option<&str>,
    config: ExecutionConfig,
) -> Fixture {
    let core = FakeCore::new(name);
    core.set_analysis(analysis);
    core.set_check_json(r#"{"command":"check","diagnostics":[],"errors":0,"success":true}"#);
    core.set_run(0, run_json, trace);
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
    let analysis = golden("control-flow.json");
    let run =
        std::fs::read_to_string(repo_root().join("tests/golden/trace-v2/control-flow.run.json"))
            .expect("run golden");
    let trace =
        std::fs::read_to_string(repo_root().join("tests/golden/trace-v2/control-flow.trace.json"))
            .expect("trace golden");
    fixture_with(
        name,
        "examples/control-flow",
        &analysis,
        &run,
        Some(&trace),
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
                "succeeded" | "failed" | "cancelled" | "timed_out" | "unavailable"
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
fn explicit_entry_run_streams_accepted_running_and_succeeded_with_verified_trace() {
    let fixture = control_flow("ctrl-success");
    let analysis = golden("control-flow.json");
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
    assert_eq!(terminal["trace"]["status"], "verified");
    assert!(!terminal["trace"]["events"].as_array().unwrap().is_empty());
    assert_eq!(terminal["trace"]["stale"], false);
    // The fake Core's stderr/stdout were read through the adapter, not guessed.
    assert_eq!(terminal["result"]["stdout"], "0\n0\n2\n");
}

#[test]
fn call_target_uses_the_call_subcommand_and_function_name() {
    let fixture = control_flow("ctrl-call");
    let analysis = golden("control-flow.json");
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
    let argv = std::fs::read_to_string(fixture.core.dir.join("argv.txt")).unwrap_or_default();
    assert!(argv.contains("arg=call"), "argv: {argv}");
    assert!(argv.contains("arg=main"), "argv: {argv}");
}

#[test]
fn same_client_request_id_is_idempotent_and_different_payload_conflicts() {
    let fixture = control_flow("ctrl-dedupe");
    let analysis = golden("control-flow.json");
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
    let analysis = golden("control-flow.json");
    let run = r#"{"command":"run","success":true,"value":{"type":"int","decimal":"1"}}"#;
    let fixture = fixture_with(
        "ctrl-cancel",
        "examples/control-flow",
        &analysis,
        run,
        None,
        ExecutionConfig::default().with_timeout(30),
    );
    fixture.core.set_sleep_seconds(30);
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
    // Repeat cancel is idempotent.
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
        .filter(|event| {
            event["state"]
                .as_str()
                .map(|state| {
                    matches!(
                        state,
                        "succeeded" | "failed" | "cancelled" | "timed_out" | "unavailable"
                    )
                })
                .unwrap_or(false)
        })
        .count();
    assert_eq!(terminal_count, 1, "exactly one terminal state: {events:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn cross_session_observation_is_rejected() {
    let fixture = control_flow("ctrl-session");
    let analysis = golden("control-flow.json");
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
fn stale_analysis_digest_is_rejected_before_any_process_starts() {
    let fixture = control_flow("ctrl-stale");
    let analysis = golden("control-flow.json");
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
    assert!(!fixture.core.dir.join("run-was-called").exists());
}

#[test]
fn project_grants_are_mapped_to_explicit_cli_flags_and_pure_grants_are_rejected() {
    let fixture = control_flow("ctrl-grants");
    let analysis = golden("control-flow.json");
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
    let argv = std::fs::read_to_string(fixture.core.dir.join("argv.txt")).unwrap_or_default();
    assert!(argv.contains("arg=--mode\narg=project"), "argv: {argv}");
    assert!(
        argv.contains("arg=--allow-read\narg=data/**"),
        "argv: {argv}"
    );
    assert!(
        argv.contains("arg=--allow-write\narg=out/*.txt"),
        "argv: {argv}"
    );
    assert!(!argv.contains("arg=--mode\narg=pure"));

    // pure with grants is rejected by request validation before the Core runs.
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
fn run_without_a_trace_file_succeeds_but_reports_unavailable_trace() {
    let analysis = golden("control-flow.json");
    let run = r#"{"command":"run","success":true,"value":{"type":"int","decimal":"7"},"stdout":"hi\n","stderr":"","diagnostics":[]}"#;
    let fixture = fixture_with(
        "ctrl-no-trace",
        "examples/control-flow",
        &analysis,
        run,
        None,
        ExecutionConfig::default().with_timeout(10),
    );
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-no-trace",
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
    assert_eq!(terminal["result"]["value_text"], "Int(7)");
    assert_eq!(terminal["trace"], serde_json::Value::Null);
    assert_eq!(terminal["result"]["trace_status"], "unavailable");
    assert!(!fixture.core.dir.join("trace.json").exists() || terminal["trace"].is_null());
}

#[test]
fn event_frames_use_the_view_run_events_schema_and_numeric_ids() {
    let fixture = control_flow("ctrl-frames");
    let analysis = golden("control-flow.json");
    let body = request_body(
        &analysis,
        serde_json::json!({"kind": "entry"}),
        "pure",
        serde_json::json!({}),
        "req-frames",
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
    assert_eq!(terminal["schema"], "marqdo.view-run-events");
    assert_eq!(terminal["schema_version"], 1);
    assert_eq!(terminal["run_id"], accepted.run_id.as_str());
    assert!(!events.is_empty());
}
