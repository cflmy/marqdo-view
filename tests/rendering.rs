//! Rendering contract tests: one renderer, safe Markdown, exact source view,
//! verified semantic annotations and Live/Export link differences.

mod common;

use marqdo_view::document::{DocumentIdentity, DocumentModel};
use marqdo_view::presentation::{self, BuildInput, Navigation, SourceStatus};
use marqdo_view::project::{Project, SafeRelativePath, ViewConfig};
use marqdo_view::render::{ReaderRenderer, RenderContext};
use marqdo_view::semantic;

fn example(name: &str) -> (marqdo_view::presentation::PresentationModel, String) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(name);
    let project = Project::open(&root).unwrap();
    let entry = project.entry().relative.clone();
    let source = project.read_source_text(&entry).unwrap();
    let document = DocumentModel::parse(source.clone(), DocumentIdentity::placeholder(&entry));
    let golden_name = name;
    let golden = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/analysis-v2")
            .join(format!("{golden_name}.json")),
    )
    .unwrap_or_else(|err| panic!("missing golden for {name}: {err}"));
    let verified =
        semantic::verify_analysis(&golden, &source, &document.source_index, entry.as_str())
            .unwrap_or_else(|err| panic!("golden does not verify for {name}: {err}"));
    let config = ViewConfig::load(project.root()).unwrap_or_default();
    let navigation = Navigation::from_project(&project, &config, &entry);
    let model = presentation::build(BuildInput {
        document,
        analysis: Some(&verified),
        status: SourceStatus::StaticOk,
        status_note: None,
        diagnostics: Vec::new(),
        navigation,
        core_version: Some("3.0.0-beta.1".to_string()),
    });
    (model, source)
}

fn render_live(model: &marqdo_view::presentation::PresentationModel) -> String {
    ReaderRenderer::new()
        .render(model, &RenderContext::live())
        .html
}

fn render_static(model: &marqdo_view::presentation::PresentationModel, depth: usize) -> String {
    ReaderRenderer::new()
        .render(model, &RenderContext::static_site(depth))
        .html
}

fn heading_ids(html: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for part in html.split("<h").skip(1) {
        let Some(after_tag) = part.split_once(' ') else {
            continue;
        };
        let rest = after_tag.1;
        if let Some(id_part) = rest.split_once("id=\"") {
            if let Some((id, _)) = id_part.1.split_once('"') {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

#[test]
fn page_has_exactly_one_theme_and_one_script() {
    let (model, _) = example("mixed-modes");
    let html = render_live(&model);
    assert_eq!(html.matches("<link rel=\"stylesheet\"").count(), 1);
    assert_eq!(
        html.matches("<script").count(),
        1,
        "no inline scripts allowed"
    );
    assert!(html.contains("<script defer src=\"/assets/reader."));
    assert!(html.contains("Content-Security-Policy"));
    assert!(html.contains("default-src 'none'"));
    assert!(html.contains("data-live=\"true\""));
    // The one source of prose is the Markdown document itself.
    assert!(html.contains("计算笔记"));
    assert!(html.contains("静态阅读"));
}

#[test]
fn semantic_annotations_come_from_analysis_and_are_unique() {
    let (model, _) = example("mixed-modes");
    let html = render_live(&model);
    assert!(html.contains("data-source-status=\"static-ok\""));
    assert!(html.contains("data-statement-kind=\"return\""));
    assert!(html.contains("data-statement-kind=\"branch\""));
    assert!(html.contains("data-statement-kind=\"assign\""));
    assert!(html.contains("mq-mode-badge"));
    assert!(html.contains("EXEC"));
    assert!(html.contains("params(n: Int)"));
    let ids = heading_ids(&html);
    let mut deduped = ids.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(ids.len(), deduped.len(), "duplicate heading ids: {ids:?}");
}

#[test]
fn tables_math_and_code_are_inert_and_readable() {
    let (model, source) = example("tables-math");
    let html = render_live(&model);
    assert!(html.contains("<table>"));
    assert!(html.contains("mq-table-wrap"));
    assert!(html.contains("mq-math-block"));
    assert!(html.contains("代码围栏永远是惰性资源"));
    assert!(html.contains("mq-codeblock"));
    // Inert resources are never executed: no run controls are enabled.
    assert!(html.contains("data-live=\"true\""));
    assert!(!html.contains("RUNNING"));
    // Source is preserved exactly in the textarea (escaped only).
    assert!(source.contains("```python"));
    assert!(html.contains("mq-raw-source"));
}

#[test]
fn welcome_hero_only_when_configured() {
    let (welcome, _) = example("welcome");
    let html = render_live(&welcome);
    assert!(welcome.navigation.welcome);
    assert!(html.contains("mq-welcome"));
    assert!(html.contains("WELCOME TO"));
    assert!(html.contains("代码即文档 · 文档即知识库 · 知识即可执行"));

    let (ordinary, _) = example("tables-math");
    let html = render_live(&ordinary);
    assert!(!ordinary.navigation.welcome);
    assert!(!html.contains("mq-welcome"));
}

#[test]
fn live_and_static_share_content_but_rewrite_links() {
    let (model, _) = example("welcome");
    let live = render_live(&model);
    let static_site = render_static(&model, 0);
    assert!(live.contains("/file?path=guide.mq.md"));
    assert!(static_site.contains("pages/guide.mq.md.html"));
    assert!(!static_site.contains("/file?path="));
    // Same prose and semantics.
    for needle in ["欢迎来到 Marqdo", "代码即文档", "mq-document"] {
        assert!(live.contains(needle));
        assert!(static_site.contains(needle));
    }
    // Assets are relative from the site root.
    assert!(static_site.contains("href=\"assets/reader."));
    assert!(live.contains("href=\"/assets/reader."));
}

#[test]
fn static_pages_use_parent_relative_assets_and_links() {
    let (model, _) = example("welcome");
    let html = render_static(&model, 1);
    assert!(html.contains("href=\"../assets/reader."));
    assert!(html.contains("../pages/guide.mq.md.html"));
}

#[test]
fn malicious_markdown_cannot_inject_markup_or_scripts() {
    let source = r#"# 安全测试

<script>alert('xss')</script>

<img src=x onerror="alert(1)">

[危险链接](javascript:alert(1))

[数据链接](data:text/html;base64,PHNjcmlwdD4=)

[正常链接](https://example.com/ok)

<svg onload="alert(1)"></svg>

正常 **正文** 与 `代码`。
"#
    .to_string();
    let document = DocumentModel::parse(
        source,
        DocumentIdentity {
            project_root_id: String::new(),
            module_id: String::new(),
            relative_path: "index.mq.md".into(),
            source_sha256: String::new(),
        },
    );
    let project = Project::open(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/tables-math"),
    )
    .unwrap();
    let config = ViewConfig::default();
    let navigation = Navigation::from_project(
        &project,
        &config,
        &SafeRelativePath::parse("index.mq.md").unwrap(),
    );
    let model = presentation::build(BuildInput {
        document,
        analysis: None,
        status: SourceStatus::CoreUnavailable,
        status_note: Some("test".into()),
        diagnostics: Vec::new(),
        navigation,
        core_version: None,
    });
    let html = render_live(&model);
    assert!(!html.contains("<script>alert"));
    assert!(!html.contains("<img src=x"));
    assert!(!html.contains("<svg onload"));
    assert!(!html.contains("href=\"javascript:"));
    assert!(!html.contains("href=\"data:text/html"));
    // Ordinary page chrome (logo, dock icons) is allowed, but no tag carries an
    // attacker-supplied event handler.
    assert!(!html.contains("onerror=\"") && !html.contains("onload=\""));
    // The exact source is still available in the source pane, safely escaped.
    assert!(html.contains("&lt;script&gt;alert"));
    assert!(html.contains("https://example.com/ok"));
    assert!(html.contains("已忽略原始 HTML"));
    assert!(html.contains("正文"));
}

#[test]
fn rendering_is_deterministic() {
    let (model, _) = example("control-flow");
    let first = render_live(&model);
    let second = render_live(&model);
    assert_eq!(first, second);
}

#[test]
fn run_controls_are_explicitly_unavailable_before_pr04() {
    let (model, _) = example("control-flow");
    let html = render_live(&model);
    assert!(html.contains("data-live=\"true\""));
    assert!(
        html.contains("data-disabled=\"true\"") || html.contains("aria-disabled"),
        "run control must have an explicit disabled marker"
    );
    // No fake RUNNING/SUCCEEDED marker on a freshly opened document.
    assert!(!html.contains("data-run-state=\"running\""));
    assert!(!html.contains("data-run-state=\"succeeded\""));
}
