//! Static export contract tests: pure read-only, same renderer, offline assets,
//! safe output paths and no execution (RFC-VIEW-0001 §11.3, R-02, X-04).

mod common;

use common::{run_view, sha256_hex, FakeCore, TempProject};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn exported_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn fake_core_for_welcome(name: &str) -> FakeCore {
    let core = FakeCore::new(name);
    let golden = std::fs::read(repo_root().join("tests/golden/analysis-v2/welcome.json")).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());
    core
}

fn export(
    project: &Path,
    out: &Path,
    extra: &[&str],
    core: Option<&FakeCore>,
) -> std::process::Output {
    let mut args = vec![
        "export".to_string(),
        project.to_string_lossy().to_string(),
        "--out".to_string(),
        out.to_string_lossy().to_string(),
    ];
    args.extend(extra.iter().map(|value| value.to_string()));
    let mut core_arg = Vec::new();
    if let Some(core) = core {
        core_arg.push("--marqdo-bin".to_string());
        core_arg.push(core.script.to_string_lossy().to_string());
    }
    let mut all: Vec<&str> = args.iter().map(String::as_str).collect();
    all.extend(core_arg.iter().map(String::as_str));
    run_view(
        &all,
        &repo_root(),
        &[
            ("MARQDO_BIN", None),
            ("MARQDO_TOOL_PROTOCOL", None),
            ("MARQDO_PROTOCOL_ANALYSIS", None),
            ("MARQDO_PROTOCOL_TRACE", None),
        ],
    )
}

fn manifest(out: &Path) -> serde_json::Value {
    let bytes = std::fs::read(out.join("manifest.json")).expect("manifest");
    serde_json::from_slice(&bytes).expect("manifest json")
}

#[test]
fn exports_offline_site_with_shared_renderer_and_relative_links() {
    let project = repo_root().join("examples/welcome");
    let out = exported_dir("export-welcome");
    let core = fake_core_for_welcome("export-core-welcome");
    let output = export(&project, &out, &[], Some(&core));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");

    for file in [
        "index.html",
        "pages/00-welcome.mq.md.html",
        "pages/guide.mq.md.html",
        "manifest.json",
    ] {
        assert!(out.join(file).is_file(), "missing {file}");
    }
    let index = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(index.contains("mq-reader"));
    assert!(index.contains("href=\"assets/reader."));
    assert!(!index.contains("/file?path="));
    assert!(!index.contains("http://127.0.0.1"));
    assert!(index.contains("Content-Security-Policy"));
    assert!(index.contains("script-src 'self' file:"));
    // Shared renderer: the only stylesheet/script are hash-named local assets.
    assert_eq!(index.matches("<link rel=\"stylesheet\"").count(), 1);
    assert_eq!(index.matches("<script").count(), 1);

    let page = std::fs::read_to_string(out.join("pages/guide.mq.md.html")).unwrap();
    assert!(page.contains("href=\"../assets/reader."));
    assert!(page.contains("../pages/00-welcome.mq.md.html"));

    let manifest = manifest(&out);
    assert_eq!(manifest["schema"], "marqdo.view-export");
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["executed"], false);
    assert_eq!(manifest["view"]["name"], "marqdo-view");
    assert!(manifest["pages"].as_array().unwrap().len() >= 2);
    assert!(manifest["assets"].as_array().unwrap().len() >= 5);
    let manifest_text = serde_json::to_string(&manifest).unwrap();
    assert!(
        !manifest_text.contains(&project.to_string_lossy().to_string()),
        "absolute project path leaked into manifest"
    );
    assert!(
        !manifest_text.contains("/root/"),
        "absolute path in manifest"
    );

    // Every manifest file hash matches the bytes on disk.
    for entry in manifest["files"].as_array().unwrap() {
        let rel = entry["path"].as_str().unwrap();
        assert!(!rel.starts_with('/'));
        let bytes = std::fs::read(out.join(rel)).unwrap();
        assert_eq!(sha256_hex(&bytes), entry["sha256"].as_str().unwrap());
    }
    // Assets are exactly the embedded bundle bytes.
    let bundle = marqdo_view::assets::AssetBundle::get();
    let css = std::fs::read(out.join("assets").join(bundle.hashed_css_name())).unwrap();
    assert_eq!(css, bundle.css_bytes());

    // Export never spawned a run.
    let argv = core.argv_lines();
    assert!(!argv.iter().any(|line| line == "arg=run"));
    assert!(!core.dir.join("run-was-called").exists());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn export_never_executes_side_effects() {
    let project = repo_root().join("examples/control-flow");
    let out = exported_dir("export-no-exec");
    let core = FakeCore::new("export-core-noexec");
    let golden =
        std::fs::read(repo_root().join("tests/golden/analysis-v2/control-flow.json")).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());
    let output = export(&project, &out, &[], Some(&core));
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !core.dir.join("run-was-called").exists(),
        "export invoked Core run"
    );
    assert!(!core.argv_lines().iter().any(|line| line == "arg=run"));
    // Export page explicitly has no run panel and no fake RUNNING state.
    let index = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(!index.contains("mq-run-panel"));
    assert!(!index.contains("data-run-state=\"running\""));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn no_annotations_keeps_prose_but_removes_anchors() {
    let project = repo_root().join("examples/mixed-modes");
    let out = exported_dir("export-no-annotations");
    let core = FakeCore::new("export-core-noannot");
    let golden =
        std::fs::read(repo_root().join("tests/golden/analysis-v2/mixed-modes.json")).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());
    let output = export(&project, &out, &["--no-annotations"], Some(&core));
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let index = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(index.contains("data-annotations=\"off\""));
    assert!(!index.contains("class=\"mq-anchor\""));
    assert!(index.contains("计算笔记"), "prose must remain readable");
    assert!(index.contains("mq-document"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn refuses_to_export_inside_the_project_root() {
    // Using a TempProject keeps the committed examples untouched.
    let project = TempProject::new("export-inside-project");
    project.write(
        "marqdo.yaml",
        "project:\n  name: export-inside\n  version: 0.1.0\n",
    );
    project.write("index.mq.md", "# @knowledge hi\n");
    let core = FakeCore::new("export-core-inside");
    let out = project.path("site");
    let output = export(&project.dir, &out, &[], Some(&core));
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("view.export_failed"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn refuses_to_overwrite_unrelated_non_empty_directory() {
    let project = repo_root().join("examples/welcome");
    let out = exported_dir("export-unrelated-dir");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("important.txt"), "do not delete").unwrap();
    let core = fake_core_for_welcome("export-core-unrelated");
    let output = export(&project, &out, &[], Some(&core));
    assert_eq!(output.status.code(), Some(1));
    assert!(out.join("important.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(out.join("important.txt")).unwrap(),
        "do not delete"
    );
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn replaces_a_previous_export_atomically() {
    let project = repo_root().join("examples/welcome");
    let out = exported_dir("export-replace");
    let core = fake_core_for_welcome("export-core-replace");
    let first = export(&project, &out, &[], Some(&core));
    assert_eq!(first.status.code(), Some(0));
    let first_manifest = manifest(&out);
    let second = export(&project, &out, &[], Some(&core));
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_manifest = manifest(&out);
    assert_eq!(first_manifest["schema"], second_manifest["schema"]);
    assert!(out.join("index.html").is_file());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn real_frozen_core_export_marks_both_pages_static_ok() {
    let Ok(core_bin) = std::env::var("MARQDO_FROZEN_BIN") else {
        eprintln!("MARQDO_FROZEN_BIN not set; skipping real-core export test");
        return;
    };
    let project = repo_root().join("examples/welcome");
    let out = exported_dir("export-real-core");
    let output = run_view(
        &[
            "export",
            project.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        &repo_root(),
        &[
            ("MARQDO_BIN", Some(core_bin.as_str())),
            ("MARQDO_TOOL_PROTOCOL", None),
            ("MARQDO_PROTOCOL_ANALYSIS", None),
            ("MARQDO_PROTOCOL_TRACE", None),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for page in [
        "index.html",
        "pages/00-welcome.mq.md.html",
        "pages/guide.mq.md.html",
    ] {
        let html = std::fs::read_to_string(out.join(page)).unwrap();
        assert!(
            html.contains("data-source-status=\"static-ok\""),
            "{page} not static-ok"
        );
        assert!(!html.contains("/file?path="));
    }
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn exported_html_has_no_remote_subresources() {
    let project = repo_root().join("examples/tables-math");
    let out = exported_dir("export-offline");
    let core = FakeCore::new("export-core-offline");
    let golden =
        std::fs::read(repo_root().join("tests/golden/analysis-v2/tables-math.json")).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());
    let output = export(&project, &out, &[], Some(&core));
    assert_eq!(output.status.code(), Some(0));
    for file in ["index.html", "pages/index.mq.md.html"] {
        let html = std::fs::read_to_string(out.join(file)).unwrap();
        assert!(!html.contains("<script src=\"http"));
        assert!(!html.contains("<link rel=\"stylesheet\" href=\"http"));
        assert!(!html.contains("@import url(http"));
        assert!(!html.contains("url(http"));
        assert!(!html.contains("<img src=\"http"));
        // An external *anchor* is allowed: it is user navigation, not a
        // subresource and is never fetched automatically.
        assert!(html.contains("https://github.com/cflmy/marqdo-cli"));
    }
    let _ = std::fs::remove_dir_all(&out);
}
