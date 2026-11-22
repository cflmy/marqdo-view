//! Contract tests against a real fake-core executable: argv shape, exit codes,
//! stderr handling, temp artifacts and process-tree cleanup.
//!
//! The fake core is a shell script, not a mocked Rust trait, so everything at
//! the process boundary is exercised for real.

use crate::common;

use common::{minimal_analysis, sha256_hex, FakeCore, TempProject};
use marqdo_view::core_client::{CheckOptions, CheckResponse, CoreClient, RunOptions};
use marqdo_view::reader::source::Project;

fn fixture(name: &str) -> (TempProject, FakeCore) {
    let project = TempProject::new(name);
    project.marqdo_project("# @knowledge fixture\n");
    let core = FakeCore::new(&format!("{name}-core"));
    (project, core)
}

#[test]
fn check_returns_exact_analysis_bytes_and_digest() {
    let (project, core) = fixture("check_ok");
    let source = std::fs::read(project.path("index.mq.md")).unwrap();
    let analysis = minimal_analysis("index.mq.md", &sha256_hex(&source), 2);
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
    // No `--refs` unless requested.
    assert!(!args.contains(&"--refs".to_string()));
}

#[test]
fn check_refs_is_forwarded_as_a_stricter_flag() {
    let (project, core) = fixture("check_refs");
    let source = std::fs::read(project.path("index.mq.md")).unwrap();
    core.set_analysis(&minimal_analysis("index.mq.md", &sha256_hex(&source), 2));
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
fn run_returns_json_and_raw_trace() {
    let (project, core) = fixture("run_ok");
    core.set_run(
        0,
        r#"{"command":"run","success":true,"stdout":"ok","stderr":"","trace":[]}"#,
        Some(r#"{"schema":"marqdo.trace","schema_version":2,"events":[]}"#),
    );
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let output = client
        .run(&project, &entry, &RunOptions::default())
        .unwrap();
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.json["success"], true);
    let trace = output.trace_bytes.expect("trace bytes");
    assert_eq!(output.trace_sha256.unwrap(), sha256_hex(&trace));
    // Mode is always explicit; pure is the View default.
    let args = core.last_args();
    assert!(args.contains(&"--mode".to_string()));
    let mode_index = args.iter().position(|a| a == "--mode").unwrap();
    assert_eq!(args[mode_index + 1], "pure");
    assert!(args.contains(&"--json".to_string()));
    assert!(args.contains(&"--trace-file".to_string()));
}

#[test]
fn run_without_trace_file_reports_no_trace() {
    let (project, core) = fixture("run_no_trace");
    core.set_run(
        0,
        r#"{"command":"run","success":true,"stdout":"","stderr":"","trace":[]}"#,
        None,
    );
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let output = client
        .run(&project, &entry, &RunOptions::default())
        .unwrap();
    assert!(output.trace_bytes.is_none());
    assert!(output.trace_sha256.is_none());
}

#[test]
fn run_invalid_json_is_a_protocol_io_error() {
    let (project, core) = fixture("run_bad_json");
    core.set_run(0, "this is not json", None);
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let err = client
        .run(&project, &entry, &RunOptions::default())
        .unwrap_err();
    assert_eq!(err.code(), "view.io");
    assert!(err.to_string().contains("exactly one JSON"));
}

#[test]
fn timeout_kills_the_process_group_and_reports_timeout() {
    let (project, core) = fixture("run_timeout");
    core.set_sleep_seconds(5);
    core.set_run(0, r#"{"command":"run","success":true}"#, None);
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let started = std::time::Instant::now();
    let err = client
        .run(
            &project,
            &entry,
            &RunOptions {
                view_timeout: Some(std::time::Duration::from_millis(300)),
                ..Default::default()
            },
        )
        .unwrap_err();
    let elapsed = started.elapsed();
    assert_eq!(err.code(), "view.core_timeout");
    assert!(
        elapsed < std::time::Duration::from_secs(3),
        "timeout took {elapsed:?}"
    );
}

#[test]
fn incompatible_product_version_is_rejected_before_any_work() {
    let (project, core) = fixture("version_old");
    core.set_version("marqdo 2.9.1\n");
    let client = CoreClient::with_bin(&core.script).unwrap();
    let err = client.negotiate().unwrap_err();
    assert_eq!(err.code(), "view.protocol_incompatible");
    assert_eq!(err.exit_code(), 2);
    // Only `--version` was invoked; no check/run happened.
    assert_eq!(core.last_args(), vec!["--version".to_string()]);
    let _ = project;
}

#[test]
fn missing_emit_after_success_is_an_analysis_failure() {
    // Fake core exits 0 but writes nothing: never accept a phantom analysis.
    let (project, core) = fixture("check_no_emit");
    let client = CoreClient::with_bin(&core.script).unwrap();
    let project = Project::open(&project.path("index.mq.md")).unwrap();
    let entry = project.entry().relative.clone();
    let err = client
        .check(&project, &entry, &CheckOptions::default())
        .unwrap_err();
    assert_eq!(err.code(), "view.analysis_failed");
}

fn expect_check_ok(response: CheckResponse) -> marqdo_view::core_client::CheckOutput {
    assert!(response.success, "check failed: {:?}", response.diagnostics);
    response.analysis.expect("analysis on success")
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
    // The raw absolute `file` field is not part of the normalized type at all.
    let rendered = serde_json::to_string(&response.diagnostics).unwrap();
    assert!(!rendered.contains("/home/secret"), "{rendered}");
}

#[test]
fn cli_verify_end_to_end_against_fake_core() {
    let (project, core) = fixture("verify_cli");
    let source = std::fs::read(project.path("index.mq.md")).unwrap();
    core.set_analysis(&minimal_analysis("index.mq.md", &sha256_hex(&source), 2));
    let script = core.script.to_str().unwrap().to_string();
    let dir = project.dir.to_str().unwrap().to_string();
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[
            ("MARQDO_BIN", None),
            ("MARQDO_TOOL_PROTOCOL", None),
            ("MARQDO_PROTOCOL_ANALYSIS", None),
            ("MARQDO_PROTOCOL_TRACE", None),
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert!(stdout.contains("result: PASS"), "{stdout}");
    assert!(stdout.contains("marqdo 3.0.0-beta.1"), "{stdout}");
    assert!(!stdout.contains(&dir), "absolute path leaked: {stdout}");
}

#[test]
fn cli_verify_rejects_schema_v3_and_digest_mismatch() {
    let (project, core) = fixture("verify_negative");
    let source = std::fs::read(project.path("index.mq.md")).unwrap();
    let script = core.script.to_str().unwrap().to_string();
    let dir = project.dir.to_str().unwrap().to_string();

    core.set_analysis(&minimal_analysis("index.mq.md", &sha256_hex(&source), 3));
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[("MARQDO_BIN", None)],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("view.protocol_incompatible"));

    core.set_analysis(&minimal_analysis("index.mq.md", &"0".repeat(64), 2));
    let output = common::run_view(
        &["verify", "index.mq.md", "--marqdo-bin", &script],
        std::path::Path::new(&dir),
        &[("MARQDO_BIN", None)],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("view.source_mismatch"));
}
