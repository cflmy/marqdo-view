//! Pure static site export.
//!
//! Export uses the same `AppContext::load` (static `check` only) and the same
//! `ReaderRenderer` as Live; it never calls `run`, never starts a provider and
//! never writes into the source project (RFC-VIEW-0001 §11.3).

use crate::app::AppContext;
use crate::assets::AssetBundle;
use crate::error::{ViewError, ViewResult};
use crate::project::SafeRelativePath;
use crate::render::ReaderRenderer;
use crate::source_map::sha256_hex;
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

pub fn export(context: &AppContext, options: &ExportOptions) -> ViewResult<ExportSummary> {
    let bundle = AssetBundle::get();
    let renderer = ReaderRenderer::new();
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

    let documents = context.project().list_documents()?;
    if documents.is_empty() {
        return Err(ViewError::ExportFailed {
            detail: "项目内没有 .mq.md 文档可导出".to_string(),
        });
    }
    let entry = context.project().entry().relative.clone();
    let staging = staging_dir(&out)?;

    let result = (|| -> ViewResult<ExportSummary> {
        let assets_dir = staging.join("assets");
        let pages_dir = staging.join("pages");
        std::fs::create_dir_all(&assets_dir)
            .map_err(|err| ViewError::io(&format!("create {}", assets_dir.display()), err))?;
        std::fs::create_dir_all(&pages_dir)
            .map_err(|err| ViewError::io(&format!("create {}", pages_dir.display()), err))?;

        // Assets: the exact same bytes Live serves.
        let mut asset_manifest = Vec::new();
        for asset in bundle.all() {
            let name = if asset.extension == "css" && asset.name == "reader" {
                bundle.hashed_css_name()
            } else {
                asset.hashed_name()
            };
            let bytes = if asset.extension == "css" && asset.name == "reader" {
                bundle.css_bytes().to_vec()
            } else {
                asset.bytes.to_vec()
            };
            std::fs::write(assets_dir.join(&name), &bytes)
                .map_err(|err| ViewError::io(&format!("write asset {name}"), err))?;
            asset_manifest.push(serde_json::json!({
                "path": format!("assets/{name}"),
                "sha256": sha256_hex(&bytes),
                "size": bytes.len(),
            }));
        }

        let mut pages = Vec::new();
        let mut file_manifest: Vec<serde_json::Value> = Vec::new();
        for relative in &documents {
            let model = context.load(relative)?;
            let depth = relative.as_str().split('/').count();
            let context =
                crate::render::RenderContext::static_export(depth, !options.no_annotations);
            let page = renderer.render(&model, &context);
            let page_path = pages_dir.join(format!("{}.html", relative.as_str()));
            if let Some(parent) = page_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| ViewError::io(&format!("create {}", parent.display()), err))?;
            }
            std::fs::write(&page_path, page.html.as_bytes())
                .map_err(|err| ViewError::io(&format!("write {}", page_path.display()), err))?;
            let page_sha = sha256_hex(page.html.as_bytes());
            let page_rel = format!("pages/{}.html", relative.as_str());
            file_manifest.push(serde_json::json!({
                "path": page_rel,
                "sha256": page_sha,
                "size": page.html.len(),
            }));
            pages.push(serde_json::json!({
                "relative_path": relative.as_str(),
                "module_id": model.document.identity.module_id,
                "source_sha256": sha256_hex(model.document.source.as_bytes()),
                "source_status": model.source_status.data_value(),
                "analysis_digest": model.semantic.as_ref().map(|semantic| semantic.analysis_digest.clone()),
                "file": page_rel,
                "sha256": page_sha,
                "annotations": !options.no_annotations,
            }));
        }

        // index.html is the same renderer output with site-root depth.
        let entry_model = context.load(&entry)?;
        let entry_context = crate::render::RenderContext::static_export(0, !options.no_annotations);
        let index_page = renderer.render(&entry_model, &entry_context);
        let index_path = staging.join("index.html");
        std::fs::write(&index_path, index_page.html.as_bytes())
            .map_err(|err| ViewError::io(&format!("write {}", index_path.display()), err))?;
        let index_sha = sha256_hex(index_page.html.as_bytes());
        file_manifest.push(serde_json::json!({
            "path": "index.html",
            "sha256": index_sha,
            "size": index_page.html.len(),
        }));

        file_manifest.sort_by(|a, b| {
            a["path"]
                .as_str()
                .unwrap_or("")
                .cmp(b["path"].as_str().unwrap_or(""))
        });
        let manifest = serde_json::json!({
            "schema": MANIFEST_SCHEMA,
            "schema_version": 1,
            "view": {
                "name": "marqdo-view",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "compatibility": {
                "marqdo": crate::version::PRODUCT_RANGE,
                "tool_protocol": crate::version::TOOL_PROTOCOL,
                "analysis": crate::version::ANALYSIS_PROTOCOL,
                "trace": crate::version::TRACE_PROTOCOL,
                "markdown_profile": "marqdo.md.v1",
            },
            "core_version": entry_model.core_version,
            "entry": {
                "relative_path": entry.as_str(),
                "file": "index.html",
                "sha256": index_sha,
            },
            "pages": pages,
            "assets": asset_manifest,
            "files": file_manifest,
            "executed": false,
            "note": "Static export only; reading or exporting never executes the document.",
        });
        let manifest_bytes =
            serde_json::to_vec_pretty(&manifest).map_err(|err| ViewError::ExportFailed {
                detail: format!("serialize export manifest: {err}"),
            })?;
        let manifest_path = staging.join("manifest.json");
        std::fs::write(&manifest_path, &manifest_bytes)
            .map_err(|err| ViewError::io(&format!("write {}", manifest_path.display()), err))?;

        publish(&staging, &out)?;
        Ok(ExportSummary {
            out,
            pages: documents.len(),
            files: file_manifest.len() + asset_manifest.len() + 1,
            manifest_sha256: sha256_hex(&manifest_bytes),
        })
    })();

    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
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
