//! Contract tests against a real fake-core executable: argv shape, exit codes,
//! stderr handling, temp artifacts and the current Analysis v3 revision.
//!
//! The fake core is a shell script, not a mocked Rust trait, so everything at
//! the process boundary is exercised for real.

use crate::common;

use common::{minimal_analysis, sha256_hex, FakeCore, TempProject};
use marqdo_view::core_client::{
    build_call_args, build_run_args, CheckOptions, CheckResponse, CoreClient, RunOptions,
};
use marqdo_view::reader::source::Project;

fn fixture(name: &str) -> (TempProject, FakeCore) {
    let project = TempProject::new(name);
    project.marqdo_project("# @knowledge fixture\n");
    let core = FakeCore::new(&format!("{name}-core"));
    (project, core)
}

fn project_source(project: &TempProject) -> String {
    std::fs::read_to_string(project.path("index.mq.md")).unwrap()
}

#[test]
fn check_returns_exact_analysis_bytes_and_digest() {
    let (project, core) = fixture("check_ok");
    let source = project_source(&project);
    let analysis = minimal_analysis("index.mq.md", &source, 3);
    core.set_analysis(&analysis);

    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let output = expect_check_ok(
        client
            .check(&project, &entry, &CheckOptions::default())
            .unwrap(),
    );

    assert_eq!(output.analysis_bytes, analysis.as_bytes());
    assert_eq!(output.analysis_sha256, sha256_hex(analysis.as_bytes()));
    assert_eq!(
        output.analysis_digest,
        format!("sha256:{}", sha256_hex(analysis.as_bytes()))
    );
    let args = core.last_args();
    assert_eq!(args[0], "check");
    assert!(args.contains(&"--emit".to_string()));
    assert!(!args.contains(&"--refs".to_string()));
    // The retired Analysis v2 selector and trace flags never appear.
    for retired in ["--trace-file", "--event-stream"] {
        assert!(!args.contains(&retired.to_string()), "retired {retired}");
    }
}

#[test]
fn check_refs_is_forwarded_as_a_stricter_flag() {
    let (project, core) = fixture("check_refs");
    let source = project_source(&project);
    core.set_analysis(&minimal_analysis("index.mq.md", &source, 3));
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    expect_check_ok(
        client
            .check(
                &project,
                &entry,
                &CheckOptions {
                    refs: true,
                    timeout: None,
                },
            )
            .unwrap(),
    );
    assert!(core.last_args().contains(&"--refs".to_string()));
}

#[test]
fn check_failure_preserves_core_stderr_and_code() {
    let (project, core) = fixture("check_fail");
    core.set_check(1, "index.mq.md:3:1: sem.unclosed_control\n");
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let response = client
        .check(&project, &entry, &CheckOptions::default())
        .unwrap();
    assert!(!response.success);
    assert_eq!(response.exit_code, 1);
    assert!(response.stderr.contains("sem.unclosed_control"));
}

#[test]
fn run_and_call_argv_use_the_current_observation_flags_only() {
    let args = build_run_args(
        std::path::Path::new("/p/index.mq.md"),
        std::path::Path::new("/t/observation.json"),
        &RunOptions::default(),
    );
    let text: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
    assert_eq!(text[0], "run");
    assert!(text.contains(&"--mode".to_string()));
    assert!(text.contains(&"pure".to_string()));
    assert!(text.contains(&"--output".to_string()));
    assert!(text.contains(&"jsonl".to_string()));
    assert!(text.contains(&"--observation-file".to_string()));
    assert!(text.contains(&"--observe-max-events".to_string()));
    assert!(text.contains(&"--observe-max-bytes".to_string()));
    assert!(text.contains(&"--observe-max-value-bytes".to_string()));
    for retired in [
        "--json",
        "--trace-file",
        "--event-stream",
        "--trace-max-events",
        "--trace-max-bytes",
        "--trace-max-value-bytes",
    ] {
        assert!(
            !text.contains(&retired.to_string()),
            "retired flag {retired}"
        );
    }
    assert!(!text
        .iter()
        .any(|token| token.contains(';') || token.contains('|') || token.contains('&')));

    let args = build_call_args(
        std::path::Path::new("/p/index.mq.md"),
        "demo.calculate",
        &[("n".to_string(), "3".to_string())],
        std::path::Path::new("/t/observation.json"),
        &RunOptions::default(),
    );
    let text: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
    assert_eq!(text[0], "call");
    assert!(text.contains(&"--arg".to_string()));
    assert!(text.contains(&"n=3".to_string()));
    assert!(text.contains(&"--observation-file".to_string()));
}

#[test]
fn default_run_does_not_request_variable_or_parameter_values() {
    let args = build_run_args(
        std::path::Path::new("/p/index.mq.md"),
        std::path::Path::new("/t/observation.json"),
        &RunOptions::default(),
    );
    let text: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();
    // Budgets are explicit, but no value name is selected by default.
    for flag in [
        "--observe-variable",
        "--observe-parameter",
        "--observe-text-values",
    ] {
        assert!(!text.contains(&flag.to_string()), "default {flag}");
    }
}

#[test]
fn incompatible_product_version_is_rejected_before_any_work() {
    let (_project, core) = fixture("version_old");
    core.set_version("marqdo 3.0.0-rc.2\n");
    let client = CoreClient::with_bin(&core.script).unwrap();
    let err = client.negotiate().unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert_eq!(err.exit_code(), 2);
    // Only `--version` was invoked; no check/run happened.
    assert_eq!(core.last_args(), vec!["--version".to_string()]);
}

#[test]
fn current_rc_version_is_accepted() {
    let (_project, core) = fixture("version_rc4");
    core.set_version("marqdo 3.0.0-rc.4\n");
    let client = CoreClient::with_bin(&core.script).unwrap();
    assert_eq!(
        client.negotiate().unwrap().version.to_string(),
        "3.0.0-rc.4"
    );
}

#[test]
fn missing_emit_after_success_is_an_analysis_failure() {
    let (project, core) = fixture("check_no_emit");
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let err = client
        .check(&project, &entry, &CheckOptions::default())
        .unwrap_err();
    assert_eq!(err.code(), "view.analysis_failed");
}

#[test]
fn failed_check_normalizes_diagnostics_without_absolute_paths() {
    let (project, core) = fixture("check_diagnostics");
    core.set_check(1, "");
    core.set_check_json(
        r#"{"command":"check","success":false,"errors":1,"diagnostics":[{"code":"sem.unclosed_control","column":1,"file":"/home/secret/project/index.mq.md","hints":[],"line":5,"message":"`if` is not closed by `end`","module":"root:fixture@0.1.0::index.mq.md","severity":"error"}]}"#,
    );
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let response = client
        .check(&project, &entry, &CheckOptions::default())
        .unwrap();
    assert!(!response.success);
    assert_eq!(response.diagnostics.len(), 1);
    assert_eq!(response.diagnostics[0].code, "sem.unclosed_control");
    assert_eq!(response.diagnostics[0].line, Some(5));
    let rendered = serde_json::to_string(&response.diagnostics).unwrap();
    assert!(!rendered.contains("/home/secret"), "{rendered}");
}

#[test]
fn analysis_v2_revision_is_a_hard_protocol_error() {
    let (project, core) = fixture("analysis_v2");
    let source = project_source(&project);
    core.set_analysis(&minimal_analysis("index.mq.md", &source, 2));
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let check = expect_check_ok(
        client
            .check(&project, &entry, &CheckOptions::default())
            .unwrap(),
    );
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        &check.analysis_bytes,
        entry.as_str(),
        Some(&source),
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn retired_revision_3_shape_is_rejected_with_a_rebuild_hint() {
    let (project, core) = fixture("analysis_old_shape");
    let source = project_source(&project);
    let mut analysis: serde_json::Value =
        serde_json::from_str(&minimal_analysis("index.mq.md", &source, 3)).unwrap();
    // An old same-revision artifact has no source/document/anchors/entry id.
    let module = analysis["modules"][0].as_object_mut().unwrap();
    module.remove("source");
    module.remove("document");
    module.remove("anchors");
    analysis["project"]
        .as_object_mut()
        .unwrap()
        .remove("entry_module_id");
    core.set_analysis(&analysis.to_string());
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let check = expect_check_ok(
        client
            .check(&project, &entry, &CheckOptions::default())
            .unwrap(),
    );
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        &check.analysis_bytes,
        entry.as_str(),
        Some(&source),
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert!(err.to_string().contains("revision 3"), "{err}");
}

#[test]
fn source_digest_mismatch_is_not_silently_accepted() {
    let (project, core) = fixture("analysis_digest_mismatch");
    let source = project_source(&project);
    core.set_analysis(&minimal_analysis("index.mq.md", "# another document\n", 3));
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let check = expect_check_ok(
        client
            .check(&project, &entry, &CheckOptions::default())
            .unwrap(),
    );
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        &check.analysis_bytes,
        entry.as_str(),
        Some(&source),
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.source_mismatch");
}

#[test]
fn tool_api_revision_environment_is_an_exact_expectation() {
    // Pure validation: no process-global environment mutation (which would race
    // with tests running in parallel).
    use marqdo_view::protocol::tool_v1;
    assert!(tool_v1::validate_api_revision_env(None).is_ok());
    assert!(tool_v1::validate_api_revision_env(Some("1")).is_ok());
    let err = tool_v1::validate_api_revision_env(Some("2")).unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert!(tool_v1::validate_api_revision_env(Some("nope")).is_err());
}

#[test]
fn installed_tool_api_revision_is_enforced_at_the_process_boundary() {
    let (project, core) = fixture("api_revision_cli");
    core.set_analysis(&minimal_analysis(
        "index.mq.md",
        &project_source(&project),
        3,
    ));
    let script = core.script.to_str().unwrap().to_string();
    let dir = project.dir.to_str().unwrap().to_string();
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[("MARQDO_BIN", None), ("MARQDO_API_REVISION", Some("2"))],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("view.protocol_incompatible"),
        "stderr: {stderr}"
    );
}

#[test]
fn cli_verify_end_to_end_against_fake_core() {
    let (project, core) = fixture("verify_cli");
    let source = project_source(&project);
    core.set_analysis(&minimal_analysis("index.mq.md", &source, 3));
    let script = core.script.to_str().unwrap().to_string();
    let dir = project.dir.to_str().unwrap().to_string();
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[("MARQDO_BIN", None), ("MARQDO_API_REVISION", None)],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stdout.contains("result: PASS"), "{stdout}");
    assert!(stdout.contains("marqdo 3.0.0-rc.4"), "{stdout}");
    assert!(!stdout.contains(&dir), "absolute path leaked: {stdout}");
}

#[test]
fn cli_verify_rejects_schema_v2_and_digest_mismatch() {
    let (project, core) = fixture("verify_negative");
    let source = project_source(&project);
    let script = core.script.to_str().unwrap().to_string();
    let dir = project.dir.to_str().unwrap().to_string();

    core.set_analysis(&minimal_analysis("index.mq.md", &source, 2));
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[("MARQDO_BIN", None)],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("view.protocol_incompatible"));

    // A different, internally consistent document: the exact artifact does not
    // bind to the checked source. This is a source-mismatch, not a silent retry.
    core.set_analysis(&minimal_analysis(
        "index.mq.md",
        "# a different but internally consistent document\n",
        3,
    ));
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[("MARQDO_BIN", None)],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("view.source_mismatch"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // A forged source_digest is an internal artifact inconsistency.
    let mut analysis: serde_json::Value =
        serde_json::from_str(&minimal_analysis("index.mq.md", &source, 3)).unwrap();
    analysis["modules"][0]["source_digest"] = serde_json::json!("0".repeat(64));
    core.set_analysis(&analysis.to_string());
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[("MARQDO_BIN", None)],
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("view.analysis_failed"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn expect_check_ok(response: CheckResponse) -> marqdo_view::core_client::CheckOutput {
    assert!(response.success, "check failed: {:?}", response.diagnostics);
    response.analysis.expect("analysis on success")
}

#[test]
fn unknown_analysis_schema_name_or_major_is_rejected() {
    // N02: an artifact with the wrong schema identity or an unknown major is a
    // protocol error; there is no guess or fallback decoder.
    let source = "# @knowledge fixture\n";
    let mut analysis: serde_json::Value =
        serde_json::from_str(&minimal_analysis("index.mq.md", source, 3)).unwrap();
    analysis["schema"] = serde_json::json!("marqdo.analysis.legacy");
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        analysis.to_string().as_bytes(),
        "index.mq.md",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");

    let mut analysis: serde_json::Value =
        serde_json::from_str(&minimal_analysis("index.mq.md", source, 3)).unwrap();
    analysis["schema_version"] = serde_json::json!(5);
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        analysis.to_string().as_bytes(),
        "index.mq.md",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
}

#[test]
fn missing_required_analysis_fields_are_rejected() {
    // N03: required fields are not optional; a missing module/document/unit field
    // must never decode as an empty, successful document.
    for field in ["modules", "diagnostics"] {
        let source = "# @knowledge fixture\n";
        let mut analysis: serde_json::Value =
            serde_json::from_str(&minimal_analysis("index.mq.md", source, 3)).unwrap();
        analysis.as_object_mut().unwrap().remove(field);
        let err = marqdo_view::protocol::analysis_v3::verify_analysis(
            analysis.to_string().as_bytes(),
            "index.mq.md",
            None,
        )
        .unwrap_err();
        assert_eq!(err.code(), "view.protocol_incompatible", "field {field}");
    }
    let source = "# @knowledge fixture\n";
    let mut analysis: serde_json::Value =
        serde_json::from_str(&minimal_analysis("index.mq.md", source, 3)).unwrap();
    analysis["modules"][0]
        .as_object_mut()
        .unwrap()
        .remove("functions");
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        analysis.to_string().as_bytes(),
        "index.mq.md",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
}

#[test]
fn unknown_module_and_unit_references_are_rejected() {
    // N08: resolved references, imports, functions and anchors must point at
    // records that exist in the same artifact.
    let source = "# @knowledge fixture\n";
    let base = minimal_analysis("index.mq.md", source, 3);
    let module_id = serde_json::from_str::<serde_json::Value>(&base).unwrap()["modules"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut analysis: serde_json::Value = serde_json::from_str(&base).unwrap();
    analysis["modules"][0]["references"] = serde_json::json!([{
        "source": module_id,
        "source_unit": format!("{module_id}#missing"),
        "url": "./theory.mq.md",
        "span": {"start": 0, "end": 1},
        "state": "resolved",
        "target_kind": "module",
        "target_module": module_id,
        "target_unit": null,
        "candidate_package_instance_id": "root:fixture@0.1.0",
        "candidate_path": "theory.mq.md",
        "fragment": null,
        "candidates": []
    }]);
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        analysis.to_string().as_bytes(),
        "index.mq.md",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.analysis_failed");

    let mut analysis: serde_json::Value = serde_json::from_str(&base).unwrap();
    analysis["modules"][0]["imports"] = serde_json::json!([{
        "alias": "theory",
        "target": "./theory.mq.md",
        "resolved": "root:fixture@0.1.0::theory.mq.md",
        "span": {"start": 0, "end": 1}
    }]);
    let err = marqdo_view::protocol::analysis_v3::verify_analysis(
        analysis.to_string().as_bytes(),
        "index.mq.md",
        None,
    )
    .unwrap_err();
    assert_eq!(err.code(), "view.analysis_failed");
}

#[test]
fn strict_mode_run_request_is_rejected_by_view_validation() {
    // N13: View never passes a `strict` mode to the CLI. The request schema
    // rejects it before any process starts (no retry with project).
    let body = serde_json::json!({
        "document": "index.mq.md",
        "target": {"kind": "entry"},
        "analysis_digest": format!("sha256:{}", "0".repeat(64)),
        "mode": "strict",
        "grants": {"read": [], "write": []},
        "client_request_id": "strict-1"
    })
    .to_string();
    let err = marqdo_view::execution::request::validate_request(body.as_bytes()).unwrap_err();
    assert!(
        matches!(err.code(), "view.run_invalid" | "view.run_denied"),
        "strict mode must be refused before spawn, got {}",
        err.code()
    );
    assert_eq!(err.exit_code(), 2);
}
