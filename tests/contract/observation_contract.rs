//! Observation v1 contract: one JSONL event sequence decoded by one decoder,
//! schema/lifecycle/sequence/source-binding validation, the atomic snapshot
//! equivalence, and negative fail-closed cases.

use crate::common::golden_bytes;
use marqdo_view::protocol::analysis_v3::{verify_analysis, SemanticProjection};
use marqdo_view::protocol::observation_v1::{
    decode_snapshot, ObservationDecoder, ObservedKind, RunDisplayState,
};

const ANALYSIS: &str = "tests/golden/analysis-v3/control-flow.json";
const JSONL: &str = "tests/golden/observation-v1/control-flow.jsonl";
const SNAPSHOT: &str = "tests/golden/observation-v1/control-flow.snapshot.json";
const TRUNCATED_JSONL: &str = "tests/golden/observation-v1/control-flow.truncated.jsonl";
const TRUNCATED_SNAPSHOT: &str = "tests/golden/observation-v1/control-flow.truncated.snapshot.json";

fn projection() -> SemanticProjection {
    verify_analysis(&golden_bytes(ANALYSIS), "index.mq.md", None)
        .expect("golden Analysis v3 verifies")
        .projection
}

fn decode(jsonl: &str) -> Result<marqdo_view::protocol::observation_v1::RunProjection, String> {
    let projection = projection();
    let mut decoder = ObservationDecoder::new(&projection);
    let frames = decoder.push(&golden_bytes(jsonl));
    if let Some(detail) = decoder.invalid() {
        return Err(detail.to_string());
    }
    assert!(!frames.is_empty(), "decoder published no browser frames");
    decoder
        .finish(0, "", false)
        .map_err(|err| format!("{}: {err}", err.code()))
}

#[test]
fn real_jsonl_stream_verifies_and_binds_to_analysis_digest() {
    let run = decode(JSONL).expect("real Observation v1 stream verifies");
    assert_eq!(run.state, RunDisplayState::Succeeded);
    assert_eq!(run.run_status, "ok");
    assert_eq!(run.exit_code, 0);
    assert!(run.source_verified);
    assert_eq!(run.analysis_digest, projection().analysis_digest);
    assert!(run.truncated.is_none());
    assert_eq!(
        run.events.first().map(|event| event.kind.clone()),
        Some(ObservedKind::StreamOpened)
    );
    assert_eq!(
        run.events.get(1).map(|event| event.kind.clone()),
        Some(ObservedKind::RunStarted)
    );
    // Terminal is unique and last.
    let terminals = run
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                ObservedKind::RunCompleted | ObservedKind::RunFailed | ObservedKind::RunRejected
            )
        })
        .count();
    assert_eq!(terminals, 1);
    assert_eq!(
        run.events.last().map(|event| event.kind.clone()),
        Some(ObservedKind::RunCompleted)
    );
    assert!(run.counts.effect > 0);
    assert!(run.counts.branch_eval > 0);
    assert!(run.counts.loop_iteration > 0);
    assert!(!run.stdout.is_empty(), "stdout chunks are decoded");
    assert_eq!(
        run.result_value.as_ref().map(|value| value.ty.as_str()),
        Some("Int")
    );
}

#[test]
fn stream_and_snapshot_of_the_same_run_agree() {
    let projection = projection();
    let from_stream = decode(JSONL).expect("stream decodes");
    let from_snapshot =
        decode_snapshot(&golden_bytes(SNAPSHOT), &projection).expect("snapshot decodes");
    assert_eq!(from_stream.run_id, from_snapshot.run_id);
    assert_eq!(from_stream.state, from_snapshot.state);
    assert_eq!(from_stream.events.len(), from_snapshot.events.len());
    for (stream, snapshot) in from_stream.events.iter().zip(from_snapshot.events.iter()) {
        assert_eq!(stream.seq, snapshot.seq);
        assert_eq!(stream.kind, snapshot.kind);
        assert_eq!(stream.span, snapshot.span);
        assert_eq!(stream.anchor_span, snapshot.anchor_span);
        assert_eq!(stream.detail, snapshot.detail);
        assert_eq!(stream.operation, snapshot.operation);
    }
}

#[test]
fn truncated_observation_is_a_terminal_but_never_complete() {
    let run = decode(TRUNCATED_JSONL).expect("truncated stream decodes");
    assert_eq!(run.state, RunDisplayState::Truncated);
    let truncated = run.truncated.clone().expect("observation.truncated");
    assert_eq!(truncated.reason, "event_budget");
    assert_eq!(truncated.limit, 5);
    let terminals = run
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                ObservedKind::RunCompleted | ObservedKind::RunFailed | ObservedKind::RunRejected
            )
        })
        .count();
    assert_eq!(terminals, 1);
    assert!(!run.is_success() || run.state == RunDisplayState::Truncated);

    let projection = projection();
    let snapshot = decode_snapshot(&golden_bytes(TRUNCATED_SNAPSHOT), &projection)
        .expect("truncated snapshot decodes");
    assert_eq!(snapshot.state, RunDisplayState::Truncated);
    assert_eq!(
        snapshot
            .truncated
            .as_ref()
            .map(|value| value.reason.as_str()),
        Some("event_budget")
    );
}

#[test]
fn lifecycle_negative_cases_fail_closed() {
    let projection = projection();
    let module = projection.module_id.clone();
    let digest = projection.analysis_digest.clone();
    let root = projection.project_root_id.clone();

    let cases: Vec<(&str, Vec<serde_json::Value>)> = vec![
        ("missing stream.opened", vec![started(0, &digest, &root)]),
        (
            "terminal without start",
            vec![opened(0, &module), completed(1, &module)],
        ),
        (
            "unknown kind",
            vec![
                opened(0, &module),
                started(1, &digest, &root),
                event(2, "future.event", serde_json::json!({})),
            ],
        ),
        (
            "sequence gap",
            vec![
                opened(0, &module),
                started(1, &digest, &root),
                effect(3, &module),
            ],
        ),
        (
            "duplicate sequence",
            vec![
                opened(0, &module),
                started(1, &digest, &root),
                effect(2, &module),
                effect(2, &module),
            ],
        ),
        (
            "mixed run id",
            vec![
                opened(0, &module),
                event(
                    1,
                    "run.started",
                    serde_json::json!({
                        "entry": format!("{module}::main"),
                        "mode": "pure",
                        "source": {"root_id": root, "analysis_digest": digest}
                    }),
                ),
                event_with_run(2, "other-run", "effect", effect_payload(&module)),
            ],
        ),
        (
            "continued after terminal",
            vec![
                opened(0, &module),
                started(1, &digest, &root),
                completed(2, &module),
                effect(3, &module),
            ],
        ),
        (
            "two terminals",
            vec![
                opened(0, &module),
                started(1, &digest, &root),
                completed(2, &module),
                completed(3, &module),
            ],
        ),
        (
            "span outside source",
            vec![
                opened(0, &module),
                started(1, &digest, &root),
                event(
                    2,
                    "effect",
                    serde_json::json!({
                        "frame_id": "f1", "operation": "fs.exists",
                        "result_code": "ok", "module_id": module,
                        "span": {"start": 0, "end": 999_999}
                    }),
                ),
            ],
        ),
        (
            "value without consent",
            vec![
                opened(0, &module),
                started(1, &digest, &root),
                event(
                    2,
                    "variable.assigned",
                    serde_json::json!({
                        "frame_id": "f1", "module_id": module, "name": "x",
                        "value_state": "observed", "value_type": "Int",
                        "span": {"start": 0, "end": 1},
                        "value": {"type": "int", "decimal": "1"}
                    }),
                ),
            ],
        ),
    ];
    for (name, events) in cases {
        let mut decoder = ObservationDecoder::new(&projection);
        let bytes = encode(&events);
        decoder.push(&bytes);
        if let Some(detail) = decoder.invalid() {
            // Push-time rejection is the expected fail-closed path.
            assert!(!detail.is_empty(), "{name}");
            continue;
        }
        let result = decoder.finish(0, "", false);
        assert!(result.is_err(), "{name} was accepted as {result:?}");
    }
}

#[test]
fn unknown_schema_and_revision_are_rejected() {
    let projection = projection();
    let module = projection.module_id.clone();
    let mut event = opened(0, &module);
    event["schema"] = serde_json::json!("marqdo.execution-events");
    let mut decoder = ObservationDecoder::new(&projection);
    decoder.push(&encode(&[event]));
    assert!(decoder.invalid().is_some(), "old schema must be rejected");

    let mut event = opened(0, &module);
    event["schema_version"] = serde_json::json!(2);
    let mut decoder = ObservationDecoder::new(&projection);
    decoder.push(&encode(&[event]));
    assert!(
        decoder.invalid().is_some(),
        "unknown revision must be rejected"
    );
}

#[test]
fn stale_and_wrong_root_bindings_are_observation_stale() {
    let projection = projection();
    let module = projection.module_id.clone();
    let wrong_digest = format!("sha256:{}", "0".repeat(64));
    let events = vec![
        opened(0, &module),
        started(1, &wrong_digest, &projection.project_root_id),
        completed(2, &module),
    ];
    let mut decoder = ObservationDecoder::new(&projection);
    decoder.push(&encode(&events));
    let err = decoder.finish(0, "", false).unwrap_err();
    assert_eq!(err.code(), "view.observation_stale");
    assert!(!err.detail().is_empty());

    let events = vec![
        opened(0, &module),
        started(1, &projection.analysis_digest, "root:other@0.1.0"),
        completed(2, &module),
    ];
    let mut decoder = ObservationDecoder::new(&projection);
    decoder.push(&encode(&events));
    let err = decoder.finish(0, "", false).unwrap_err();
    assert_eq!(err.code(), "view.observation_stale");
}

#[test]
fn rejected_before_start_is_a_valid_failed_terminal() {
    let projection = projection();
    let module = projection.module_id.clone();
    let events = vec![opened(0, &module), rejected(1, &module)];
    let mut decoder = ObservationDecoder::new(&projection);
    decoder.push(&encode(&events));
    let run = decoder
        .finish(1, "error[sem.unclosed_control]: refusing to run", false)
        .expect("compile rejection is a valid terminal");
    assert_eq!(run.state, RunDisplayState::Failed);
    assert_eq!(run.run_status, "rejected");
    assert_eq!(run.error_code.as_deref(), Some("sem.unclosed_control"));
    assert!(!run.is_success());
}

#[test]
fn eof_without_terminal_is_incomplete_never_success() {
    let projection = projection();
    let module = projection.module_id.clone();
    let events = vec![
        opened(0, &module),
        started(1, &projection.analysis_digest, &projection.project_root_id),
        effect(2, &module),
    ];
    let mut decoder = ObservationDecoder::new(&projection);
    decoder.push(&encode(&events));
    let run = decoder.finish(0, "", false).expect("no terminal decodes");
    assert_eq!(run.state, RunDisplayState::Incomplete);
    assert_eq!(
        run.error_code.as_deref(),
        Some("view.observation_incomplete")
    );
    assert!(!run.is_success());
    assert!(!run.output_complete);
}

// ---------------------------------------------------------------------------
// Synthetic Observation v1 builders (only for negative-path tests).
// ---------------------------------------------------------------------------

fn encode(events: &[serde_json::Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for event in events {
        bytes.extend_from_slice(event.to_string().as_bytes());
        bytes.push(b'\n');
    }
    bytes
}

fn envelope(seq: u64, kind: &str, payload: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "schema": "marqdo.observation",
        "schema_version": 1,
        "run_id": "test-run-1",
        "seq": seq,
        "kind": kind,
        "payload": payload
    })
}

fn event(seq: u64, kind: &str, payload: serde_json::Value) -> serde_json::Value {
    envelope(seq, kind, payload)
}

fn event_with_run(
    seq: u64,
    run: &str,
    kind: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let mut value = envelope(seq, kind, payload);
    value["run_id"] = serde_json::json!(run);
    value
}

fn opened(seq: u64, _module: &str) -> serde_json::Value {
    envelope(
        seq,
        "stream.opened",
        serde_json::json!({
            "producer": {"name": "marqdo", "version": "3.0.0-rc.4"},
            "observation": {
                "level": "structural",
                "values": false,
                "selected_variables": [],
                "selected_parameters": [],
                "allow_text_values": false
            },
            "limits": {
                "max_observation_events": 10000,
                "max_observation_bytes": 4194304,
                "max_value_bytes": 256
            }
        }),
    )
}

fn started(seq: u64, digest: &str, root: &str) -> serde_json::Value {
    let projection = projection();
    envelope(
        seq,
        "run.started",
        serde_json::json!({
            "entry": format!("{}::main", projection.module_id),
            "mode": "pure",
            "source": {"root_id": root, "analysis_digest": digest},
            "started_unix_ms": 1
        }),
    )
}

fn effect(seq: u64, module: &str) -> serde_json::Value {
    event(seq, "effect", effect_payload(module))
}

fn effect_payload(module: &str) -> serde_json::Value {
    serde_json::json!({
        "frame_id": "f1",
        "operation": "fs.exists",
        "result_code": "ok",
        "module_id": module,
        "span": {"start": 0, "end": 1}
    })
}

fn completed(seq: u64, _module: &str) -> serde_json::Value {
    envelope(
        seq,
        "run.completed",
        serde_json::json!({
            "status": "ok",
            "value": {"type": "int", "decimal": "3"},
            "value_type": "Int"
        }),
    )
}

fn rejected(seq: u64, _module: &str) -> serde_json::Value {
    envelope(
        seq,
        "run.rejected",
        serde_json::json!({
            "status": "rejected",
            "phase": "compile",
            "error": {"code": "sem.unclosed_control", "message": "`if` is not closed"}
        }),
    )
}
