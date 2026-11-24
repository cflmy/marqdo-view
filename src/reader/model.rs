//! Presentation model: the one intermediate form between verified domain data
//! and the one `ReaderRenderer` (RFC-VIEW-0002 §5.2).
//!
//! Nothing here spawns processes, builds HTML or parses Markdown semantics.
//! Assembly from Analysis/Trace happens in `reader::assemble`.

use crate::config::ViewConfig;
use crate::protocol::analysis_v2::{CoreDiagnostic, SemanticProjection, ViewWarning};
use crate::protocol::trace_v2::{RunDisplayState, RunProjection};
use crate::reader::markdown::DocumentModel;
use crate::reader::source::{Project, SafeRelativePath};

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
    pub heading_slugs: std::collections::HashMap<crate::reader::source::SourceSpan, String>,
    pub media: std::collections::BTreeMap<String, std::sync::Arc<super::media::Media>>,
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
    /// Present only after an explicit user-triggered run whose trace verified.
    pub run: Option<RunProjection>,
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
    /// True when the entry module exposes a resolvable `main` (canonical or
    /// unique local name). The dock Run button never substitutes another
    /// function for a missing entry function (RFC-VIEW-0002 §2.4).
    pub fn has_entry_main(&self) -> bool {
        let Some(semantic) = self.semantic.as_ref() else {
            return false;
        };
        semantic.functions.iter().any(|function| {
            let canonical = canonical_function_name(&function.id, &semantic.module_id);
            canonical == "main" || function.local_name == "main"
        })
    }

    pub fn run_state(&self) -> RunDisplayState {
        match &self.run {
            Some(run) => run.state,
            None => RunDisplayState::NotRun,
        }
    }

    /// True when a run button may be offered (explicit confirmation still
    /// required). Never true for exports or without a verified core.
    pub fn can_offer_run(&self) -> bool {
        self.source_status.semantics_usable()
            && self
                .semantic
                .as_ref()
                .map(|semantic| {
                    semantic
                        .functions
                        .iter()
                        .any(|function| function.local_name == "main")
                })
                .unwrap_or(false)
    }

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

/// Deterministic heading anchor derived from the heading byte span. It never
/// depends on rendered text, so duplicate headings cannot collide.
pub fn anchor_for_span(start: u64, end: u64) -> String {
    format!("mq-h-{start:x}-{end:x}")
}

/// Analysis v2 function ids are `<module id>::<canonical name>`.
fn canonical_function_name(function_id: &str, module_id: &str) -> String {
    function_id
        .strip_prefix(&format!("{module_id}::"))
        .unwrap_or(function_id)
        .to_string()
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
