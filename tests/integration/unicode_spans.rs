//! Unicode/CRLF/duplicate-text span contract (RFC-VIEW-0001 S-04).
//! Goldens are emitted by the frozen CLI from the exact fixture bytes.

use marqdo_view::config::ViewConfig;
use marqdo_view::protocol::analysis_v2 as semantic;
use marqdo_view::reader::assemble::{self as assemble, BuildInput};
use marqdo_view::reader::markdown::{DocumentIdentity, DocumentModel};
use marqdo_view::reader::model::{Navigation, SourceStatus};
use marqdo_view::reader::render::{ReaderRenderer, RenderContext};
use marqdo_view::reader::source::sha256_hex;
use marqdo_view::reader::source::{Project, SafeRelativePath};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load(path: &str, golden: &str) -> (String, Vec<u8>, semantic::VerifiedAnalysis) {
    let root = repo_root();
    let source = std::fs::read_to_string(root.join(path)).unwrap();
    let index = marqdo_view::reader::source::SourceIndex::new(&source);
    let analysis = std::fs::read(root.join(golden)).unwrap();
    let relative = std::path::Path::new(path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let verified = semantic::verify_analysis(&analysis, &source, &index, &relative).unwrap();
    (source, analysis, verified)
}

fn render(verified: &semantic::VerifiedAnalysis, source: &str, relative: &str) -> String {
    let root = repo_root().join("examples/unicode-spans");
    let project = Project::open(&root).unwrap();
    let entry = SafeRelativePath::parse(relative).unwrap();
    let document = DocumentModel::parse(source.to_string(), DocumentIdentity::placeholder(&entry));
    let navigation = Navigation::from_project(&project, &ViewConfig::default(), &entry);
    let model = assemble::build(BuildInput {
        document,
        analysis: Some(verified),
        status: SourceStatus::StaticOk,
        status_note: None,
        diagnostics: Vec::new(),
        navigation,
        core_version: Some("3.0.0-beta.1".to_string()),
        run: None,
    });
    ReaderRenderer::new()
        .render(&model, &RenderContext::live())
        .html
}

#[test]
fn unicode_spans_are_exact_and_duplicate_returns_get_distinct_anchors() {
    let (source, analysis, verified) = load(
        "examples/unicode-spans/index.mq.md",
        "tests/golden/analysis-v2/unicode-spans.json",
    );
    assert_eq!(
        verified.projection.source_digest,
        sha256_hex(source.as_bytes())
    );
    let function = &verified.projection.functions[0];
    let returns: Vec<_> = function
        .flattened_statements()
        .into_iter()
        .filter(|statement| statement.kind == semantic::StatementKind::Return)
        .collect();
    assert_eq!(returns.len(), 2);
    let first = returns[0].span;
    let second = returns[1].span;
    assert_ne!(first, second);
    let index = marqdo_view::reader::source::SourceIndex::new(&source);
    // Core statement spans cover the whole carrier node (including markers).
    assert_eq!(index.source_text(&source, first), Some("`return 1`"));
    assert_eq!(index.source_text(&source, second), Some("`return 1`"));
    // The carrier text is identical, so only spans can distinguish the two.
    assert_eq!(first.start, source.find("`return 1`").unwrap() as u64);
    assert_ne!(first.start, second.start);

    let html = render(&verified, &source, "index.mq.md");
    let statement_ids: Vec<&str> = html.matches("data-source-id=\"").map(|_| "").collect();
    let _ = statement_ids;
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
    // Emoji and CJK survive both rendering and source mapping.
    assert!(html.contains("中文 🙂 标题"));
    assert!(html.contains("甲🙂"));
    let _ = analysis;
}

#[test]
fn crlf_source_keeps_exact_digest_and_line_mapping() {
    let (source, _analysis, verified) = load(
        "examples/unicode-spans/crlf.mq.md",
        "tests/golden/analysis-v2/unicode-spans-crlf.json",
    );
    assert!(source.contains("\r\n"), "fixture must stay CRLF");
    assert_eq!(
        verified.projection.source_digest,
        sha256_hex(source.as_bytes())
    );
    let index = marqdo_view::reader::source::SourceIndex::new(&source);
    let function = &verified.projection.functions[0];
    for (_, statement) in verified
        .projection
        .all_statements()
        .into_iter()
        .filter(|(_, statement)| statement.kind == semantic::StatementKind::Return)
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
    let html = render(&verified, &source, "crlf.mq.md");
    assert!(html.contains("CRLF 文档"));
    // One list item per source line (SourceIndex counts the trailing line too).
    let source_lines = index.line_count();
    assert_eq!(html.matches("class=\"mq-line-text\"").count(), source_lines);
    let _ = function;
}

#[test]
fn knowledge_emphasis_never_becomes_an_execution_anchor() {
    let root = repo_root();
    let project = Project::open(&root.join("examples/mixed-modes")).unwrap();
    let entry = project.entry().relative.clone();
    let source = project.read_source_text(&entry).unwrap();
    let index = marqdo_view::reader::source::SourceIndex::new(&source);
    let analysis = std::fs::read(root.join("tests/golden/analysis-v2/mixed-modes.json")).unwrap();
    let verified = semantic::verify_analysis(&analysis, &source, &index, entry.as_str()).unwrap();
    assert_eq!(
        verified.projection.source_digest,
        sha256_hex(source.as_bytes())
    );
    let document = DocumentModel::parse(source.clone(), DocumentIdentity::placeholder(&entry));
    let navigation = Navigation::from_project(&project, &ViewConfig::default(), &entry);
    let model = assemble::build(BuildInput {
        document,
        analysis: Some(&verified),
        status: SourceStatus::StaticOk,
        status_note: None,
        diagnostics: Vec::new(),
        navigation,
        core_version: Some("3.0.0-beta.1".to_string()),
        run: None,
    });
    let html = ReaderRenderer::new()
        .render(&model, &RenderContext::live())
        .html;
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
