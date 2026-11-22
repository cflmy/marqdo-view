//! Project-root and path boundary enforcement.
//!
//! Production, HTTP, Export and tests all use these functions; there is no
//! second path validator (RFC-VIEW-0001 §14.4).

use crate::error::{ViewError, ViewResult};
use crate::source_map::sha256_hex;
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
                entry: DocumentRef {
                    relative: SafeRelativePath("placeholder".to_string()),
                    absolute: canonical.clone(),
                },
                root,
            };
            let relative = project.relative_for(&canonical)?;
            Ok(Self {
                root: project.root,
                entry: DocumentRef {
                    relative,
                    absolute: canonical,
                },
            })
        } else if metadata.is_dir() {
            let project = Self {
                root: canonical.clone(),
                entry: DocumentRef {
                    relative: SafeRelativePath("placeholder".to_string()),
                    absolute: canonical.clone(),
                },
            };
            let documents = project.list_documents()?;
            let chosen = choose_entry(&project.root, &documents)?;
            Ok(Self {
                root: project.root,
                entry: chosen,
            })
        } else {
            Err(ViewError::PathForbidden {
                detail: format!("{} is not a regular file or directory", path.display()),
            })
        }
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

/// Optional View-only site configuration (`view.json`).
///
/// This is **not** a Marqdo language protocol and cannot grant execution: only
/// the fields below are read, and the welcome path is still subject to the same
/// project-root boundary (RFC-VIEW-0001 §9.4).
#[derive(Debug, Clone, Default)]
pub struct ViewConfig {
    pub welcome: Option<SafeRelativePath>,
    pub site_title: Option<String>,
    pub theme: Option<String>,
}

impl ViewConfig {
    pub fn load(root: &Path) -> ViewResult<Self> {
        let path = root.join("view.json");
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(ViewError::io(&format!("stat {}", path.display()), err)),
        };
        if !metadata.is_file() || metadata.len() > 64 * 1024 {
            return Err(ViewError::PathForbidden {
                detail: "view.json must be a regular file under 64 KiB".to_string(),
            });
        }
        let bytes = std::fs::read(&path)
            .map_err(|err| ViewError::io(&format!("read {}", path.display()), err))?;
        #[derive(serde::Deserialize)]
        struct RawConfig {
            #[serde(default)]
            schema: String,
            #[serde(default)]
            schema_version: u32,
            #[serde(default)]
            welcome: Option<String>,
            #[serde(default)]
            site_title: Option<String>,
            #[serde(default)]
            theme: Option<String>,
        }
        let raw: RawConfig = serde_json::from_slice(&bytes).map_err(|err| ViewError::Io {
            detail: format!("view.json is not valid JSON: {err}"),
        })?;
        if raw.schema != "marqdo.view-config" || raw.schema_version != 1 {
            return Err(ViewError::ProtocolIncompatible {
                detail: format!(
                    "unsupported view.json schema {:?} version {}; expected marqdo.view-config v1",
                    raw.schema, raw.schema_version
                ),
            });
        }
        let welcome = match raw.welcome {
            Some(path) => Some(SafeRelativePath::parse(&path)?),
            None => None,
        };
        if let Some(welcome) = &welcome {
            if !root.join(welcome.to_path()).is_file() {
                return Err(ViewError::PathForbidden {
                    detail: format!(
                        "view.json welcome file {welcome} does not exist under the project root"
                    ),
                });
            }
        }
        let site_title = raw
            .site_title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty() && title.chars().count() <= 80);
        let theme = raw
            .theme
            .map(|theme| theme.trim().to_string())
            .filter(|theme| theme == "botanical" || theme == "paper");
        Ok(Self {
            welcome,
            site_title,
            theme,
        })
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
mod tests {
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
