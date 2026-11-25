//! Rendering contract tests: one renderer, safe Markdown, exact source view,
//! verified Analysis v3 annotations and Live/Export link differences.

use marqdo_view::config::ViewConfig;
use marqdo_view::protocol::analysis_v3;
use marqdo_view::reader::assemble::{build, BuildInput};
use marqdo_view::reader::markdown::{DocumentIdentity, DocumentModel};
use marqdo_view::reader::model::{Navigation, SourceStatus};
use marqdo_view::reader::render::{ReaderRenderer, RenderContext};
use marqdo_view::reader::source::{Project, SafeRelativePath};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn example(
    name: &str,
    entry: &str,
    golden: &str,
) -> (marqdo_view::reader::model::PresentationModel, String) {
    let project = Project::open(&repo_root().join("examples").join(name)).unwrap();
    let analysis = std::fs::read(repo_root().join(golden)).unwrap();
    let verified = analysis_v3::verify_analysis(&analysis, entry, None)
        .unwrap_or_else(|err| panic!("golden does not verify for {name}: {err}"));
    let source = verified.projection.source.clone();
    assert_eq!(
        verified.projection.source_digest,
        marqdo_view::reader::source::sha256_hex(source.as_bytes())
    );
    let entry_path = SafeRelativePath::parse(entry).unwrap();
    let document = DocumentModel::parse(source.clone(), DocumentIdentity::placeholder(&entry_path));
    let config = ViewConfig::load(project.root()).unwrap_or_default();
    let navigation = Navigation::from_project(&project, &config, &entry_path);
    let model = build(BuildInput {
        document,
        analysis: Some(&verified),
        status: SourceStatus::StaticOk,
        status_note: None,
        diagnostics: Vec::new(),
        navigation,
        core_version: Some("3.0.0-rc.4".to_string()),
        run: None,
    });
    (model, source)
}

fn control_flow() -> (marqdo_view::reader::model::PresentationModel, String) {
    example(
        "control-flow",
        "index.mq.md",
        "tests/golden/analysis-v3/control-flow.json",
    )
}

fn mixed_modes() -> (marqdo_view::reader::model::PresentationModel, String) {
    example(
        "mixed-modes",
        "index.mq.md",
        "tests/golden/analysis-v3/mixed-modes.json",
    )
}

fn tables_math() -> (marqdo_view::reader::model::PresentationModel, String) {
    example(
        "tables-math",
        "index.mq.md",
        "tests/golden/analysis-v3/tables-math.json",
    )
}

fn welcome() -> (marqdo_view::reader::model::PresentationModel, String) {
    example(
        "welcome",
        "00-welcome.mq.md",
        "tests/golden/analysis-v3/welcome.json",
    )
}

fn render_live(model: &marqdo_view::reader::model::PresentationModel) -> String {
    ReaderRenderer::new()
        .render(model, &RenderContext::live())
        .html
}

fn render_static(model: &marqdo_view::reader::model::PresentationModel, depth: usize) -> String {
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
        if let Some(id_part) = after_tag.1.split_once("id=\"") {
            if let Some((id, _)) = id_part.1.split_once('"') {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

#[test]
fn page_has_exactly_one_theme_and_one_script() {
    let (model, _) = mixed_modes();
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
    assert!(html.contains("计算笔记"));
    assert!(html.contains("静态阅读"));
}

#[test]
fn semantic_annotations_come_from_analysis_and_are_unique() {
    let (model, _) = mixed_modes();
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
    let (model, source) = tables_math();
    let html = render_live(&model);
    assert!(html.contains("<table>"));
    assert!(html.contains("mq-table-wrap"));
    assert!(html.contains("mq-math-block"));
    assert!(html.contains("代码围栏永远是惰性资源"));
    assert!(html.contains("mq-codeblock"));
    assert!(html.contains("data-live=\"true\""));
    assert!(!html.contains("RUNNING"));
    assert!(source.contains("```python"));
    assert!(html.contains("mq-raw-source"));
}

#[test]
fn welcome_hero_only_when_configured() {
    let (welcome, _) = welcome();
    let html = render_live(&welcome);
    assert!(welcome.navigation.welcome);
    assert!(html.contains("mq-welcome"));
    assert!(html.contains("WELCOME TO"));
    assert!(html.contains("代码即文档 · 文档即知识库 · 知识即可执行"));

    let (ordinary, _) = tables_math();
    let html = render_live(&ordinary);
    assert!(!ordinary.navigation.welcome);
    assert!(!html.contains("mq-welcome"));
}

#[test]
fn live_and_static_share_content_but_rewrite_links() {
    let (model, _) = welcome();
    let live = render_live(&model);
    let static_site = render_static(&model, 0);
    assert!(live.contains("/file?path=guide.mq.md"));
    assert!(static_site.contains("pages/guide.mq.md.html"));
    assert!(!static_site.contains("/file?path="));
    for needle in ["欢迎来到 Marqdo", "代码即文档", "mq-document"] {
        assert!(live.contains(needle), "live misses {needle}");
        assert!(static_site.contains(needle), "static misses {needle}");
    }
    assert!(static_site.contains("href=\"assets/reader."));
    assert!(live.contains("href=\"/assets/reader."));
}

#[test]
fn static_pages_use_parent_relative_assets_and_links() {
    let (model, _) = welcome();
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
    let project = Project::open(&repo_root().join("examples/tables-math")).unwrap();
    let navigation = Navigation::from_project(
        &project,
        &ViewConfig::default(),
        &SafeRelativePath::parse("index.mq.md").unwrap(),
    );
    let model = build(BuildInput {
        document,
        analysis: None,
        status: SourceStatus::CoreUnavailable,
        status_note: Some("test".into()),
        diagnostics: Vec::new(),
        navigation,
        core_version: None,
        run: None,
    });
    let html = render_live(&model);
    assert!(!html.contains("<script>alert"));
    assert!(!html.contains("<img src=x"));
    assert!(!html.contains("<svg onload"));
    assert!(!html.contains("href=\"javascript:"));
    assert!(!html.contains("href=\"data:text/html"));
    assert!(!html.contains("onerror=\"") && !html.contains("onload=\""));
    assert!(html.contains("&lt;script&gt;alert"));
    assert!(html.contains("https://example.com/ok"));
    assert!(html.contains("已忽略原始 HTML"));
    assert!(html.contains("正文"));
}

#[test]
fn rendering_is_deterministic() {
    let (model, _) = control_flow();
    assert_eq!(render_live(&model), render_live(&model));
}

#[test]
fn run_controls_are_explicitly_unavailable_before_an_explicit_run() {
    let (model, _) = control_flow();
    let html = render_live(&model);
    assert!(html.contains("data-live=\"true\""));
    assert!(
        html.contains("data-disabled=\"true\"") || html.contains("aria-disabled"),
        "run control must have an explicit disabled marker"
    );
    assert!(!html.contains("data-run-state=\"running\""));
    assert!(!html.contains("data-run-state=\"succeeded\""));
}

#[test]
fn reader_exposes_simple_theme_toggle_and_no_variable_or_memory_tabs() {
    let (model, _) = welcome();
    let html = render_live(&model);
    assert!(html.contains("data-theme=\"light\""));
    assert!(html.contains("data-action=\"theme\""));
    assert!(!html.contains("data-action=\"variables\""));
    assert!(!html.contains("data-action=\"memory\""));
    assert!(html.contains("data-action=\"unsupported\""));
}

#[test]
fn headings_use_heading_span_while_sections_use_the_complete_span() {
    // ADR-0069: `heading_span` locates the heading node; the complete `span`
    // includes child sections. The renderer must not use the complete span as the
    // heading id source (that would shift ids when a section grows).
    let (model, _) = control_flow();
    let projection = model.semantic.as_ref().unwrap();
    for unit in &projection.units {
        if let Some(heading) = unit.heading_span {
            if unit.span != heading {
                assert!(unit.span.contains(heading));
            }
        }
    }
    let function = &projection.functions[0];
    let html = render_live(&model);
    let expected = marqdo_view::reader::model::anchor_for_span(
        function.heading_span.start,
        function.heading_span.end,
    );
    assert!(
        html.contains(&format!("id=\"{expected}\"")),
        "{expected} missing"
    );
}
