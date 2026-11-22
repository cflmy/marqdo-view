//! CLI surface tests: this program owns its own help/subcommands and refuses
//! unsafe defaults. Exit codes follow RFC-VIEW-0001 §10.2.

mod common;

use common::run_view;

fn clean_env() -> Vec<(&'static str, Option<&'static str>)> {
    vec![
        ("MARQDO_BIN", None),
        ("MARQDO_TOOL_PROTOCOL", None),
        ("MARQDO_PROTOCOL_ANALYSIS", None),
        ("MARQDO_PROTOCOL_TRACE", None),
    ]
}

#[test]
fn help_comes_from_view_not_core() {
    let cwd = std::env::current_dir().unwrap();
    let output = run_view(&["--help"], &cwd, &clean_env());
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    for needle in ["marqdo-view", "export", "verify", "PATH", "--marqdo-bin"] {
        assert!(
            stdout.contains(needle),
            "missing {needle} in help:\n{stdout}"
        );
    }
    // The Core's own help wording must not leak here.
    assert!(!stdout.contains("Statically check a document"));
}

#[test]
fn version_is_the_tool_version() {
    let cwd = std::env::current_dir().unwrap();
    let output = run_view(&["--version"], &cwd, &clean_env());
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("marqdo-view 0.1.0"), "{stdout}");
}

#[test]
fn export_requires_an_output_directory() {
    let cwd = std::env::current_dir().unwrap();
    let output = run_view(&["export", "example.mq.md"], &cwd, &clean_env());
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn non_loopback_bind_is_refused_by_default() {
    let cwd = std::env::current_dir().unwrap();
    let output = run_view(&["--host", "0.0.0.0"], &cwd, &clean_env());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("view.bind_denied"), "{stderr}");
    assert!(stderr.contains("--unsafe-expose"), "{stderr}");
}

#[test]
fn browse_on_missing_path_fails_without_starting_a_server() {
    let cwd = std::env::current_dir().unwrap();
    let output = run_view(
        &["definitely-missing.mq.md", "--no-open"],
        &cwd,
        &clean_env(),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("view.io"));
}

#[test]
fn degradation_without_core_keeps_prose_readable() {
    // The reader must degrade to prose-only with an explicit status, never
    // invent semantic annotations or a fake run state.
    let project = common::TempProject::new("cli-degrade-no-core");
    project.marqdo_project("# @knowledge 标题\n\n正文 **加粗**。\n");
    let context = marqdo_view::app::AppContext::open(&project.dir, None).unwrap();
    let relative = context.project().entry().relative.clone();
    let model = context.load(&relative).unwrap();
    assert_eq!(model.source_status.data_value(), "core-unavailable");
    assert!(model.semantic.is_none());
    assert!(model.document.plain_text().contains("正文"));
    assert!(model.status_note.is_some());
}

#[test]
fn verify_without_core_is_an_argument_error() {
    let project = common::TempProject::new("cli-verify-no-core");
    project.marqdo_project("# @knowledge fixture\n");
    let output = run_view(&["verify", "index.mq.md"], &project.dir, &clean_env());
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("view.core_missing"), "{stderr}");
}

#[test]
fn missing_document_is_an_io_error() {
    let project = common::TempProject::new("cli-missing-doc");
    let output = run_view(&["verify", "missing.mq.md"], &project.dir, &clean_env());
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("view.io"));
}
