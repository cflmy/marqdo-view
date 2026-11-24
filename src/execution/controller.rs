//! Single-job execution controller.
//!
//! One explicit `POST /api/runs` request becomes exactly one Core subprocess.
//! The controller owns the job state machine, deduplication, cancellation, the
//! bounded event log and Trace verification; it never polls and never starts a
//! second process for the same accepted job (RFC-VIEW-0002 §6.6/§6.7).

use crate::core_client::{build_call_args, build_run_args, RunOptions, SandboxMode, ScratchDir};
use crate::error::{ViewError, ViewResult};
use crate::execution::events::{EventLog, RunEvent, RunState, Subscription};
use crate::execution::export_candidate::TerminalExportCandidate;
use crate::execution::process::{
    run_bounded_with_hook, Captured, ProcessBudget, ProcessHooks, ProcessOutcome,
};
use crate::execution::request::{self, payload_digest, ExecutionPlan, ExecutionTarget, RunMode};
use crate::protocol::execution_events_v1 as core_events;
use crate::protocol::trace_v2::{verify_trace, TraceVerificationInput};
use crate::reader::assemble::{DocumentService, StaticDocument};
use crate::reader::source::{sha256_hex, sha256_prefixed};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_RUN_TIMEOUT_SECS: u64 = 30;
pub const MAX_RUN_TIMEOUT_SECS: u64 = 120;
pub const DEFAULT_MAX_JOBS: usize = 32;
pub const DEFAULT_TERMINAL_TTL: Duration = Duration::from_secs(15 * 60);
pub const DEFAULT_MAX_SSE_CONNECTIONS: usize = 2;
/// Execution budgets sent explicitly on every `run`/`call` (RFC §6.7).
pub const DEFAULT_FUEL: u64 = 1_000_000;
pub const DEFAULT_MAX_OUTPUT: u64 = 1_048_576;
pub const DEFAULT_TRACE_MAX_EVENTS: u64 = 10_000;
pub const DEFAULT_TRACE_MAX_BYTES: u64 = 4_194_304;
/// Maximum JSON/trace bytes accepted from one execution.
pub const MAX_RESULT_JSON_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TRACE_FILE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// View-owned wall clock; kept equal to the Core `--timeout` value.
    pub run_timeout: Duration,
    pub budget: ProcessBudget,
    pub max_jobs: usize,
    pub terminal_ttl: Duration,
    pub max_sse_connections: usize,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            run_timeout: Duration::from_secs(DEFAULT_RUN_TIMEOUT_SECS),
            budget: ProcessBudget::default(),
            max_jobs: DEFAULT_MAX_JOBS,
            terminal_ttl: DEFAULT_TERMINAL_TTL,
            max_sse_connections: DEFAULT_MAX_SSE_CONNECTIONS,
        }
    }
}

impl ExecutionConfig {
    /// Clamp a user-provided timeout to the RFC range and keep both caps equal.
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        let seconds = seconds.clamp(1, MAX_RUN_TIMEOUT_SECS);
        self.run_timeout = Duration::from_secs(seconds);
        self.budget.wall_clock = self.run_timeout;
        self
    }

    pub fn run_timeout_secs(&self) -> u64 {
        self.run_timeout.as_secs().clamp(1, MAX_RUN_TIMEOUT_SECS)
    }
}

/// What the browser gets from `POST /api/runs` (202).
#[derive(Debug, Clone)]
pub struct AcceptedRun {
    pub run_id: String,
    pub state: RunState,
    pub events_url: String,
    pub cancel_url: String,
    /// True when this is the same request id + same payload as an earlier job.
    pub reused: bool,
}

/// One SSE observer handle. Dropping it releases the per-job connection slot.
pub struct SseSubscription {
    pub run_id: String,
    pub replay: Vec<String>,
    pub receiver: Receiver<RunEvent>,
    pub terminal: bool,
    _guard: ConnectionGuard,
}

impl std::fmt::Debug for SseSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SseSubscription")
            .field("run_id", &self.run_id)
            .field("replay", &self.replay.len())
            .field("terminal", &self.terminal)
            .finish()
    }
}

struct ConnectionGuard {
    job: Arc<Job>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.job.inner.lock() {
            inner.connections = inner.connections.saturating_sub(1);
        }
    }
}

pub struct Job {
    pub run_id: String,
    pub session_id: String,
    pub document: String,
    pub created: Instant,
    cancel: Arc<AtomicBool>,
    inner: Mutex<JobInner>,
}

struct JobInner {
    log: EventLog,
    terminal_at: Option<Instant>,
    export_candidate: Option<Arc<TerminalExportCandidate>>,
    connections: usize,
}

impl Job {
    fn new(run_id: String, session_id: String, document: String) -> Self {
        Self {
            run_id,
            session_id,
            document,
            created: Instant::now(),
            cancel: Arc::new(AtomicBool::new(false)),
            inner: Mutex::new(JobInner {
                log: EventLog::new(),
                terminal_at: None,
                export_candidate: None,
                connections: 0,
            }),
        }
    }

    pub fn state(&self) -> RunState {
        self.inner
            .lock()
            .map(|inner| inner.log.last_state())
            .unwrap_or(RunState::Unavailable)
    }

    pub fn is_terminal(&self) -> bool {
        self.inner
            .lock()
            .map(|inner| inner.log.terminal())
            .unwrap_or(true)
    }
}

struct DedupeEntry {
    digest: String,
    run_id: String,
    created: Instant,
}

#[derive(Default)]
struct ControllerState {
    jobs: HashMap<String, Arc<Job>>,
    order: VecDeque<String>,
    active: Option<String>,
    dedupe: HashMap<(String, String), DedupeEntry>,
}

struct CoreRun {
    process: ProcessOutcome,
    trace: ViewResult<Option<Vec<u8>>>,
    stream: Option<core_events::Stream>,
}

pub struct ExecutionController {
    service: Arc<DocumentService>,
    config: ExecutionConfig,
    state: Mutex<ControllerState>,
}

impl ExecutionController {
    pub fn new(service: Arc<DocumentService>, config: ExecutionConfig) -> Self {
        Self {
            service,
            config,
            state: Mutex::new(ControllerState::default()),
        }
    }

    /// Sweep inactive jobs even if the browser never makes another request.
    pub fn start_expiry_sweep(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        let period = self
            .config
            .terminal_ttl
            .min(Duration::from_secs(30))
            .max(Duration::from_millis(100));
        std::thread::spawn(move || loop {
            std::thread::sleep(period);
            let Some(controller) = weak.upgrade() else {
                break;
            };
            if let Ok(mut state) = controller.state.lock() {
                controller.purge_locked(&mut state);
            };
        });
    }

    pub fn config(&self) -> &ExecutionConfig {
        &self.config
    }

    /// Validate and accept one explicit run request. The read-only Analysis
    /// check and digest comparison happen **before** acceptance; the user
    /// program is only spawned in the background afterwards.
    pub fn submit(self: &Arc<Self>, session_id: &str, body: &[u8]) -> ViewResult<AcceptedRun> {
        let validated = request::validate_request(body)?;
        let digest = payload_digest(body);
        let request_identity = (session_id.to_string(), validated.client_request_id.clone());
        let mut state = self.state.lock().map_err(|_| ViewError::Io {
            detail: "execution controller lock poisoned".to_string(),
        })?;
        self.purge_locked(&mut state);

        if let Some(entry) = state.dedupe.get(&request_identity) {
            if entry.digest != digest {
                return Err(ViewError::RunConflict {
                    detail: "同一 client_request_id 携带了不同请求内容；拒绝执行以避免偷换任务"
                        .to_string(),
                });
            }
            if let Some(job) = state.jobs.get(&entry.run_id).cloned() {
                return Ok(self.accepted_run(&job, true));
            }
            return Err(ViewError::RunInvalid {
                detail: "client_request_id 对应的运行已过期；请刷新页面后重新提交".to_string(),
            });
        }
        if let Some(active) = state.active.as_deref() {
            return Err(ViewError::RunBusy {
                detail: format!(
                    "已有运行 {active} 正在进行；本版本每个 View 服务只允许一个活动任务，请等待或取消"
                ),
            });
        }

        // Static, read-only validation while holding the controller lock so two
        // concurrent submissions cannot both pass the single-job gate.
        let static_doc = self.service.static_document(&validated.document)?;
        if static_doc.analysis_digest != validated.analysis_digest {
            return Err(ViewError::RunInvalid {
                detail: "页面携带的 analysis_digest 与当前源码的 check --emit 不一致（页面已过期）；请刷新后重试"
                    .to_string(),
            });
        }
        let plan = validated.plan_against(&static_doc.verified.projection)?;
        let run_id = new_run_id();
        let job = Arc::new(Job::new(
            run_id.clone(),
            session_id.to_string(),
            plan.document.to_string(),
        ));
        {
            let mut inner = job.inner.lock().map_err(|_| ViewError::Io {
                detail: "job lock poisoned".to_string(),
            })?;
            inner.log.publish_state(RunState::Accepted);
        }
        state.order.push_back(run_id.clone());
        state.jobs.insert(run_id.clone(), Arc::clone(&job));
        state.active = Some(run_id.clone());
        state.dedupe.insert(
            request_identity,
            DedupeEntry {
                digest,
                run_id: run_id.clone(),
                created: Instant::now(),
            },
        );
        let accepted = self.accepted_run(&job, false);
        drop(state);

        let controller = Arc::clone(self);
        std::thread::spawn(move || controller.execute_job(job, static_doc, plan));
        Ok(accepted)
    }

    fn accepted_run(&self, job: &Arc<Job>, reused: bool) -> AcceptedRun {
        AcceptedRun {
            run_id: job.run_id.clone(),
            state: job.state(),
            events_url: format!("/api/runs/{}/events", job.run_id),
            cancel_url: format!("/api/runs/{}/cancel", job.run_id),
            reused,
        }
    }

    fn execute_job(
        self: Arc<Self>,
        job: Arc<Job>,
        static_doc: StaticDocument,
        plan: ExecutionPlan,
    ) {
        let started = Instant::now();
        let outcome = self.run_core(&job, &plan, &static_doc);
        let cancel_requested = job.cancel.load(Ordering::SeqCst);
        eprintln!(
            "JOB_DEBUG id={} run_core elapsed={:?} cancel={cancel_requested}",
            job.run_id,
            started.elapsed()
        );
        let terminal = self.build_terminal(&plan, &static_doc, started, outcome, cancel_requested);
        let Ok(mut inner) = job.inner.lock() else {
            return;
        };
        if inner.log.terminal() {
            return;
        }
        let (state, result, trace, trace_status, error_code, error_message) = terminal;
        let mut result = result;
        result["trace_status"] = serde_json::json!(trace_status.clone());
        if let Some(candidate) = TerminalExportCandidate::from_terminal(
            state,
            &plan,
            &static_doc,
            &result,
            trace.as_ref(),
        ) {
            result["export_preview"] = candidate.preview();
            inner.export_candidate = Some(Arc::new(candidate));
        }
        match state {
            RunState::Succeeded => {
                inner.log.publish_complete(result, trace);
            }
            _ => {
                inner.log.publish_error(
                    state,
                    error_code.as_deref().unwrap_or("view.run_failed"),
                    error_message.as_deref().unwrap_or("运行失败"),
                    Some(result),
                    trace,
                );
            }
        }
        eprintln!(
            "JOB_DEBUG id={} terminal state={:?} total={:?}",
            job.run_id,
            state,
            started.elapsed()
        );
        inner.terminal_at = Some(Instant::now());
        drop(inner);

        if let Ok(mut state) = self.state.lock() {
            if state.active.as_deref() == Some(job.run_id.as_str()) {
                state.active = None;
            }
            self.purge_locked(&mut state);
        }
    }

    /// Spawn and wait for the one Core subprocess. `running` is published only
    /// after the process was actually spawned (through the spawn hook).
    fn run_core(
        &self,
        job: &Arc<Job>,
        plan: &ExecutionPlan,
        static_doc: &StaticDocument,
    ) -> ViewResult<CoreRun> {
        let client = self
            .service
            .core_client()
            .ok_or_else(|| ViewError::RunDenied {
                detail: "没有可用的冻结核心（MARQDO_BIN），运行被拒绝".to_string(),
            })?;
        let entry = self.service.project().absolute_for(&plan.document)?;
        let scratch = ScratchDir::create("run")?;
        let trace_path = scratch.file("trace.json");
        let mut options = self.run_options(plan);
        let projection = &static_doc.verified.projection;
        options.event_stream =
            crate::protocol::tool_v1::supports_execution_events(&projection.producer_version);
        let args = match &plan.target {
            ExecutionTarget::Entry => build_run_args(&entry, &trace_path, &options),
            ExecutionTarget::Function { cli_name, .. } => {
                build_call_args(&entry, cli_name, &plan.arguments, &trace_path, &options)
            }
        };
        let running_job = Arc::clone(job);
        let hook = move || {
            if let Ok(mut inner) = running_job.inner.lock() {
                if !inner.log.terminal() {
                    inner.log.publish_state(RunState::Running);
                }
            }
        };
        let budget = ProcessBudget {
            wall_clock: self.config.run_timeout,
            ..self.config.budget.clone()
        };
        let (send_stream, receive_stream) = std::sync::mpsc::channel();
        let mut hooks = ProcessHooks {
            on_spawn: Some(Box::new(hook)),
            ..Default::default()
        };
        if options.event_stream {
            let entry = match &plan.target {
                ExecutionTarget::Entry => "main",
                ExecutionTarget::Function { cli_name, .. } => cli_name,
            };
            let binding = core_events::Binding {
                digest: static_doc.analysis_digest.clone(),
                root: projection.project_root_id.clone(),
                entry: format!("{}::{entry}", projection.module_id),
                function: plan.function_id.clone(),
                mode: plan.mode.as_str().to_string(),
                producer: projection.producer_version.clone(),
                modules: projection
                    .modules
                    .iter()
                    .map(|module| module.id.clone())
                    .collect(),
                max_output: DEFAULT_MAX_OUTPUT as usize,
            };
            let observed_job = Arc::clone(job);
            hooks.stdout = Some(Box::new(move |stdout| {
                let stream = core_events::read(stdout, &binding, |observation| {
                    if let Ok(mut inner) = observed_job.inner.lock() {
                        inner.log.publish_observation(observation);
                    }
                });
                let _ = send_stream.send(stream);
                Captured::default()
            }));
        }
        let outcome = run_bounded_with_hook(
            client.bin(),
            &args,
            self.service.project().root(),
            &budget,
            Some(Arc::clone(&job.cancel)),
            hooks,
        )?;
        let trace = read_trace_limited(&trace_path);
        Ok(CoreRun {
            process: outcome,
            trace,
            stream: receive_stream.try_recv().ok(),
        })
    }

    fn run_options(&self, plan: &ExecutionPlan) -> RunOptions {
        RunOptions {
            event_stream: false,
            mode: match plan.mode {
                RunMode::Pure => SandboxMode::Pure,
                RunMode::Project => SandboxMode::Project,
            },
            grants: plan.grants.clone(),
            fuel: Some(DEFAULT_FUEL),
            stack: None,
            max_output: Some(DEFAULT_MAX_OUTPUT),
            timeout_seconds: Some(self.config.run_timeout_secs()),
            view_timeout: Some(self.config.run_timeout),
            trace_max_events: Some(DEFAULT_TRACE_MAX_EVENTS),
            trace_max_bytes: Some(DEFAULT_TRACE_MAX_BYTES),
        }
    }

    fn build_terminal(
        &self,
        plan: &ExecutionPlan,
        static_doc: &StaticDocument,
        started: Instant,
        outcome: ViewResult<CoreRun>,
        cancel_requested: bool,
    ) -> (
        RunState,
        serde_json::Value,
        Option<serde_json::Value>,
        String,
        Option<String>,
        Option<String>,
    ) {
        let duration_ms = started.elapsed().as_millis() as u64;
        let CoreRun {
            process: outcome,
            trace: trace_bytes,
            stream,
        } = match outcome {
            Ok(value) => value,
            Err(err) => {
                let state = match err.code() {
                    "view.output_limit" => RunState::Failed,
                    _ => RunState::Unavailable,
                };
                let result = base_result(plan, static_doc, duration_ms);
                return (
                    state,
                    result,
                    None,
                    "unavailable".to_string(),
                    Some(err.code().to_string()),
                    Some(err.detail().to_string()),
                );
            }
        };
        if outcome.cancelled {
            let mut result = base_result(plan, static_doc, duration_ms);
            result["stdout"] = serde_json::json!(outcome.stdout.text_lossy());
            result["stderr"] = serde_json::json!(outcome.stderr.text_lossy());
            return (
                RunState::Cancelled,
                result,
                None,
                "unavailable".to_string(),
                Some("view.run_cancelled".to_string()),
                Some("运行已按用户请求取消，进程组已终止".to_string()),
            );
        }
        if cancel_requested && !outcome.timed_out && !outcome.status_ok() {
            // The user cancelled; the process may have exited from SIGTERM
            // before the grace-period SIGKILL, so the adapter did not mark it
            // itself. A successful exit still wins the race (RFC §6.6).
            let mut result = base_result(plan, static_doc, duration_ms);
            result["stderr"] = serde_json::json!(outcome.stderr.text_lossy());
            return (
                RunState::Cancelled,
                result,
                None,
                "unavailable".to_string(),
                Some("view.run_cancelled".to_string()),
                Some("运行已按用户请求取消，进程组已终止".to_string()),
            );
        }
        if outcome.timed_out {
            let mut result = base_result(plan, static_doc, duration_ms);
            result["stderr"] = serde_json::json!(outcome.stderr.text_lossy());
            return (
                RunState::TimedOut,
                result,
                None,
                "unavailable".to_string(),
                Some("view.core_timeout".to_string()),
                Some(format!(
                    "运行超过 {} 秒墙钟上限，进程组已终止",
                    self.config.run_timeout_secs()
                )),
            );
        }
        if outcome.stdout.total > MAX_RESULT_JSON_BYTES {
            let mut result = base_result(plan, static_doc, duration_ms);
            result["stdout_truncated"] = serde_json::json!(true);
            return (
                RunState::Failed,
                result,
                None,
                "unavailable".to_string(),
                Some("view.output_limit".to_string()),
                Some(format!(
                    "CLI JSON 输出超过 {} 字节上限，已停止接收",
                    MAX_RESULT_JSON_BYTES
                )),
            );
        }
        let json: serde_json::Value = if let Some(stream) = &stream {
            let Some(mut terminal) = stream
                .terminal
                .clone()
                .filter(|terminal| terminal["success"].as_bool() == Some(outcome.status_ok()))
            else {
                let mut result = base_result(plan, static_doc, duration_ms);
                result["stdout"] = serde_json::json!(String::from_utf8_lossy(&stream.stdout));
                result["stderr"] = serde_json::json!(String::from_utf8_lossy(&stream.stderr));
                result["output_complete"] = serde_json::json!(false);
                return (
                    RunState::Incomplete,
                    result,
                    None,
                    "unavailable".into(),
                    Some("view.core_protocol".into()),
                    Some(
                        if stream.invalid {
                            "CLI 事件流无效"
                        } else {
                            "CLI 事件流缺少可信终态"
                        }
                        .into(),
                    ),
                );
            };
            terminal["command"] = serde_json::json!(match plan.target {
                ExecutionTarget::Entry => "run",
                ExecutionTarget::Function { .. } => "call",
            });
            terminal["stdout"] = serde_json::json!(String::from_utf8_lossy(&stream.stdout));
            terminal["stderr"] = serde_json::json!(String::from_utf8_lossy(&stream.stderr));
            terminal
        } else {
            match serde_json::from_slice::<serde_json::Value>(&outcome.stdout.bytes) {
                Ok(value) if value.is_object() => value,
                Ok(_) => {
                    return (
                        RunState::Failed,
                        base_result(plan, static_doc, duration_ms),
                        None,
                        "unavailable".to_string(),
                        Some("view.core_protocol".to_string()),
                        Some("CLI 的 --json 输出不是 JSON 对象".to_string()),
                    )
                }
                Err(err) => {
                    return (
                        RunState::Failed,
                        base_result(plan, static_doc, duration_ms),
                        None,
                        "unavailable".to_string(),
                        Some("view.core_protocol".to_string()),
                        Some(format!(
                            "无法解析 CLI --json 输出（exit {:?}）：{err}",
                            outcome.exit_code
                        )),
                    )
                }
            }
        };

        // Re-read the source after the run: a changed source makes the result
        // stale and forbids anchoring events to the current text.
        let current_source = self
            .service
            .project()
            .read_source(&plan.document)
            .unwrap_or_default();
        let stale = sha256_hex(&current_source) != static_doc.source_sha256;

        let (mut trace, mut trace_status) = match trace_bytes {
            Ok(trace_bytes) => build_trace_summary(
                trace_bytes.as_deref(),
                static_doc,
                &json,
                outcome.exit_code.unwrap_or(-1),
                stale,
            ),
            Err(_) => (None, "invalid".into()),
        };
        let mut result = build_cli_result(plan, static_doc, duration_ms, &json, &outcome);
        result["stale"] = serde_json::json!(stale);
        if let Some(stream) = stream {
            result["event_protocol"] = serde_json::json!(1);
            result["core_run_id"] = serde_json::json!(stream.id);
            result["source_verified"] = serde_json::json!(stream.verified && !stale);
            result["observation_truncated"] = serde_json::json!(stream.truncated);
            result["stdout_bytes"] = serde_json::json!(stream.stdout.len());
            result["stderr_bytes"] = serde_json::json!(stream.stderr.len());
            result["output_complete"] = serde_json::json!(
                std::str::from_utf8(&stream.stdout).is_ok()
                    && std::str::from_utf8(&stream.stderr).is_ok()
                    && !json["error"]["code"]
                        .as_str()
                        .is_some_and(|code| code.contains("output_limit"))
            );
            if trace_status == "verified"
                && trace
                    .as_ref()
                    .and_then(|trace| trace["core_run_id"].as_str())
                    != Some(&stream.id)
            {
                trace = None;
                trace_status = "invalid".into();
            }
        }
        let success = json
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if success {
            (RunState::Succeeded, result, trace, trace_status, None, None)
        } else {
            let (code, message) = cli_error(&json);
            (
                RunState::Failed,
                result,
                trace,
                trace_status,
                Some(code),
                Some(message),
            )
        }
    }

    /// Idempotent cancellation: only requests it; the executor owns the unique
    /// terminal transition.
    pub fn cancel(&self, session_id: &str, run_id: &str) -> ViewResult<RunState> {
        let job = self.lookup(session_id, run_id)?;
        let mut inner = job.inner.lock().map_err(|_| ViewError::Io {
            detail: "job lock poisoned".to_string(),
        })?;
        let state_now = inner.log.last_state();
        eprintln!("CANCEL_DEBUG id={} last_state={state_now:?}", job.run_id);
        if state_now.is_terminal() {
            return Ok(state_now);
        }
        job.cancel.store(true, Ordering::SeqCst);
        if state_now != RunState::CancelRequested {
            inner.log.publish_state(RunState::CancelRequested);
        }
        Ok(RunState::CancelRequested)
    }

    /// Snapshot + subscribe must be one atomic step under the job lock so no
    /// event can slip between the replay and the live subscription (§6.5).
    pub fn subscribe(
        &self,
        session_id: &str,
        run_id: &str,
        last_event_id: Option<u64>,
    ) -> ViewResult<SseSubscription> {
        let job = self.lookup(session_id, run_id)?;
        let mut inner = job.inner.lock().map_err(|_| ViewError::Io {
            detail: "job lock poisoned".to_string(),
        })?;
        if inner.connections >= self.config.max_sse_connections {
            return Err(ViewError::SseBusy {
                detail: format!(
                    "同一运行最多 {} 个并发 SSE 连接",
                    self.config.max_sse_connections
                ),
            });
        }
        inner.connections += 1;
        let Subscription {
            replay,
            receiver,
            terminal,
        } = inner.log.subscribe(last_event_id);
        drop(inner);
        Ok(SseSubscription {
            run_id: job.run_id.clone(),
            replay: replay
                .iter()
                .map(|event| event.to_sse(&job.run_id))
                .collect(),
            receiver,
            terminal,
            _guard: ConnectionGuard {
                job: Arc::clone(&job),
            },
        })
    }

    /// Public read-only job state for tests and diagnostics (not an HTTP
    /// polling route). Never exposes another session's job.
    pub fn job_state(&self, session_id: &str, run_id: &str) -> ViewResult<RunState> {
        Ok(self.lookup(session_id, run_id)?.state())
    }

    pub fn get_export_candidate(
        &self,
        session_id: &str,
        run_id: &str,
    ) -> ViewResult<Arc<TerminalExportCandidate>> {
        let job = self.lookup(session_id, run_id)?;
        let inner = job.inner.lock().map_err(|_| ViewError::Io {
            detail: "job lock poisoned".into(),
        })?;
        inner
            .export_candidate
            .clone()
            .ok_or_else(|| ViewError::ExportUnavailable {
                detail: "job has no verified exportable terminal observation".into(),
            })
    }

    fn lookup(&self, session_id: &str, run_id: &str) -> ViewResult<Arc<Job>> {
        let mut state = self.state.lock().map_err(|_| ViewError::Io {
            detail: "execution controller lock poisoned".to_string(),
        })?;
        self.purge_locked(&mut state);
        let job = state
            .jobs
            .get(run_id)
            .cloned()
            .ok_or_else(|| ViewError::RunNotFound {
                detail: "运行任务不存在或已过期".to_string(),
            })?;
        if job.session_id != session_id {
            return Err(ViewError::RunNotFound {
                detail: "运行任务不存在或已过期".to_string(),
            });
        }
        Ok(job)
    }

    fn purge_locked(&self, state: &mut ControllerState) {
        let now = Instant::now();
        let ttl = self.config.terminal_ttl;
        let mut keep_order = VecDeque::with_capacity(state.order.len());
        for run_id in state.order.drain(..) {
            let mut expired = false;
            if let Some(job) = state.jobs.get(&run_id) {
                if let Ok(inner) = job.inner.lock() {
                    if let Some(terminal_at) = inner.terminal_at {
                        expired = now.duration_since(terminal_at) >= ttl;
                    }
                }
            } else {
                expired = true;
            }
            if expired {
                state.jobs.remove(&run_id);
            } else {
                keep_order.push_back(run_id);
            }
        }
        state.order = keep_order;
        while state.jobs.len() > self.config.max_jobs {
            let Some(oldest) = state.order.pop_front() else {
                break;
            };
            state.jobs.remove(&oldest);
        }
        state
            .dedupe
            .retain(|_, entry| now.duration_since(entry.created) < ttl);
    }
}

fn read_trace_limited(path: &std::path::Path) -> ViewResult<Option<Vec<u8>>> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(ViewError::io("open Core Trace", err)),
    };
    let mut bytes = Vec::new();
    file.take(MAX_TRACE_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| ViewError::io("read Core Trace", err))?;
    if bytes.len() > MAX_TRACE_FILE_BYTES {
        return Err(ViewError::OutputLimit {
            detail: "Trace v2 byte limit exceeded".into(),
        });
    }
    Ok((!bytes.is_empty()).then_some(bytes))
}

fn base_result(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
) -> serde_json::Value {
    serde_json::json!({
        "document": plan.document.as_str(),
        "function_id": plan.function_id,
        "target": match &plan.target {
            ExecutionTarget::Entry => serde_json::json!({"kind": "entry"}),
            ExecutionTarget::Function { cli_name, label } => serde_json::json!({
                "kind": "function", "cli_name": cli_name, "label": label,
            }),
        },
        "mode": plan.mode.as_str(),
        "grants": {
            "read": plan.grants.allow_read,
            "write": plan.grants.allow_write,
        },
        "analysis_digest": static_doc.analysis_digest,
        "source_digest": sha256_prefixed(static_doc.source.as_bytes()),
        "duration_ms": duration_ms,
        "stale": false,
    })
}

fn build_cli_result(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
    json: &serde_json::Value,
    outcome: &ProcessOutcome,
) -> serde_json::Value {
    let mut result = base_result(plan, static_doc, duration_ms);
    result["command"] = json
        .get("command")
        .cloned()
        .unwrap_or(serde_json::json!("run"));
    result["success"] = json
        .get("success")
        .cloned()
        .unwrap_or(serde_json::json!(false));
    result["exit_code"] = serde_json::json!(outcome.exit_code);
    result["value"] = json
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let (value_type, value_text) = summarize_value(json.get("value"));
    result["value_type"] = serde_json::json!(value_type);
    result["value_text"] = serde_json::json!(value_text);
    result["stdout"] = json.get("stdout").cloned().unwrap_or(serde_json::json!(""));
    result["stderr"] = json.get("stderr").cloned().unwrap_or(serde_json::json!(""));
    result["diagnostics"] = json
        .get("diagnostics")
        .cloned()
        .unwrap_or(serde_json::json!([]));
    let success = json
        .get("success")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    result["error"] = if success {
        serde_json::Value::Null
    } else {
        let (code, message) = cli_error(json);
        serde_json::json!({"code": code, "message": message})
    };
    result["stdout_bytes"] = serde_json::json!(outcome.stdout.total);
    result["stderr_bytes"] = serde_json::json!(outcome.stderr.total);
    result
}

fn cli_error(json: &serde_json::Value) -> (String, String) {
    if let Some(error) = json.get("error") {
        if let Some(text) = error.as_str() {
            let code = json
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("core.error")
                .to_string();
            return (code, text.to_string());
        }
        if error.is_object() {
            let code = error
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("core.error")
                .to_string();
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("冻结核心报告运行失败")
                .to_string();
            return (code, message);
        }
    }
    let code = json
        .get("code")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("core.run_failed")
        .to_string();
    (code, "冻结核心报告运行失败".to_string())
}

/// Safe Wire Value summary: recognized scalars are rendered as text; everything
/// else stays inert JSON that the frontend only shows via text nodes.
fn summarize_value(value: Option<&serde_json::Value>) -> (String, String) {
    let Some(value) = value else {
        return ("none".to_string(), "无返回值".to_string());
    };
    let Some(object) = value.as_object() else {
        return (
            "unknown".to_string(),
            "未知 Wire Value（已折叠）".to_string(),
        );
    };
    let ty = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let text = match ty {
        "unit" => "Unit（无返回值）".to_string(),
        "none" => "None".to_string(),
        "bool" => format!(
            "Bool({})",
            object
                .get("value")
                .map(serde_json::Value::to_string)
                .unwrap_or_else(|| "?".to_string())
        ),
        "int" => format!(
            "Int({})",
            object
                .get("decimal")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?")
        ),
        "text" => format!(
            "Text({})",
            object
                .get("value")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
        ),
        "float" => match object.get("special").and_then(serde_json::Value::as_str) {
            Some(special) => format!("Float({special})"),
            None => format!(
                "Float({})",
                object
                    .get("value")
                    .map(serde_json::Value::to_string)
                    .unwrap_or_else(|| "?".to_string())
            ),
        },
        "list" => format!(
            "List({} 项)",
            object
                .get("items")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        ),
        "map" => format!(
            "Map({} 项)",
            object
                .get("entries")
                .and_then(serde_json::Value::as_object)
                .map(serde_json::Map::len)
                .unwrap_or(0)
        ),
        "table" => {
            let rows = object
                .get("rows")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            let cols = object
                .get("header")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            format!("Table({rows}×{cols})")
        }
        "formula" => "Formula（惰性数据）".to_string(),
        "code" => "Code（惰性数据）".to_string(),
        "function_ref" => "FunctionRef".to_string(),
        "plugin_handle" => "PluginHandle（不可传输）".to_string(),
        "memory" => "Memory（不可传输）".to_string(),
        "resource" => "Resource（不可传输）".to_string(),
        other => format!("{other}（未识别，已折叠为 JSON）"),
    };
    (ty.to_string(), text)
}

/// Verify Trace v2 against the exact pre-run Analysis bytes and project a safe
/// event summary. Missing/invalid trace never fakes execution evidence.
fn build_trace_summary(
    trace_bytes: Option<&[u8]>,
    static_doc: &StaticDocument,
    json: &serde_json::Value,
    exit_code: i32,
    stale: bool,
) -> (Option<serde_json::Value>, String) {
    let Some(trace_bytes) = trace_bytes else {
        return (None, "unavailable".to_string());
    };
    let projection = &static_doc.verified.projection;
    let verified = verify_trace(TraceVerificationInput {
        trace_bytes,
        analysis_bytes: &static_doc.analysis_bytes,
        semantic: projection,
        modules: &projection.modules,
        source: &static_doc.source,
        source_index: &static_doc.source_index,
        run_json: json,
        exit_code,
    });
    let verified = match verified {
        Ok(verified) => verified,
        Err(_) => {
            return (
                Some(serde_json::json!({
                    "status": "invalid",
                    "stale": stale,
                    "note": "Trace 与本次 Analysis 字节不匹配或格式无效；不绘制任何执行标注。",
                })),
                "invalid".to_string(),
            )
        }
    };
    let projection = verified.projection;
    let statements = static_doc.verified.projection.all_statements();
    let events = projection
        .events
        .iter()
        .map(|event| {
            let source_id = if stale {
                None
            } else {
                event.anchor_span.and_then(|span| {
                    statements
                        .iter()
                        .find(|(_, statement)| statement.span == span)
                        .map(|(_, statement)| {
                            crate::protocol::analysis_v2::source_id(
                                &static_doc.verified.projection.module_id,
                                span,
                                &format!("statement-{}", statement.kind.data_value()),
                            )
                        })
                })
            };
            serde_json::json!({
                "seq": event.seq,
                "kind": event.kind_label(),
                "function_id": event.function_id,
                "module_id": event.module_id,
                "span": event.raw_span,
                "anchor": if stale { serde_json::Value::Null } else {
                    event.anchor_span.map(|span| serde_json::json!({"start": span.start, "end": span.end}))
                        .unwrap_or(serde_json::Value::Null)
                },
                "source_id": source_id,
                "detail": event.detail,
                "operation": event.operation,
                "duration_micros": event.duration_micros,
            })
        })
        .collect::<Vec<_>>();
    let truncated = projection.truncated.as_ref().map(|truncated| {
        serde_json::json!({
            "reason": truncated.reason,
            "limit": truncated.limit,
            "written": truncated.written,
            "label": truncated.label(),
        })
    });
    let summary = serde_json::json!({
        "status": "verified",
        "analysis_digest": projection.analysis_digest,
        "core_run_id": projection.run_id,
        "run_status": projection.run_status,
        "state": projection.state.data_value(),
        "stale": stale,
        "truncated": truncated,
        "events": events,
        "counts": {
            "total": projection.counts.total(),
            "call_enter": projection.counts.call_enter,
            "call_exit": projection.counts.call_exit,
            "branch_eval": projection.counts.branch_eval,
            "loop_iteration": projection.counts.loop_iteration,
            "return": projection.counts.return_events,
            "effect": projection.counts.effect,
            "other": projection.counts.other,
            "entry": projection.entry_event_count,
            "other_modules": projection.other_event_count,
        },
        "note": "Trace v2 已按 Analysis 原始字节摘要绑定；不包含参数值、逐语句变量或秘密。",
    });
    (Some(summary), "verified".to_string())
}

fn new_run_id() -> String {
    let mut bytes = [0u8; 16];
    let random = {
        #[cfg(unix)]
        {
            use std::io::Read as _;
            std::fs::File::open("/dev/urandom")
                .and_then(|mut file| file.read_exact(&mut bytes).map(|_| ()))
                .is_ok()
        }
        #[cfg(not(unix))]
        {
            false
        }
    };
    if !random {
        let material = format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0),
            unique_counter()
        );
        bytes.copy_from_slice(&sha256_hex(material.as_bytes()).as_bytes()[..16]);
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unique_counter() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_terminal_job_is_never_returned_for_export() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/welcome");
        let service = Arc::new(DocumentService::open(&root, None).unwrap());
        let config = ExecutionConfig {
            terminal_ttl: Duration::ZERO,
            ..Default::default()
        };
        let controller = ExecutionController::new(service, config);
        let job = Arc::new(Job::new(
            "expired".into(),
            "owner".into(),
            "index.mq.md".into(),
        ));
        {
            let mut inner = job.inner.lock().unwrap();
            inner.log.publish_state(RunState::Succeeded);
            inner.terminal_at = Some(Instant::now());
        }
        {
            let mut state = controller.state.lock().unwrap();
            state.jobs.insert(job.run_id.clone(), Arc::clone(&job));
            state.order.push_back(job.run_id.clone());
        }
        assert_eq!(
            controller
                .get_export_candidate("owner", &job.run_id)
                .unwrap_err()
                .code(),
            "view.run_not_found"
        );
        assert!(controller.state.lock().unwrap().jobs.is_empty());
    }

    #[test]
    fn wire_value_summary_is_safe_and_never_html() {
        assert_eq!(
            summarize_value(Some(&serde_json::json!({"type": "int", "decimal": "10"}))),
            ("int".to_string(), "Int(10)".to_string())
        );
        assert_eq!(
            summarize_value(Some(
                &serde_json::json!({"type": "text", "value": "<b>x</b>"})
            ))
            .1,
            "Text(<b>x</b>)"
        );
        assert!(
            summarize_value(Some(&serde_json::json!({"type": "memory"})))
                .1
                .contains("不可传输")
        );
    }

    #[test]
    fn cli_errors_support_both_envelope_shapes() {
        let prepare = serde_json::json!({
            "success": false,
            "code": "sem.no_entry_function",
            "error": "no main",
        });
        assert_eq!(
            cli_error(&prepare),
            ("sem.no_entry_function".to_string(), "no main".to_string())
        );
        let runtime = serde_json::json!({
            "success": false,
            "error": {"code": "runtime.type", "message": "bad type"},
        });
        assert_eq!(
            cli_error(&runtime),
            ("runtime.type".to_string(), "bad type".to_string())
        );
    }

    #[test]
    fn run_ids_are_unguessable_and_unique() {
        let a = new_run_id();
        let b = new_run_id();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
