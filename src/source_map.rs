//! Exact UTF-8 byte ↔ line/column ↔ JavaScript UTF-16 offset mapping.
//!
//! Core Analysis/Trace spans are half-open UTF-8 **byte** offsets into the
//! original file bytes. Browsers count UTF-16 code units. This module is the
//! only place that converts between them (RFC-VIEW-0001 §8.2).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Half-open byte span `[start, end)` into one module's exact source bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SourceSpan {
    pub start: u64,
    pub end: u64,
}

impl SourceSpan {
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn contains(&self, other: SourceSpan) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    pub fn intersects(&self, other: SourceSpan) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn as_usize_range(&self) -> Option<std::ops::Range<usize>> {
        let start = usize::try_from(self.start).ok()?;
        let end = usize::try_from(self.end).ok()?;
        if start > end {
            return None;
        }
        Some(start..end)
    }
}

/// Compute lowercase hex SHA-256 of exact bytes (no normalization).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// SHA-256 with the `sha256:` prefix used by Analysis/Trace identity fields.
pub fn sha256_prefixed(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// Immutable source index built from the exact original bytes.
#[derive(Debug, Clone)]
pub struct SourceIndex {
    /// Byte length of the source.
    len: usize,
    /// Byte offset of the start of every line (0-based line number).
    line_starts: Vec<usize>,
    /// Number of leading UTF-8 BOM bytes tolerated before the first content
    /// byte. Spans in Analysis are relative to the file bytes, so the BOM
    /// stays part of the index.
    bom_len: usize,
}

impl SourceIndex {
    pub fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let bom_len = if source.starts_with('\u{feff}') { 3 } else { 0 };
        let mut line_starts = vec![0usize];
        let mut index = bom_len;
        while index < bytes.len() {
            if bytes[index] == b'\n' {
                line_starts.push(index + 1);
            }
            index += 1;
        }
        Self {
            len: bytes.len(),
            line_starts,
            bom_len,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn bom_len(&self) -> usize {
        self.bom_len
    }

    pub fn source_text<'a>(&self, source: &'a str, span: SourceSpan) -> Option<&'a str> {
        if !self.validate_span_bytes(source.as_bytes(), span) {
            return None;
        }
        let range = span.as_usize_range()?;
        source.get(range)
    }

    /// True when the span is in range and both ends are UTF-8 code-point
    /// boundaries of the supplied original bytes.
    pub fn validate_span_bytes(&self, bytes: &[u8], span: SourceSpan) -> bool {
        let (Ok(start), Ok(end)) = (usize::try_from(span.start), usize::try_from(span.end)) else {
            return false;
        };
        if start > end || end > bytes.len() {
            return false;
        }
        is_char_boundary(bytes, start) && is_char_boundary(bytes, end)
    }

    /// Byte offset of the start of a 1-based line.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        if line == 0 {
            return None;
        }
        self.line_starts.get(line - 1).copied()
    }

    /// Number of lines in the source.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Text of a 1-based line without its trailing newline.
    pub fn line_text<'a>(&self, source: &'a str, line: usize) -> Option<&'a str> {
        let start = self.line_start(line)?;
        let end = self.line_starts.get(line).copied().unwrap_or(self.len);
        let bytes = source.as_bytes();
        let mut end = end.min(bytes.len());
        if end > start && bytes.get(end - 1) == Some(&b'\n') {
            end -= 1;
            if end > start && bytes.get(end - 1) == Some(&b'\r') {
                end -= 1;
            }
        }
        source.get(start..end)
    }

    /// 1-based line and 1-based byte column for a byte offset, clamped to the
    /// file length. Column counts bytes, matching Core's presentation fields.
    pub fn line_col(&self, byte: usize) -> (usize, usize) {
        let byte = byte.min(self.len);
        let line_index = match self.line_starts.binary_search(&byte) {
            Ok(found) => found,
            Err(insert) => insert.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        (line_index + 1, byte - line_start + 1)
    }

    /// Byte offset → number of UTF-16 code units before it (JS string index).
    pub fn utf16_offset(&self, source: &str, byte: usize) -> Option<usize> {
        if byte > source.len() || !is_char_boundary(source.as_bytes(), byte) {
            return None;
        }
        Some(source[..byte].encode_utf16().count())
    }

    /// Byte offset for a UTF-16 offset, only when it lands on a boundary.
    pub fn byte_for_utf16(&self, source: &str, utf16: usize) -> Option<usize> {
        let mut units = 0usize;
        for (byte, ch) in source.char_indices() {
            if units == utf16 {
                return Some(byte);
            }
            if units > utf16 {
                return None;
            }
            units += ch.len_utf16();
        }
        if units == utf16 {
            Some(source.len())
        } else {
            None
        }
    }

    /// True when an arbitrary `(line, column)` from a Core diagnostic is inside
    /// the source index; line/column are presentation hints, spans are primary.
    pub fn line_col_in_range(&self, line: u64, column: u64) -> bool {
        let Ok(line) = usize::try_from(line) else {
            return false;
        };
        let Ok(column) = usize::try_from(column) else {
            return false;
        };
        if line == 0 || column == 0 || line > self.line_starts.len() {
            return false;
        }
        let start = self.line_starts[line - 1];
        let end = self.line_starts.get(line).copied().unwrap_or(self.len);
        start + column - 1 <= end
    }
}

fn is_char_boundary(bytes: &[u8], offset: usize) -> bool {
    if offset == 0 || offset >= bytes.len() {
        return offset <= bytes.len();
    }
    // UTF-8 continuation bytes start with 0b10.
    bytes[offset] & 0b1100_0000 != 0b1000_0000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_multibyte_and_emoji_offsets() {
        let source = "中文\nemoji🙂x\r\nlast";
        let index = SourceIndex::new(source);
        let emoji_byte = source.find('🙂').unwrap();
        assert_eq!(index.line_col(emoji_byte), (2, 6));
        let utf16 = index.utf16_offset(source, emoji_byte).unwrap();
        assert_eq!(index.byte_for_utf16(source, utf16), Some(emoji_byte));
        // Inside the emoji is not a char boundary.
        assert!(index.utf16_offset(source, emoji_byte + 1).is_none());
        let span = SourceSpan::new(emoji_byte as u64, (emoji_byte + 4) as u64);
        assert_eq!(index.source_text(source, span), Some("🙂"));
    }

    #[test]
    fn rejects_out_of_range_and_misaligned_spans() {
        let source = "# 数学";
        let index = SourceIndex::new(source);
        assert!(
            index.validate_span_bytes(source.as_bytes(), SourceSpan::new(0, source.len() as u64))
        );
        assert!(!index.validate_span_bytes(
            source.as_bytes(),
            SourceSpan::new(0, source.len() as u64 + 1)
        ));
        assert!(!index.validate_span_bytes(source.as_bytes(), SourceSpan::new(2, 1)));
        // 2 is the middle of the 3-byte 字/数? Either way it is not a boundary
        // for the CJK character starting at 2..5; check the interior byte 3.
        assert!(!index.validate_span_bytes(source.as_bytes(), SourceSpan::new(3, 5)));
    }

    #[test]
    fn digest_is_exact_bytes() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_prefixed(b"a"),
            "sha256:ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"
        );
    }

    #[test]
    fn bom_is_still_indexed() {
        let source = "\u{feff}# hi";
        let index = SourceIndex::new(source);
        assert_eq!(index.bom_len(), 3);
        assert_eq!(index.line_col(3), (1, 4));
        assert_eq!(
            index.source_text(source, SourceSpan::new(0, 3)),
            Some("\u{feff}")
        );
    }
}
