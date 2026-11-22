//! View-only site configuration (`view.json`).
//!
//! This is **not** a Marqdo language protocol and cannot grant execution. Only
//! welcome/site_title/theme are read, and the welcome path is subject to the
//! same project-root boundary (RFC-VIEW-0002 §4.2/§5.3).

use crate::error::{ViewError, ViewResult};
use crate::reader::source::SafeRelativePath;
use std::path::Path;

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
