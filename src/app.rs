//! Application layer: turns parsed commands into use cases.
//!
//! No HTML is built here; no Markdown is parsed here. Browse uses the single
//! renderer (PR-02+), verify only checks bindings, export only writes static
//! files through the same renderer (PR-05).

use crate::cli::{ExportArgs, RuntimeOpts, VerifyArgs};
use crate::core_client::{CheckOptions, CoreClient};
use crate::error::{ViewError, ViewResult};
use crate::project::Project;
use crate::source_map::sha256_hex;
use crate::version;

pub fn verify(opts: &RuntimeOpts, args: &VerifyArgs) -> ViewResult<i32> {
    let path = args
        .path
        .clone()
        .or_else(|| opts.path.clone())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let project = Project::open(&path)?;
    let entry = project.entry().relative.clone();
    let source_bytes = project.read_source(&entry)?;
    let source_text = String::from_utf8(source_bytes.clone()).map_err(|err| ViewError::Io {
        detail: format!("{entry} is not valid UTF-8: {err}"),
    })?;
    let _ = source_text;

    let client = CoreClient::from_env(opts.marqdo_bin.as_deref())?;
    let core_version = client.negotiate()?;
    let check = client.check(
        &project,
        &entry,
        &CheckOptions {
            refs: args.refs,
            timeout: None,
        },
    )?;

    let analysis: serde_json::Value =
        serde_json::from_slice(&check.analysis_bytes).map_err(|err| ViewError::AnalysisFailed {
            detail: format!("emitted Analysis is not JSON: {err}"),
        })?;
    let summary = validate_static_binding(&analysis, entry.as_str(), &source_bytes)?;

    if args.json {
        let doc = serde_json::json!({
            "schema": "marqdo.view-verify",
            "schema_version": 1,
            "ok": true,
            "core": {
                "version": core_version.version.to_string(),
                "supported_range": version::PRODUCT_RANGE,
            },
            "analysis": {
                "schema": "marqdo.analysis",
                "schema_version": 2,
                "digest": check.analysis_digest,
                "producer_version": summary.producer_version,
            },
            "project": {
                "root_id": summary.root_id,
                "identity_kind": summary.identity_kind,
            },
            "document": {
                "relative_path": entry.as_str(),
                "source_digest": summary.source_digest,
                "source_digest_matches": true,
            },
            "result": "PASS",
            "executed": false,
            "note": "Static verification only; the document was not executed.",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!(
            "core: marqdo {} (supported {})",
            core_version.version,
            version::PRODUCT_RANGE
        );
        println!("project: {} ({})", summary.root_id, summary.identity_kind);
        println!("document: {}", entry);
        println!(
            "analysis: schema=marqdo.analysis v2 modules={} digest={}",
            summary.module_count, check.analysis_digest
        );
        println!("source digest: match");
        println!("result: PASS (static only; document not executed)");
    }
    Ok(0)
}

pub fn export(_opts: &RuntimeOpts, _args: &ExportArgs) -> ViewResult<i32> {
    Err(ViewError::Unsupported {
        detail: "static export lands with PR-05; this build teaches Tool Protocol and statics only"
            .to_string(),
    })
}

pub fn browse(_opts: &RuntimeOpts) -> ViewResult<i32> {
    Err(ViewError::Unsupported {
        detail: "the Live reader lands with PR-02/PR-03; this build teaches Tool Protocol and statics only"
            .to_string(),
    })
}

struct BindingSummary {
    root_id: String,
    identity_kind: String,
    producer_version: String,
    module_count: usize,
    source_digest: String,
}

/// Structural binding checks that do not need the full typed projection yet
/// (the typed `SemanticProjection` replaces this in PR-02 without changing the
/// externally visible `verify` result).
fn validate_static_binding(
    analysis: &serde_json::Value,
    relative_path: &str,
    source_bytes: &[u8],
) -> ViewResult<BindingSummary> {
    let schema = analysis
        .get("schema")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if schema != "marqdo.analysis" {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!("unexpected Analysis schema {schema:?}"),
        });
    }
    match analysis.get("schema_version").and_then(|v| v.as_u64()) {
        Some(2) => {}
        other => {
            return Err(ViewError::ProtocolIncompatible {
                detail: format!(
                    "Analysis schema_version {other:?} is not supported (View implements v{})",
                    version::ANALYSIS_PROTOCOL
                ),
            })
        }
    }
    let producer = analysis
        .get("producer")
        .ok_or_else(|| ViewError::AnalysisFailed {
            detail: "Analysis is missing producer".to_string(),
        })?;
    let producer_version = producer
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let markdown_profile = producer
        .get("markdown_profile")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if markdown_profile != "marqdo.md.v1" {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "unexpected Markdown profile {markdown_profile:?}; this View targets marqdo.md.v1"
            ),
        });
    }
    let project = analysis
        .get("project")
        .ok_or_else(|| ViewError::AnalysisFailed {
            detail: "Analysis is missing project".to_string(),
        })?;
    let root_id = project
        .get("root_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if root_id.is_empty() {
        return Err(ViewError::AnalysisFailed {
            detail: "Analysis project.root_id is empty".to_string(),
        });
    }
    let identity_kind = project
        .get("identity_kind")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let modules = analysis
        .get("modules")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ViewError::AnalysisFailed {
            detail: "Analysis is missing modules[]".to_string(),
        })?;
    let expected_digest = sha256_hex(source_bytes);
    let mut matches = 0usize;
    for module in modules {
        if module.get("relative_path").and_then(|v| v.as_str()) == Some(relative_path) {
            matches += 1;
            let digest = module
                .get("source_digest")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if digest != expected_digest {
                return Err(ViewError::SourceMismatch {
                    expected: digest.to_string(),
                    actual: expected_digest,
                });
            }
        }
    }
    if matches == 0 {
        return Err(ViewError::AnalysisFailed {
            detail: format!("Analysis has no module for {relative_path}"),
        });
    }
    Ok(BindingSummary {
        root_id,
        identity_kind,
        producer_version,
        module_count: modules.len(),
        source_digest: expected_digest,
    })
}
