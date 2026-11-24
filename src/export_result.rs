//! Validate the selected session result against a freshly loaded document.
//! The output is an owned reader projection, independent of the job.

use crate::{
    execution::{events::RunState, export_candidate::TerminalExportCandidate},
    reader::{
        assemble::DocumentService,
        media,
        model::PresentationModel,
        snapshot::{ExportResultOverlay, SnapshotExecution, SnapshotProtocols, SnapshotSource},
        source::{sha256_hex, SafeRelativePath},
    },
    ViewError, ViewResult,
};
use serde::Deserialize;

pub const MAX_EXPORT_REQUEST: usize = 8 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportSelection {
    schema: String,
    schema_version: u32,
    pub run_id: String,
    document: String,
    analysis_digest: String,
    scope: Scope,
    include: Include,
    confirm_sensitive_output: bool,
    #[serde(default)]
    confirm_partial: bool,
    client_export_request_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Scope {
    Document,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Include {
    result_value: bool,
    stdout: bool,
    stderr: bool,
    trace_summary: bool,
    arguments: bool,
}

impl ExportSelection {
    pub fn parse(body: &[u8]) -> ViewResult<Self> {
        let invalid = || ViewError::ExportInvalid {
            detail: "invalid v1 static export request".into(),
        };
        let selection: Self = serde_json::from_slice(body).map_err(|_| invalid())?;
        let valid_token = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        };
        let path = SafeRelativePath::parse(&selection.document).map_err(|_| invalid())?;
        let digest = selection
            .analysis_digest
            .strip_prefix("sha256:")
            .ok_or_else(invalid)?;
        if selection.schema != "marqdo.view-static-export-request"
            || selection.schema_version != 1
            || !matches!(selection.scope, Scope::Document)
            || selection.document.len() > 1024
            || path.as_str() != selection.document
            || !selection.document.ends_with(".mq.md")
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !valid_token(&selection.run_id)
            || !valid_token(&selection.client_export_request_id)
        {
            return Err(invalid());
        }
        Ok(selection)
    }

    pub fn prepare(
        &self,
        service: &DocumentService,
        candidate: &TerminalExportCandidate,
    ) -> ViewResult<(PresentationModel, ExportResultOverlay)> {
        let changed = || ViewError::ExportSourceChanged {
            detail: "selected execution no longer matches the document".into(),
        };
        let invalid = || ViewError::ExportInvalid {
            detail: "missing confirmation for selected fields or partial result".into(),
        };
        if self.document != candidate.document || self.analysis_digest != candidate.analysis_digest
        {
            return Err(changed());
        }
        if (self.include.stdout
            || self.include.stderr
            || self.include.arguments
            || self.include.result_value
                && candidate.value.is_some()
                && !candidate.value_is_public_scalar)
            && !self.confirm_sensitive_output
        {
            return Err(invalid());
        }
        if (candidate.state != RunState::Succeeded
            || !candidate.output_complete
            || !candidate.result_complete
            || candidate.observation_truncated
            || candidate.trace_status == "truncated")
            && !self.confirm_partial
        {
            return Err(invalid());
        }
        let document = SafeRelativePath::parse(&self.document)?;
        let model = service.load(&document).map_err(|_| changed())?;
        let analysis = model.semantic.as_ref().ok_or_else(changed)?;
        if analysis.analysis_digest != candidate.analysis_digest
            || analysis.schema_version != candidate.analysis_version
            || analysis.project_root_id != candidate.project_root_id
            || analysis.module_id != candidate.module_id
            || analysis.lock_digest != candidate.lock_digest
            || sha256_hex(model.document.source.as_bytes()) != candidate.source_sha256
            || media::digests(&model.media) != candidate.media
            || !analysis
                .functions
                .iter()
                .any(|function| function.id == candidate.function_id)
        {
            return Err(changed());
        }
        let overlay = ExportResultOverlay {
            source: SnapshotSource {
                project_root_id: candidate.project_root_id.clone(),
                document: candidate.document.clone(),
                module_id: candidate.module_id.clone(),
                source_sha256: candidate.source_sha256.clone(),
                analysis_digest: candidate.analysis_digest.clone(),
                lock_digest: (!candidate.lock_digest.is_empty())
                    .then(|| candidate.lock_digest.clone()),
            },
            execution: SnapshotExecution {
                included: true,
                core_run_id: candidate.core_run_id.clone(),
                target: candidate.function_id.clone(),
                core_version: candidate.core_version.clone(),
                status: candidate.state.data_value(),
                terminal_at: candidate.terminal_at.clone(),
                source_verified: true,
                trace_status: candidate.trace_status.clone(),
                result_complete: candidate.result_complete,
                output_complete: candidate.output_complete,
                observation_truncated: candidate.observation_truncated,
                parameters: if self.include.arguments {
                    "included"
                } else {
                    "omitted"
                },
                mode: candidate.mode.clone(),
                core_protocol: SnapshotProtocols {
                    analysis: candidate.analysis_version,
                    trace: 2,
                    events: 1,
                },
            },
            label: candidate.label.clone(),
            value: self
                .include
                .result_value
                .then(|| candidate.value.clone())
                .flatten(),
            stdout: self.include.stdout.then(|| candidate.stdout.clone()),
            stderr: self.include.stderr.then(|| candidate.stderr.clone()),
            trace_counts: self
                .include
                .trace_summary
                .then(|| candidate.trace_counts.clone())
                .flatten(),
            arguments: self.include.arguments.then(|| candidate.arguments.clone()),
        };
        Ok((model, overlay))
    }
}
