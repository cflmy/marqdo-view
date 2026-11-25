//! Renderer input for one explicitly selected, already verified terminal result.
//!
//! The overlay is bound to the exact pre-run Analysis bytes and to the
//! Observation v1 terminal: View never emits a result whose observation run id,
//! capture level or truncation state disagrees with the verified run.

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

/// Exact contract revisions this overlay was produced against. There is no
/// range negotiation: View only consumes Analysis 3 + Observation 1 + CLI Tool
/// API revision 1.
#[derive(Serialize)]
pub struct SnapshotContracts {
    pub api_revision: u32,
    pub analysis: u32,
    pub observation: u32,
}

#[derive(Serialize)]
pub struct SnapshotExecution {
    pub included: bool,
    /// Observation v1 run id (`run.id`/`run_id`).
    pub core_run_id: String,
    pub core_version: String,
    pub target: String,
    pub status: &'static str,
    pub terminal_at: String,
    pub source_verified: bool,
    /// `verified` | `truncated` | `invalid` | `stale` | `unavailable`.
    pub observation_status: String,
    pub capture_level: String,
    pub result_complete: bool,
    pub output_complete: bool,
    pub observation_truncated: bool,
    pub parameters: &'static str,
    pub mode: String,
    pub contracts: SnapshotContracts,
}

pub struct ExportResultOverlay {
    pub source: SnapshotSource,
    pub execution: SnapshotExecution,
    pub label: String,
    pub value: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub observation_counts: Option<BTreeMap<String, u64>>,
    pub arguments: Option<Vec<(String, String)>>,
}
