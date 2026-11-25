//! Reader-oriented Markdown document model.
//!
//! Comrak (the same parser version as the frozen Markdown profile) parses the
//! source for **typography only**; the resulting tree is converted into a
//! View-owned `DocBlock`/`Inline` model that keeps exact UTF-8 byte spans.
//! No semantic decision (which node is executable) is taken here — that comes
//! exclusively from validated Analysis v3 (`protocol::analysis_v3`).

use crate::reader::source::SafeRelativePath;
use crate::reader::source::{sha256_hex, SourceIndex, SourceSpan};
use comrak::nodes::{ListType, NodeValue, Sourcepos, TableAlignment};
use comrak::{parse_document, Arena, Options};

/// Class of a table column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlign {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone)]
pub enum Inline {
    Text {
        span: SourceSpan,
        literal: String,
    },
    Code {
        span: SourceSpan,
        literal: String,
        ticks: usize,
    },
    Strong {
        span: SourceSpan,
        children: Vec<Inline>,
    },
    Emph {
        span: SourceSpan,
        children: Vec<Inline>,
    },
    Strikethrough {
        span: SourceSpan,
        children: Vec<Inline>,
    },
    Link {
        span: SourceSpan,
        url: String,
        title: String,
        children: Vec<Inline>,
    },
    Image {
        span: SourceSpan,
        url: String,
        title: String,
        alt: String,
    },
    Math {
        span: SourceSpan,
        literal: String,
        display: bool,
    },
    HtmlInline {
        span: SourceSpan,
        raw: String,
    },
    SoftBreak {
        span: SourceSpan,
    },
    LineBreak {
        span: SourceSpan,
    },
    Other {
        span: SourceSpan,
    },
}

impl Inline {
    pub fn span(&self) -> SourceSpan {
        match self {
            Inline::Text { span, .. }
            | Inline::Code { span, .. }
            | Inline::Strong { span, .. }
            | Inline::Emph { span, .. }
            | Inline::Strikethrough { span, .. }
            | Inline::Link { span, .. }
            | Inline::Image { span, .. }
            | Inline::Math { span, .. }
            | Inline::HtmlInline { span, .. }
            | Inline::SoftBreak { span }
            | Inline::LineBreak { span }
            | Inline::Other { span } => *span,
        }
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        self.push_plain_text(&mut out);
        out
    }

    fn push_plain_text(&self, out: &mut String) {
        match self {
            Inline::Text { literal, .. } | Inline::Code { literal, .. } => out.push_str(literal),
            Inline::Strong { children, .. }
            | Inline::Emph { children, .. }
            | Inline::Strikethrough { children, .. }
            | Inline::Link { children, .. } => {
                for child in children {
                    child.push_plain_text(out);
                }
            }
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Math { literal, .. } => out.push_str(literal),
            Inline::SoftBreak { .. } | Inline::LineBreak { .. } => out.push(' '),
            Inline::HtmlInline { .. } | Inline::Other { .. } => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum DocBlock {
    Heading {
        level: u8,
        span: SourceSpan,
        content: Vec<Inline>,
    },
    Paragraph {
        span: SourceSpan,
        content: Vec<Inline>,
    },
    List {
        ordered: bool,
        start: Option<usize>,
        tight: bool,
        span: SourceSpan,
        items: Vec<DocBlock>,
    },
    ListItem {
        span: SourceSpan,
        task: Option<bool>,
        children: Vec<DocBlock>,
    },
    BlockQuote {
        span: SourceSpan,
        children: Vec<DocBlock>,
    },
    CodeBlock {
        span: SourceSpan,
        info: String,
        literal: String,
    },
    Table {
        span: SourceSpan,
        alignments: Vec<TableAlign>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    MathBlock {
        span: SourceSpan,
        literal: String,
    },
    ThematicBreak {
        span: SourceSpan,
    },
    HtmlBlock {
        span: SourceSpan,
        literal: String,
    },
    Other {
        span: SourceSpan,
    },
}

impl DocBlock {
    pub fn span(&self) -> SourceSpan {
        match self {
            DocBlock::Heading { span, .. }
            | DocBlock::Paragraph { span, .. }
            | DocBlock::List { span, .. }
            | DocBlock::ListItem { span, .. }
            | DocBlock::BlockQuote { span, .. }
            | DocBlock::CodeBlock { span, .. }
            | DocBlock::Table { span, .. }
            | DocBlock::MathBlock { span, .. }
            | DocBlock::ThematicBreak { span }
            | DocBlock::HtmlBlock { span, .. }
            | DocBlock::Other { span } => *span,
        }
    }

    pub fn heading_text(&self) -> Option<String> {
        match self {
            DocBlock::Heading { content, .. } => {
                Some(content.iter().map(Inline::plain_text).collect::<String>())
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentIdentity {
    pub project_root_id: String,
    pub module_id: String,
    pub relative_path: String,
    pub source_sha256: String,
}

impl DocumentIdentity {
    pub fn placeholder(relative_path: &SafeRelativePath) -> Self {
        Self {
            project_root_id: String::new(),
            module_id: String::new(),
            relative_path: relative_path.as_str().to_string(),
            source_sha256: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentModel {
    pub identity: DocumentIdentity,
    pub source: String,
    pub blocks: Vec<DocBlock>,
    pub source_index: SourceIndex,
    /// Number of leading BOM bytes stripped before parsing.
    pub bom_len: usize,
}

impl DocumentModel {
    /// Parse the exact source for display. Never decides executable semantics.
    pub fn parse(source: String, identity: DocumentIdentity) -> Self {
        let (bom_len, body) = strip_bom(&source);
        let options = markdown_options();
        let arena = Arena::new();
        let root = parse_document(&arena, body, &options);
        let lines = line_starts(body);
        let body_len = body.len();
        let blocks = root
            .children()
            .filter_map(|child| convert_block(child, &lines, bom_len, body_len))
            .collect();
        let source_index = SourceIndex::new(&source);
        Self {
            identity,
            source,
            blocks,
            source_index,
            bom_len,
        }
    }

    /// First level-1 heading text, else the document stem.
    pub fn title(&self) -> String {
        for block in &self.blocks {
            if let DocBlock::Heading {
                level: 1, content, ..
            } = block
            {
                let raw = content
                    .iter()
                    .map(Inline::plain_text)
                    .collect::<String>()
                    .trim()
                    .to_string();
                // Display-only: strip a legal mode declaration prefix. Source
                // view still shows the exact original text.
                let text = raw
                    .strip_prefix("@exec ")
                    .or_else(|| raw.strip_prefix("@knowledge "))
                    .unwrap_or(&raw)
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    return text;
                }
            }
        }
        let name = self
            .identity
            .relative_path
            .rsplit('/')
            .next()
            .unwrap_or(&self.identity.relative_path);
        name.trim_end_matches(".mq.md").to_string()
    }

    /// Plain text of the whole document (search/alt helpers).
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        collect_block_text(&self.blocks, &mut out);
        out
    }

    pub fn source_digest(&self) -> String {
        sha256_hex(self.source.as_bytes())
    }
}

/// The frozen `marqdo.md.v1` parser options (same set as the Core profile).
pub fn markdown_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.math_dollars = true;
    options.extension.math_dollars_block = true;
    options.extension.front_matter_delimiter = Some("---".to_string());
    options
}

fn collect_block_text(blocks: &[DocBlock], out: &mut String) {
    for block in blocks {
        match block {
            DocBlock::Heading { content, .. } | DocBlock::Paragraph { content, .. } => {
                for inline in content {
                    out.push_str(&inline.plain_text());
                }
                out.push('\n');
            }
            DocBlock::List { items, .. } => collect_block_text(items, out),
            DocBlock::ListItem { children, .. } => collect_block_text(children, out),
            DocBlock::BlockQuote { children, .. } => collect_block_text(children, out),
            DocBlock::CodeBlock { literal, .. } | DocBlock::MathBlock { literal, .. } => {
                out.push_str(literal);
                out.push('\n');
            }
            DocBlock::Table { header, rows, .. } => {
                for cell in header {
                    for inline in cell {
                        out.push_str(&inline.plain_text());
                    }
                }
                for row in rows {
                    for cell in row {
                        for inline in cell {
                            out.push_str(&inline.plain_text());
                        }
                    }
                }
                out.push('\n');
            }
            DocBlock::ThematicBreak { .. }
            | DocBlock::HtmlBlock { .. }
            | DocBlock::Other { .. } => {}
        }
    }
}

fn strip_bom(source: &str) -> (usize, &str) {
    match source.strip_prefix('\u{feff}') {
        Some(rest) => (source.len() - rest.len(), rest),
        None => (0, source),
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = vec![0usize];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn span_of(pos: Sourcepos, lines: &[usize], shift: usize, body_len: usize) -> SourceSpan {
    fn offset_of(line: usize, column: usize, lines: &[usize], inclusive: bool) -> Option<usize> {
        if line == 0 {
            return None;
        }
        let line_start = *lines.get(line - 1)?;
        Some(line_start + column.saturating_sub(1) + usize::from(inclusive))
    }
    let start = offset_of(pos.start.line, pos.start.column, lines, false)
        .map(|offset| offset.min(body_len))
        .unwrap_or(0);
    let end = offset_of(pos.end.line, pos.end.column, lines, true)
        .map(|offset| offset.min(body_len))
        .unwrap_or(start)
        .max(start);
    SourceSpan::new((start + shift) as u64, (end + shift) as u64)
}

fn convert_block<'a>(
    node: &'a comrak::nodes::AstNode<'a>,
    lines: &[usize],
    shift: usize,
    body_len: usize,
) -> Option<DocBlock> {
    let data = node.data();
    let span = span_of(data.sourcepos, lines, shift, body_len);
    let value = &data.value;
    let block = match value {
        NodeValue::Document => return None,
        NodeValue::FrontMatter(_) => return None,
        NodeValue::Paragraph => DocBlock::Paragraph {
            span,
            content: convert_inlines(node, lines, shift, body_len),
        },
        NodeValue::Heading(heading) => DocBlock::Heading {
            level: heading.level,
            span,
            content: convert_inlines(node, lines, shift, body_len),
        },
        NodeValue::List(list) => {
            let ordered = list.list_type == ListType::Ordered;
            let items = node
                .children()
                .filter_map(|child| convert_block(child, lines, shift, body_len))
                .collect();
            DocBlock::List {
                ordered,
                start: if ordered { Some(list.start) } else { None },
                tight: list.tight,
                span,
                items,
            }
        }
        NodeValue::Item(_) => DocBlock::ListItem {
            span,
            task: None,
            children: node
                .children()
                .filter_map(|child| convert_block(child, lines, shift, body_len))
                .collect(),
        },
        NodeValue::TaskItem(item) => DocBlock::ListItem {
            span,
            task: Some(item.symbol.is_some()),
            children: node
                .children()
                .filter_map(|child| convert_block(child, lines, shift, body_len))
                .collect(),
        },
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => DocBlock::BlockQuote {
            span,
            children: node
                .children()
                .filter_map(|child| convert_block(child, lines, shift, body_len))
                .collect(),
        },
        NodeValue::CodeBlock(code) => DocBlock::CodeBlock {
            span,
            info: code.info.clone(),
            literal: code.literal.clone(),
        },
        NodeValue::HtmlBlock(html) => DocBlock::HtmlBlock {
            span,
            literal: html.literal.clone(),
        },
        NodeValue::ThematicBreak(_) => DocBlock::ThematicBreak { span },
        NodeValue::MathBlock(math) => DocBlock::MathBlock {
            span,
            literal: math.literal.clone(),
        },
        NodeValue::Table(table) => {
            let alignments = table
                .alignments
                .iter()
                .map(|alignment| match alignment {
                    TableAlignment::None => TableAlign::None,
                    TableAlignment::Left => TableAlign::Left,
                    TableAlignment::Center => TableAlign::Center,
                    TableAlignment::Right => TableAlign::Right,
                })
                .collect();
            let mut header = Vec::new();
            let mut rows = Vec::new();
            for row in node.children() {
                let row_data = row.data();
                let is_header = match &row_data.value {
                    NodeValue::TableRow(table_row) => table_row.header,
                    _ => false,
                };
                let mut cells = Vec::new();
                for cell in row.children() {
                    cells.push(convert_inlines(cell, lines, shift, body_len));
                }
                if is_header {
                    header = cells;
                } else {
                    rows.push(cells);
                }
            }
            DocBlock::Table {
                span,
                alignments,
                header,
                rows,
            }
        }
        _ => {
            // Unknown block kinds are inert. If they carry block children, keep
            // them so future additive nodes cannot hide readable content.
            let children: Vec<DocBlock> = node
                .children()
                .filter_map(|child| convert_block(child, lines, shift, body_len))
                .collect();
            if children.is_empty() {
                DocBlock::Other { span }
            } else {
                DocBlock::BlockQuote { span, children }
            }
        }
    };
    Some(block)
}

fn convert_inlines<'a>(
    node: &'a comrak::nodes::AstNode<'a>,
    lines: &[usize],
    shift: usize,
    body_len: usize,
) -> Vec<Inline> {
    node.children()
        .filter_map(|child| convert_inline(child, lines, shift, body_len))
        .collect()
}

fn convert_inline<'a>(
    node: &'a comrak::nodes::AstNode<'a>,
    lines: &[usize],
    shift: usize,
    body_len: usize,
) -> Option<Inline> {
    let data = node.data();
    let span = span_of(data.sourcepos, lines, shift, body_len);
    let inline = match &data.value {
        NodeValue::Text(text) => Inline::Text {
            span,
            literal: text.to_string(),
        },
        NodeValue::Code(code) => Inline::Code {
            span,
            literal: code.literal.clone(),
            ticks: code.num_backticks,
        },
        NodeValue::SoftBreak => Inline::SoftBreak { span },
        NodeValue::LineBreak => Inline::LineBreak { span },
        NodeValue::Strong(_) => Inline::Strong {
            span,
            children: convert_inlines(node, lines, shift, body_len),
        },
        NodeValue::Emph(_) => Inline::Emph {
            span,
            children: convert_inlines(node, lines, shift, body_len),
        },
        NodeValue::Strikethrough(_) => Inline::Strikethrough {
            span,
            children: convert_inlines(node, lines, shift, body_len),
        },
        NodeValue::Link(link) => Inline::Link {
            span,
            url: link.url.clone(),
            title: link.title.clone(),
            children: convert_inlines(node, lines, shift, body_len),
        },
        NodeValue::Image(link) => {
            let children = convert_inlines(node, lines, shift, body_len);
            Inline::Image {
                span,
                url: link.url.clone(),
                title: link.title.clone(),
                alt: children.iter().map(Inline::plain_text).collect(),
            }
        }
        NodeValue::Math(math) => Inline::Math {
            span,
            literal: math.literal.clone(),
            display: math.display_math,
        },
        NodeValue::HtmlInline(raw) => Inline::HtmlInline {
            span,
            raw: raw.clone(),
        },
        _ => Inline::Other { span },
    };
    Some(inline)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> DocumentModel {
        DocumentModel::parse(
            source.to_string(),
            DocumentIdentity {
                project_root_id: "root:t@0.1.0".into(),
                module_id: "root:t@0.1.0::index.mq.md".into(),
                relative_path: "index.mq.md".into(),
                source_sha256: String::new(),
            },
        )
    }

    #[test]
    fn parses_gfm_and_preserves_utf8_spans() {
        let source =
            "# @knowledge 标题\n\n| 名称 | 值 |\n| --- | --- |\n| 甲 | 1 |\n\n**加粗** 和 `代码`\n";
        let document = parse(source);
        // Display title drops a legal mode prefix; the source keeps it.
        assert_eq!(document.title(), "标题");
        assert!(document.source.starts_with("# @knowledge 标题"));
        assert!(document
            .blocks
            .iter()
            .any(|block| matches!(block, DocBlock::Table { .. })));
        // Every block span slices valid UTF-8.
        for block in &document.blocks {
            let span = block.span();
            assert!(
                document
                    .source_index
                    .source_text(&document.source, span)
                    .is_some(),
                "invalid span {span:?}"
            );
        }
    }

    #[test]
    fn parses_math_and_code_as_inert_literals() {
        let source = "行内 $a^2$ 与\n\n$$\n\\sum i\n$$\n\n```rust\nfn x() {}\n```\n";
        let document = parse(source);
        assert!(document.plain_text().contains("a^2"));
        assert!(document.plain_text().contains("sum i"));
        assert!(document.plain_text().contains("fn x()"));
    }

    #[test]
    fn parses_lists_quotes_and_task_items() {
        let source = "- [x] 完成\n- [ ] 未完成\n\n> 引用\n\n1. 一\n2. 二\n";
        let document = parse(source);
        let has_task = document.blocks.iter().any(|block| match block {
            DocBlock::List { items, .. } => items.iter().any(|item| {
                matches!(
                    item,
                    DocBlock::ListItem {
                        task: Some(true),
                        ..
                    }
                )
            }),
            _ => false,
        });
        assert!(has_task);
        assert!(document
            .blocks
            .iter()
            .any(|block| matches!(block, DocBlock::BlockQuote { .. })));
        assert_eq!(document.blocks.len(), 3);
    }

    #[test]
    fn heading_spans_are_exact() {
        let source = "# 数学 #\n\n正文\n";
        let document = parse(source);
        match &document.blocks[0] {
            DocBlock::Heading { span, .. } => {
                let text = document
                    .source_index
                    .source_text(&document.source, *span)
                    .unwrap();
                assert_eq!(text, "# 数学 #");
            }
            other => panic!("expected heading, got {other:?}"),
        }
    }

    #[test]
    fn front_matter_is_not_rendered_as_a_block() {
        let source = "---\nimport:\n  lib: \"./lib.mq.md\"\n---\n\n# hi\n";
        let document = parse(source);
        assert_eq!(document.blocks.len(), 1);
        assert!(matches!(document.blocks[0], DocBlock::Heading { .. }));
    }
}
