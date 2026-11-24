//! View-owned run event log and SSE frames (`marqdo.view-run-events` v1).
//!
//! This is View's own protocol, not a CLI protocol (RFC-VIEW-0002 §6.4). It
//! only carries four event types and a bounded, sequenced replay buffer; it is
//! not an event bus and never carries token streams or per-instruction values.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

/// In-memory replay buffer per job (RFC-VIEW-0002 §6.5).
pub const EVENT_RING_CAPACITY: usize = 128;
/// Heartbeat comment cadence; not an event and does not use a sequence number.
pub const HEARTBEAT_INTERVAL_SECS: u64 = 15;
pub const HEARTBEAT_FRAME: &str = ": heartbeat\n\n";

pub const EVENTS_SCHEMA: &str = "marqdo.view-run-events";
pub const EVENTS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Accepted,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Unavailable,
    Incomplete,
}

impl RunState {
    pub fn data_value(&self) -> &'static str {
        match self {
            RunState::Accepted => "accepted",
            RunState::Running => "running",
            RunState::CancelRequested => "cancel_requested",
            RunState::Succeeded => "succeeded",
            RunState::Failed => "failed",
            RunState::Cancelled => "cancelled",
            RunState::TimedOut => "timed_out",
            RunState::Unavailable => "unavailable",
            RunState::Incomplete => "incomplete",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            RunState::Accepted => "已接受",
            RunState::Running => "运行中",
            RunState::CancelRequested => "正在取消",
            RunState::Succeeded => "成功",
            RunState::Failed => "失败",
            RunState::Cancelled => "已取消",
            RunState::TimedOut => "超时",
            RunState::Unavailable => "不可用",
            RunState::Incomplete => "事件流不完整",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunState::Succeeded
                | RunState::Failed
                | RunState::Cancelled
                | RunState::TimedOut
                | RunState::Unavailable
                | RunState::Incomplete
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    State,
    Complete,
    Error,
    Snapshot,
    Observation,
}

impl EventKind {
    pub fn sse_name(&self) -> &'static str {
        match self {
            EventKind::State => "run.state",
            EventKind::Complete => "run.complete",
            EventKind::Error => "run.error",
            EventKind::Snapshot => "run.snapshot",
            EventKind::Observation => "run.observation",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RunEvent {
    pub seq: u64,
    pub kind: EventKind,
    pub state: RunState,
    pub at: String,
    /// Safe parsed CLI result summary (never raw HTML/innerHTML material).
    pub result: Option<serde_json::Value>,
    /// Verified Trace v2 summary, only after digest/binding checks.
    pub trace: Option<serde_json::Value>,
    /// `{code, message}` for terminal failures.
    pub error: Option<serde_json::Value>,
    pub observation: Option<serde_json::Value>,
}

impl RunEvent {
    pub fn data(&self, run_id: &str) -> serde_json::Value {
        let mut data = serde_json::json!({
            "schema": EVENTS_SCHEMA,
            "schema_version": EVENTS_SCHEMA_VERSION,
            "run_id": run_id,
            "seq": self.seq,
            "state": self.state.data_value(),
            "at": self.at,
        });
        if let Some(result) = &self.result {
            data["result"] = result.clone();
        }
        if let Some(trace) = &self.trace {
            data["trace"] = trace.clone();
        }
        if let Some(error) = &self.error {
            data["error"] = error.clone();
        }
        if let Some(observation) = &self.observation {
            data["observation"] = observation.clone();
        }
        data
    }

    /// One SSE frame with `event:`, numeric `id` equal to `data.seq`, and JSON
    /// data on a single line.
    pub fn to_sse(&self, run_id: &str) -> String {
        let data = serde_json::to_string(&self.data(run_id))
            .unwrap_or_else(|_| "{\"schema\":\"marqdo.view-run-events\"}".to_string());
        format!(
            "event: {}\nid: {}\ndata: {}\n\n",
            self.kind.sse_name(),
            self.seq,
            data
        )
    }
}

/// Result of subscribing: the frames to write first, then a bounded receiver.
pub struct Subscription {
    pub replay: Vec<RunEvent>,
    pub receiver: Receiver<RunEvent>,
    /// True when the stream must be closed after writing replay (terminal job).
    pub terminal: bool,
}

/// Per-job event log. Callers provide the locking (the controller holds the job
/// lock while publishing and while snapshot+subscribe to avoid races).
pub struct EventLog {
    next_seq: u64,
    ring: VecDeque<RunEvent>,
    subscribers: Vec<SyncSender<RunEvent>>,
    last_state: RunState,
    terminal_snapshot: Option<RunEvent>,
}

impl Default for EventLog {
    fn default() -> Self {
        Self::new()
    }
}

impl EventLog {
    pub fn new() -> Self {
        Self {
            next_seq: 1,
            ring: VecDeque::with_capacity(EVENT_RING_CAPACITY),
            subscribers: Vec::new(),
            last_state: RunState::Accepted,
            terminal_snapshot: None,
        }
    }

    fn next_event(&mut self, kind: EventKind, state: RunState) -> RunEvent {
        let event = RunEvent {
            seq: self.next_seq,
            kind,
            state,
            at: utc_now(),
            result: None,
            trace: None,
            error: None,
            observation: None,
        };
        self.next_seq += 1;
        self.last_state = state;
        event
    }

    fn record(&mut self, mut event: RunEvent) -> RunEvent {
        if self.ring.len() == EVENT_RING_CAPACITY {
            self.ring.pop_front();
        }
        // A single terminal event is the only one ever stored as the final
        // snapshot; once terminal, later publishes are ignored by the caller.
        if event.state.is_terminal() {
            event.kind = match event.kind {
                EventKind::State => EventKind::State,
                other => other,
            };
            self.terminal_snapshot = Some(event.clone());
        }
        self.ring.push_back(event.clone());
        self.subscribers
            .retain(|sender| match sender.try_send(event.clone()) {
                Ok(()) => true,
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
            });
        event
    }

    pub fn publish_state(&mut self, state: RunState) -> RunEvent {
        let event = self.next_event(EventKind::State, state);
        self.record(event)
    }

    pub fn publish_observation(&mut self, observation: serde_json::Value) {
        let mut event = self.next_event(EventKind::Observation, self.last_state);
        event.observation = Some(observation);
        self.record(event);
    }

    pub fn publish_complete(
        &mut self,
        result: serde_json::Value,
        trace: Option<serde_json::Value>,
    ) -> RunEvent {
        let mut event = self.next_event(EventKind::Complete, RunState::Succeeded);
        event.result = Some(result);
        event.trace = trace;
        self.record(event)
    }

    pub fn publish_error(
        &mut self,
        state: RunState,
        code: &str,
        message: &str,
        result: Option<serde_json::Value>,
        trace: Option<serde_json::Value>,
    ) -> RunEvent {
        let mut event = self.next_event(EventKind::Error, state);
        event.error = Some(serde_json::json!({
            "code": code,
            "message": message,
        }));
        event.result = result;
        event.trace = trace;
        self.record(event)
    }

    pub fn last_state(&self) -> RunState {
        self.last_state
    }

    pub fn terminal(&self) -> bool {
        self.terminal_snapshot.is_some() || self.last_state.is_terminal()
    }

    pub fn oldest_seq(&self) -> Option<u64> {
        self.ring.front().map(|event| event.seq)
    }

    /// Build the current snapshot frame from the latest known state (and the
    /// terminal result when present). A snapshot always carries the newest
    /// sequence number, never a fake historical one.
    pub fn snapshot_event(&mut self) -> RunEvent {
        // A snapshot always takes a fresh sequence so a client that already saw
        // earlier frames never mistakes it for a duplicate and skips it.
        let seq = self.next_seq;
        self.next_seq += 1;
        if let Some(terminal) = &self.terminal_snapshot {
            let mut snapshot = terminal.clone();
            snapshot.seq = seq;
            snapshot.kind = EventKind::Snapshot;
            return snapshot;
        }
        RunEvent {
            seq,
            kind: EventKind::Snapshot,
            state: self.last_state,
            at: utc_now(),
            result: None,
            trace: None,
            error: None,
            observation: None,
        }
    }

    /// Atomically build replay and register the subscriber. The caller must
    /// hold the job lock (controller's Mutex) across this call.
    pub fn subscribe(&mut self, last_event_id: Option<u64>) -> Subscription {
        let (sender, receiver) = mpsc::sync_channel(EVENT_RING_CAPACITY);
        let terminal = self.terminal();
        let mut replay: Vec<RunEvent> = Vec::new();

        match last_event_id {
            Some(cursor) => {
                let oldest = self.oldest_seq().unwrap_or(self.next_seq);
                let last = self.next_seq.saturating_sub(1);
                if cursor > last || cursor + 1 < oldest {
                    // Too old (already evicted) or forged: synchronise with one
                    // authoritative snapshot instead of a discontinuous history.
                    replay.push(self.snapshot_event());
                } else {
                    replay.extend(self.ring.iter().filter(|event| event.seq > cursor).cloned());
                }
            }
            None => {
                replay.push(self.snapshot_event());
            }
        }

        if !terminal {
            self.subscribers.push(sender);
        }
        Subscription {
            replay,
            receiver,
            terminal,
        }
    }
}

/// Minimal RFC3339 UTC timestamp without a date/time dependency.
pub fn utc_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_utc(now.as_secs())
}

fn format_utc(epoch_secs: u64) -> String {
    let days = (epoch_secs / 86_400) as i64;
    let seconds = epoch_secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

/// Howard Hinnant's `civil_from_days`, valid for the Unix epoch range.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_have_stable_wire_values_and_terminals() {
        assert_eq!(RunState::CancelRequested.data_value(), "cancel_requested");
        assert!(!RunState::CancelRequested.is_terminal());
        for state in [
            RunState::Succeeded,
            RunState::Failed,
            RunState::Cancelled,
            RunState::TimedOut,
            RunState::Unavailable,
        ] {
            assert!(state.is_terminal());
        }
    }

    #[test]
    fn frames_have_event_id_and_matching_data_seq() {
        let mut log = EventLog::new();
        let event = log.publish_state(RunState::Running);
        let frame = event.to_sse("run-1");
        assert!(frame.starts_with("event: run.state\n"));
        assert!(frame.contains(&format!("id: {}", event.seq)));
        let data_line = frame
            .lines()
            .find(|line| line.starts_with("data: "))
            .expect("data line");
        let data: serde_json::Value =
            serde_json::from_str(data_line.trim_start_matches("data: ")).expect("json");
        assert_eq!(data["seq"], event.seq);
        assert_eq!(data["run_id"], "run-1");
        assert_eq!(data["state"], "running");
        assert_eq!(data["schema"], EVENTS_SCHEMA);
        assert_eq!(data["schema_version"], 1);
    }

    #[test]
    fn ring_is_bounded_and_old_cursor_gets_snapshot() {
        let mut log = EventLog::new();
        for _ in 0..(EVENT_RING_CAPACITY + 40) {
            log.publish_state(RunState::Running);
        }
        assert_eq!(log.ring.len(), EVENT_RING_CAPACITY);
        let subscription = log.subscribe(Some(1));
        assert_eq!(subscription.replay.len(), 1);
        assert_eq!(subscription.replay[0].kind, EventKind::Snapshot);
        assert!(!subscription.terminal);
    }

    #[test]
    fn terminal_job_replays_snapshot_and_closes() {
        let mut log = EventLog::new();
        log.publish_state(RunState::Running);
        log.publish_complete(serde_json::json!({"success": true}), None);
        let subscription = log.subscribe(None);
        assert!(subscription.terminal);
        assert_eq!(subscription.replay[0].kind, EventKind::Snapshot);
        assert_eq!(subscription.replay[0].state, RunState::Succeeded);
        assert!(subscription.replay[0].result.is_some());
    }

    #[test]
    fn utc_formatting_is_stable() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn newest_cursor_replays_only_new_events() {
        let mut log = EventLog::new();
        let first = log.publish_state(RunState::Accepted);
        let second = log.publish_state(RunState::Running);
        let subscription = log.subscribe(Some(first.seq));
        assert_eq!(subscription.replay.len(), 1);
        assert_eq!(subscription.replay[0].seq, second.seq);
    }
}
