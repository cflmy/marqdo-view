//! Trace v2 contract tests against goldens emitted by the frozen CLI. The
//! goldens bind to the committed Analysis v2 bytes by digest, so a changed
//! source cannot silently reuse an old trace.

use marqdo_view::semantic;
use marqdo_view::source_map::{sha256_prefixed, SourceIndex};
use marqdo_view::trace::{verify_trace, RunDisplayState, TraceVerificationInput};
use serde_json::Value;

struct Fixture {
    source: String,
    index: SourceIndex,
    analysis: Vec<u8>,
    semantic: semantic::VerifiedAnalysis,
}

fn fixture(analysis_golden: &str, source_path: &str) -> Fixture {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join(source_path)).unwrap();
    let index = SourceIndex::new(&source);
    let analysis = std::fs::read(root.join(analysis_golden)).unwrap();
    let semantic = semantic::verify_analysis(&analysis, &source, &index, "index.mq.md").unwrap();
    Fixture {
        source,
        index,
        analysis,
        semantic,
    }
}

fn golden_json(path: &str) -> Value {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    serde_json::from_slice(&std::fs::read(root.join(path)).unwrap()).unwrap()
}

fn golden_bytes(path: &str) -> Vec<u8> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::read(root.join(path)).unwrap()
}

fn verify(
    fixture: &Fixture,
    trace_golden: &str,
    run_golden: &str,
    exit_code: i32,
) -> marqdo_view::trace::VerifiedRun {
    let trace = golden_bytes(trace_golden);
    let run_json = golden_json(run_golden);
    verify_trace(TraceVerificationInput {
        trace_bytes: &trace,
        analysis_bytes: &fixture.analysis,
        semantic: &fixture.semantic.projection,
        modules: &fixture.semantic.projection.modules,
        source: &fixture.source,
        source_index: &fixture.index,
        run_json: &run_json,
        exit_code,
    })
    .expect("trace verifies")
}

#[test]
fn real_control_flow_trace_binds_and_projects_events() {
    let fixture = fixture(
        "tests/golden/analysis-v2/control-flow.json",
        "examples/control-flow/index.mq.md",
    );
    let run = verify(
        &fixture,
        "tests/golden/trace-v2/control-flow.trace.json",
        "tests/golden/trace-v2/control-flow.run.json",
        0,
    );
    let projection = &run.projection;
    assert_eq!(projection.state, RunDisplayState::Succeeded);
    assert_eq!(projection.counts.total(), 285);
    assert_eq!(projection.entry_event_count, 18);
    assert_eq!(projection.other_event_count, 267);
    assert_eq!(projection.run_status, "ok");
    assert!(projection.truncated.is_none());
    assert_eq!(projection.stdout, "0\n0\n2\n");
    let value = projection.result_value.as_ref().expect("return value");
    assert_eq!(value.ty, "int");
    assert_eq!(value.display, "3");
    // Public JSON final value only; never a per-assignment value claim.
    assert!(projection.analysis_digest.starts_with("sha256:"));

    // The loop header is evaluated four times (three iterations + exit).
    let loop_span = projection
        .events
        .iter()
        .find(|event| event.anchor_span.is_some() && event.detail.as_deref() == Some("exit"))
        .and_then(|event| event.anchor_span)
        .expect("loop anchor");
    let summary = projection.anchor_summary(loop_span).expect("loop summary");
    assert_eq!(summary.count, 4);
    assert_eq!(summary.detail, "loop exit");
    assert!(summary.kinds.contains(&"loop.iteration".to_string()));

    let return_anchors: Vec<_> = projection
        .events
        .iter()
        .filter(|event| event.anchor_span.is_some())
        .collect();
    assert!(!return_anchors.is_empty());
    // Only entry-module events ever get statement anchors.
    for event in &projection.events {
        if event.anchor_span.is_some() {
            assert_eq!(
                event.module_id.as_deref(),
                Some(fixture.semantic.projection.module_id.as_str())
            );
        }
    }
}

#[test]
fn truncated_trace_is_explicitly_marked() {
    let fixture = fixture(
        "tests/golden/analysis-v2/control-flow.json",
        "examples/control-flow/index.mq.md",
    );
    let run = verify(
        &fixture,
        "tests/golden/trace-v2/control-flow.truncated.trace.json",
        "tests/golden/trace-v2/control-flow.truncated.run.json",
        0,
    );
    let projection = &run.projection;
    assert_eq!(projection.state, RunDisplayState::Truncated);
    let truncated = projection.truncated.as_ref().expect("truncation");
    assert_eq!(truncated.reason, "event_budget");
    assert_eq!(truncated.limit, 5);
    assert_eq!(truncated.written, 5);
    assert!(truncated.label().contains("事件数量上限"));
    assert_ne!(projection.state, RunDisplayState::Succeeded);
}

#[test]
fn failed_run_keeps_the_core_error_code() {
    let fixture = fixture(
        "tests/golden/analysis-v2/mixed-modes.json",
        "examples/mixed-modes/index.mq.md",
    );
    let run = verify(
        &fixture,
        "tests/golden/trace-v2/mixed-modes.arity.trace.json",
        "tests/golden/trace-v2/mixed-modes.arity.run.json",
        1,
    );
    let projection = &run.projection;
    assert_eq!(projection.state, RunDisplayState::Failed);
    assert_eq!(projection.error_code.as_deref(), Some("call.arity"));
    assert_eq!(projection.run_status, "call.arity");
    assert!(projection.result_value.is_none());
    assert_eq!(projection.state.label(), "运行失败");
}

#[test]
fn stale_analysis_bytes_are_rejected() {
    let fixture = fixture(
        "tests/golden/analysis-v2/control-flow.json",
        "examples/control-flow/index.mq.md",
    );
    let mut tampered = fixture.analysis.clone();
    tampered.push(b' ');
    let trace = golden_bytes("tests/golden/trace-v2/control-flow.trace.json");
    let run_json = golden_json("tests/golden/trace-v2/control-flow.run.json");
    let err = verify_trace(TraceVerificationInput {
        trace_bytes: &trace,
        analysis_bytes: &tampered,
        semantic: &fixture.semantic.projection,
        modules: &fixture.semantic.projection.modules,
        source: &fixture.source,
        source_index: &fixture.index,
        run_json: &run_json,
        exit_code: 0,
    })
    .unwrap_err();
    assert_eq!(err.code(), "view.trace_stale");
    assert_eq!(err.exit_code(), 1);
}

#[test]
fn unknown_event_module_is_rejected() {
    let fixture = fixture(
        "tests/golden/analysis-v2/control-flow.json",
        "examples/control-flow/index.mq.md",
    );
    let mut trace: Value = golden_json("tests/golden/trace-v2/control-flow.trace.json");
    trace["events"][0]["module_id"] = Value::String("rogue::module.mq.md".to_string());
    let trace_bytes = serde_json::to_vec(&trace).unwrap();
    let run_json = golden_json("tests/golden/trace-v2/control-flow.run.json");
    let err = verify_trace(TraceVerificationInput {
        trace_bytes: &trace_bytes,
        analysis_bytes: &fixture.analysis,
        semantic: &fixture.semantic.projection,
        modules: &fixture.semantic.projection.modules,
        source: &fixture.source,
        source_index: &fixture.index,
        run_json: &run_json,
        exit_code: 0,
    })
    .unwrap_err();
    assert_eq!(err.code(), "view.trace_stale");
}

#[test]
fn non_monotonic_sequence_is_rejected() {
    let fixture = fixture(
        "tests/golden/analysis-v2/control-flow.json",
        "examples/control-flow/index.mq.md",
    );
    let mut trace: Value = golden_json("tests/golden/trace-v2/control-flow.trace.json");
    trace["events"][1]["seq"] = Value::Number(0.into());
    let trace_bytes = serde_json::to_vec(&trace).unwrap();
    let run_json = golden_json("tests/golden/trace-v2/control-flow.run.json");
    let err = verify_trace(TraceVerificationInput {
        trace_bytes: &trace_bytes,
        analysis_bytes: &fixture.analysis,
        semantic: &fixture.semantic.projection,
        modules: &fixture.semantic.projection.modules,
        source: &fixture.source,
        source_index: &fixture.index,
        run_json: &run_json,
        exit_code: 0,
    })
    .unwrap_err();
    assert_eq!(err.code(), "view.trace_stale");
}

#[test]
fn unknown_schema_major_is_a_protocol_error_not_a_stale_trace() {
    let fixture = fixture(
        "tests/golden/analysis-v2/control-flow.json",
        "examples/control-flow/index.mq.md",
    );
    let mut trace: Value = golden_json("tests/golden/trace-v2/control-flow.trace.json");
    trace["schema_version"] = Value::Number(3.into());
    let trace_bytes = serde_json::to_vec(&trace).unwrap();
    let run_json = golden_json("tests/golden/trace-v2/control-flow.run.json");
    let err = verify_trace(TraceVerificationInput {
        trace_bytes: &trace_bytes,
        analysis_bytes: &fixture.analysis,
        semantic: &fixture.semantic.projection,
        modules: &fixture.semantic.projection.modules,
        source: &fixture.source,
        source_index: &fixture.index,
        run_json: &run_json,
        exit_code: 0,
    })
    .unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn golden_digest_is_really_sha256_of_analysis_bytes() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis = std::fs::read(root.join("tests/golden/analysis-v2/control-flow.json")).unwrap();
    let trace: Value = golden_json("tests/golden/trace-v2/control-flow.trace.json");
    assert_eq!(
        trace["source"]["analysis_digest"].as_str().unwrap(),
        sha256_prefixed(&analysis)
    );
    assert_eq!(
        trace["source"]["root_id"].as_str().unwrap(),
        "root:control-flow@0.1.0"
    );
}
