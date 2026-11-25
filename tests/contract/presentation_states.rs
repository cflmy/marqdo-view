//! Degradation/state matrix contract: prose stays readable while semantics
//! become explicitly unusable; run states never claim success from an incomplete or
//! truncated observation.

use crate::common::{golden_bytes, minimal_analysis, FakeCore, TempProject};
use marqdo_view::protocol::analysis_v3::{verify_analysis, SemanticProjection};
use marqdo_view::protocol::observation_v1::{ObservationDecoder, RunDisplayState, RunProjection};
use marqdo_view::reader::assemble::{build, BuildInput, DocumentService};
use marqdo_view::reader::markdown::{DocumentIdentity, DocumentModel};
use marqdo_view::reader::model::{Navigation, SourceStatus};
use marqdo_view::reader::source::Project;

const ANALYSIS: &str = "tests/golden/analysis-v3/control-flow.json";
const JSONL: &str = "tests/golden/observation-v1/control-flow.jsonl";
const TRUNCATED_JSONL: &str = "tests/golden/observation-v1/control-flow.truncated.jsonl";

fn project(name: &str) -> (TempProject, FakeCore) {
    let project = TempProject::new(name);
    project.marqdo_project("# @knowledge 标题\n\n正文 **加粗**，包含 `n`。\n");
    let core = FakeCore::new(&format!("{name}-core"));
    (project, core)
}

fn projection() -> SemanticProjection {
    verify_analysis(&golden_bytes(ANALYSIS), "index.mq.md", None)
        .expect("control-flow golden verifies")
        .projection
}

fn decode(jsonl: &str) -> RunProjection {
    let projection = projection();
    let mut decoder = ObservationDecoder::new(&projection);
    decoder.push(&golden_bytes(jsonl));
    assert!(decoder.invalid().is_none(), "{:?}", decoder.invalid());
    decoder
        .finish(0, "", false)
        .expect("real observation stream verifies")
}

fn empty_model(
    document: DocumentModel,
    run: Option<&RunProjection>,
) -> marqdo_view::reader::model::PresentationModel {
    build(BuildInput {
        document,
        analysis: None,
        status: SourceStatus::CoreUnavailable,
        status_note: Some("test".into()),
        diagnostics: Vec::new(),
        navigation: Navigation {
            site_title: "test".into(),
            documents: Vec::new(),
            active: None,
            welcome: false,
        },
        core_version: None,
        run,
    })
}

fn document_for(relative: &str, source: &str) -> DocumentModel {
    let path = marqdo_view::reader::source::SafeRelativePath::parse(relative).unwrap();
    DocumentModel::parse(source.to_string(), DocumentIdentity::placeholder(&path))
}

#[test]
fn failed_check_keeps_prose_and_shows_core_diagnostics() {
    let (project, core) = project("state-check-fail");
    let source = std::fs::read_to_string(project.path("index.mq.md")).unwrap();
    core.set_analysis(&minimal_analysis("index.mq.md", &source, 3));
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
    assert_eq!(model.run_state().data_value(), "not-run");
}

#[test]
fn source_digest_mismatch_degrades_instead_of_misbinding() {
    let (project, core) = project("state-source-mismatch");
    core.set_analysis(&minimal_analysis("index.mq.md", "# 完全不同的源码\n", 3));
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
    let source = std::fs::read_to_string(project.path("index.mq.md")).unwrap();
    core.set_analysis(&minimal_analysis("index.mq.md", &source, 4));
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();
    let err = context.load(&relative).unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn invalid_analysis_span_is_rejected_not_trimmed() {
    // N09: an out-of-range/negative span in a protocol artifact is a hard
    // failure; View must never clip it and claim success.
    let (project, core) = project("state-span-invalid");
    let source = std::fs::read_to_string(project.path("index.mq.md")).unwrap();
    let mut analysis: serde_json::Value =
        serde_json::from_str(&minimal_analysis("index.mq.md", &source, 3)).unwrap();
    let module_id = analysis["modules"][0]["id"].as_str().unwrap().to_string();
    analysis["modules"][0]["units"] = serde_json::json!([{
        "id": format!("{module_id}#坏"),
        "path": ["坏"],
        "parent_id": module_id,
        "kind": "namespace",
        "name": "坏",
        "level": 1,
        "mode": "knowledge",
        "mode_origin": "explicit",
        "mode_source_unit_id": format!("{module_id}#坏"),
        "heading_span": {"start": 0, "end": 1},
        "span": {"start": 0, "end": 999_999},
        "function": null
    }]);
    let bytes = analysis.to_string().into_bytes();
    let err = verify_analysis(&bytes, "index.mq.md", Some(&source)).unwrap_err();
    assert_eq!(err.code(), "view.analysis_failed");

    // Through the reader the prose stays readable, but no annotation may be
    // attached from the rejected artifact.
    core.set_analysis(&analysis.to_string());
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();
    let model = context.load(&relative).unwrap();
    assert!(model.semantic.is_none());
    assert_eq!(model.source_status, SourceStatus::CoreError);
    assert!(model.document.plain_text().contains("正文"));
}

#[test]
fn run_attached_to_changed_source_becomes_stale() {
    let project = TempProject::new("state-stale-run");
    project.write(
        "marqdo.yaml",
        "project:\n  name: control-flow\n  version: 0.1.0\n",
    );
    let source = golden_bytes("tests/golden/analysis-v3/control-flow.json");
    let source = serde_json::from_slice::<serde_json::Value>(&source).unwrap()["modules"][0]
        ["source"]
        .as_str()
        .unwrap()
        .to_string();
    project.write("index.mq.md", &source);
    let core = FakeCore::new("state-stale-run-core");
    let analysis = golden_bytes(ANALYSIS);
    core.set_analysis(std::str::from_utf8(&analysis).unwrap());
    let context = DocumentService::open(&project.dir, Some(&core.script)).unwrap();
    let relative = context.project().entry().relative.clone();

    let run = decode(JSONL);
    let attached = context.load_with_run(&relative, Some(&run)).unwrap();
    assert_eq!(attached.run_state().data_value(), "succeeded");

    // The document changed after the run; the same projection must never be
    // re-anchored to the new source.
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
}

#[test]
fn truncated_and_incomplete_runs_never_display_as_success() {
    let run = decode(TRUNCATED_JSONL);
    assert_eq!(run.state, RunDisplayState::Truncated);
    assert!(run.truncated.is_some());
    let model = empty_model(document_for("index.mq.md", "# @knowledge x\n"), Some(&run));
    assert_eq!(model.run_state().data_value(), "truncated");
    assert!(model.run_state().is_terminal());
    assert_ne!(model.run_state().data_value(), "succeeded");
    assert_eq!(model.run_state().label(), "观察已截断");

    // A stream that ends before its terminal event is incomplete, not success.
    let projection = projection();
    let mut decoder = ObservationDecoder::new(&projection);
    let mut lines = Vec::new();
    for line in golden_bytes(JSONL).split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_slice(line).unwrap();
        if value["kind"] == "run.completed" {
            break;
        }
        lines.extend_from_slice(line);
        lines.push(b'\n');
    }
    decoder.push(&lines);
    let incomplete = decoder.finish(0, "", false).unwrap();
    assert_eq!(incomplete.state, RunDisplayState::Incomplete);
    assert!(!incomplete.is_success());
    let model = empty_model(
        document_for("index.mq.md", "# @knowledge x\n"),
        Some(&incomplete),
    );
    assert_eq!(model.run_state().data_value(), "incomplete");
}

#[test]
fn missing_core_stays_readable_and_explicitly_unverified() {
    let project = TempProject::new("state-no-core");
    project.marqdo_project("# @knowledge fixture\n");
    let context = DocumentService::open(
        &project.dir,
        Some(std::path::Path::new("/definitely/missing/marqdo-frozen")),
    )
    .unwrap();
    let relative = context.project().entry().relative.clone();
    let model = context.load(&relative).unwrap();
    assert_eq!(model.source_status, SourceStatus::CoreUnavailable);
    assert!(model.semantic.is_none());
    assert!(model.status_note.is_some());
}

#[test]
fn external_projection_can_be_rendered_from_artifact_source() {
    // The artifact source is the only source used to interpret spans, even when
    // no checkout file exists for a dependency module.
    let verified = verify_analysis(&golden_bytes(ANALYSIS), "index.mq.md", None).unwrap();
    assert_eq!(
        verified.projection.source,
        verified.projection.entry_module().unwrap().source
    );
    let _ = Project::open(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/control-flow"),
    )
    .unwrap();
}
