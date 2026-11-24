//! A bounded projection retained only inside an existing terminal Job.

use crate::execution::events::RunState;
use crate::execution::request::ExecutionPlan;
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
    pub analysis_version: u32,
    pub source_sha256: String,
    pub lock_digest: String,
    pub media: BTreeMap<String, String>,
    pub core_run_id: String,
    pub core_version: String,
    pub mode: String,
    pub terminal_at: String,
    pub state: RunState,
    pub value: Option<String>,
    pub value_is_public_scalar: bool,
    pub stdout: String,
    pub stderr: String,
    pub arguments: Vec<(String, String)>,
    pub trace_status: String,
    pub trace_counts: Option<BTreeMap<String, u64>>,
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
        trace: Option<&Value>,
    ) -> Option<Self> {
        if !matches!(state, RunState::Succeeded | RunState::Failed)
            || result.get("source_verified").and_then(Value::as_bool) != Some(true)
            || result.get("event_protocol").and_then(Value::as_u64) != Some(1)
        {
            return None;
        }
        let projection = &document.verified.projection;
        let function = projection
            .functions
            .iter()
            .find(|function| function.id == plan.function_id)?;
        let (stdout, stdout_complete) = bounded_text(result["stdout"].as_str());
        let (stderr, stderr_complete) = bounded_text(result["stderr"].as_str());
        let wire = &result["value"];
        let value_is_scalar = matches!(
            wire["type"].as_str(),
            Some("int" | "float" | "bool" | "unit" | "none" | "text")
        );
        let value_text = match state {
            RunState::Succeeded if value_is_scalar => {
                result["value_text"].as_str().map(str::to_string)
            }
            RunState::Succeeded if crate::protocol::value_wire_v1::transferable(wire) => {
                Some(wire.to_string())
            }
            _ => None,
        };
        let (value, value_complete) = match value_text {
            Some(text) => {
                let (text, complete) = bounded_text(Some(&text));
                (Some(text), complete)
            }
            None => (None, false),
        };
        let trace_status = match &result["trace_status"] {
            Value::String(status) if status == "verified" => {
                if trace.is_some_and(|trace| !trace["truncated"].is_null()) {
                    "truncated"
                } else {
                    "verified"
                }
            }
            Value::String(status) if status == "invalid" => "invalid",
            _ => "unavailable",
        };
        let trace_counts = trace
            .filter(|trace| trace["status"] == "verified")
            .and_then(|trace| trace["counts"].as_object())
            .map(|counts| {
                counts
                    .iter()
                    .filter_map(|(kind, count)| Some((kind.clone(), count.as_u64()?)))
                    .collect()
            });
        let value_is_public_scalar = value_is_scalar && wire["type"] != "text";
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
            core_run_id: result["core_run_id"].as_str()?.to_string(),
            core_version: projection.producer_version.clone(),
            mode: plan.mode.as_str().to_string(),
            terminal_at: crate::execution::events::utc_now(),
            state,
            value,
            value_is_public_scalar,
            stdout,
            stderr,
            arguments: plan.arguments.clone(),
            trace_status: trace_status.to_string(),
            trace_counts,
            output_complete: result["output_complete"].as_bool() == Some(true)
                && stdout_complete
                && stderr_complete,
            result_complete: state == RunState::Succeeded && value_complete,
            observation_truncated: result["observation_truncated"].as_bool().unwrap_or(false),
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
            "trace_summary": self.trace_counts.is_some(),
            "trace_status": self.trace_status,
            "observation_truncated": self.observation_truncated,
            "output_complete": self.output_complete,
            "result_complete": self.result_complete,
        })
    }
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
