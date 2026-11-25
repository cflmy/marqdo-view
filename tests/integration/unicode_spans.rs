//! Unicode/CRLF/duplicate-text span contract. The only source is the exact
//! `modules[].source` carried by the Analysis v3 artifact; all spans are
//! half-open UTF-8 byte ranges into it.

use marqdo_view::config::ViewConfig;
use marqdo_view::protocol::analysis_v3;
use marqdo_view::reader::assemble::{build, BuildInput};
use marqdo_view::reader::markdown::{DocumentIdentity, DocumentModel};
use marqdo_view::reader::model::{anchor_for_span, Navigation, SourceStatus};
use marqdo_view::reader::render::{ReaderRenderer, RenderContext};
use marqdo_view::reader::source::sha256_hex;
use marqdo_view::reader::source::{Project, SafeRelativePath};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load(golden: &str, relative: &str) -> analysis_v3::VerifiedAnalysis {
    let bytes = std::fs::read(repo_root().join(golden)).unwrap();
    let verified = analysis_v3::verify_analysis(&bytes, relative, None)
        .unwrap_or_else(|err| panic!("{golden} does not verify: {err}"));
    let source = &verified.projection.source;
    assert_eq!(
        verified.projection.source_digest,
        sha256_hex(source.as_bytes())
    );
    verified
}

fn render(verified: &analysis_v3::VerifiedAnalysis, project_dir: &str, relative: &str) -> String {
    let root = repo_root().join(project_dir);
    let project = Project::open(&root).unwrap();
    let entry = SafeRelativePath::parse(relative).unwrap();
    let source = verified.projection.source.clone();
    let document = DocumentModel::parse(source, DocumentIdentity::placeholder(&entry));
    let navigation = Navigation::from_project(&project, &ViewConfig::default(), &entry);
    let model = build(BuildInput {
        document,
        analysis: Some(verified),
        status: SourceStatus::StaticOk,
        status_note: None,
        diagnostics: Vec::new(),
        navigation,
        core_version: Some("3.0.0-rc.4".to_string()),
        run: None,
    });
    ReaderRenderer::new()
        .render(&model, &RenderContext::live())
        .html
}

#[test]
fn unicode_spans_are_exact_and_duplicate_returns_get_distinct_anchors() {
    let verified = load("tests/golden/analysis-v3/unicode-spans.json", "index.mq.md");
    let source = verified.projection.source.clone();
    let function = &verified.projection.functions[0];
    let returns: Vec<_> = function
        .flattened_statements()
        .into_iter()
        .filter(|statement| statement.kind == analysis_v3::StatementKind::Return)
        .collect();
    assert_eq!(returns.len(), 2);
    let first = returns[0].span;
    let second = returns[1].span;
    assert_ne!(first, second);
    let index = marqdo_view::reader::source::SourceIndex::new(&source);
    assert_eq!(index.source_text(&source, first), Some("`return 1`"));
    assert_eq!(index.source_text(&source, second), Some("`return 1`"));
    assert_eq!(first.start, source.find("`return 1`").unwrap() as u64);
    assert_ne!(first.start, second.start);

    let html = render(&verified, "examples/unicode-spans", "index.mq.md");
    // 3 anchored statements: branch header + two returns.
    assert_eq!(html.matches("class=\"mq-anchor\"").count(), 3, "{html}");
    let ids: Vec<&str> = html
        .split("data-source-id=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(ids.len(), unique.len(), "duplicate source ids: {ids:?}");
    assert!(html.contains("中文 🙂 标题"));
    assert!(html.contains("甲🙂"));
}

#[test]
fn crlf_source_keeps_exact_digest_and_line_mapping() {
    let verified = load(
        "tests/golden/analysis-v3/unicode-spans-crlf.json",
        "crlf.mq.md",
    );
    let source = verified.projection.source.clone();
    assert!(source.contains("\r\n"), "fixture must stay CRLF");
    assert_eq!(
        verified.projection.source_digest,
        sha256_hex(source.as_bytes())
    );
    let index = marqdo_view::reader::source::SourceIndex::new(&source);
    for (_, statement) in verified
        .projection
        .all_statements()
        .into_iter()
        .filter(|(_, statement)| statement.kind == analysis_v3::StatementKind::Return)
    {
        assert!(
            index
                .source_text(&source, statement.span)
                .map(|text| text.contains("return"))
                .unwrap_or(false),
            "span {:?} does not slice a return",
            statement.span
        );
    }
    let html = render(&verified, "examples/unicode-spans", "crlf.mq.md");
    assert!(html.contains("CRLF 文档"));
    assert_eq!(
        html.matches("class=\"mq-line-text\"").count(),
        index.line_count()
    );
}

#[test]
fn knowledge_emphasis_never_becomes_an_execution_anchor() {
    let project = Project::open(&repo_root().join("examples/mixed-modes")).unwrap();
    let entry = project.entry().relative.clone();
    let verified = load("tests/golden/analysis-v3/mixed-modes.json", entry.as_str());
    let html = render(&verified, "examples/mixed-modes", entry.as_str());
    // Exactly the four Exec statements are anchored (branch + assign + 2 returns).
    assert_eq!(html.matches("class=\"mq-anchor\"").count(), 4, "{html}");
    let knowledge_bold = "此处加粗是知识内容，不执行。";
    let position = html.find(knowledge_bold).expect("knowledge bold text");
    let before = &html[..position];
    let last_anchor = before.rfind("class=\"mq-anchor\"");
    let last_paragraph = before.rfind("<p>");
    assert!(
        last_anchor.is_none() || last_paragraph > last_anchor,
        "knowledge emphasis received an execution anchor"
    );
}

#[test]
fn heading_ui_uses_heading_span_and_section_span_includes_children() {
    // ADR-0069: heading_span is the heading AST node; the complete span runs
    // through child sections. A section heading id must be derived from its own
    // heading span, so adding a child never moves it.
    let verified = load("tests/golden/analysis-v3/control-flow.json", "index.mq.md");
    let projection = &verified.projection;
    let mut sections_with_children = 0;
    for unit in &projection.units {
        let Some(heading) = unit.heading_span else {
            assert_eq!(unit.kind, analysis_v3::UnitKind::Root);
            continue;
        };
        assert!(unit.span.contains(heading));
        let anchor = anchor_for_span(heading.start, heading.end);
        // A heading id is derived from the heading itself, not the section body.
        assert_ne!(
            anchor,
            anchor_for_span(unit.span.start, unit.span.end),
            "unit {:?} used the complete section span as its heading id",
            unit.id
        );
        if projection
            .units
            .iter()
            .any(|child| child.parent_id.as_deref() == Some(unit.id.as_str()))
        {
            sections_with_children += 1;
            assert!(
                unit.span.end > heading.end,
                "section {:?} complete span does not include its children",
                unit.id
            );
        }
    }
    assert!(
        sections_with_children > 0,
        "fixture needs a section with children"
    );
    let html = render(&verified, "examples/control-flow", "index.mq.md");
    let function_anchor = anchor_for_span(
        projection.functions[0].heading_span.start,
        projection.functions[0].heading_span.end,
    );
    assert!(html.contains(&format!("id=\"{function_anchor}\"")));
}
