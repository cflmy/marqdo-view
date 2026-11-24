//! Exact UTF-8 byte ↔ line/column ↔ JavaScript UTF-16 offset mapping.
//!
//! Core Analysis/Trace spans are half-open UTF-8 **byte** offsets into the
//! original file bytes. Browsers count UTF-16 code units. This module is the
//! only place that converts between them (RFC-VIEW-0001 §8.2).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Half-open byte span `[start, end)` into one module's exact source bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
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

// ---------------------------------------------------------------------------
// Controlled project root, path boundary and document source access.
//
// All production/HTTP/Export/tests use these functions; there is no second
// path validator (RFC-VIEW-0002 §5.2/§6.8).
// ---------------------------------------------------------------------------

use crate::error::{ViewError, ViewResult};
use std::path::{Path, PathBuf};

/// Hard cap on one source document, matching the Core read limit family.
pub const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
/// Hard cap on discovered documents per project to bound scans.
pub const MAX_DOCUMENTS: usize = 5_000;
/// Directories never scanned for documents (VCS, caches, build output).
pub const SKIP_DIRS: &[&str] = &[
    ".git",
    ".marqdo",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
];

/// A project-relative path that is safe to join under a project root.
///
/// Rejects absolute paths, `..`, root/prefix components, NUL and control bytes.
/// Backslashes are normalized to `/` so a Windows-style input can never be
/// reinterpreted as a separator by the OS while looking harmless here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SafeRelativePath(String);

impl SafeRelativePath {
    pub fn parse(raw: &str) -> ViewResult<Self> {
        if raw.is_empty() {
            return Err(ViewError::PathForbidden {
                detail: "empty relative path".to_string(),
            });
        }
        if raw.contains('\0') {
            return Err(ViewError::PathForbidden {
                detail: "path contains NUL".to_string(),
            });
        }
        if raw.chars().any(|ch| ch.is_control()) {
            return Err(ViewError::PathForbidden {
                detail: "path contains control characters".to_string(),
            });
        }
        let normalized = raw.replace('\\', "/");
        if normalized.starts_with('/') {
            return Err(ViewError::PathForbidden {
                detail: format!("absolute path is not a project-relative path: {raw:?}"),
            });
        }
        if normalized
            .split('/')
            .next()
            .map(|first| first.len() == 2 && first.ends_with(':'))
            .unwrap_or(false)
        {
            return Err(ViewError::PathForbidden {
                detail: format!("drive-absolute path is not allowed: {raw:?}"),
            });
        }
        let mut parts = Vec::new();
        for part in normalized.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if part == ".." {
                return Err(ViewError::PathForbidden {
                    detail: format!("path escapes the project root: {raw:?}"),
                });
            }
            parts.push(part);
        }
        if parts.is_empty() {
            return Err(ViewError::PathForbidden {
                detail: "path does not name a file".to_string(),
            });
        }
        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_path(&self) -> PathBuf {
        PathBuf::from(self.0.replace('/', std::path::MAIN_SEPARATOR_STR))
    }

    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    pub fn parent(&self) -> Option<SafeRelativePath> {
        self.0
            .rsplit_once('/')
            .map(|(parent, _)| SafeRelativePath(parent.to_string()))
    }
}

impl std::fmt::Display for SafeRelativePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One resolved document inside a project.
#[derive(Debug, Clone)]
pub struct DocumentRef {
    pub relative: SafeRelativePath,
    pub absolute: PathBuf,
}

/// An opened project root (directory) and its selected entry document.
#[derive(Debug, Clone)]
pub struct Project {
    root: PathBuf,
    #[cfg(target_os = "linux")]
    root_handle: std::sync::Arc<std::fs::File>,
    entry: DocumentRef,
}

impl Project {
    /// Open `path` (file or directory) and select the entry document.
    ///
    /// File input: the project root is the nearest ancestor containing
    /// `marqdo.yaml`, else the file's own directory.
    /// Directory input: the directory is the root; `index.mq.md` wins, else the
    /// first document in stable sorted order (RFC-VIEW-0001 §9.4).
    pub fn open(path: &Path) -> ViewResult<Self> {
        let canonical = canonicalize(path, "project path")?;
        let metadata = std::fs::metadata(&canonical)
            .map_err(|err| ViewError::io(&format!("stat {}", canonical.display()), err))?;
        if metadata.is_file() {
            let root = discover_project_root(&canonical);
            let project = Self {
                #[cfg(target_os = "linux")]
                root_handle: Self::open_root(&root)?,
                entry: DocumentRef {
                    relative: SafeRelativePath("placeholder".to_string()),
                    absolute: canonical.clone(),
                },
                root,
            };
            let relative = project.relative_for(&canonical)?;
            Ok(Self {
                root: project.root,
                #[cfg(target_os = "linux")]
                root_handle: project.root_handle,
                entry: DocumentRef {
                    relative,
                    absolute: canonical,
                },
            })
        } else if metadata.is_dir() {
            let project = Self {
                root: canonical.clone(),
                #[cfg(target_os = "linux")]
                root_handle: Self::open_root(&canonical)?,
                entry: DocumentRef {
                    relative: SafeRelativePath("placeholder".to_string()),
                    absolute: canonical.clone(),
                },
            };
            let documents = project.list_documents()?;
            let chosen = choose_entry(&project.root, &documents)?;
            Ok(Self {
                entry: chosen,
                ..project
            })
        } else {
            Err(ViewError::PathForbidden {
                detail: format!("{} is not a regular file or directory", path.display()),
            })
        }
    }

    #[cfg(target_os = "linux")]
    fn open_root(path: &Path) -> ViewResult<std::sync::Arc<std::fs::File>> {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(path)
            .map(std::sync::Arc::new)
            .map_err(|err| ViewError::io("open project root", err))
    }

    #[cfg(target_os = "linux")]
    pub(super) fn root_handle(&self) -> &std::fs::File {
        &self.root_handle
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn entry(&self) -> &DocumentRef {
        &self.entry
    }

    /// Resolve an absolute path under the root into a safe relative path.
    pub fn relative_for(&self, absolute: &Path) -> ViewResult<SafeRelativePath> {
        let canonical = canonicalize(absolute, "document path")?;
        if !canonical.starts_with(&self.root) {
            return Err(ViewError::PathForbidden {
                detail: format!(
                    "document {} is outside project root {}",
                    canonical.display(),
                    self.root.display()
                ),
            });
        }
        let relative =
            canonical
                .strip_prefix(&self.root)
                .map_err(|_| ViewError::PathForbidden {
                    detail: "document path is not under the project root".to_string(),
                })?;
        SafeRelativePath::parse(&relative.to_string_lossy())
    }

    /// Absolute path for a safe relative path, with a final real-path check.
    pub fn absolute_for(&self, relative: &SafeRelativePath) -> ViewResult<PathBuf> {
        let joined = self.root.join(relative.to_path());
        let canonical = canonicalize(&joined, "document path")?;
        if !canonical.starts_with(&self.root) {
            return Err(ViewError::PathForbidden {
                detail: format!("{} escapes the project root", relative),
            });
        }
        Ok(canonical)
    }

    /// Read exact source bytes. Never normalizes line endings or Unicode.
    pub fn read_source(&self, relative: &SafeRelativePath) -> ViewResult<Vec<u8>> {
        let absolute = self.absolute_for(relative)?;
        let metadata = std::fs::metadata(&absolute)
            .map_err(|err| ViewError::io(&format!("stat {}", absolute.display()), err))?;
        if !metadata.is_file() {
            return Err(ViewError::PathForbidden {
                detail: format!("{} is not a regular file", relative),
            });
        }
        if metadata.len() > MAX_SOURCE_BYTES {
            return Err(ViewError::ResourceLimit {
                detail: format!(
                    "{} is {} bytes, above the {} byte View limit",
                    relative,
                    metadata.len(),
                    MAX_SOURCE_BYTES
                ),
            });
        }
        std::fs::read(&absolute)
            .map_err(|err| ViewError::io(&format!("read {}", absolute.display()), err))
    }

    /// Read one document as UTF-8 text.
    pub fn read_source_text(&self, relative: &SafeRelativePath) -> ViewResult<String> {
        let bytes = self.read_source(relative)?;
        String::from_utf8(bytes).map_err(|err| ViewError::Io {
            detail: format!("{} is not valid UTF-8: {err}", relative),
        })
    }

    /// Stable sorted list of `*.mq.md` documents under the root.
    pub fn list_documents(&self) -> ViewResult<Vec<SafeRelativePath>> {
        let mut out = Vec::new();
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = Vec::new();
            for entry in std::fs::read_dir(&dir)
                .map_err(|err| ViewError::io(&format!("read dir {}", dir.display()), err))?
            {
                entries.push(entry.map_err(|err| ViewError::io("read dir entry", err))?);
            }
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let file_type = entry
                    .file_type()
                    .map_err(|err| ViewError::io("file type", err))?;
                if file_type.is_symlink() {
                    // Never follow links while scanning; explicit file input is
                    // canonicalized and boundary-checked separately.
                    continue;
                }
                let path = entry.path();
                if file_type.is_dir() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if SKIP_DIRS.contains(&name.as_ref()) {
                        continue;
                    }
                    stack.push(path);
                    continue;
                }
                if file_type.is_file() && is_marqdo_document(&path) {
                    let relative = self.relative_for(&path)?;
                    out.push(relative);
                    if out.len() > MAX_DOCUMENTS {
                        return Err(ViewError::ResourceLimit {
                            detail: format!(
                                "more than {MAX_DOCUMENTS} documents under project root"
                            ),
                        });
                    }
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// Resolve a browser-facing relative path to a document reference.
    pub fn document(&self, relative: &SafeRelativePath) -> ViewResult<DocumentRef> {
        Ok(DocumentRef {
            relative: relative.clone(),
            absolute: self.absolute_for(relative)?,
        })
    }

    /// `source_digest` of the exact bytes, matching Analysis `source_digest`.
    pub fn source_digest(&self, relative: &SafeRelativePath) -> ViewResult<String> {
        Ok(sha256_hex(&self.read_source(relative)?))
    }
}

/// Discover the project root for a source file: nearest ancestor with
/// `marqdo.yaml`, else the file's directory. The file itself is canonical.
pub fn discover_project_root(file: &Path) -> PathBuf {
    let start = file.parent().unwrap_or_else(|| Path::new("/"));
    for ancestor in start.ancestors() {
        if ancestor.join("marqdo.yaml").is_file() {
            return ancestor.to_path_buf();
        }
    }
    start.to_path_buf()
}

pub fn is_marqdo_document(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.ends_with(".mq.md"))
        .unwrap_or(false)
}

fn choose_entry(root: &Path, documents: &[SafeRelativePath]) -> ViewResult<DocumentRef> {
    if documents.is_empty() {
        return Err(ViewError::PathForbidden {
            detail: format!("no .mq.md documents under {}", root.display()),
        });
    }
    if let Some(index) = documents.iter().find(|doc| doc.as_str() == "index.mq.md") {
        return Ok(DocumentRef {
            relative: index.clone(),
            absolute: root.join(index.to_path()),
        });
    }
    let first = documents[0].clone();
    Ok(DocumentRef {
        relative: first.clone(),
        absolute: root.join(first.to_path()),
    })
}

fn canonicalize(path: &Path, what: &str) -> ViewResult<PathBuf> {
    // CLI arguments may legitimately contain `..`; the security boundary is
    // the canonicalized result staying under the canonicalized project root.
    // Browser/Export never reach this layer with raw user input: they must go
    // through `SafeRelativePath` first.
    path.canonicalize()
        .map_err(|err| ViewError::io(&format!("canonicalize {} ({what})", path.display()), err))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn rejects_unsafe_relative_paths() {
        for raw in [
            "",
            "/etc/passwd",
            "../secret",
            "a/../../b",
            "a\0b",
            "a\nb",
            "..",
            ".",
        ] {
            assert!(
                SafeRelativePath::parse(raw).is_err(),
                "expected rejection for {raw:?}"
            );
        }
    }

    #[test]
    fn normalizes_harmless_forms() {
        assert_eq!(
            SafeRelativePath::parse("./docs/../x").unwrap_err().code(),
            "view.path_forbidden"
        );
        assert_eq!(
            SafeRelativePath::parse("./docs/index.mq.md")
                .unwrap()
                .as_str(),
            "docs/index.mq.md"
        );
        assert_eq!(
            SafeRelativePath::parse("docs\\index.mq.md")
                .unwrap()
                .as_str(),
            "docs/index.mq.md"
        );
    }
}
