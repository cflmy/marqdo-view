//! Degradation/state matrix contract (RFC-VIEW-0001 S-05..S-07, §12.3):
//! prose stays readable while semantics become explicitly unusable.

use crate::common;

use common::{minimal_analysis, sha256_hex, FakeCore, TempProject};
use marqdo_view::reader::assemble::DocumentService;
use marqdo_view::reader::model::SourceStatus;

fn project(name: &str) -> (TempProject, FakeCore) {
    let project = TempProject::new(name);
    project.marqdo_project("# @knowledge 标题\n\n正文 **加粗**，包含 `n`。\n");
    let core = FakeCore::new(&format!("{name}-core"));
    (project, core)
}

#[test]
fn failed_check_keeps_prose_and_shows_core_diagnostics() {
    let (project, core) = project("state-check-fail");
    let source = std::fs::read(project.path("index.mq.md")).unwrap();
    // Emit a broken analysis too; it must NOT be used after a failed check.
    core.set_analysis(&minimal_analysis("index.mq.md", &sha256_hex(&source), 2));
    core.set_check(1, "");
    core.set_check_json(
        r#"{"command":"check","success":false,"errors":1,"diagnostics":[{"code":"sem.unclosed_control","column":1,"file":"/home/secret/index.mq.md","line":3,"message":"`if` is not closed by `end`","module":"root:fixture@0.1.0::index.mq.md","severity":"error"}]}"#,
    );
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();
    let model = context.load(&relative).unwrap();
    assert_eq!(model.source_status, SourceStatus::CoreError);
    assert!(
        model.semantic.is_none(),
        "failed check must revoke semantics"
    );
    assert!(model.document.plain_text().contains("正文"));
    assert!(model
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "sem.unclosed_control"));
    assert!(!model.source_status.semantics_usable());
    // Pending run state is explicit, never a success claim.
    assert_eq!(model.run_state().data_value(), "not-run");
}

#[test]
fn source_digest_mismatch_degrades_instead_of_misbinding() {
    let (project, core) = project("state-source-mismatch");
    core.set_analysis(&minimal_analysis("index.mq.md", &"0".repeat(64), 2));
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();
    let model = context.load(&relative).unwrap();
    assert_eq!(model.source_status, SourceStatus::SourceMismatch);
    assert!(model.semantic.is_none());
    assert!(model.source_status.label().contains("不一致"));
}

#[test]
fn unknown_analysis_schema_is_a_hard_protocol_error() {
    let (project, core) = project("state-protocol");
    let source = std::fs::read(project.path("index.mq.md")).unwrap();
    core.set_analysis(&minimal_analysis("index.mq.md", &sha256_hex(&source), 4));
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();
    let err = context.load(&relative).unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn invalid_core_span_is_dropped_with_an_explicit_warning() {
    let (project, core) = project("state-span-invalid");
    let source = std::fs::read(project.path("index.mq.md")).unwrap();
    let mut analysis: serde_json::Value =
        serde_json::from_str(&minimal_analysis("index.mq.md", &sha256_hex(&source), 2)).unwrap();
    // A span in the middle of a multi-byte character / beyond EOF.
    analysis["modules"][0]["units"] = serde_json::json!([{
        "id": "x", "name": "坏", "level": 1, "kind": "namespace", "function": null,
        "span": {"start": 0, "end": 999_999}
    }]);
    core.set_analysis(&serde_json::to_string(&analysis).unwrap());
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();
    let model = context.load(&relative).unwrap();
    assert_eq!(model.source_status, SourceStatus::StaticOk);
    assert!(model.semantic.as_ref().unwrap().units.is_empty());
    assert!(model
        .warnings
        .iter()
        .any(|warning| warning.code == "view.source_span_invalid"));
    let _ = source;
}

#[test]
fn run_attached_to_changed_source_becomes_stale() {
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = TempProject::new("state-stale-run");
    project.write(
        "marqdo.yaml",
        "project:\n  name: control-flow\n  version: 0.1.0\n",
    );
    let source =
        std::fs::read_to_string(repo_root.join("examples/control-flow/index.mq.md")).unwrap();
    project.write("index.mq.md", &source);
    let core = FakeCore::new("state-stale-run-core");
    let analysis =
        std::fs::read(repo_root.join("tests/golden/analysis-v2/control-flow.json")).unwrap();
    core.set_analysis(std::str::from_utf8(&analysis).unwrap());
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();

    // Build a verified run directly from the golden trace + golden analysis.
    let source_index = marqdo_view::reader::source::SourceIndex::new(&source);
    let verified_analysis = marqdo_view::protocol::analysis_v2::verify_analysis(
        &analysis,
        &source,
        &source_index,
        "index.mq.md",
    )
    .unwrap();
    let trace =
        std::fs::read(repo_root.join("tests/golden/trace-v2/control-flow.trace.json")).unwrap();
    let run_json: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo_root.join("tests/golden/trace-v2/control-flow.run.json")).unwrap(),
    )
    .unwrap();
    let run = marqdo_view::protocol::trace_v2::verify_trace(
        marqdo_view::protocol::trace_v2::TraceVerificationInput {
            trace_bytes: &trace,
            analysis_bytes: &analysis,
            semantic: &verified_analysis.projection,
            modules: &verified_analysis.projection.modules,
            source: &source,
            source_index: &source_index,
            run_json: &run_json,
            exit_code: 0,
        },
    )
    .unwrap();
    let attached = context.load_with_run(&relative, Some(&run)).unwrap();
    assert_eq!(attached.run_state().data_value(), "succeeded");

    // Change the source and rebuild through the same server path.
    std::fs::write(
        project.path("index.mq.md"),
        format!("{source}\n<!-- changed -->\n"),
    )
    .unwrap();
    let changed = context.load_with_run(&relative, Some(&run)).unwrap();
    assert!(
        changed.run_state().data_value() == "stale"
            || changed.source_status == SourceStatus::SourceMismatch,
        "expected stale/source-mismatch, got run={} source={}",
        changed.run_state().data_value(),
        changed.source_status.data_value()
    );
    if let Some(run) = &changed.run {
        assert!(run.events.iter().all(|event| event.anchor_span.is_none()));
    }
    let _ = core;
}
