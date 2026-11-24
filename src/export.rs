//! Pure static site export.
//!
//! Export uses the same `DocumentService::load` (static `check` only) and the same
//! `ReaderRenderer` as Live; it never calls `run`, never starts a provider and
//! never writes into the source project (RFC-VIEW-0001 §11.3).

use crate::error::{ViewError, ViewResult};
use crate::reader::assemble::DocumentService;
use crate::reader::assets::AssetBundle;
use crate::reader::render::ReaderRenderer;
use crate::reader::source::sha256_hex;
use crate::reader::source::SafeRelativePath;
use std::path::{Path, PathBuf};

const MANIFEST_SCHEMA: &str = "marqdo.view-export";

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub out: PathBuf,
    pub no_annotations: bool,
}

#[derive(Debug)]
pub struct ExportSummary {
    pub out: PathBuf,
    pub pages: usize,
    pub files: usize,
    pub manifest_sha256: String,
}

pub fn export(context: &DocumentService, options: &ExportOptions) -> ViewResult<ExportSummary> {
    let root = context.project().root().to_path_buf();
    let out = absolute_export_target(&options.out)?;
    if out.starts_with(&root) {
        return Err(ViewError::ExportFailed {
            detail: format!(
                "输出目录 {} 位于项目根 {} 内；请选择项目外的目录以避免递归/覆盖源文件",
                out.display(),
                root.display()
            ),
        });
    }
    validate_existing_target(&out)?;

    let staging = staging_dir(&out)?;
    let result = (|| {
        let entry = context.load(&context.project().entry().relative)?;
        let site = build_site(context, &entry, &staging, !options.no_annotations)?;
        publish(&staging, &out)?;
        Ok(ExportSummary {
            out,
            pages: site.pages,
            files: site.entries.len(),
            manifest_sha256: site.manifest_sha256,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

struct Site {
    pages: usize,
    entries: Vec<String>,
    manifest_sha256: String,
}

struct SiteWriter<'a> {
    root: &'a Path,
    entries: std::collections::BTreeSet<String>,
    remaining_bytes: usize,
}

impl SiteWriter<'_> {
    fn write(&mut self, relative: &str, bytes: &[u8]) -> ViewResult<serde_json::Value> {
        if relative.contains([':', '\\'])
            || SafeRelativePath::parse(relative).is_err()
            || relative
                .split('/')
                .any(|part| part.is_empty() || part == ".")
        {
            return Err(ViewError::ExportFailed {
                detail: "unsafe static asset path".into(),
            });
        }
        if self.entries.insert(relative.to_string()) {
            self.remaining_bytes =
                self.remaining_bytes
                    .checked_sub(bytes.len())
                    .ok_or_else(|| ViewError::ResourceLimit {
                        detail: "static package size limit exceeded".into(),
                    })?;
            if self.entries.len() > 6_000 {
                return Err(ViewError::ResourceLimit {
                    detail: "static package file limit exceeded".into(),
                });
            }
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| ViewError::io("create static package directory", err))?;
            }
            std::fs::write(path, bytes)
                .map_err(|err| ViewError::io("write static package file", err))?;
        }
        Ok(serde_json::json!({"path": relative, "sha256": sha256_hex(bytes), "size": bytes.len()}))
    }
}

fn build_site(
    context: &DocumentService,
    entry_model: &crate::reader::model::PresentationModel,
    staging: &Path,
    annotations: bool,
) -> ViewResult<Site> {
    let documents = context.project().list_documents()?;
    let entry = SafeRelativePath::parse(&entry_model.document.identity.relative_path)?;
    if !documents.contains(&entry) {
        return Err(ViewError::ExportFailed {
            detail: "entry document is missing".into(),
        });
    }
    let mut writer = SiteWriter {
        root: staging,
        entries: Default::default(),
        remaining_bytes: 96 * 1024 * 1024,
    };
    let bundle = AssetBundle::get();
    let renderer = ReaderRenderer::new();
    let mut assets = Vec::new();
    for asset in bundle.all() {
        let (name, bytes) = if asset.name == "reader" && asset.extension == "css" {
            (bundle.hashed_css_name(), bundle.css_bytes())
        } else {
            (asset.hashed_name(), asset.bytes)
        };
        assets.push(writer.write(&format!("assets/{name}"), bytes)?);
    }

    let mut pages = Vec::new();
    let mut files = Vec::new();
    let mut media_files = std::collections::BTreeMap::new();
    for relative in &documents {
        let loaded;
        let model = if *relative == entry {
            entry_model
        } else {
            loaded = context.load(relative)?;
            &loaded
        };
        for media in model.media.values() {
            let path = format!("media/{}", media.filename());
            if !media_files.contains_key(&path) {
                let mut record = writer.write(&path, &media.bytes)?;
                record["mime"] = serde_json::json!(media.mime);
                media_files.insert(path, record);
            }
        }
        let depth = relative.as_str().split('/').count();
        let settings = crate::reader::render::RenderContext::static_export(depth, annotations);
        let page = renderer.render(model, &settings);
        let path = format!("pages/{}.html", relative.as_str());
        let file = writer.write(&path, page.html.as_bytes())?;
        pages.push(serde_json::json!({
            "relative_path": relative.as_str(),
            "module_id": model.document.identity.module_id,
            "source_sha256": sha256_hex(model.document.source.as_bytes()),
            "source_status": model.source_status.data_value(),
            "analysis_digest": model.semantic.as_ref().map(|semantic| &semantic.analysis_digest),
            "file": path,
            "sha256": file["sha256"],
            "annotations": annotations,
        }));
        files.push(file);
    }
    files.extend(media_files.values().cloned());

    let index_context = crate::reader::render::RenderContext::static_export(0, annotations);
    let index_page = renderer.render(entry_model, &index_context);
    let index_file = writer.write("index.html", index_page.html.as_bytes())?;
    let index_sha = index_file["sha256"].clone();
    files.push(index_file);
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let manifest = serde_json::json!({
        "schema": MANIFEST_SCHEMA,
        "schema_version": 1,
        "view": {"name": "marqdo-view", "version": env!("CARGO_PKG_VERSION")},
        "compatibility": {
            "marqdo": crate::protocol::tool_v1::PRODUCT_RANGE,
            "tool_protocol": crate::protocol::tool_v1::TOOL_PROTOCOL,
            "analysis": crate::protocol::tool_v1::ANALYSIS_PROTOCOL,
            "analysis_supported": crate::protocol::tool_v1::ANALYSIS_PROTOCOLS,
            "trace": crate::protocol::tool_v1::TRACE_PROTOCOL,
            "markdown_profile": "marqdo.md.v1",
        },
        "core_version": entry_model.core_version,
        "entry": {"relative_path": entry.as_str(), "file": "index.html", "sha256": index_sha},
        "pages": pages,
        "assets": assets,
        "files": files,
        "executed": false,
        "note": "Static export only; reading or exporting never executes the document.",
    });
    let manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|err| ViewError::ExportFailed {
            detail: format!("serialize export manifest: {err}"),
        })?;
    writer.write("manifest.json", &manifest_bytes)?;
    Ok(Site {
        pages: documents.len(),
        entries: writer.entries.into_iter().collect(),
        manifest_sha256: sha256_hex(&manifest_bytes),
    })
}

fn absolute_export_target(raw: &Path) -> ViewResult<PathBuf> {
    let absolute = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|err| ViewError::io("resolve current dir", err))?
            .join(raw)
    };
    // Canonicalize the nearest existing ancestor so `..` cannot escape checks,
    // then re-append the remaining components.
    let mut existing = absolute.clone();
    let mut tail = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(|name| name.to_os_string()) else {
            break;
        };
        tail.push(name);
        if !existing.pop() {
            break;
        }
    }
    let mut canonical = existing
        .canonicalize()
        .map_err(|err| ViewError::ExportFailed {
            detail: format!("无法解析输出目录 {}: {err}", raw.display()),
        })?;
    for component in tail.into_iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

fn validate_existing_target(out: &Path) -> ViewResult<()> {
    if !out.exists() {
        return Ok(());
    }
    let metadata = std::fs::metadata(out)
        .map_err(|err| ViewError::io(&format!("stat {}", out.display()), err))?;
    if !metadata.is_dir() {
        return Err(ViewError::ExportFailed {
            detail: format!("输出路径 {} 已存在且不是目录", out.display()),
        });
    }
    let entries = std::fs::read_dir(out)
        .map_err(|err| ViewError::io(&format!("read {}", out.display()), err))?
        .count();
    if entries == 0 {
        return Ok(());
    }
    let manifest = out.join("manifest.json");
    let bytes = std::fs::read(&manifest).map_err(|_| ViewError::ExportFailed {
        detail: format!(
            "输出目录 {} 非空且不是 marqdo-view 导出（缺少 manifest.json）；拒绝覆盖",
            out.display()
        ),
    })?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| ViewError::ExportFailed {
            detail: format!("{} 不是合法的 View 导出清单", manifest.display()),
        })?;
    if value.get("schema").and_then(|schema| schema.as_str()) != Some(MANIFEST_SCHEMA) {
        return Err(ViewError::ExportFailed {
            detail: format!(
                "输出目录 {} 的 manifest 不是 {}；拒绝覆盖",
                out.display(),
                MANIFEST_SCHEMA
            ),
        });
    }
    Ok(())
}

fn staging_dir(out: &Path) -> ViewResult<PathBuf> {
    let parent = out.parent().ok_or_else(|| ViewError::ExportFailed {
        detail: "输出目录没有父目录".to_string(),
    })?;
    std::fs::create_dir_all(parent)
        .map_err(|err| ViewError::io(&format!("create {}", parent.display()), err))?;
    let staging = parent.join(format!(
        ".marqdo-view-export-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir(&staging)
        .map_err(|err| ViewError::io(&format!("create staging {}", staging.display()), err))?;
    Ok(staging)
}

fn publish(staging: &Path, out: &Path) -> ViewResult<()> {
    if out.exists() {
        // Only reaches here for an empty dir or a previous View export (checked
        // before any write), so replacing it cannot destroy user content.
        std::fs::remove_dir_all(out)
            .map_err(|err| ViewError::io(&format!("replace {}", out.display()), err))?;
    }
    std::fs::rename(staging, out).map_err(|err| {
        let _ = std::fs::remove_dir_all(staging);
        ViewError::ExportFailed {
            detail: format!("原子发布 {} 失败: {err}", out.display()),
        }
    })?;
    Ok(())
}

/// Export link target check used by tests: no path in the manifest may be
/// absolute or contain parent traversal.
pub fn manifest_paths_are_relative(manifest: &serde_json::Value) -> bool {
    let checks = manifest
        .get("files")
        .and_then(|files| files.as_array())
        .cloned()
        .unwrap_or_default();
    checks.iter().all(|entry| {
        entry
            .get("path")
            .and_then(|path| path.as_str())
            .map(|path| SafeRelativePath::parse(path).is_ok())
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_target_outside_project_is_accepted() {
        let dir = std::env::current_dir().unwrap();
        let target = absolute_export_target(&dir.join("target/test-export-out")).unwrap();
        assert!(target.is_absolute());
        assert!(!target.to_string_lossy().contains(".."));
    }

    #[test]
    fn manifest_relative_paths_are_validated() {
        let manifest = serde_json::json!({
            "files": [
                {"path": "index.html"},
                {"path": "pages/docs/a.mq.md.html"},
                {"path": "assets/reader.0123456789abcdef.css"}
            ]
        });
        assert!(manifest_paths_are_relative(&manifest));
        let bad = serde_json::json!({"files": [{"path": "../escape.html"}]});
        assert!(!manifest_paths_are_relative(&bad));
        let absolute = serde_json::json!({"files": [{"path": "/etc/passwd"}]});
        assert!(!manifest_paths_are_relative(&absolute));
    }
}
