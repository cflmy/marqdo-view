//! Renderer input for one explicitly selected, already verified terminal result.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct SnapshotSource {
    pub project_root_id: String,
    pub document: String,
    pub module_id: String,
    pub source_sha256: String,
    pub analysis_digest: String,
    pub lock_digest: Option<String>,
}

#[derive(Serialize)]
pub struct SnapshotProtocols {
    pub analysis: u32,
    pub trace: u32,
    pub events: u32,
}

#[derive(Serialize)]
pub struct SnapshotExecution {
    pub included: bool,
    pub core_run_id: String,
    pub core_version: String,
    pub target: String,
    pub status: &'static str,
    pub terminal_at: String,
    pub source_verified: bool,
    pub trace_status: String,
    pub result_complete: bool,
    pub output_complete: bool,
    pub observation_truncated: bool,
    pub parameters: &'static str,
    pub mode: String,
    pub core_protocol: SnapshotProtocols,
}

pub struct ExportResultOverlay {
    pub source: SnapshotSource,
    pub execution: SnapshotExecution,
    pub label: String,
    pub value: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub trace_counts: Option<BTreeMap<String, u64>>,
    pub arguments: Option<Vec<(String, String)>>,
}
