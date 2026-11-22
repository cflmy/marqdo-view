//! Application layer: turns parsed commands into use cases.
//!
//! No HTML is built here and no Markdown semantics are decided here. Browse and
//! (later) Export call the same `load` + one `ReaderRenderer`.

use crate::cli::{ExportArgs, RuntimeOpts, VerifyArgs};
use crate::core_client::{
    CheckOptions, CheckResponse, CoreClient, RawCheckDiagnostic, RunOptions, SandboxMode,
};
use crate::document::{DocumentIdentity, DocumentModel};
use crate::error::{ViewError, ViewResult};
use crate::presentation::{self, BuildInput, Navigation, PresentationModel, SourceStatus};
use crate::project::{Project, SafeRelativePath, ViewConfig};
use crate::semantic::{self, CoreDiagnostic};
use crate::trace::{self, TraceVerificationInput, VerifiedRun};
use crate::version;

/// Process-wide context for one browsed project: root, optional site config and
/// an optional negotiated frozen core. A missing core degrades to prose only;
/// an incompatible protocol is a hard error (RFC §4).
pub struct AppContext {
    project: Project,
    config: ViewConfig,
    core: Option<CoreClient>,
    core_version: Option<String>,
    core_note: Option<String>,
}

impl AppContext {
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
            match client.check(&self.project, relative, &CheckOptions::default())? {
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
                } => match semantic::verify_analysis(
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
                        projection: trace::stale_projection(
                            &run.projection.analysis_digest,
                            &document.source_index,
                        ),
                    })
                }
            }
            (Some(run), None) => Some(VerifiedRun {
                projection: trace::stale_projection(
                    &run.projection.analysis_digest,
                    &document.source_index,
                ),
            }),
            (None, _) => None,
        };

        Ok(presentation::build(BuildInput {
            document,
            analysis: verified.as_ref(),
            status,
            status_note,
            diagnostics,
            navigation,
            core_version: self.core_version.clone(),
            run: attached_run.as_ref(),
        }))
    }

    /// Execute one document through the public Core `run` channel after the
    /// caller has obtained explicit user confirmation and a CSRF token. Never
    /// called from page loads or exports.
    pub fn run_document(&self, relative: &SafeRelativePath) -> ViewResult<ExecutedRun> {
        let client = self.core.as_ref().ok_or_else(|| ViewError::RunDenied {
            detail: "没有可用的冻结核心（MARQDO_BIN），运行被拒绝".to_string(),
        })?;
        let source_bytes = self.project.read_source(relative)?;
        let source_digest = crate::source_map::sha256_hex(&source_bytes);
        let source = String::from_utf8(source_bytes).map_err(|err| ViewError::Io {
            detail: format!("{relative} is not valid UTF-8: {err}"),
        })?;
        let source_index = crate::source_map::SourceIndex::new(&source);

        let run = client.run(
            &self.project,
            relative,
            &RunOptions {
                mode: SandboxMode::Pure,
                view_timeout: Some(std::time::Duration::from_secs(120)),
                timeout_seconds: Some(90),
                ..Default::default()
            },
        )?;
        let trace_bytes = run
            .trace_bytes
            .clone()
            .ok_or_else(|| ViewError::RunDenied {
                detail: "Core 未写出 Trace v2 文件，运行结果不可绑定".to_string(),
            })?;

        // Re-check after the run so the digest binds to the exact Analysis of
        // the source that was executed and is still current.
        let response = client.check(&self.project, relative, &CheckOptions::default())?;
        let check = response.analysis.ok_or_else(|| ViewError::AnalysisFailed {
            detail: "运行后的静态检查失败，无法绑定 Trace".to_string(),
        })?;
        let verified = semantic::verify_analysis(
            &check.analysis_bytes,
            &source,
            &source_index,
            relative.as_str(),
        )?;
        let verified_run = trace::verify_trace(TraceVerificationInput {
            trace_bytes: &trace_bytes,
            analysis_bytes: &check.analysis_bytes,
            semantic: &verified.projection,
            modules: &verified.projection.modules,
            source: &source,
            source_index: &source_index,
            run_json: &run.json,
            exit_code: run.exit_code,
        })?;
        Ok(ExecutedRun {
            verified: verified_run,
            source_digest,
            analysis_digest: check.analysis_digest,
        })
    }
}

pub struct ExecutedRun {
    pub verified: VerifiedRun,
    pub source_digest: String,
    pub analysis_digest: String,
}

pub fn verify(opts: &RuntimeOpts, args: &VerifyArgs) -> ViewResult<i32> {
    let path = args
        .path
        .clone()
        .or_else(|| opts.path.clone())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let project = Project::open(&path)?;
    let entry = project.entry().relative.clone();
    let source_bytes = project.read_source(&entry)?;
    let source = String::from_utf8(source_bytes).map_err(|err| ViewError::Io {
        detail: format!("{entry} is not valid UTF-8: {err}"),
    })?;
    let source_index = crate::source_map::SourceIndex::new(&source);

    let client = CoreClient::from_env(opts.marqdo_bin.as_deref())?;
    let core_version = client.negotiate()?;
    let response = client.check(
        &project,
        &entry,
        &CheckOptions {
            refs: args.refs,
            timeout: None,
        },
    )?;
    let check = match response {
        CheckResponse {
            success: false,
            diagnostics,
            exit_code,
            ..
        } => {
            let first = diagnostics
                .first()
                .map(|diagnostic| {
                    format!(
                        "{}: {}",
                        if diagnostic.code.is_empty() {
                            "diagnostic"
                        } else {
                            &diagnostic.code
                        },
                        diagnostic.message
                    )
                })
                .unwrap_or_else(|| "no diagnostic detail".to_string());
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "`marqdo check` failed with exit code {exit_code} ({} diagnostic(s)); first: {first}",
                    diagnostics.len()
                ),
            });
        }
        CheckResponse {
            analysis: Some(check),
            ..
        } => check,
        CheckResponse { analysis: None, .. } => {
            return Err(ViewError::AnalysisFailed {
                detail: "`marqdo check` succeeded but emitted no Analysis".to_string(),
            })
        }
    };
    let verified = semantic::verify_analysis(
        &check.analysis_bytes,
        &source,
        &source_index,
        entry.as_str(),
    )?;
    let projection = &verified.projection;

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
                "schema_version": version::ANALYSIS_PROTOCOL,
                "digest": projection.analysis_digest,
                "producer_version": projection.producer_version,
                "markdown_profile": projection.markdown_profile,
                "functions": projection.functions.len(),
                "units": projection.units.len(),
                "diagnostics": projection.diagnostics.len(),
            },
            "project": {
                "root_id": projection.project_root_id,
                "identity_kind": projection.identity_kind,
                "lock_digest": projection.lock_digest,
                "stdlib_version": projection.stdlib_version,
            },
            "document": {
                "relative_path": entry.as_str(),
                "module_id": projection.module_id,
                "source_digest": projection.source_digest,
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
        println!(
            "project: {} ({})",
            projection.project_root_id, projection.identity_kind
        );
        println!("document: {entry}");
        println!(
            "analysis: schema=marqdo.analysis v2 modules={} functions={} digest={}",
            projection.modules.len(),
            projection.functions.len(),
            projection.analysis_digest
        );
        println!("source digest: match");
        println!("result: PASS (static only; document not executed)");
    }
    Ok(0)
}

pub fn browse(opts: &RuntimeOpts) -> ViewResult<i32> {
    let path = opts
        .path
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let context = AppContext::open(&path, opts.marqdo_bin.as_deref())?;
    crate::server::serve(context, opts)
}

pub fn export(_opts: &RuntimeOpts, _args: &ExportArgs) -> ViewResult<i32> {
    Err(ViewError::Unsupported {
        detail: "static export lands with PR-05; browse and verify are available in this build"
            .to_string(),
    })
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
