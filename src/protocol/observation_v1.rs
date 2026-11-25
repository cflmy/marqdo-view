//! Public Observation v1 consumer (`marqdo.observation`).
//!
//! `run`/`call --output jsonl` emits one event per line with a single `run_id`
//! and a zero-based contiguous `seq`; `--observation-file` atomically publishes
//! the equivalent snapshot. Both are decoded here into one domain shape and
//! validated centrally: format identity, sequence/lifecycle, unique terminal
//! event, `observation.truncated`, source binding (`analysis_digest`/`root_id`),
//! module identity, UTF-8 half-open spans, value-consent and byte chunks.
//!
//! Production consumes the live JSONL stream only; the atomic snapshot is
//! used by tests to assert stream/snapshot equivalence. No second decoder or
//! double-read merge exists.

use crate::error::{ViewError, ViewResult};
use crate::protocol::analysis_v3::{source_id, SemanticProjection, SemanticStatement};
use crate::protocol::value_wire_v1;
use crate::reader::source::{SourceIndex, SourceSpan};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

pub const SCHEMA: &str = "marqdo.observation";
pub const SCHEMA_VERSION: u32 = 1;
const MAX_LINE: usize = 4 * 1024 * 1024;
const MAX_TRANSPORT: usize = 16 * 1024 * 1024;
const MAX_DISPLAY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunDisplayState {
    NotRun,
    StaticOnly,
    Running,
    Succeeded,
    Failed,
    Stale,
    Truncated,
    Incomplete,
    Unsupported,
}

impl RunDisplayState {
    pub fn data_value(&self) -> &'static str {
        match self {
            RunDisplayState::NotRun => "not-run",
            RunDisplayState::StaticOnly => "static-only",
            RunDisplayState::Running => "running",
            RunDisplayState::Succeeded => "succeeded",
            RunDisplayState::Failed => "failed",
            RunDisplayState::Stale => "stale",
            RunDisplayState::Truncated => "truncated",
            RunDisplayState::Incomplete => "incomplete",
            RunDisplayState::Unsupported => "unsupported",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RunDisplayState::NotRun => "未运行",
            RunDisplayState::StaticOnly => "仅静态检查",
            RunDisplayState::Running => "运行中",
            RunDisplayState::Succeeded => "运行成功",
            RunDisplayState::Failed => "运行失败",
            RunDisplayState::Stale => "结果已过期",
            RunDisplayState::Truncated => "观察已截断",
            RunDisplayState::Incomplete => "未完成/无结论",
            RunDisplayState::Unsupported => "当前协议不支持",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunDisplayState::Succeeded
                | RunDisplayState::Failed
                | RunDisplayState::Stale
                | RunDisplayState::Truncated
                | RunDisplayState::Incomplete
                | RunDisplayState::Unsupported
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedKind {
    StreamOpened,
    RunStarted,
    CallEnter,
    CallExit,
    BranchEval,
    LoopIteration,
    ControlBreak,
    ControlContinue,
    Return,
    Effect,
    ParameterObserved,
    VariableAssigned,
    ObservationTruncated,
    StdoutChunk,
    StderrChunk,
    RunCompleted,
    RunFailed,
    RunRejected,
}

impl ObservedKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "stream.opened" => Some(ObservedKind::StreamOpened),
            "run.started" => Some(ObservedKind::RunStarted),
            "call.enter" => Some(ObservedKind::CallEnter),
            "call.exit" => Some(ObservedKind::CallExit),
            "branch.eval" => Some(ObservedKind::BranchEval),
            "loop.iteration" => Some(ObservedKind::LoopIteration),
            "control.break" => Some(ObservedKind::ControlBreak),
            "control.continue" => Some(ObservedKind::ControlContinue),
            "return" => Some(ObservedKind::Return),
            "effect" => Some(ObservedKind::Effect),
            "parameter.observed" => Some(ObservedKind::ParameterObserved),
            "variable.assigned" => Some(ObservedKind::VariableAssigned),
            "observation.truncated" => Some(ObservedKind::ObservationTruncated),
            "stdout.chunk" => Some(ObservedKind::StdoutChunk),
            "stderr.chunk" => Some(ObservedKind::StderrChunk),
            "run.completed" => Some(ObservedKind::RunCompleted),
            "run.failed" => Some(ObservedKind::RunFailed),
            "run.rejected" => Some(ObservedKind::RunRejected),
            _ => None,
        }
    }

    pub fn data_value(&self) -> &'static str {
        match self {
            ObservedKind::StreamOpened => "stream.opened",
            ObservedKind::RunStarted => "run.started",
            ObservedKind::CallEnter => "call.enter",
            ObservedKind::CallExit => "call.exit",
            ObservedKind::BranchEval => "branch.eval",
            ObservedKind::LoopIteration => "loop.iteration",
            ObservedKind::ControlBreak => "control.break",
            ObservedKind::ControlContinue => "control.continue",
            ObservedKind::Return => "return",
            ObservedKind::Effect => "effect",
            ObservedKind::ParameterObserved => "parameter.observed",
            ObservedKind::VariableAssigned => "variable.assigned",
            ObservedKind::ObservationTruncated => "observation.truncated",
            ObservedKind::StdoutChunk => "stdout.chunk",
            ObservedKind::StderrChunk => "stderr.chunk",
            ObservedKind::RunCompleted => "run.completed",
            ObservedKind::RunFailed => "run.failed",
            ObservedKind::RunRejected => "run.rejected",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct EventCounts {
    pub call_enter: usize,
    pub call_exit: usize,
    pub branch_eval: usize,
    pub loop_iteration: usize,
    pub control_break: usize,
    pub control_continue: usize,
    pub return_events: usize,
    pub effect: usize,
    pub other: usize,
}

impl EventCounts {
    pub fn total(&self) -> usize {
        self.call_enter
            + self.call_exit
            + self.branch_eval
            + self.loop_iteration
            + self.control_break
            + self.control_continue
            + self.return_events
            + self.effect
            + self.other
    }
}

#[derive(Debug, Clone)]
pub struct ObservedEvent {
    pub seq: u64,
    pub kind: ObservedKind,
    pub function_id: Option<String>,
    pub module_id: Option<String>,
    /// Validated span in the event's own module.
    pub span: Option<SourceSpan>,
    /// Presentation-only span in the event's own module.
    pub raw_span: Option<SourceSpan>,
    /// Entry-document statement span this event attaches to, if any.
    pub anchor_span: Option<SourceSpan>,
    pub detail: Option<String>,
    pub operation: Option<String>,
    pub duration_micros: Option<u64>,
}

impl ObservedEvent {
    pub fn kind_label(&self) -> &'static str {
        self.kind.data_value()
    }

    pub fn is_entry_module(&self, entry_module: &str) -> bool {
        self.module_id.as_deref() == Some(entry_module)
    }
}

#[derive(Debug, Clone)]
pub struct Truncation {
    pub reason: String,
    pub limit: u64,
    pub written: u64,
    pub dropped: Option<u64>,
}

impl Truncation {
    pub fn label(&self) -> String {
        let reason = match self.reason.as_str() {
            "event_budget" => "事件数量上限",
            "byte_budget" => "字节上限",
            other => other,
        };
        format!("{reason}（上限 {}，已写入 {}）", self.limit, self.written)
    }
}

#[derive(Debug, Clone)]
pub struct RunValue {
    pub ty: String,
    pub display: String,
}

#[derive(Debug, Clone, Default)]
pub struct Capture {
    pub level: String,
    pub values: bool,
    pub selected_variables: Vec<String>,
    pub selected_parameters: Vec<String>,
    pub allow_text_values: bool,
}

#[derive(Debug, Clone)]
pub struct RunProjection {
    pub run_id: String,
    pub entry: Option<String>,
    /// Raw Core terminal status (`ok`, `error`, `rejected` or empty/incomplete).
    pub run_status: String,
    pub state: RunDisplayState,
    pub error_code: Option<String>,
    pub analysis_digest: String,
    pub source_verified: bool,
    pub events: Vec<ObservedEvent>,
    pub truncated: Option<Truncation>,
    pub capture_level: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub result_value: Option<RunValue>,
    pub counts: EventCounts,
    pub entry_event_count: usize,
    pub other_event_count: usize,
    pub warnings: Vec<String>,
    pub output_complete: bool,
}

impl RunProjection {
    pub fn state_label(&self) -> &'static str {
        self.state.label()
    }

    pub fn is_success(&self) -> bool {
        matches!(
            self.state,
            RunDisplayState::Succeeded | RunDisplayState::Truncated
        )
    }

    /// Events whose statement anchor is exactly `span`, in sequence order.
    pub fn events_for_anchor(&self, span: SourceSpan) -> Vec<&ObservedEvent> {
        self.events
            .iter()
            .filter(|event| event.anchor_span == Some(span))
            .collect()
    }

    /// Compact inline summary for one statement anchor, or `None` when the
    /// statement has no sourced run event.
    pub fn anchor_summary(&self, span: SourceSpan) -> Option<RunAnchorSummary> {
        let events = self.events_for_anchor(span);
        if events.is_empty() {
            return None;
        }
        let last = events.last().expect("non-empty");
        let mut kinds: Vec<&str> = Vec::new();
        for event in &events {
            if !kinds.contains(&event.kind_label()) {
                kinds.push(event.kind_label());
            }
        }
        let detail = match last.kind {
            ObservedKind::BranchEval => {
                format!("if → {}", last.detail.as_deref().unwrap_or("?"))
            }
            ObservedKind::LoopIteration => {
                let outcome = match last.detail.as_deref() {
                    Some("continue") => "continue",
                    Some("exit") => "exit",
                    other => other.unwrap_or("?"),
                };
                format!("loop {outcome}")
            }
            ObservedKind::Return => {
                format!("return {}", last.detail.as_deref().unwrap_or("Unit"))
            }
            ObservedKind::CallEnter => "call".to_string(),
            ObservedKind::CallExit => "call exit".to_string(),
            ObservedKind::Effect => format!("effect {}", last.operation.as_deref().unwrap_or("ok")),
            _ => last.kind_label().to_string(),
        };
        Some(RunAnchorSummary {
            count: events.len(),
            detail,
            kinds: kinds.into_iter().map(str::to_string).collect(),
            last_seq: last.seq,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RunAnchorSummary {
    pub count: usize,
    pub detail: String,
    pub kinds: Vec<String>,
    pub last_seq: u64,
}

// ---------------------------------------------------------------------------
// Raw event/snapshot decoding
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEvent {
    schema: String,
    schema_version: u32,
    run_id: String,
    seq: u64,
    kind: String,
    payload: Value,
}

#[derive(Debug, Clone)]
enum TerminalKind {
    Completed {
        value: Option<Value>,
        value_type: String,
    },
    Failed {
        error_code: String,
        message: String,
    },
    Rejected {
        phase: String,
        error_code: String,
        message: String,
    },
}

struct Terminal {
    kind: TerminalKind,
}

#[derive(Debug, Clone)]
struct BindingSource {
    root_id: String,
    analysis_digest: String,
}

/// Incremental JSONL decoder shared by the live stream. Each complete line is
/// validated before its normalized browser projection is published.
pub struct ObservationDecoder {
    partial: Vec<u8>,
    total_bytes: usize,
    invalid: Option<String>,
    run_id: Option<String>,
    next_seq: u64,
    capture: Option<Capture>,
    started: Option<BindingSource>,
    entry: Option<String>,
    terminal: Option<Terminal>,
    truncated: Option<Truncation>,
    counts: EventCounts,
    entry_event_count: usize,
    other_event_count: usize,
    warnings: Vec<String>,
    events: Vec<ObservedEvent>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stdout_truncated: bool,
    source_indexes: HashMap<String, SourceIndex>,
    sources: HashMap<String, String>,
    entry_module_id: String,
    statements: Vec<SemanticStatement>,
    analysis_digest: String,
    project_root_id: String,
}

impl ObservationDecoder {
    /// Build a decoder bound to the verified Analysis that was checked before
    /// the run. Module sources and statement anchors come only from it.
    pub fn new(analysis: &SemanticProjection) -> Self {
        Self {
            partial: Vec::new(),
            total_bytes: 0,
            invalid: None,
            run_id: None,
            next_seq: 0,
            capture: None,
            started: None,
            entry: None,
            terminal: None,
            truncated: None,
            counts: EventCounts::default(),
            entry_event_count: 0,
            other_event_count: 0,
            warnings: Vec::new(),
            events: Vec::new(),
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            source_indexes: HashMap::new(),
            sources: analysis
                .modules
                .iter()
                .map(|module| (module.id.clone(), module.source.clone()))
                .collect(),
            entry_module_id: analysis.module_id.clone(),
            statements: analysis
                .functions
                .iter()
                .flat_map(|function| function.flattened_statements().into_iter().cloned())
                .collect(),
            analysis_digest: analysis.analysis_digest.clone(),
            project_root_id: analysis.project_root_id.clone(),
        }
    }

    pub fn invalid(&self) -> Option<&str> {
        self.invalid.as_deref()
    }

    /// Feed raw stdout bytes; returns the normalized browser projections of
    /// complete events. A line that fails validation stops the decoder.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<Value> {
        if self.invalid.is_some() {
            return Vec::new();
        }
        self.total_bytes += chunk.len();
        if self.total_bytes > MAX_TRANSPORT {
            self.invalid = Some("Observation transport exceeded 16 MiB".to_string());
            return Vec::new();
        }
        self.partial.extend_from_slice(chunk);
        let mut out = Vec::new();
        loop {
            let Some(position) = self.partial.iter().position(|byte| *byte == b'\n') else {
                if self.partial.len() > MAX_LINE {
                    self.invalid = Some("Observation line exceeds 4 MiB".to_string());
                }
                break;
            };
            let mut line: Vec<u8> = self.partial.drain(..=position).collect();
            line.pop();
            if line.len() > MAX_LINE {
                self.invalid = Some("Observation line exceeds 4 MiB".to_string());
                break;
            }
            match self.consume_line(&line) {
                Ok(Some(value)) => out.push(value),
                Ok(None) => {}
                Err(detail) => {
                    self.invalid = Some(detail);
                    break;
                }
            }
        }
        out
    }

    fn consume_line(&mut self, line: &[u8]) -> Result<Option<Value>, String> {
        if line.iter().all(|byte| byte.is_ascii_whitespace()) {
            return Ok(None);
        }
        let raw: RawEvent = serde_json::from_slice(line).map_err(|err| {
            format!("Observation line is not a valid `marqdo.observation` event: {err}")
        })?;
        self.consume(raw)
    }

    fn consume(&mut self, raw: RawEvent) -> Result<Option<Value>, String> {
        if raw.schema != SCHEMA {
            return Err(format!(
                "unexpected Observation schema {:?} (this View consumes `{SCHEMA}`)",
                raw.schema
            ));
        }
        if raw.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "Observation schema_version {} is not supported (this View implements v{SCHEMA_VERSION})",
                raw.schema_version
            ));
        }
        if raw.run_id.is_empty() || raw.run_id.len() > 128 {
            return Err("Observation run_id is empty or too long".to_string());
        }
        match &self.run_id {
            Some(run_id) if run_id != &raw.run_id => {
                return Err("Observation mixes more than one run_id".to_string())
            }
            None => self.run_id = Some(raw.run_id.clone()),
            _ => {}
        }
        if raw.seq != self.next_seq {
            return Err(format!(
                "Observation seq is not contiguous: expected {}, got {}",
                self.next_seq, raw.seq
            ));
        }
        if self.terminal.is_some() {
            return Err("Observation continued after a terminal event".to_string());
        }
        let kind = ObservedKind::parse(&raw.kind)
            .ok_or_else(|| format!("unknown Observation kind {:?}", raw.kind))?;
        let normalized = self.validate_event(&raw, kind)?;
        self.next_seq += 1;
        Ok(normalized)
    }

    fn validate_event(
        &mut self,
        raw: &RawEvent,
        kind: ObservedKind,
    ) -> Result<Option<Value>, String> {
        let payload = raw
            .payload
            .as_object()
            .ok_or_else(|| format!("Observation {} payload is not an object", kind.data_value()))?;
        let mut event = ObservedEvent {
            seq: raw.seq,
            kind: kind.clone(),
            function_id: None,
            module_id: None,
            span: None,
            raw_span: None,
            anchor_span: None,
            detail: None,
            operation: None,
            duration_micros: None,
        };
        match kind {
            ObservedKind::StreamOpened => {
                if raw.seq != 0 || self.capture.is_some() {
                    return Err("stream.opened must be the first event".to_string());
                }
                let producer = payload
                    .get("producer")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "stream.opened payload.producer is missing".to_string())?;
                if producer.get("name").and_then(Value::as_str) != Some("marqdo") {
                    return Err("stream.opened producer.name is not `marqdo`".to_string());
                }
                if producer
                    .get("version")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
                {
                    return Err("stream.opened producer.version is missing".to_string());
                }
                let observation = payload
                    .get("observation")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "stream.opened payload.observation is missing".to_string())?;
                let level = observation
                    .get("level")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "stream.opened observation.level is missing".to_string())?;
                if !matches!(level, "base" | "structural" | "values") {
                    return Err(format!(
                        "stream.opened observation.level {level:?} is unknown"
                    ));
                }
                let values = observation
                    .get("values")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "stream.opened observation.values is missing".to_string())?;
                let allow_text_values = observation
                    .get("allow_text_values")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| {
                        "stream.opened observation.allow_text_values is missing".to_string()
                    })?;
                let selected_variables = string_array(
                    observation.get("selected_variables"),
                    "stream.opened observation.selected_variables",
                )?;
                let selected_parameters = string_array(
                    observation.get("selected_parameters"),
                    "stream.opened observation.selected_parameters",
                )?;
                let limits = payload
                    .get("limits")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "stream.opened payload.limits is missing".to_string())?;
                for key in [
                    "max_observation_events",
                    "max_observation_bytes",
                    "max_value_bytes",
                ] {
                    limits.get(key).and_then(Value::as_u64).ok_or_else(|| {
                        format!("stream.opened limits.{key} is missing or negative")
                    })?;
                }
                self.capture = Some(Capture {
                    level: level.to_string(),
                    values,
                    selected_variables,
                    selected_parameters,
                    allow_text_values,
                });
            }
            ObservedKind::RunStarted => {
                if raw.seq != 1 || self.started.is_some() || self.capture.is_none() {
                    return Err("run.started must be the second event".to_string());
                }
                let entry = payload
                    .get("entry")
                    .and_then(Value::as_str)
                    .filter(|entry| !entry.is_empty())
                    .ok_or_else(|| "run.started payload.entry is missing".to_string())?;
                let mode = payload
                    .get("mode")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "run.started payload.mode is missing".to_string())?;
                if !matches!(mode, "pure" | "project" | "strict") {
                    return Err(format!("run.started payload.mode {mode:?} is unknown"));
                }
                let source = payload
                    .get("source")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "run.started payload.source is missing".to_string())?;
                let root_id = source
                    .get("root_id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "run.started payload.source.root_id is missing".to_string())?;
                let analysis_digest = source
                    .get("analysis_digest")
                    .and_then(Value::as_str)
                    .filter(|value| value.starts_with("sha256:") && value.len() == 71)
                    .ok_or_else(|| {
                        "run.started payload.source.analysis_digest is not sha256:<64 hex>"
                            .to_string()
                    })?;
                self.entry = Some(entry.to_string());
                self.started = Some(BindingSource {
                    root_id: root_id.to_string(),
                    analysis_digest: analysis_digest.to_string(),
                });
            }
            ObservedKind::StdoutChunk | ObservedKind::StderrChunk => {
                let encoding = payload
                    .get("encoding")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{} payload.encoding is missing", kind.data_value()))?;
                if encoding != "base64" {
                    return Err(format!(
                        "{} payload.encoding {encoding:?} is not base64",
                        kind.data_value()
                    ));
                }
                let data = payload
                    .get("data_base64")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("{} payload.data_base64 is missing", kind.data_value())
                    })?;
                let byte_count = payload
                    .get("byte_count")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        format!("{} payload.byte_count is missing", kind.data_value())
                    })?;
                let bytes = STANDARD.decode(data).map_err(|err| {
                    format!(
                        "{} payload.data_base64 is invalid: {err}",
                        kind.data_value()
                    )
                })?;
                if bytes.len() as u64 != byte_count {
                    return Err(format!(
                        "{} payload.byte_count {} does not match decoded {} bytes",
                        kind.data_value(),
                        byte_count,
                        bytes.len()
                    ));
                }
                let target = match kind {
                    ObservedKind::StdoutChunk => &mut self.stdout,
                    _ => &mut self.stderr,
                };
                let remaining = MAX_DISPLAY_BYTES.saturating_sub(target.len());
                target.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
                if bytes.len() > remaining {
                    self.stdout_truncated = true;
                }
            }
            ObservedKind::ObservationTruncated => {
                let reason = payload
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "observation.truncated payload.reason is missing".to_string())?;
                if !matches!(reason, "event_budget" | "byte_budget") {
                    return Err(format!(
                        "observation.truncated payload.reason {reason:?} is unknown"
                    ));
                }
                let limit = payload
                    .get("limit")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "observation.truncated payload.limit is missing".to_string())?;
                let written = payload
                    .get("written")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        "observation.truncated payload.written is missing".to_string()
                    })?;
                let dropped = match payload.get("dropped") {
                    Some(Value::Null) | None => None,
                    Some(value) => Some(value.as_u64().ok_or_else(|| {
                        "observation.truncated payload.dropped is not a non-negative integer"
                            .to_string()
                    })?),
                };
                self.truncated = Some(Truncation {
                    reason: reason.to_string(),
                    limit,
                    written,
                    dropped,
                });
            }
            ObservedKind::RunCompleted => {
                let status = payload
                    .get("status")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "run.completed payload.status is missing".to_string())?;
                if status != "ok" {
                    return Err("run.completed payload.status must be `ok`".to_string());
                }
                let value_type = payload
                    .get("value_type")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "run.completed payload.value_type is missing".to_string())?;
                let value = match payload.get("value") {
                    Some(Value::Null) | None => None,
                    Some(value) if value.is_object() => {
                        if !value_wire_v1::valid(value) {
                            return Err(
                                "run.completed payload.value is not a Value Wire value".to_string()
                            );
                        }
                        Some(value.clone())
                    }
                    Some(_) => {
                        return Err(
                            "run.completed payload.value is not an object or null".to_string()
                        )
                    }
                };
                self.terminal = Some(Terminal {
                    kind: TerminalKind::Completed {
                        value,
                        value_type: value_type.to_string(),
                    },
                });
            }
            ObservedKind::RunFailed => {
                let status = payload
                    .get("status")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "run.failed payload.status is missing".to_string())?;
                if status != "error" {
                    return Err("run.failed payload.status must be `error`".to_string());
                }
                let error = payload
                    .get("error")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "run.failed payload.error is missing".to_string())?;
                let error_code = error
                    .get("code")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "run.failed payload.error.code is missing".to_string())?;
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                validate_error_location(error)?;
                self.terminal = Some(Terminal {
                    kind: TerminalKind::Failed {
                        error_code: error_code.to_string(),
                        message,
                    },
                });
            }
            ObservedKind::RunRejected => {
                let status = payload
                    .get("status")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "run.rejected payload.status is missing".to_string())?;
                if status != "rejected" {
                    return Err("run.rejected payload.status must be `rejected`".to_string());
                }
                let phase = payload
                    .get("phase")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "run.rejected payload.phase is missing".to_string())?;
                if !matches!(
                    phase,
                    "options" | "prepare" | "compile" | "authorization" | "strict"
                ) {
                    return Err(format!("run.rejected payload.phase {phase:?} is unknown"));
                }
                let error = payload
                    .get("error")
                    .and_then(Value::as_object)
                    .ok_or_else(|| "run.rejected payload.error is missing".to_string())?;
                let error_code = error
                    .get("code")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "run.rejected payload.error.code is missing".to_string())?;
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                validate_error_location(error)?;
                self.terminal = Some(Terminal {
                    kind: TerminalKind::Rejected {
                        phase: phase.to_string(),
                        error_code: error_code.to_string(),
                        message,
                    },
                });
            }
            _ => {
                if self.started.is_none() {
                    return Err(format!(
                        "Observation {} appeared before run.started",
                        kind.data_value()
                    ));
                }
                event.function_id = opt_string(payload, "function_id");
                event.module_id = opt_string(payload, "module_id");
                event.detail = opt_string(payload, "detail");
                event.operation = opt_string(payload, "operation");
                event.duration_micros = payload.get("duration_micros").and_then(Value::as_u64);
                // Value-bearing events must not carry values without consent.
                if matches!(
                    kind,
                    ObservedKind::ParameterObserved | ObservedKind::VariableAssigned
                ) {
                    self.validate_observed_value(payload, &kind)?;
                    event.detail = event.detail.or_else(|| opt_string(payload, "name"));
                }
                // Kind-specific required fields.
                match kind {
                    ObservedKind::CallEnter => {
                        validate_frame_id(payload, "call.enter")?;
                        match payload.get("parent_frame_id") {
                            Some(Value::Null) | None => {}
                            Some(Value::String(value)) if valid_frame_id(value) => {}
                            _ => {
                                return Err("call.enter payload.parent_frame_id is not null or fN"
                                    .to_string())
                            }
                        }
                        require_nonempty(payload, "function_id", "call.enter")?;
                        let function_id = payload
                            .get("function_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if !function_id.contains("::") {
                            return Err("call.enter function_id is not canonical".to_string());
                        }
                        event.detail = Some(function_id.to_string());
                    }
                    ObservedKind::CallExit => {
                        validate_frame_id(payload, "call.exit")?;
                        require_nonempty(payload, "status", "call.exit")?;
                        event.detail = opt_string(payload, "status");
                    }
                    ObservedKind::Return => {
                        validate_frame_id(payload, "return")?;
                        require_nonempty(payload, "value_type", "return")?;
                        event.detail = opt_string(payload, "value_type");
                    }
                    ObservedKind::BranchEval => {
                        validate_frame_id(payload, "branch.eval")?;
                        let result = payload
                            .get("result")
                            .and_then(Value::as_bool)
                            .ok_or_else(|| "branch.eval payload.result is missing".to_string())?;
                        event.detail = Some(if result { "then" } else { "else" }.to_string());
                    }
                    ObservedKind::LoopIteration => {
                        validate_frame_id(payload, "loop.iteration")?;
                        let decision =
                            payload
                                .get("decision")
                                .and_then(Value::as_str)
                                .ok_or_else(|| {
                                    "loop.iteration payload.decision is missing".to_string()
                                })?;
                        if !matches!(decision, "continue" | "exit") {
                            return Err(format!(
                                "loop.iteration payload.decision {decision:?} is unknown"
                            ));
                        }
                        event.detail = Some(decision.to_string());
                    }
                    ObservedKind::ControlBreak | ObservedKind::ControlContinue => {
                        validate_frame_id(payload, kind.data_value())?;
                    }
                    ObservedKind::Effect => {
                        match payload.get("frame_id") {
                            Some(Value::Null) | None => {}
                            Some(Value::String(value)) if valid_frame_id(value) => {}
                            _ => {
                                return Err("effect payload.frame_id is not null or fN".to_string())
                            }
                        }
                        require_nonempty(payload, "operation", "effect")?;
                        require_nonempty(payload, "result_code", "effect")?;
                        event.operation = opt_string(payload, "operation");
                        event.detail = opt_string(payload, "result_code");
                    }
                    _ => {}
                }
                // Module identity and span validation.
                let module_id = event
                    .module_id
                    .clone()
                    .ok_or_else(|| format!("{} payload.module_id is missing", kind.data_value()))?;
                let span = payload
                    .get("span")
                    .ok_or_else(|| format!("{} payload.span is missing", kind.data_value()))?;
                let raw_span = parse_span_value(span)?;
                if let Some(span) = raw_span {
                    let source = self.source_for(&module_id)?;
                    let index = self
                        .source_indexes
                        .entry(module_id.clone())
                        .or_insert_with(|| SourceIndex::new(source.as_str()));
                    if !index.validate_span_bytes(source.as_bytes(), span) {
                        return Err(format!(
                            "{} span {}..{} is outside module {module_id:?} or not on a UTF-8 boundary",
                            kind.data_value(),
                            span.start,
                            span.end
                        ));
                    }
                    event.span = Some(span);
                    event.raw_span = Some(span);
                }
                event.module_id = Some(module_id);
            }
        }
        if let Some(anchor) = self.anchor_for(&event) {
            event.anchor_span = Some(anchor);
        }
        self.count_event(&mut event);
        self.events.push(event.clone());
        Ok(Some(normalized_event(&event, raw)))
    }

    fn validate_observed_value(
        &self,
        payload: &serde_json::Map<String, Value>,
        kind: &ObservedKind,
    ) -> Result<(), String> {
        let value_state = payload
            .get("value_state")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{} payload.value_state is missing", kind.data_value()))?;
        if !matches!(
            value_state,
            "not_requested" | "redacted" | "truncated" | "unsupported" | "observed"
        ) {
            return Err(format!(
                "{} payload.value_state {value_state:?} is unknown",
                kind.data_value()
            ));
        }
        require_nonempty(payload, "name", kind.data_value())?;
        require_nonempty(payload, "value_type", kind.data_value())?;
        if let Some(source) = payload.get("source") {
            let raw = source
                .as_str()
                .ok_or_else(|| format!("{} payload.source is not a string", kind.data_value()))?;
            if !matches!(raw, "provided" | "default") {
                return Err(format!(
                    "{} payload.source {raw:?} is unknown",
                    kind.data_value()
                ));
            }
        }
        let has_value = match payload.get("value") {
            Some(Value::Null) | None => false,
            Some(value) if value.is_object() && value_wire_v1::valid(value) => true,
            Some(_) => {
                return Err(format!(
                    "{} payload.value is not a Value Wire value",
                    kind.data_value()
                ))
            }
        };
        let capture = self
            .capture
            .as_ref()
            .ok_or_else(|| "observation values appeared before stream.opened".to_string())?;
        if value_state == "observed" && !has_value {
            return Err(format!(
                "{} observed a value but carries no value",
                kind.data_value()
            ));
        }
        if value_state != "observed" && has_value {
            return Err(format!(
                "{} carries a value without an observed value_state",
                kind.data_value()
            ));
        }
        if has_value && !capture.values {
            return Err(format!(
                "{} carries a value although stream.opened selected no values",
                kind.data_value()
            ));
        }
        if has_value && capture.values {
            // Selection must have been explicit for this exact name.
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let selected = if matches!(kind, ObservedKind::VariableAssigned) {
                &capture.selected_variables
            } else {
                &capture.selected_parameters
            };
            if !selected.iter().any(|entry| entry == name) {
                return Err(format!(
                    "{} carries value {name:?} without an explicit selection",
                    kind.data_value()
                ));
            }
        }
        Ok(())
    }

    fn source_for(&self, module_id: &str) -> Result<String, String> {
        self.sources
            .get(module_id)
            .cloned()
            .ok_or_else(|| format!("Observation names unknown module {module_id:?}"))
    }

    fn anchor_for(&self, event: &ObservedEvent) -> Option<SourceSpan> {
        let span = event.span?;
        if event.module_id.as_deref() != Some(self.entry_module_id.as_str()) {
            return None;
        }
        if matches!(event.kind, ObservedKind::CallEnter | ObservedKind::CallExit) {
            return None;
        }
        self.statements
            .iter()
            .filter(|statement| {
                statement.span == span
                    || statement.condition_span == Some(span)
                    || statement
                        .arms
                        .iter()
                        .any(|arm| arm.condition_span == Some(span))
            })
            .min_by_key(|statement| statement.span.len())
            .map(|statement| statement.span)
            .or_else(|| {
                self.statements
                    .iter()
                    .filter(|statement| statement.span.contains(span))
                    .min_by_key(|statement| statement.span.len())
                    .map(|statement| statement.span)
            })
    }

    fn count_event(&mut self, event: &mut ObservedEvent) {
        match event.kind {
            ObservedKind::CallEnter => self.counts.call_enter += 1,
            ObservedKind::CallExit => self.counts.call_exit += 1,
            ObservedKind::BranchEval => self.counts.branch_eval += 1,
            ObservedKind::LoopIteration => self.counts.loop_iteration += 1,
            ObservedKind::ControlBreak => self.counts.control_break += 1,
            ObservedKind::ControlContinue => self.counts.control_continue += 1,
            ObservedKind::Return => self.counts.return_events += 1,
            ObservedKind::Effect => self.counts.effect += 1,
            ObservedKind::StreamOpened
            | ObservedKind::RunStarted
            | ObservedKind::ObservationTruncated
            | ObservedKind::StdoutChunk
            | ObservedKind::StderrChunk
            | ObservedKind::ParameterObserved
            | ObservedKind::VariableAssigned
            | ObservedKind::RunCompleted
            | ObservedKind::RunFailed
            | ObservedKind::RunRejected => {}
        }
        if event.module_id.as_deref() == Some(&self.entry_module_id) {
            self.entry_event_count += 1;
        } else if event.module_id.is_some() {
            self.other_event_count += 1;
        }
    }

    /// Consume the decoder after the child exited and build the verified run
    /// projection. A missing terminal is an incomplete delivery, never a
    /// success; malformed/mixed/stale observations are hard errors.
    pub fn finish(
        mut self,
        exit_code: i32,
        process_stderr: &str,
        stale: bool,
    ) -> ViewResult<RunProjection> {
        if let Some(detail) = self.invalid {
            return Err(ViewError::ObservationInvalid { detail });
        }
        let expected_digest = self.analysis_digest.clone();
        // A run rejected before start legitimately has no run.started; every
        // other lifecycle requires the exact source binding.
        let rejected_before_start = matches!(
            self.terminal,
            Some(Terminal {
                kind: TerminalKind::Rejected { .. },
                ..
            })
        );
        if let Some(started) = self.started.as_ref() {
            if started.root_id != self.project_root_id {
                return Err(ViewError::ObservationStale {
                    detail: format!(
                        "Observation source.root_id {} does not match Analysis project {}",
                        started.root_id, self.project_root_id
                    ),
                });
            }
            if started.analysis_digest != expected_digest {
                return Err(ViewError::ObservationStale {
                    detail: format!(
                        "Observation source.analysis_digest {} does not match the exact Analysis bytes {}",
                        started.analysis_digest, expected_digest
                    ),
                });
            }
            if stale {
                return Err(ViewError::ObservationStale {
                    detail:
                        "the document source changed during the run; results cannot be attributed"
                            .to_string(),
                });
            }
        } else if !rejected_before_start {
            return Err(ViewError::ObservationInvalid {
                detail: "Observation has no run.started event".to_string(),
            });
        }
        let (state, run_status, error_code, result_value) = match &self.terminal {
            Some(Terminal {
                kind: TerminalKind::Completed { value, value_type },
                ..
            }) => {
                let result_value = value.as_ref().map(|value| RunValue {
                    ty: value_type.clone(),
                    display: display_value(value),
                });
                if self.truncated.is_some() {
                    (
                        RunDisplayState::Truncated,
                        "ok".to_string(),
                        None,
                        result_value,
                    )
                } else {
                    (
                        RunDisplayState::Succeeded,
                        "ok".to_string(),
                        None,
                        result_value,
                    )
                }
            }
            Some(Terminal {
                kind:
                    TerminalKind::Failed {
                        error_code,
                        message,
                    },
                ..
            }) => {
                self.warnings.push(message.clone());
                (
                    RunDisplayState::Failed,
                    "error".to_string(),
                    Some(error_code.clone()),
                    None,
                )
            }
            Some(Terminal {
                kind:
                    TerminalKind::Rejected {
                        phase,
                        error_code,
                        message,
                    },
                ..
            }) => {
                self.warnings.push(message.clone());
                self.warnings
                    .push(format!("出运行在阶段 {phase} 被拒绝（{error_code}）"));
                (
                    RunDisplayState::Failed,
                    "rejected".to_string(),
                    Some(error_code.clone()),
                    None,
                )
            }
            None => {
                self.warnings
                    .push("CLI 事件流在终态前结束；本次运行未完成，不显示为成功。".to_string());
                (
                    RunDisplayState::Incomplete,
                    "incomplete".to_string(),
                    Some("view.observation_incomplete".to_string()),
                    None,
                )
            }
        };
        if self.stdout_truncated {
            self.warnings
                .push("标准输出超过 View 展示上限，页面仅显示前 64 KiB。".to_string());
        }
        if !process_stderr.trim().is_empty() {
            self.warnings.push(format!(
                "CLI stderr: {}",
                process_stderr.trim().chars().take(400).collect::<String>()
            ));
        }
        let complete = self.terminal.is_some()
            && self
                .capture
                .as_ref()
                .is_some_and(|capture| !capture.level.is_empty());
        Ok(RunProjection {
            run_id: self.run_id.clone().unwrap_or_default(),
            entry: self.entry.clone(),
            run_status,
            state,
            error_code,
            analysis_digest: expected_digest,
            source_verified: true,
            truncated: self.truncated.clone(),
            capture_level: self
                .capture
                .as_ref()
                .map(|capture| capture.level.clone())
                .unwrap_or_default(),
            stdout: String::from_utf8_lossy(&self.stdout).to_string(),
            stderr: String::from_utf8_lossy(&self.stderr).to_string(),
            exit_code,
            result_value,
            counts: self.counts,
            entry_event_count: self.entry_event_count,
            other_event_count: self.other_event_count,
            events: self.events,
            warnings: self.warnings,
            output_complete: complete,
        })
    }
}

fn normalized_event(event: &ObservedEvent, raw: &RawEvent) -> Value {
    let mut value = raw.payload.clone();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "kind".to_string(),
            Value::String(event.kind.data_value().to_string()),
        );
        object.insert("seq".to_string(), Value::Number(raw.seq.into()));
        object.insert("run_id".to_string(), Value::String(raw.run_id.clone()));
    }
    value
}

fn opt_string(payload: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

fn require_nonempty(
    payload: &serde_json::Map<String, Value>,
    key: &str,
    kind: &str,
) -> Result<(), String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|_| ())
        .ok_or_else(|| format!("{kind} payload.{key} is missing or empty"))
}

fn string_array(value: Option<&Value>, field: &str) -> Result<Vec<String>, String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("{field} contains a non-string entry"))
            })
            .collect(),
        Some(_) => Err(format!("{field} is not an array")),
        None => Err(format!("{field} is missing")),
    }
}

fn valid_frame_id(value: &str) -> bool {
    value.strip_prefix('f').is_some_and(|digits| {
        !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn validate_frame_id(payload: &serde_json::Map<String, Value>, kind: &str) -> Result<(), String> {
    let frame = payload
        .get("frame_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{kind} payload.frame_id is missing"))?;
    if !valid_frame_id(frame) {
        return Err(format!("{kind} payload.frame_id {frame:?} is not fN"));
    }
    Ok(())
}

fn parse_span_value(value: &Value) -> Result<Option<SourceSpan>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Object(map) => {
            let start = map
                .get("start")
                .and_then(Value::as_u64)
                .ok_or_else(|| "Observation span.start is missing".to_string())?;
            let end = map
                .get("end")
                .and_then(Value::as_u64)
                .ok_or_else(|| "Observation span.end is missing".to_string())?;
            if start > end {
                return Err(format!("Observation span {start}..{end} is negative"));
            }
            if map.keys().any(|key| key != "start" && key != "end") {
                return Err("Observation span has unknown fields".to_string());
            }
            Ok(Some(SourceSpan::new(start, end)))
        }
        _ => Err("Observation span is not an object or null".to_string()),
    }
}

fn validate_error_location(error: &serde_json::Map<String, Value>) -> Result<(), String> {
    if let Some(module) = error.get("module_id") {
        if !module.is_null() && module.as_str().is_none_or(str::is_empty) {
            return Err("Observation error.module_id is not a non-empty string".to_string());
        }
    }
    if let Some(span) = error.get("span") {
        parse_span_value(span)?;
    }
    if error.keys().any(|key| {
        !matches!(
            key.as_str(),
            "code"
                | "message"
                | "module_id"
                | "span"
                | "relative_path"
                | "line"
                | "column"
                | "hint"
        )
    }) {
        return Err("Observation error carries an unknown field".to_string());
    }
    Ok(())
}

fn display_value(value: &Value) -> String {
    let ty = value.get("type").and_then(Value::as_str).unwrap_or("value");
    let display = match ty {
        "int" => value.get("decimal").and_then(Value::as_str),
        "float" => value.get("value").and_then(Value::as_f64).map(|_| ""),
        "bool" => None,
        "text" => value.get("value").and_then(Value::as_str),
        _ => None,
    };
    if let Some(text) = display {
        if !text.is_empty() {
            return text.chars().take(80).collect();
        }
    }
    let compact = value.to_string();
    if compact.chars().count() > 80 {
        format!("{}…", compact.chars().take(80).collect::<String>())
    } else {
        compact
    }
}

/// Browser-facing event summary that never contains payloads the user did not
/// request.
pub fn summary_json(run: &RunProjection, analysis: &SemanticProjection, stale: bool) -> Value {
    let events = run
        .events
        .iter()
        .map(|event| {
            let source = if stale {
                None
            } else {
                event.anchor_span.and_then(|span| {
                    analysis
                        .all_statements()
                        .into_iter()
                        .find(|(_, statement)| statement.span == span)
                        .map(|(_, statement)| {
                            source_id(
                                &analysis.module_id,
                                span,
                                &format!("statement-{}", statement.kind.data_value()),
                            )
                        })
                })
            };
            json!({
                "seq": event.seq,
                "kind": event.kind_label(),
                "function_id": event.function_id,
                "module_id": event.module_id,
                "span": event.raw_span,
                "anchor": if stale { Value::Null } else {
                    event.anchor_span.map(|span| json!({"start": span.start, "end": span.end}))
                        .unwrap_or(Value::Null)
                },
                "source_id": source,
                "detail": event.detail,
                "operation": event.operation,
                "duration_micros": event.duration_micros,
            })
        })
        .collect::<Vec<_>>();
    let truncated = run.truncated.as_ref().map(|truncated| {
        json!({
            "reason": truncated.reason,
            "limit": truncated.limit,
            "written": truncated.written,
            "label": truncated.label(),
        })
    });
    json!({
        "status": if stale { "stale" } else { "verified" },
        "analysis_digest": run.analysis_digest,
        "core_run_id": run.run_id,
        "run_status": run.run_status,
        "state": run.state.data_value(),
        "stale": stale,
        "truncated": truncated,
        "events": events,
        "counts": {
            "total": run.counts.total(),
            "call_enter": run.counts.call_enter,
            "call_exit": run.counts.call_exit,
            "branch_eval": run.counts.branch_eval,
            "loop_iteration": run.counts.loop_iteration,
            "control_break": run.counts.control_break,
            "control_continue": run.counts.control_continue,
            "return": run.counts.return_events,
            "effect": run.counts.effect,
            "other": run.counts.other,
            "entry": run.entry_event_count,
            "other_modules": run.other_event_count,
        },
        "note": "Observation v1 已按 Analysis 原始字节摘要绑定；不包含未请求的参数值、变量值或秘密。",
    })
}

// ---------------------------------------------------------------------------
// Atomic snapshot decoding (`--observation-file`)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSnapshot {
    schema: String,
    schema_version: u32,
    run: RawSnapshotRun,
    capture: RawSnapshotCapture,
    events: Vec<RawEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSnapshotRun {
    id: String,
    status: String,
    source: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSnapshotCapture {
    level: String,
    values: bool,
    selected_variables: Vec<String>,
    selected_parameters: Vec<String>,
    allow_text_values: bool,
    truncated: Value,
}

/// Decode the atomic observation snapshot and validate that it agrees with its
/// own events and with the Analysis it binds. Production consumes the live
/// JSONL stream; this entry point exists for the stream/snapshot equivalence
/// test and for verifying a snapshot handed to the exporter.
pub fn decode_snapshot(bytes: &[u8], analysis: &SemanticProjection) -> ViewResult<RunProjection> {
    let raw: RawSnapshot =
        serde_json::from_slice(bytes).map_err(|err| ViewError::ObservationInvalid {
            detail: format!("Observation snapshot is not valid `marqdo.observation`: {err}"),
        })?;
    if raw.schema != SCHEMA || raw.schema_version != SCHEMA_VERSION {
        return Err(ViewError::ObservationInvalid {
            detail: format!(
                "Observation snapshot schema {:?} revision {} is not `{SCHEMA}` revision {SCHEMA_VERSION}",
                raw.schema, raw.schema_version
            ),
        });
    }
    if raw.run.id.is_empty() || raw.events.is_empty() {
        return Err(ViewError::ObservationInvalid {
            detail: "Observation snapshot has no run.id or events".to_string(),
        });
    }
    if !matches!(
        raw.run.status.as_str(),
        "ok" | "error" | "rejected" | "incomplete"
    ) {
        return Err(ViewError::ObservationInvalid {
            detail: format!(
                "Observation snapshot run.status {:?} is not ok/error/rejected/incomplete",
                raw.run.status
            ),
        });
    }
    if !matches!(raw.capture.level.as_str(), "base" | "structural" | "values") {
        return Err(ViewError::ObservationInvalid {
            detail: format!(
                "Observation snapshot capture.level {:?} is unknown",
                raw.capture.level
            ),
        });
    }
    let has_truncation = raw
        .events
        .iter()
        .any(|event| event.kind == "observation.truncated");
    match &raw.capture.truncated {
        Value::Null => {
            if has_truncation {
                return Err(ViewError::ObservationInvalid {
                    detail: "snapshot capture.truncated is null but an observation.truncated event exists"
                        .to_string(),
                });
            }
        }
        Value::Object(_) => {
            if !has_truncation {
                return Err(ViewError::ObservationInvalid {
                    detail: "snapshot capture.truncated is set but no observation.truncated event exists"
                        .to_string(),
                });
            }
        }
        _ => {
            return Err(ViewError::ObservationInvalid {
                detail: "snapshot capture.truncated is not an object or null".to_string(),
            })
        }
    }

    let capture_values = raw.capture.values;
    let capture_selected_variables = raw.capture.selected_variables.clone();
    let capture_selected_parameters = raw.capture.selected_parameters.clone();
    let capture_allow_text_values = raw.capture.allow_text_values;
    let _ = (
        capture_values,
        capture_selected_variables,
        capture_selected_parameters,
        capture_allow_text_values,
    );
    let mut lines = Vec::with_capacity(raw.events.len());
    for event in &raw.events {
        if event.run_id != raw.run.id {
            return Err(ViewError::ObservationInvalid {
                detail: "snapshot events mix a different run_id than run.id".to_string(),
            });
        }
        let encoded = serde_json::to_vec(event).map_err(|err| ViewError::ObservationInvalid {
            detail: format!("snapshot event cannot be encoded: {err}"),
        })?;
        lines.push(encoded);
    }
    let mut decoder = ObservationDecoder::new(analysis);
    let mut chunk = Vec::new();
    for line in lines {
        chunk.extend_from_slice(&line);
        chunk.push(b'\n');
        decoder.push(&chunk);
        chunk.clear();
        if let Some(detail) = decoder.invalid() {
            return Err(ViewError::ObservationInvalid {
                detail: format!("snapshot event sequence is invalid: {detail}"),
            });
        }
    }
    let projection = decoder.finish(0, "", false)?;
    if let Some(started) = projection.events.iter().find(|event| event.seq == 1) {
        let _ = started;
    }
    if let Some(source) = raw.run.source.as_object() {
        let root = source
            .get("root_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let digest = source
            .get("analysis_digest")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if root != analysis.project_root_id || digest != analysis.analysis_digest {
            return Err(ViewError::ObservationStale {
                detail: "snapshot run.source does not match the verified Analysis".to_string(),
            });
        }
    } else if !raw.run.source.is_null() {
        return Err(ViewError::ObservationInvalid {
            detail: "snapshot run.source is not an object or null".to_string(),
        });
    }
    let terminal_agrees = match raw.run.status.as_str() {
        "ok" => matches!(
            projection.state,
            RunDisplayState::Succeeded | RunDisplayState::Truncated
        ),
        "error" | "rejected" => projection.state == RunDisplayState::Failed,
        "incomplete" => projection.state == RunDisplayState::Incomplete,
        _ => false,
    };
    if !terminal_agrees {
        return Err(ViewError::ObservationInvalid {
            detail: format!(
                "snapshot run.status {:?} disagrees with its terminal event ({:?})",
                raw.run.status, projection.state
            ),
        });
    }
    Ok(projection)
}

/// Build a stale run projection that keeps the raw result accessible without
/// binding any event to the current document.
pub fn stale_projection(analysis_digest: &str) -> RunProjection {
    RunProjection {
        run_id: String::new(),
        entry: None,
        run_status: "stale".to_string(),
        state: RunDisplayState::Stale,
        error_code: Some("view.observation_stale".to_string()),
        analysis_digest: analysis_digest.to_string(),
        source_verified: false,
        events: Vec::new(),
        truncated: None,
        capture_level: String::new(),
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
        result_value: None,
        counts: EventCounts::default(),
        entry_event_count: 0,
        other_event_count: 0,
        warnings: vec!["Observation 与当前 Analysis 字节摘要不匹配，未绑定任何事件。".to_string()],
        output_complete: false,
    }
}
