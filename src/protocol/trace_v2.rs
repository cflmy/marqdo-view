//! Trace v2 validation and run projection.
//!
//! Trace data is observed execution, not execution itself. Validation is hard:
//! the trace must bind to the exact Analysis bytes and project identity, and
//! every event module id must exist in Analysis. Missing capabilities (per
//! assignment values, memory snapshots, proofs) are explicit `UNSUPPORTED`
//! states, never invented (RFC-VIEW-0001 §4.4, §12).

use crate::error::{ViewError, ViewResult};
use crate::protocol::analysis_v2::{ModuleSummary, SemanticProjection};
use crate::reader::source::{sha256_hex, sha256_prefixed, SourceIndex, SourceSpan};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunDisplayState {
    NotRun,
    StaticOnly,
    Running,
    Succeeded,
    Failed,
    Stale,
    Truncated,
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
            RunDisplayState::Truncated => "Trace 已截断",
            RunDisplayState::Unsupported => "当前协议不支持",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TraceKind {
    CallEnter,
    CallExit,
    BranchEval,
    LoopIteration,
    Return,
    Effect,
    Truncated,
    Other(String),
}

impl TraceKind {
    pub fn data_value(&self) -> &str {
        match self {
            TraceKind::CallEnter => "call.enter",
            TraceKind::CallExit => "call.exit",
            TraceKind::BranchEval => "branch.eval",
            TraceKind::LoopIteration => "loop.iteration",
            TraceKind::Return => "return",
            TraceKind::Effect => "effect",
            TraceKind::Truncated => "trace.truncated",
            TraceKind::Other(kind) => kind,
        }
    }

    fn parse(raw: &str) -> TraceKind {
        match raw {
            "call.enter" => TraceKind::CallEnter,
            "call.exit" => TraceKind::CallExit,
            "branch.eval" => TraceKind::BranchEval,
            "loop.iteration" => TraceKind::LoopIteration,
            "return" => TraceKind::Return,
            "effect" => TraceKind::Effect,
            "trace.truncated" => TraceKind::Truncated,
            other => TraceKind::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraceEvent {
    pub seq: u64,
    pub kind: TraceKind,
    pub function_id: Option<String>,
    pub module_id: Option<String>,
    /// Validated span in the entry module; `None` for other modules or invalid
    /// spans (which are reported as warnings).
    pub span: Option<SourceSpan>,
    /// Presentation-only span in an event's own module, kept for the panel.
    pub raw_span: Option<SourceSpan>,
    /// Statement span in the entry document this event attaches to, if any.
    pub anchor_span: Option<SourceSpan>,
    pub detail: Option<String>,
    pub operation: Option<String>,
    pub duration_micros: Option<u64>,
}

impl TraceEvent {
    pub fn kind_label(&self) -> &str {
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
pub struct EventCounts {
    pub call_enter: usize,
    pub call_exit: usize,
    pub branch_eval: usize,
    pub loop_iteration: usize,
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
            + self.return_events
            + self.effect
            + self.other
    }
}

#[derive(Debug, Clone)]
pub struct RunProjection {
    pub run_id: String,
    pub entry: Option<String>,
    /// Raw `run.status` from the Core (`ok` or a stable runtime error code).
    pub run_status: String,
    pub state: RunDisplayState,
    pub error_code: Option<String>,
    pub analysis_digest: String,
    pub events: Vec<TraceEvent>,
    pub truncated: Option<Truncation>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub result_value: Option<RunValue>,
    pub counts: EventCounts,
    /// Counts per event module: entry document vs other modules.
    pub entry_event_count: usize,
    pub other_event_count: usize,
    pub warnings: Vec<String>,
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
    pub fn events_for_anchor(&self, span: SourceSpan) -> Vec<&TraceEvent> {
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
            TraceKind::BranchEval => format!("if → {}", last.detail.as_deref().unwrap_or("?")),
            TraceKind::LoopIteration => {
                let outcome = match last.detail.as_deref() {
                    Some("continue") => "continue",
                    Some("exit") => "exit",
                    other => other.unwrap_or("?"),
                };
                format!("loop {outcome}")
            }
            TraceKind::Return => format!("return {}", last.detail.as_deref().unwrap_or("Unit")),
            TraceKind::CallEnter => "call".to_string(),
            TraceKind::CallExit => "call exit".to_string(),
            TraceKind::Effect => format!("effect {}", last.operation.as_deref().unwrap_or("ok")),
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

#[derive(Debug, Clone)]
pub struct VerifiedRun {
    pub projection: RunProjection,
}

#[derive(Debug, Deserialize)]
struct RawTrace {
    schema: String,
    schema_version: u32,
    run: RawRun,
    source: RawTraceSource,
    #[serde(default)]
    truncated: Option<RawTruncation>,
    #[serde(default)]
    events: Vec<RawEvent>,
}

#[derive(Debug, Deserialize)]
struct RawRun {
    #[serde(default)]
    id: String,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Deserialize)]
struct RawTraceSource {
    #[serde(default)]
    root_id: String,
    #[serde(default)]
    analysis_digest: String,
}

#[derive(Debug, Deserialize)]
struct RawTruncation {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    limit: u64,
    #[serde(default)]
    written: u64,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    #[serde(default)]
    seq: u64,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    function_id: Option<String>,
    #[serde(default)]
    module_id: Option<String>,
    #[serde(default)]
    span: Option<SourceSpan>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    duration_micros: Option<u64>,
}

pub struct TraceVerificationInput<'a> {
    pub trace_bytes: &'a [u8],
    pub analysis_bytes: &'a [u8],
    pub semantic: &'a SemanticProjection,
    pub modules: &'a [ModuleSummary],
    pub source: &'a str,
    pub source_index: &'a SourceIndex,
    pub run_json: &'a Value,
    pub exit_code: i32,
}

/// Verify Trace v2 against the current Analysis and project it for the reader.
pub fn verify_trace(input: TraceVerificationInput<'_>) -> ViewResult<VerifiedRun> {
    let raw: RawTrace =
        serde_json::from_slice(input.trace_bytes).map_err(|err| ViewError::TraceStale {
            detail: format!("Trace v2 is not valid JSON: {err}"),
        })?;
    if raw.schema != "marqdo.trace" {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!("unexpected Trace schema {:?}", raw.schema),
        });
    }
    if raw.schema_version != crate::protocol::tool_v1::TRACE_PROTOCOL {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "Trace schema_version {} is not supported (View implements v{})",
                raw.schema_version,
                crate::protocol::tool_v1::TRACE_PROTOCOL
            ),
        });
    }
    let expected_digest = sha256_prefixed(input.analysis_bytes);
    if raw.source.analysis_digest != expected_digest {
        return Err(ViewError::TraceStale {
            detail: format!(
                "trace.source.analysis_digest {} does not match current Analysis {}",
                raw.source.analysis_digest, expected_digest
            ),
        });
    }
    if raw.source.root_id != input.semantic.project_root_id {
        return Err(ViewError::TraceStale {
            detail: format!(
                "trace.source.root_id {} does not match Analysis project {}",
                raw.source.root_id, input.semantic.project_root_id
            ),
        });
    }
    let known_modules: std::collections::HashSet<&str> = input
        .modules
        .iter()
        .map(|module| module.id.as_str())
        .collect();
    for event in &raw.events {
        if let Some(module) = event.module_id.as_deref() {
            if !known_modules.contains(module) {
                return Err(ViewError::TraceStale {
                    detail: format!(
                        "trace event seq {} references unknown module {module}",
                        event.seq
                    ),
                });
            }
        }
    }

    let entry_module = input.semantic.module_id.as_str();
    let statements = input.semantic.all_statements();
    let mut warnings = Vec::new();
    let mut counts = EventCounts::default();
    let mut entry_event_count = 0usize;
    let mut other_event_count = 0usize;
    let mut events = Vec::with_capacity(raw.events.len());
    for event in raw.events {
        let kind = TraceKind::parse(&event.kind);
        match kind {
            TraceKind::CallEnter => counts.call_enter += 1,
            TraceKind::CallExit => counts.call_exit += 1,
            TraceKind::BranchEval => counts.branch_eval += 1,
            TraceKind::LoopIteration => counts.loop_iteration += 1,
            TraceKind::Return => counts.return_events += 1,
            TraceKind::Effect => counts.effect += 1,
            _ => counts.other += 1,
        }
        let is_entry = event.module_id.as_deref() == Some(entry_module);
        if is_entry {
            entry_event_count += 1;
        } else {
            other_event_count += 1;
        }
        // Only entry-module spans can be anchored to the rendered document.
        let (span, anchor_span) = if is_entry {
            match event.span {
                Some(span)
                    if input
                        .source_index
                        .validate_span_bytes(input.source.as_bytes(), span) =>
                {
                    let anchor = statements
                        .iter()
                        .filter(|(_, statement)| {
                            statement.span == span
                                || statement.condition_span == Some(span)
                                || statement
                                    .arms
                                    .iter()
                                    .any(|arm| arm.condition_span == Some(span))
                        })
                        .min_by_key(|(_, statement)| statement.span.len())
                        .map(|(_, statement)| statement.span)
                        .or_else(|| {
                            statements
                                .iter()
                                .filter(|(_, statement)| statement.span.contains(span))
                                .min_by_key(|(_, statement)| statement.span.len())
                                .map(|(_, statement)| statement.span)
                        });
                    (Some(span), anchor)
                }
                other => {
                    if other.is_some() {
                        warnings.push(format!(
                            "事件 seq {} 的 span 超出当前源码或不在 UTF-8 边界，已取消位置绑定",
                            event.seq
                        ));
                    }
                    (None, None)
                }
            }
        } else {
            (None, None)
        };
        events.push(TraceEvent {
            seq: event.seq,
            kind,
            function_id: event.function_id,
            module_id: event.module_id,
            span,
            raw_span: event.span,
            anchor_span,
            detail: event.detail,
            operation: event.operation,
            duration_micros: event.duration_micros,
        });
    }
    // Sequence numbers must be strictly increasing; a broken trace is stale.
    for pair in events.windows(2) {
        if pair[1].seq <= pair[0].seq {
            return Err(ViewError::TraceStale {
                detail: format!(
                    "trace seq is not strictly increasing ({} then {})",
                    pair[0].seq, pair[1].seq
                ),
            });
        }
    }

    let truncated = raw.truncated.map(|truncated| Truncation {
        reason: truncated.reason,
        limit: truncated.limit,
        written: truncated.written,
    });
    let run_status = if raw.run.status.is_empty() {
        "ok".to_string()
    } else {
        raw.run.status.clone()
    };
    let success = input
        .run_json
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let error_code = input
        .run_json
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            if run_status == "ok" {
                None
            } else {
                Some(run_status.clone())
            }
        });
    let state = if truncated.is_some() {
        RunDisplayState::Truncated
    } else if success && run_status == "ok" {
        RunDisplayState::Succeeded
    } else {
        RunDisplayState::Failed
    };
    let stdout = bounded_text(input.run_json, "stdout", 64 * 1024);
    let stderr = bounded_text(input.run_json, "stderr", 64 * 1024);
    let result_value = input.run_json.get("value").and_then(|value| {
        let ty = value.get("type").and_then(Value::as_str)?;
        let display = value
            .get("decimal")
            .and_then(Value::as_str)
            .or_else(|| value.get("text").and_then(Value::as_str))
            .or_else(|| value.get("value").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| compact_json(value));
        Some(RunValue {
            ty: ty.to_string(),
            display,
        })
    });
    let _ = sha256_hex(input.trace_bytes);
    Ok(VerifiedRun {
        projection: RunProjection {
            run_id: raw.run.id,
            entry: raw.run.entry,
            run_status,
            state,
            error_code,
            analysis_digest: expected_digest,
            events,
            truncated,
            stdout,
            stderr,
            exit_code: input.exit_code,
            result_value,
            counts,
            entry_event_count,
            other_event_count,
            warnings,
        },
    })
}

fn bounded_text(run_json: &Value, key: &str, limit: usize) -> String {
    let text = run_json
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if text.len() <= limit {
        text
    } else {
        let mut truncated: String = text.chars().take(limit).collect();
        truncated.push_str("\n…（输出已截断）");
        truncated
    }
}

fn compact_json(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 80 {
        format!("{}…", text.chars().take(80).collect::<String>())
    } else {
        text
    }
}

/// Build a stale run projection that keeps the raw result accessible without
/// binding any event to the current document.
pub fn stale_projection(analysis_digest: &str, source: &SourceIndex) -> RunProjection {
    let _ = source;
    RunProjection {
        run_id: String::new(),
        entry: None,
        run_status: "stale".to_string(),
        state: RunDisplayState::Stale,
        error_code: Some("view.trace_stale".to_string()),
        analysis_digest: analysis_digest.to_string(),
        events: Vec::new(),
        truncated: None,
        stdout: String::new(),
        stderr: String::new(),
        exit_code: 0,
        result_value: None,
        counts: EventCounts::default(),
        entry_event_count: 0,
        other_event_count: 0,
        warnings: vec!["Trace 与当前 Analysis 字节摘要不匹配，未绑定任何事件。".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_values_are_stable() {
        assert_eq!(RunDisplayState::Succeeded.data_value(), "succeeded");
        assert_eq!(RunDisplayState::Truncated.data_value(), "truncated");
        assert_eq!(RunDisplayState::Failed.data_value(), "failed");
        assert_eq!(RunDisplayState::Stale.label(), "结果已过期");
    }

    #[test]
    fn trace_kinds_are_additive_safe() {
        assert_eq!(TraceKind::parse("branch.eval"), TraceKind::BranchEval);
        assert_eq!(
            TraceKind::parse("future.event"),
            TraceKind::Other("future.event".to_string())
        );
        assert_eq!(
            TraceKind::parse("future.event").data_value(),
            "future.event"
        );
    }
}
