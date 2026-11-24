//! Markdown navigation and URL normalization shared by Live and export.

use crate::reader::markdown::{DocBlock, Inline};
use crate::reader::source::{SafeRelativePath, SourceSpan};
use std::collections::{HashMap, HashSet};

pub fn decode(raw: &str) -> Option<String> {
    let mut result = Vec::with_capacity(raw.len());
    let mut bytes = raw.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let hi = (bytes.next()? as char).to_digit(16)?;
            let lo = (bytes.next()? as char).to_digit(16)?;
            result.push((hi * 16 + lo) as u8);
        } else {
            result.push(byte);
        }
    }
    String::from_utf8(result)
        .ok()
        .filter(|value| !value.chars().any(char::is_control))
}

pub fn relative_url(current: &SafeRelativePath, url: &str) -> Option<SafeRelativePath> {
    let relative = decode(url)?;
    if relative.is_empty() || relative.starts_with('/') || relative.contains(['\\', ':']) {
        return None;
    }
    let parent = current.parent();
    let mut components = parent
        .as_ref()
        .map_or_else(Vec::new, |parent| parent.as_str().split('/').collect());
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            _ => components.push(part),
        }
    }
    SafeRelativePath::parse(&components.join("/")).ok()
}

pub fn heading_slugs(blocks: &[DocBlock]) -> HashMap<SourceSpan, String> {
    let mut slugs = HashMap::new();
    let mut used = HashSet::new();
    visit_headings(blocks, &mut |span, content| {
        let label = content.iter().map(Inline::plain_text).collect::<String>();
        let label = ["@exec", "@knowledge"]
            .into_iter()
            .find_map(|prefix| {
                label
                    .strip_prefix(prefix)
                    .filter(|suffix| suffix.starts_with(char::is_whitespace))
            })
            .unwrap_or(&label)
            .trim();
        let base: String = label
            .to_lowercase()
            .chars()
            .filter_map(|ch| {
                if ch.is_whitespace() {
                    Some('-')
                } else if ch.is_alphanumeric() || matches!(ch, '-' | '_') {
                    Some(ch)
                } else {
                    None
                }
            })
            .collect();
        let base = if base.is_empty() { "section" } else { &base };
        let base = if base.starts_with("mq-") {
            format!("section-{base}")
        } else {
            base.to_string()
        };
        let mut slug = base.clone();
        let mut suffix = 1;
        while !used.insert(slug.clone()) {
            slug = format!("{base}-{suffix}");
            suffix += 1;
        }
        slugs.insert(span, slug);
    });
    slugs
}

fn visit_headings(blocks: &[DocBlock], visit: &mut impl FnMut(SourceSpan, &[Inline])) {
    for block in blocks {
        match block {
            DocBlock::Heading { span, content, .. } => visit(*span, content),
            DocBlock::List { items, .. } => visit_headings(items, visit),
            DocBlock::ListItem { children, .. } | DocBlock::BlockQuote { children, .. } => {
                visit_headings(children, visit)
            }
            _ => {}
        }
    }
}
