//! Presentation model: the one intermediate form between verified domain data
//! and the one `ReaderRenderer`.
//!
//! Nothing here spawns processes, builds HTML or parses Markdown semantics. It
//! only composes already-verified pieces (RFC-VIEW-0001 §6.3, §14.2).

use crate::document::{DocumentIdentity, DocumentModel};
use crate::project::{Project, SafeRelativePath, ViewConfig};
use crate::semantic::{CoreDiagnostic, SemanticProjection, ViewWarning};

/// Private View presentation-model version. Not a public protocol.
pub const PRESENTATION_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    /// No usable `MARQDO_BIN`; prose only, semantics explicitly unverified.
    CoreUnavailable,
    /// `check` succeeded and Analysis v2 was verified.
    StaticOk,
    /// `check` failed; diagnostics shown, semantics revoked.
    CoreError,
    /// Analysis/source digest mismatch or an outdated artifact.
    Stale,
    /// Unknown Analysis/Tool major or product range.
    ProtocolIncompatible,
    /// Analysis `source_digest` differs from the current source bytes.
    SourceMismatch,
}

impl SourceStatus {
    pub fn data_value(&self) -> &'static str {
        match self {
            SourceStatus::CoreUnavailable => "core-unavailable",
            SourceStatus::StaticOk => "static-ok",
            SourceStatus::CoreError => "core-error",
            SourceStatus::Stale => "stale",
            SourceStatus::ProtocolIncompatible => "protocol-incompatible",
            SourceStatus::SourceMismatch => "source-mismatch",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SourceStatus::CoreUnavailable => "核心不可用 · 语义未验证",
            SourceStatus::StaticOk => "静态检查已通过 · 未运行",
            SourceStatus::CoreError => "Core 检查失败 · 语义不可用",
            SourceStatus::Stale => "语义结果已过期",
            SourceStatus::ProtocolIncompatible => "核心协议不兼容",
            SourceStatus::SourceMismatch => "源码与分析不一致",
        }
    }

    pub fn semantics_usable(&self) -> bool {
        matches!(self, SourceStatus::StaticOk)
    }
}

#[derive(Debug, Clone)]
pub struct Navigation {
    pub site_title: String,
    pub documents: Vec<SafeRelativePath>,
    pub active: Option<SafeRelativePath>,
    /// True only when an explicit `view.json` names this document as welcome.
    pub welcome: bool,
}

impl Navigation {
    pub fn from_project(project: &Project, config: &ViewConfig, active: &SafeRelativePath) -> Self {
        let documents = project.list_documents().unwrap_or_default();
        let welcome = config
            .welcome
            .as_ref()
            .map(|welcome| welcome == active)
            .unwrap_or(false);
        Self {
            site_title: config
                .site_title
                .clone()
                .unwrap_or_else(|| "Marqdo".to_string()),
            documents,
            active: Some(active.clone()),
            welcome,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PresentationModel {
    pub version: u32,
    pub title: String,
    pub document: DocumentModel,
    pub semantic: Option<SemanticProjection>,
    pub source_status: SourceStatus,
    /// Human-readable reason for a degraded status (already normalized: no
    /// absolute paths from the Core).
    pub status_note: Option<String>,
    pub diagnostics: Vec<CoreDiagnostic>,
    pub warnings: Vec<ViewWarning>,
    pub navigation: Navigation,
    /// Function/section outline entries rendered from verified Analysis only.
    pub outline: Vec<OutlineEntry>,
    /// Frozen core product version used for this page, when one was consulted.
    pub core_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OutlineEntry {
    pub label: String,
    pub anchor: String,
    pub detail: String,
    pub kind: OutlineKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineKind {
    Function,
    Knowledge,
    Section,
}

impl PresentationModel {
    pub fn functions(&self) -> impl Iterator<Item = &OutlineEntry> {
        self.outline
            .iter()
            .filter(|entry| entry.kind == OutlineKind::Function)
    }

    pub fn error_diagnostics(&self) -> impl Iterator<Item = &CoreDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == "error")
    }
}

/// Input to the one builder. `analysis` is already protocol/byte verified.
pub struct BuildInput<'a> {
    pub document: DocumentModel,
    pub analysis: Option<&'a crate::semantic::VerifiedAnalysis>,
    pub status: SourceStatus,
    pub status_note: Option<String>,
    pub diagnostics: Vec<CoreDiagnostic>,
    pub navigation: Navigation,
    pub core_version: Option<String>,
}

pub fn build(input: BuildInput<'_>) -> PresentationModel {
    let BuildInput {
        mut document,
        analysis,
        status,
        status_note,
        mut diagnostics,
        navigation,
        core_version,
    } = input;

    let semantic = analysis.map(|verified| verified.projection.clone());
    if let Some(projection) = &semantic {
        document.identity = DocumentIdentity {
            project_root_id: projection.project_root_id.clone(),
            module_id: projection.module_id.clone(),
            relative_path: projection.relative_path.clone(),
            source_sha256: projection.source_digest.clone(),
        };
        for diagnostic in &projection.diagnostics {
            if !diagnostics.iter().any(|existing| {
                existing.code == diagnostic.code
                    && existing.message == diagnostic.message
                    && existing.span == diagnostic.span
            }) {
                diagnostics.push(diagnostic.clone());
            }
        }
    }
    let warnings = semantic
        .as_ref()
        .map(|projection| projection.warnings.clone())
        .unwrap_or_default();
    let outline = semantic.as_ref().map(build_outline).unwrap_or_default();

    PresentationModel {
        version: PRESENTATION_VERSION,
        title: document.title(),
        document,
        semantic,
        source_status: status,
        status_note,
        diagnostics,
        warnings,
        navigation,
        outline,
        core_version,
    }
}

fn build_outline(projection: &SemanticProjection) -> Vec<OutlineEntry> {
    let mut entries = Vec::new();
    for function in &projection.functions {
        let params = if function.signature.params.is_empty() {
            String::new()
        } else {
            function
                .signature
                .params
                .iter()
                .map(|param| {
                    if param.ty.is_empty() {
                        param.name.clone()
                    } else {
                        format!("{}: {}", param.name, param.ty)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let detail = if params.is_empty() {
            "无参数".to_string()
        } else {
            format!("params({params})")
        };
        entries.push(OutlineEntry {
            label: function.local_name.clone(),
            anchor: anchor_for_span(function.heading_span.start, function.heading_span.end),
            detail,
            kind: OutlineKind::Function,
        });
    }
    for unit in &projection.units {
        let kind = match unit.kind {
            crate::semantic::UnitKind::Knowledge if unit.level >= 2 => OutlineKind::Knowledge,
            crate::semantic::UnitKind::Section => OutlineKind::Section,
            _ => continue,
        };
        if unit.name.is_empty() {
            continue;
        }
        entries.push(OutlineEntry {
            label: unit.name.clone(),
            anchor: anchor_for_span(unit.span.start, unit.span.end),
            detail: if unit.level == 2 {
                "知识单元".to_string()
            } else {
                "章节".to_string()
            },
            kind,
        });
    }
    entries
}

/// Deterministic heading anchor derived from the heading byte span. It never
/// depends on rendered text, so duplicate headings cannot collide.
pub fn anchor_for_span(start: u64, end: u64) -> String {
    format!("mq-h-{start:x}-{end:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_values_are_stable() {
        assert_eq!(SourceStatus::StaticOk.data_value(), "static-ok");
        assert_eq!(SourceStatus::CoreError.data_value(), "core-error");
        assert!(SourceStatus::StaticOk.semantics_usable());
        assert!(!SourceStatus::CoreUnavailable.semantics_usable());
        assert!(!SourceStatus::SourceMismatch.semantics_usable());
    }

    #[test]
    fn heading_anchors_are_deterministic_and_collision_free() {
        assert_eq!(anchor_for_span(1, 2), "mq-h-1-2");
        assert_ne!(anchor_for_span(1, 2), anchor_for_span(1, 3));
    }
}
