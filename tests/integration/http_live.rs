//! Live HTTP/SSE integration tests over a raw HTTP client.
//!
//! The server is a real child process, the fake Core is a real executable and
//! every assertion goes through real cookies, CSRF headers, Origin checks and
//! the chunked `text/event-stream` transport.

use crate::common::http::{
    self, collect_until_terminal, open_session, post_headers, run_body, submit, SseFrame,
    TestServer,
};
use std::time::{Duration, Instant};

const OBS_JSONL: &str = "tests/golden/observation-v1/control-flow.jsonl";
const OBS_SNAPSHOT: &str = "tests/golden/observation-v1/control-flow.snapshot.json";
const ANALYSIS_GOLDEN: &str = "tests/golden/analysis-v3/control-flow.json";

fn control_flow() -> TestServer {
    http::start(
        "examples/control-flow",
        ANALYSIS_GOLDEN,
        Some((OBS_JSONL, OBS_SNAPSHOT)),
    )
}

fn welcome() -> TestServer {
    http::start(
        "examples/welcome",
        "tests/golden/analysis-v3/welcome.json",
        None,
    )
}

#[test]
fn pages_issue_a_session_cookie_and_never_execute_the_document() {
    let server = welcome();
    let (session, page) = open_session(&server, "/");
    assert_eq!(page.status, 200);
    assert!(page.headers.contains("Content-Type: text/html"));
    assert!(page.headers.contains("Content-Security-Policy"));
    assert!(page.headers.contains("default-src 'self'"));
    assert!(page.headers.contains("connect-src 'self'"));
    assert!(page.body.contains("mq-reader"));
    assert!(page.body.contains("data-live=\"true\""));
    assert!(page
        .body
        .contains(&format!("data-analysis-digest=\"{}\"", session.digest)));
    assert!(page.body.contains("mq-csrf-token"));
    assert!(page.body.contains("data-action=\"run\""));
    assert!(page.body.contains("mq-dialog-run\""));
    assert!(!page.body.contains("/api/run\""));

    // Repeated page loads with the same cookie must never run the document.
    for _ in 0..10 {
        let repeated = http::request(
            server.port,
            "GET",
            "/",
            &[("Cookie", session.cookie.clone())],
            None,
        );
        assert_eq!(repeated.status, 200);
    }
    assert!(!server.core.dir.join("run-was-called").exists());
}

#[test]
fn read_only_routes_are_the_target_contract_and_old_routes_are_gone() {
    let server = welcome();
    let documents = http::get(server.port, "/api/documents");
    assert_eq!(documents.status, 200);
    let body = documents.json();
    assert_eq!(body["schema"], "marqdo.view-documents");
    assert_eq!(body["schema_version"], 1);
    assert!(body["documents"].as_array().unwrap().len() >= 2);
    assert!(body["note"].as_str().unwrap().contains("never executes"));

    for stale in ["/api/health", "/api/document?path=index.mq.md", "/api/run"] {
        let response = http::get(server.port, stale);
        assert_eq!(response.status, 404, "{stale} must be gone");
    }
    let run_get = http::get(server.port, "/api/runs/deadbeef/events");
    assert_eq!(run_get.status, 401, "SSE requires a session");
}

#[test]
fn assets_are_immutable_and_carry_strict_headers() {
    let server = welcome();
    let (_, page) = open_session(&server, "/");
    let css_path = page
        .body
        .split("href=\"/assets/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("css link");
    let css = http::get(server.port, &format!("/assets/{css_path}"));
    assert_eq!(css.status, 200);
    assert!(css.headers.contains("text/css"));
    assert!(css.headers.contains("immutable"));
    assert!(css.headers.contains("X-Content-Type-Options: nosniff"));

    let missing = http::get(server.port, "/assets/nope.css");
    assert_eq!(missing.status, 404);
}

#[test]
fn explicit_run_streams_sse_accepted_running_and_terminal_with_verified_trace() {
    let server = control_flow();
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-sse",
    );
    let accepted = submit(&server, &session, &body);
    assert_eq!(accepted.status, 202, "body: {}", accepted.body);
    let accepted = accepted.json();
    let run_id = accepted["run_id"].as_str().expect("run id").to_string();
    assert_eq!(accepted["state"], "accepted");
    assert_eq!(accepted["events_url"], format!("/api/runs/{run_id}/events"));
    assert_eq!(accepted["cancel_url"], format!("/api/runs/{run_id}/cancel"));
    assert_eq!(accepted["reused"], false);

    let mut reader = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[
            ("Cookie", session.cookie.clone()),
            ("Last-Event-ID", "0".to_string()),
        ],
    );
    assert_eq!(reader.status, 200);
    assert!(reader
        .header("Content-Type")
        .unwrap()
        .contains("text/event-stream"));
    assert!(reader.header("Cache-Control").unwrap().contains("no-store"));
    assert_eq!(
        reader.header("Transfer-Encoding").as_deref(),
        Some("chunked")
    );
    let frames = collect_until_terminal(&mut reader);
    assert!(
        reader.next_frame().is_none(),
        "terminal frame completes the HTTP response"
    );
    assert!(!frames.is_empty());
    let mut ids = frames
        .iter()
        .filter_map(|frame| frame.id)
        .collect::<Vec<_>>();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "ids: {ids:?}");
    ids.dedup();
    let terminal = frames.last().expect("terminal frame");
    let data = terminal.data.as_ref().expect("terminal data");
    assert_eq!(data["schema"], "marqdo.view-run-events");
    assert_eq!(data["schema_version"], 1);
    assert_eq!(data["run_id"], run_id.as_str());
    assert_eq!(data["state"], "succeeded");
    assert_eq!(data["result"]["success"], true);
    assert_eq!(data["observation_summary"]["status"], "verified");
    assert!(!data["observation_summary"]["events"]
        .as_array()
        .unwrap()
        .is_empty());
    let states: Vec<_> = frames
        .iter()
        .filter_map(|frame| frame.data.as_ref().and_then(|data| data["state"].as_str()))
        .collect();
    assert!(frame_state(&frames, "accepted"), "states: {states:?}");
    assert!(frame_state(&frames, "running"), "states: {states:?}");
    assert!(server.core.dir.join("run-was-called").exists());
}

fn frame_state(frames: &[SseFrame], wanted: &str) -> bool {
    frames
        .iter()
        .any(|frame| frame.data.as_ref().and_then(|data| data["state"].as_str()) == Some(wanted))
}

#[test]
fn last_event_id_replays_only_newer_events_and_old_cursor_gets_snapshot() {
    let server = control_flow();
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-replay",
    );
    let accepted = submit(&server, &session, &body).json();
    let run_id = accepted["run_id"].as_str().unwrap().to_string();
    let mut reader = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[("Cookie", session.cookie.clone())],
    );
    let frames = collect_until_terminal(&mut reader);
    let first_seq = frames.first().and_then(|frame| frame.id).unwrap_or(0);
    assert!(first_seq >= 1);

    // Reconnect with a valid cursor inside the buffer: only newer events.
    let mut reader = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[
            ("Cookie", session.cookie.clone()),
            ("Last-Event-ID", first_seq.to_string()),
        ],
    );
    let replay = collect_until_terminal(&mut reader);
    assert!(replay.iter().all(|frame| frame.id.unwrap_or(0) > first_seq));
    assert_eq!(
        replay.last().unwrap().data.as_ref().unwrap()["state"],
        "succeeded"
    );

    // A forged/too-new cursor gets an authoritative snapshot, never fake history.
    let mut reader = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[
            ("Cookie", session.cookie.clone()),
            ("Last-Event-ID", "999999".to_string()),
        ],
    );
    let snapshot = reader.next_frame().expect("snapshot frame");
    assert_eq!(snapshot.event, "run.snapshot");
    let data = snapshot.data.as_ref().unwrap();
    assert_eq!(data["run_id"], run_id.as_str());
    assert!(data["seq"].as_u64().unwrap() > 0);
}

#[test]
fn duplicate_request_id_is_idempotent_and_busy_or_conflict_never_starts_a_second_process() {
    let server = control_flow();
    server.core.set_sleep_seconds(2);
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-dedupe-http",
    );
    let first = submit(&server, &session, &body).json();
    let second = submit(&server, &session, &body).json();
    assert_eq!(first["run_id"], second["run_id"]);
    assert_eq!(second["reused"], true);

    let other = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-busy-http",
    );
    let busy = submit(&server, &session, &other);
    assert_eq!(busy.status, 409);
    assert_eq!(busy.json()["code"], "view.run_busy");

    let mut changed: serde_json::Value = serde_json::from_str(&body).unwrap();
    changed["mode"] = serde_json::json!("project");
    let conflict = submit(&server, &session, changed.to_string().as_str());
    assert_eq!(conflict.status, 409);
    assert_eq!(conflict.json()["code"], "view.run_conflict");

    // Cancel the active job so the test process does not leave a child behind.
    let cancel = http::post(
        server.port,
        first["cancel_url"].as_str().unwrap(),
        &post_headers(&server, &session, false),
        None,
    );
    assert_eq!(cancel.status, 200);
}

#[test]
fn cancel_is_idempotent_and_yields_one_terminal_state() {
    let server = http::start_with_env(
        "examples/control-flow",
        ANALYSIS_GOLDEN,
        Some((OBS_JSONL, OBS_SNAPSHOT)),
        &[("MARQDO_VIEW_SSE_HEARTBEAT_SECS", "1")],
    );
    server.core.set_sleep_seconds(10);
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-cancel-http",
    );
    let accepted = submit(&server, &session, &body).json();
    let run_id = accepted["run_id"].as_str().unwrap().to_string();
    let mut reader = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[("Cookie", session.cookie.clone())],
    );
    // Read at least one state so the job is observed as running or accepted.
    let first = reader.next_frame().expect("first frame");
    assert!(first.data.is_some());

    let started = Instant::now();
    let cancel = http::post(
        server.port,
        &format!("/api/runs/{run_id}/cancel"),
        &post_headers(&server, &session, false),
        None,
    );
    eprintln!(
        "CANCEL_DEBUG first={:?} cancel={:?} elapsed={:?}",
        first,
        cancel.json(),
        started.elapsed()
    );
    assert_eq!(cancel.status, 200);
    let repeat = http::post(
        server.port,
        &format!("/api/runs/{run_id}/cancel"),
        &post_headers(&server, &session, false),
        None,
    );
    assert_eq!(repeat.status, 200);

    let frames = collect_until_terminal(&mut reader);
    let terminal_count = frames
        .iter()
        .filter(|frame| {
            frame
                .data
                .as_ref()
                .and_then(|data| data["state"].as_str())
                .map(|state| {
                    matches!(
                        state,
                        "succeeded"
                            | "failed"
                            | "cancelled"
                            | "timed_out"
                            | "unavailable"
                            | "incomplete"
                    )
                })
                .unwrap_or(false)
        })
        .count();
    assert_eq!(terminal_count, 1);
    assert_eq!(
        frames.last().unwrap().data.as_ref().unwrap()["state"],
        "cancelled"
    );
    assert!(started.elapsed() < Duration::from_secs(8));
}

#[test]
fn sse_heartbeats_keep_a_slow_job_connected() {
    let server = http::start_with_env(
        "examples/control-flow",
        ANALYSIS_GOLDEN,
        Some((OBS_JSONL, OBS_SNAPSHOT)),
        &[("MARQDO_VIEW_SSE_HEARTBEAT_SECS", "1")],
    );
    server.core.set_sleep_seconds(3);
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-heartbeat",
    );
    let accepted = submit(&server, &session, &body).json();
    let run_id = accepted["run_id"].as_str().unwrap().to_string();
    let mut reader = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[("Cookie", session.cookie.clone())],
    );
    let frames = collect_until_terminal(&mut reader);
    assert!(
        frames.iter().any(|frame| frame.event == "heartbeat"),
        "frames: {frames:?}"
    );
    assert_eq!(
        frames.last().unwrap().data.as_ref().unwrap()["state"],
        "succeeded"
    );
}

#[test]
fn document_without_main_is_rejected_without_running_another_function() {
    let server = http::start(
        "examples/tables-math",
        "tests/golden/analysis-v3/tables-math.json",
        None,
    );
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-no-main",
    );
    let response = submit(&server, &session, &body);
    assert_eq!(response.status, 400);
    assert_eq!(response.json()["code"], "view.no_entry_function");
    assert!(!server.core.dir.join("run-was-called").exists());
}

#[test]
fn at_most_two_sse_observers_per_run() {
    let server = http::start_with_env(
        "examples/control-flow",
        ANALYSIS_GOLDEN,
        Some((OBS_JSONL, OBS_SNAPSHOT)),
        &[("MARQDO_VIEW_SSE_HEARTBEAT_SECS", "1")],
    );
    server.core.set_sleep_seconds(5);
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-sse-limit",
    );
    let accepted = submit(&server, &session, &body).json();
    let run_id = accepted["run_id"].as_str().unwrap().to_string();
    let events_path = format!("/api/runs/{run_id}/events");
    let first = http::SseReader::open(
        server.port,
        &events_path,
        &[("Cookie", session.cookie.clone())],
    );
    assert_eq!(first.status, 200);
    let second = http::SseReader::open(
        server.port,
        &events_path,
        &[("Cookie", session.cookie.clone())],
    );
    assert_eq!(second.status, 200);
    let third = http::SseReader::open(
        server.port,
        &events_path,
        &[("Cookie", session.cookie.clone())],
    );
    assert_eq!(third.status, 429);
    let cancel = http::post(
        server.port,
        &format!("/api/runs/{run_id}/cancel"),
        &post_headers(&server, &session, false),
        None,
    );
    assert_eq!(cancel.status, 200);
}
