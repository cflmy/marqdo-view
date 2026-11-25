//! A bounded projection retained only inside an existing terminal Job.
//!
//! The overlay is bound to the verified Observation v1 terminal (run id, state,
//! capture level, truncation) and to the exact pre-run Analysis bytes; it never
//! re-parses the CLI JSON and never widens the value consent the user gave.

use crate::execution::events::RunState;
use crate::execution::request::ExecutionPlan;
use crate::protocol::observation_v1::{EventCounts, RunProjection};
use crate::reader::assemble::StaticDocument;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MAX_TEXT: usize = 128 * 1024;

#[derive(Debug)]
pub struct TerminalExportCandidate {
    pub document: String,
    pub function_id: String,
    pub label: String,
    pub project_root_id: String,
    pub module_id: String,
    pub analysis_digest: String,
    /// Analysis revision carried by the exact pre-run artifact (3).
    pub analysis_version: u32,
    pub source_sha256: String,
    pub lock_digest: String,
    pub media: BTreeMap<String, String>,
    /// Observation v1 run id (`run.id`/`run_id`).
    pub observation_run_id: String,
    pub core_version: String,
    pub mode: String,
    pub terminal_at: String,
    pub state: RunState,
    pub value: Option<String>,
    pub value_is_public_scalar: bool,
    pub stdout: String,
    pub stderr: String,
    pub arguments: Vec<(String, String)>,
    pub observation_status: String,
    pub observation_counts: Option<BTreeMap<String, u64>>,
    pub capture_level: String,
    pub output_complete: bool,
    pub result_complete: bool,
    pub observation_truncated: bool,
}

impl TerminalExportCandidate {
    pub(super) fn from_terminal(
        state: RunState,
        plan: &ExecutionPlan,
        document: &StaticDocument,
        result: &Value,
        observation: Option<&RunProjection>,
    ) -> Option<Self> {
        if !matches!(state, RunState::Succeeded | RunState::Failed) {
            return None;
        }
        let observation = observation?;
        if !observation.source_verified
            || result.get("source_verified").and_then(Value::as_bool) != Some(true)
        {
            return None;
        }
        let observation_status = result.get("observation_status").and_then(Value::as_str)?;
        if !matches!(observation_status, "verified" | "truncated") {
            return None;
        }
        let projection = &document.verified.projection;
        let function = projection
            .functions
            .iter()
            .find(|function| function.id == plan.function_id)?;
        let (stdout, stdout_complete) = bounded_text(result["stdout"].as_str());
        let (stderr, stderr_complete) = bounded_text(result["stderr"].as_str());
        // Only a public scalar result value is exportable without an extra
        // sensitive-output confirmation; container/text values stay omitted.
        let (value, value_complete, value_is_public_scalar) =
            match observation.result_value.as_ref() {
                Some(run) if public_scalar_type(&run.ty) => {
                    let (text, complete) = bounded_text(Some(run.display.as_str()));
                    (Some(text), complete, true)
                }
                Some(_) => (None, false, false),
                None => (None, true, false),
            };
        let observation_truncated = observation.truncated.is_some();
        let observation_counts = Some(counts_map(&observation.counts));
        Some(Self {
            document: plan.document.to_string(),
            function_id: function.id.clone(),
            label: function.local_name.clone(),
            project_root_id: projection.project_root_id.clone(),
            module_id: projection.module_id.clone(),
            analysis_digest: document.analysis_digest.clone(),
            analysis_version: projection.schema_version,
            source_sha256: document.source_sha256.clone(),
            lock_digest: projection.lock_digest.clone(),
            media: document.media_digests.clone(),
            observation_run_id: observation.run_id.clone(),
            core_version: projection.producer_version.clone(),
            mode: plan.mode.as_str().to_string(),
            terminal_at: crate::execution::events::utc_now(),
            state,
            value,
            value_is_public_scalar,
            stdout,
            stderr,
            arguments: plan.arguments.clone(),
            observation_status: observation_status.to_string(),
            observation_counts,
            capture_level: observation.capture_level.clone(),
            output_complete: result["output_complete"].as_bool() == Some(true)
                && stdout_complete
                && stderr_complete,
            result_complete: state == RunState::Succeeded
                && !observation_truncated
                && value_complete,
            observation_truncated,
        })
    }

    pub(super) fn preview(&self) -> Value {
        json!({
            "label": self.label,
            "value_bytes": self.value.as_ref().map(String::len),
            "value": self.value,
            "value_is_public_scalar": self.value_is_public_scalar,
            "stdout_bytes": self.stdout.len(),
            "stderr_bytes": self.stderr.len(),
            "arguments_count": self.arguments.len(),
            "observation_summary": self.observation_counts.is_some(),
            "observation_status": self.observation_status,
            "capture_level": self.capture_level,
            "observation_truncated": self.observation_truncated,
            "output_complete": self.output_complete,
            "result_complete": self.result_complete,
        })
    }
}

fn public_scalar_type(ty: &str) -> bool {
    matches!(
        ty.to_ascii_lowercase().as_str(),
        "int" | "float" | "bool" | "unit" | "none"
    )
}

fn counts_map(counts: &EventCounts) -> BTreeMap<String, u64> {
    [
        ("total", counts.total()),
        ("call_enter", counts.call_enter),
        ("call_exit", counts.call_exit),
        ("branch_eval", counts.branch_eval),
        ("loop_iteration", counts.loop_iteration),
        ("control_break", counts.control_break),
        ("control_continue", counts.control_continue),
        ("return", counts.return_events),
        ("effect", counts.effect),
        ("other", counts.other),
    ]
    .into_iter()
    .map(|(kind, count)| (kind.to_string(), count as u64))
    .collect()
}

fn bounded_text(value: Option<&str>) -> (String, bool) {
    let Some(text) = value else {
        return (String::new(), true);
    };
    let mut end = text.len().min(MAX_TEXT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), end == text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_scalar_types_exclude_text_and_containers() {
        for accepted in ["Int", "float", "Bool", "Unit", "none"] {
            assert!(public_scalar_type(accepted), "{accepted}");
        }
        for rejected in ["Text", "List", "Map", "Table", "Memory", "Resource"] {
            assert!(!public_scalar_type(rejected), "{rejected}");
        }
    }

    #[test]
    fn bounded_text_never_splits_utf8() {
        let text = "é".repeat(MAX_TEXT);
        let (bounded, complete) = bounded_text(Some(&text));
        assert!(bounded.is_char_boundary(bounded.len()));
        assert!(!complete);
    }
}
