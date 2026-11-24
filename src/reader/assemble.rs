//! Assembly of the one `PresentationModel`.
//!
//! Merges the raw document, verified Analysis v2, diagnostics and an optional
//! verified Trace v2 run into the model; decides stale/revoked semantics here
//! and nowhere else (RFC-VIEW-0002 §5.2/§5.4).

use crate::config::ViewConfig;
use crate::protocol::analysis_v2::{CoreDiagnostic, SemanticProjection};
use crate::protocol::trace_v2::VerifiedRun;
use crate::reader::markdown::{DocumentIdentity, DocumentModel};
use crate::reader::model::{
    Navigation, OutlineEntry, OutlineKind, PresentationModel, SourceStatus,
};
use crate::reader::source::{sha256_hex, Project, SafeRelativePath};

use crate::core_client::{CheckOptions, CheckResponse, CoreClient, RawCheckDiagnostic};
use crate::error::{ViewError, ViewResult};
use crate::protocol::analysis_v2;
use crate::protocol::trace_v2;
use crate::reader::model::{anchor_for_span, PRESENTATION_VERSION};

/// Input to the one builder. `analysis` is already protocol/byte verified.
pub struct BuildInput<'a> {
    pub document: DocumentModel,
    pub analysis: Option<&'a crate::protocol::analysis_v2::VerifiedAnalysis>,
    pub status: SourceStatus,
    pub status_note: Option<String>,
    pub diagnostics: Vec<CoreDiagnostic>,
    pub navigation: Navigation,
    pub core_version: Option<String>,
    pub run: Option<&'a crate::protocol::trace_v2::VerifiedRun>,
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
        run,
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
        heading_slugs: super::links::heading_slugs(&document.blocks),
        media: Default::default(),
        document,
        semantic,
        source_status: status,
        status_note,
        diagnostics,
        warnings,
        navigation,
        outline,
        core_version,
        run: run.map(|verified| verified.projection.clone()),
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
            crate::protocol::analysis_v2::UnitKind::Knowledge if unit.level >= 2 => {
                OutlineKind::Knowledge
            }
            crate::protocol::analysis_v2::UnitKind::Section => OutlineKind::Section,
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

pub struct DocumentService {
    project: Project,
    config: ViewConfig,
    core: Option<CoreClient>,
    core_version: Option<String>,
    core_note: Option<String>,
}

impl DocumentService {
    pub fn open(path: &std::path::Path, marqdo_bin: Option<&std::path::Path>) -> ViewResult<Self> {
        let project = Project::open(path)?;
        let config = ViewConfig::load(project.root())?;
        let (core, core_version, core_note) = match CoreClient::from_env(marqdo_bin) {
            Ok(client) => match client.negotiate() {
                Ok(version) => (Some(client), Some(version.version.to_string()), None),
                Err(err) if err.code() == "view.protocol_incompatible" => return Err(err),
                Err(err) => (None, None, Some(generic_core_note(&err))),
            },
            Err(err) if err.code() == "view.core_missing" => {
                (None, None, Some(generic_core_note(&err)))
            }
            Err(err) => return Err(err),
        };
        Ok(Self {
            project,
            config,
            core,
            core_version,
            core_note,
        })
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn config(&self) -> &ViewConfig {
        &self.config
    }

    pub fn core(&self) -> Option<&CoreClient> {
        self.core.as_ref()
    }

    pub fn core_client(&self) -> Option<&CoreClient> {
        self.core.as_ref()
    }

    pub fn core_version(&self) -> Option<&str> {
        self.core_version.as_deref()
    }

    pub fn core_note(&self) -> Option<&str> {
        self.core_note.as_deref()
    }

    /// Read + parse + (optionally) check one document into the one presentation
    /// model. Never executes the document.
    pub fn load(&self, relative: &SafeRelativePath) -> ViewResult<PresentationModel> {
        self.load_with_run(relative, None)
    }

    /// Same pipeline, optionally attaching an already-verified run projection.
    /// The run is re-bound to freshly checked Analysis bytes; if the source or
    /// analysis changed meanwhile the result becomes `STALE` instead of being
    /// attached to the wrong text.
    pub fn load_with_run(
        &self,
        relative: &SafeRelativePath,
        run: Option<&VerifiedRun>,
    ) -> ViewResult<PresentationModel> {
        let source_bytes = self.project.read_source(relative)?;
        let source = String::from_utf8(source_bytes).map_err(|err| ViewError::Io {
            detail: format!("{relative} is not valid UTF-8: {err}"),
        })?;
        let document = DocumentModel::parse(source, DocumentIdentity::placeholder(relative));
        let navigation = Navigation::from_project(&self.project, &self.config, relative);

        let mut status = SourceStatus::CoreUnavailable;
        let mut status_note = self.core_note.clone();
        let mut diagnostics = Vec::new();
        let mut verified = None;

        if let Some(client) = &self.core {
            match client.check(
                &self.project,
                relative,
                &CheckOptions {
                    refs: false,
                    timeout: Some(std::time::Duration::from_secs(30)),
                },
            )? {
                CheckResponse {
                    success: false,
                    diagnostics: raw_diagnostics,
                    exit_code,
                    ..
                } => {
                    status = SourceStatus::CoreError;
                    status_note = Some(format!(
                        "Core 检查失败（exit {exit_code}）：语义标注已撤销，正文仍可阅读。"
                    ));
                    diagnostics = raw_diagnostics
                        .iter()
                        .map(|diagnostic| diagnostic_to_view(diagnostic, relative))
                        .collect();
                }
                CheckResponse {
                    success: true,
                    analysis: Some(check),
                    ..
                } => match analysis_v2::verify_analysis(
                    &check.analysis_bytes,
                    &document.source,
                    &document.source_index,
                    relative.as_str(),
                ) {
                    Ok(verified_analysis) => {
                        status = SourceStatus::StaticOk;
                        status_note = None;
                        verified = Some(verified_analysis);
                    }
                    Err(err) if err.code() == "view.source_mismatch" => {
                        status = SourceStatus::SourceMismatch;
                        status_note = Some(
                            "当前源码字节与 Analysis 的 source_digest 不一致，语义标注已禁用。"
                                .to_string(),
                        );
                    }
                    Err(err) if err.code() == "view.protocol_incompatible" => {
                        return Err(err);
                    }
                    Err(err) => {
                        status = SourceStatus::CoreError;
                        status_note = Some(generic_core_note(&err));
                    }
                },
                CheckResponse {
                    success: true,
                    analysis: None,
                    ..
                } => {
                    status = SourceStatus::CoreError;
                    status_note = Some("Core 声称检查成功但没有产出 Analysis。".to_string());
                }
            }
        }

        let attached_run: Option<VerifiedRun> = match (run, verified.as_ref()) {
            (Some(run), Some(analysis)) => {
                let expected = format!("sha256:{}", analysis.projection.analysis_sha256);
                if expected == run.projection.analysis_digest {
                    Some(VerifiedRun {
                        projection: run.projection.clone(),
                    })
                } else {
                    Some(VerifiedRun {
                        projection: trace_v2::stale_projection(
                            &run.projection.analysis_digest,
                            &document.source_index,
                        ),
                    })
                }
            }
            (Some(run), None) => Some(VerifiedRun {
                projection: trace_v2::stale_projection(
                    &run.projection.analysis_digest,
                    &document.source_index,
                ),
            }),
            (None, _) => None,
        };

        let media = super::media::collect(&self.project, relative, &document.blocks);
        let mut model = build(BuildInput {
            document,
            analysis: verified.as_ref(),
            status,
            status_note,
            diagnostics,
            navigation,
            core_version: self.core_version.clone(),
            run: attached_run.as_ref(),
        });
        model.media = media;
        Ok(model)
    }
}

fn generic_core_note(err: &ViewError) -> String {
    match err.code() {
        "view.core_missing" => {
            "未找到可用的冻结核心（MARQDO_BIN）；当前为无语义标注的纯 Markdown 阅读模式。"
                .to_string()
        }
        "view.analysis_failed" => "Core 检查失败；语义标注已撤销，正文仍可阅读。".to_string(),
        other => format!("{other}：语义标注不可用，正文仍可阅读。"),
    }
}

fn diagnostic_to_view(
    diagnostic: &RawCheckDiagnostic,
    relative: &SafeRelativePath,
) -> CoreDiagnostic {
    let belongs_to_entry = diagnostic
        .module
        .as_deref()
        .map(|module| module.ends_with(relative.as_str()))
        .unwrap_or(true);
    CoreDiagnostic {
        module: diagnostic.module.clone(),
        relative_path: if belongs_to_entry {
            Some(relative.as_str().to_string())
        } else {
            None
        },
        span: None,
        line: diagnostic.line,
        column: diagnostic.column,
        severity: if diagnostic.severity.is_empty() {
            "error".to_string()
        } else {
            diagnostic.severity.clone()
        },
        code: diagnostic.code.clone(),
        message: diagnostic.message.clone(),
        hints: diagnostic.hints.clone(),
    }
}

/// One read-only, byte-verified static view of a document, used by explicit
/// run validation and Trace binding. Nothing here executes the document.
#[derive(Debug)]
pub struct StaticDocument {
    pub relative: SafeRelativePath,
    pub source: String,
    pub source_index: crate::reader::source::SourceIndex,
    pub source_sha256: String,
    /// Exact bytes written by `marqdo check --emit`.
    pub analysis_bytes: Vec<u8>,
    pub media_digests: std::collections::BTreeMap<String, String>,
    pub analysis_digest: String,
    pub verified: crate::protocol::analysis_v2::VerifiedAnalysis,
}

impl DocumentService {
    /// Re-run the read-only `check --emit` and verify Analysis v2 against the
    /// current exact source bytes. This is the only static truth used before an
    /// explicit run is accepted.
    pub fn static_document(&self, relative: &SafeRelativePath) -> ViewResult<StaticDocument> {
        let client = self.core.as_ref().ok_or_else(|| ViewError::RunDenied {
            detail: "没有可用的冻结核心（MARQDO_BIN）；无法验证运行请求".to_string(),
        })?;
        let source_bytes = self.project.read_source(relative)?;
        let source_sha256 = sha256_hex(&source_bytes);
        let source = String::from_utf8(source_bytes).map_err(|err| ViewError::Io {
            detail: format!("{relative} is not valid UTF-8: {err}"),
        })?;
        let source_index = crate::reader::source::SourceIndex::new(&source);
        let response = client.check(
            &self.project,
            relative,
            &CheckOptions {
                refs: false,
                timeout: Some(std::time::Duration::from_secs(30)),
            },
        )?;
        let check = match response {
            CheckResponse {
                success: true,
                analysis: Some(check),
                ..
            } => check,
            CheckResponse {
                success: false,
                diagnostics,
                exit_code,
                ..
            } => {
                let first = diagnostics
                    .first()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                    .unwrap_or_else(|| "no diagnostic detail".to_string());
                return Err(ViewError::AnalysisFailed {
                    detail: format!(
                        "Core 检查失败（exit {exit_code}, {} diagnostic(s)）；拒绝执行。first: {first}",
                        diagnostics.len()
                    ),
                });
            }
            CheckResponse { analysis: None, .. } => {
                return Err(ViewError::AnalysisFailed {
                    detail: "Core 声称检查成功但没有产出 Analysis".to_string(),
                });
            }
        };
        let verified = crate::protocol::analysis_v2::verify_analysis(
            &check.analysis_bytes,
            &source,
            &source_index,
            relative.as_str(),
        )?;
        let document =
            DocumentModel::parse(source.clone(), DocumentIdentity::placeholder(relative));
        let media = super::media::collect(&self.project, relative, &document.blocks);
        let media_digests = super::media::digests(&media);
        Ok(StaticDocument {
            relative: relative.clone(),
            source,
            source_index,
            source_sha256,
            analysis_digest: check.analysis_digest,
            analysis_bytes: check.analysis_bytes,
            media_digests,
            verified,
        })
    }
}
