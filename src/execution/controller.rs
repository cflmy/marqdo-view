//! Single-job execution controller.
//!
//! One explicit `POST /api/runs` request becomes exactly one Core subprocess.
//! The controller owns the job state machine, deduplication, cancellation, the
//! bounded View event log and Observation v1 binding; it never polls and never
//! starts a second process for the same accepted job (RFC-VIEW-0002 §6.6/§6.7).
//!
//! Production consumes the live `--output jsonl` Observation stream only: every
//! stdout byte chunk goes through exactly one [`ObservationDecoder`], whose
//! normalized browser projections are published as View SSE frames. The CLI's
//! atomic `--observation-file` snapshot is requested for parity/forensics but is
//! never merged back (no double read, no second decoder).

use crate::core_client::{
    build_call_args, build_run_args, RunOptions, SandboxMode, ScratchDir,
    DEFAULT_OBSERVE_MAX_BYTES, DEFAULT_OBSERVE_MAX_EVENTS, DEFAULT_OBSERVE_MAX_VALUE_BYTES,
};
use crate::error::{ViewError, ViewResult};
use crate::execution::events::{EventLog, RunEvent, RunState, Subscription};
use crate::execution::export_candidate::TerminalExportCandidate;
use crate::execution::process::{
    run_bounded_with_hook, Captured, ProcessBudget, ProcessHooks, ProcessOutcome,
};
use crate::execution::request::{self, payload_digest, ExecutionPlan, ExecutionTarget, RunMode};
use crate::protocol::observation_v1::{
    self, ObservationDecoder, RunDisplayState, RunProjection, RunValue,
};
use crate::protocol::tool_v1;
use crate::reader::assemble::{DocumentService, StaticDocument};
use crate::reader::source::{sha256_hex, sha256_prefixed};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
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

/// One spawned execution. The decoder consumed the whole live stdout JSONL
/// stream; it is returned here so the terminal classification happens once,
/// after the child exited and the post-run source re-read.
struct CoreRun {
    process: ProcessOutcome,
    decoder: Option<ObservationDecoder>,
}

/// Everything the executor needs to publish exactly one terminal frame.
struct TerminalOutcome {
    state: RunState,
    result: serde_json::Value,
    observation_summary: Option<serde_json::Value>,
    observation: Option<RunProjection>,
    error_code: Option<String>,
    error_message: Option<String>,
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
        let terminal = self.build_terminal(&plan, &static_doc, started, outcome, cancel_requested);
        let Ok(mut inner) = job.inner.lock() else {
            return;
        };
        if inner.log.terminal() {
            return;
        }
        let mut result = terminal.result;
        if let Some(candidate) = TerminalExportCandidate::from_terminal(
            terminal.state,
            &plan,
            &static_doc,
            &result,
            terminal.observation.as_ref(),
        ) {
            result["export_preview"] = candidate.preview();
            inner.export_candidate = Some(Arc::new(candidate));
        }
        match terminal.state {
            RunState::Succeeded => {
                inner
                    .log
                    .publish_complete(result, terminal.observation_summary);
            }
            _ => {
                inner.log.publish_error(
                    terminal.state,
                    terminal.error_code.as_deref().unwrap_or("view.run_failed"),
                    terminal
                        .error_message
                        .as_deref()
                        .unwrap_or("运行失败或观察未完成"),
                    Some(result),
                    terminal.observation_summary,
                );
            }
        }
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
    ///
    /// Every stdout byte is fed into the *same* [`ObservationDecoder`] on the
    /// reader thread; each normalized browser projection is published as a View
    /// SSE frame immediately. The decoder is handed back after EOF so the
    /// terminal classification can validate the exact stream once.
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
        let observation_path = scratch.file("observation.json");
        let options = self.run_options(plan);
        let args = build_argv(plan, &entry, &observation_path, &options);

        let running_job = Arc::clone(job);
        let hook = move || {
            if let Ok(mut inner) = running_job.inner.lock() {
                if !inner.log.terminal() {
                    inner.log.publish_state(RunState::Running);
                }
            }
        };

        let (send_decoder, receive_decoder) = std::sync::mpsc::channel();
        let decoder = ObservationDecoder::new(&static_doc.verified.projection);
        let observed_job = Arc::clone(job);
        let mut hooks = ProcessHooks {
            on_spawn: Some(Box::new(hook)),
            ..Default::default()
        };
        hooks.stdout = Some(Box::new(move |stdout| {
            let mut reader = stdout;
            let mut decoder = decoder;
            let mut buffer = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        let values = decoder.push(&buffer[..read]);
                        if !values.is_empty() {
                            if let Ok(mut inner) = observed_job.inner.lock() {
                                for value in values {
                                    inner.log.publish_observation(value);
                                }
                            }
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let _ = send_decoder.send(decoder);
            Captured::default()
        }));

        let budget = ProcessBudget {
            wall_clock: self.config.run_timeout,
            ..self.config.budget.clone()
        };
        let outcome = run_bounded_with_hook(
            client.bin(),
            &args,
            self.service.project().root(),
            &budget,
            Some(Arc::clone(&job.cancel)),
            hooks,
        )?;

        // Exit code 2 is the frozen CLI rejecting the explicit argument/option
        // combination before the run started (for example
        // `strict.unsupported_grant`). View never strips a flag and retries.
        classify_exit_code(
            outcome.exit_code.unwrap_or(-1),
            &outcome.stderr.text_lossy(),
        )?;

        Ok(CoreRun {
            process: outcome,
            decoder: receive_decoder.try_recv().ok(),
        })
    }

    fn run_options(&self, plan: &ExecutionPlan) -> RunOptions {
        build_run_options(plan, &self.config)
    }

    fn build_terminal(
        &self,
        plan: &ExecutionPlan,
        static_doc: &StaticDocument,
        started: Instant,
        outcome: ViewResult<CoreRun>,
        cancel_requested: bool,
    ) -> TerminalOutcome {
        let duration_ms = started.elapsed().as_millis() as u64;
        let CoreRun {
            process: outcome,
            decoder,
        } = match outcome {
            Ok(core_run) => core_run,
            Err(err) => return error_terminal(plan, static_doc, duration_ms, &err),
        };
        if outcome.cancelled {
            return cancelled_terminal(
                plan,
                static_doc,
                duration_ms,
                &outcome,
                "运行已按用户请求取消，进程组已终止",
            );
        }
        if cancel_requested && !outcome.timed_out && !outcome.status_ok() {
            // The user cancelled; the process may have exited from SIGTERM
            // before the grace-period SIGKILL, so the adapter did not mark it
            // itself. A successful exit still wins the race (RFC §6.6).
            return cancelled_terminal(
                plan,
                static_doc,
                duration_ms,
                &outcome,
                "运行已按用户请求取消，进程组已终止",
            );
        }
        if outcome.timed_out {
            return timed_out_terminal(
                plan,
                static_doc,
                duration_ms,
                &outcome,
                self.config.run_timeout_secs(),
            );
        }
        let Some(decoder) = decoder else {
            return error_terminal(
                plan,
                static_doc,
                duration_ms,
                &ViewError::ObservationInvalid {
                    detail: "CLI 观察事件流解码器未返回终态".to_string(),
                },
            );
        };

        // Re-read the source after the run: a changed source makes the result
        // stale and forbids anchoring events to the current text.
        let current_source = self
            .service
            .project()
            .read_source(&plan.document)
            .unwrap_or_default();
        let stale = sha256_hex(&current_source) != static_doc.source_sha256;
        let process_stderr = outcome.stderr.text_lossy();
        let exit_code = outcome.exit_code.unwrap_or(-1);
        match decoder.finish(exit_code, &process_stderr, stale) {
            Ok(projection) => terminal_from_projection(plan, static_doc, duration_ms, projection),
            Err(err) => error_terminal(plan, static_doc, duration_ms, &err),
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

/// Build the exact argv for one already-approved execution. Kept pure so a unit
/// test can prove retired Trace/Events flags can never appear again.
fn build_argv(
    plan: &ExecutionPlan,
    entry: &Path,
    observation_file: &Path,
    options: &RunOptions,
) -> Vec<OsString> {
    match &plan.target {
        ExecutionTarget::Entry => build_run_args(entry, observation_file, options),
        ExecutionTarget::Function { cli_name, .. } => {
            build_call_args(entry, cli_name, &plan.arguments, observation_file, options)
        }
    }
}

fn build_run_options(plan: &ExecutionPlan, config: &ExecutionConfig) -> RunOptions {
    RunOptions {
        mode: match plan.mode {
            RunMode::Pure => SandboxMode::Pure,
            RunMode::Project => SandboxMode::Project,
        },
        grants: plan.grants.clone(),
        fuel: Some(DEFAULT_FUEL),
        stack: None,
        max_output: Some(DEFAULT_MAX_OUTPUT),
        timeout_seconds: Some(config.run_timeout_secs()),
        view_timeout: Some(config.run_timeout),
        observe_max_events: DEFAULT_OBSERVE_MAX_EVENTS,
        observe_max_bytes: DEFAULT_OBSERVE_MAX_BYTES,
        observe_max_value_bytes: DEFAULT_OBSERVE_MAX_VALUE_BYTES,
        // View requests no variable/parameter values unless a future explicit
        // consent path is added; text values additionally need the second
        // confirmation. Empty here means the CLI records types only.
        observe_variables: Vec::new(),
        observe_parameters: Vec::new(),
        observe_text_values: false,
    }
}

/// Exit code 2 means the frozen CLI refused the explicit option combination
/// before any execution. This is a hard View error with the CLI stderr text;
/// View never removes an argument and retries.
fn classify_exit_code(exit_code: i32, stderr: &str) -> ViewResult<()> {
    if exit_code != 2 {
        return Ok(());
    }
    let text: String = stderr.trim().chars().take(400).collect();
    let detail = if text.is_empty() {
        "冻结核心以 exit 2 拒绝了本次运行参数；View 不会移除参数重试。".to_string()
    } else {
        format!("冻结核心以 exit 2 拒绝了本次运行参数；View 不会移除参数重试：{text}")
    };
    Err(ViewError::CoreOptionRejected { detail })
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

/// One terminal result where no Observation projection could be produced
/// (spawn failure, invalid/stale stream, timeout, cancellation). It can never
/// be success and never has complete output.
fn unavailable_result(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
    observation_status: &str,
    stale: bool,
) -> serde_json::Value {
    let mut result = base_result(plan, static_doc, duration_ms);
    result["success"] = serde_json::json!(false);
    result["command"] = serde_json::json!(command_for(plan));
    result["observation_status"] = serde_json::json!(observation_status);
    result["observation_protocol"] = serde_json::json!(tool_v1::OBSERVATION_REVISION);
    result["observation_truncated"] = serde_json::json!(false);
    result["capture_level"] = serde_json::json!("");
    result["core_run_id"] = serde_json::Value::Null;
    result["source_verified"] = serde_json::json!(false);
    result["output_complete"] = serde_json::json!(false);
    result["stale"] = serde_json::json!(stale);
    result["value"] = serde_json::Value::Null;
    result["value_type"] = serde_json::json!("none");
    result["value_text"] = serde_json::json!("无返回值");
    result["stdout"] = serde_json::json!("");
    result["stderr"] = serde_json::json!("");
    result["stdout_bytes"] = serde_json::json!(0);
    result["stderr_bytes"] = serde_json::json!(0);
    result["exit_code"] = serde_json::Value::Null;
    result["diagnostics"] = serde_json::json!([]);
    result["warnings"] = serde_json::json!([]);
    result
}

fn command_for(plan: &ExecutionPlan) -> &'static str {
    match plan.target {
        ExecutionTarget::Entry => "run",
        ExecutionTarget::Function { .. } => "call",
    }
}

fn error_terminal_state(err: &ViewError) -> (RunState, &'static str, bool) {
    match err.code() {
        "view.observation_stale" => (RunState::Failed, "stale", true),
        "view.observation_invalid" => (RunState::Failed, "invalid", false),
        "view.core_option_rejected" => (RunState::Failed, "unavailable", false),
        _ => (RunState::Unavailable, "unavailable", false),
    }
}

fn error_terminal(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
    err: &ViewError,
) -> TerminalOutcome {
    let (state, observation_status, stale) = error_terminal_state(err);
    let mut result = unavailable_result(plan, static_doc, duration_ms, observation_status, stale);
    result["exit_code"] = if err.code() == "view.core_option_rejected" {
        serde_json::json!(2)
    } else {
        serde_json::Value::Null
    };
    result["error"] = serde_json::json!({
        "code": err.code(),
        "message": err.detail(),
    });
    let (observation, observation_summary) = if err.code() == "view.observation_stale" {
        let projection = observation_v1::stale_projection(&static_doc.analysis_digest);
        let summary =
            observation_v1::summary_json(&projection, &static_doc.verified.projection, true);
        (Some(projection), Some(summary))
    } else {
        (None, None)
    };
    TerminalOutcome {
        state,
        result,
        observation_summary,
        observation,
        error_code: Some(err.code().to_string()),
        error_message: Some(err.detail().to_string()),
    }
}

fn cancelled_terminal(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
    outcome: &ProcessOutcome,
    message: &str,
) -> TerminalOutcome {
    let mut result = unavailable_result(plan, static_doc, duration_ms, "unavailable", false);
    result["exit_code"] = serde_json::json!(outcome.exit_code);
    result["stdout"] = serde_json::json!(outcome.stdout.text_lossy());
    result["stderr"] = serde_json::json!(outcome.stderr.text_lossy());
    result["stdout_bytes"] = serde_json::json!(outcome.stdout.total);
    result["stderr_bytes"] = serde_json::json!(outcome.stderr.total);
    result["error"] = serde_json::json!({
        "code": "view.run_cancelled",
        "message": message,
    });
    TerminalOutcome {
        state: RunState::Cancelled,
        result,
        observation_summary: None,
        observation: None,
        error_code: Some("view.run_cancelled".to_string()),
        error_message: Some(message.to_string()),
    }
}

fn timed_out_terminal(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
    outcome: &ProcessOutcome,
    run_timeout_secs: u64,
) -> TerminalOutcome {
    let message = format!("运行超过 {run_timeout_secs} 秒墙钟上限，进程组已终止");
    let mut result = unavailable_result(plan, static_doc, duration_ms, "unavailable", false);
    result["exit_code"] = serde_json::json!(outcome.exit_code);
    result["stdout"] = serde_json::json!(outcome.stdout.text_lossy());
    result["stderr"] = serde_json::json!(outcome.stderr.text_lossy());
    result["stdout_bytes"] = serde_json::json!(outcome.stdout.total);
    result["stderr_bytes"] = serde_json::json!(outcome.stderr.total);
    result["error"] = serde_json::json!({
        "code": "view.core_timeout",
        "message": message,
    });
    TerminalOutcome {
        state: RunState::TimedOut,
        result,
        observation_summary: None,
        observation: None,
        error_code: Some("view.core_timeout".to_string()),
        error_message: Some(message),
    }
}

/// Map a verified Observation display state to the View-owned terminal state.
/// Truncation is a *successful terminal* (the run finished) but the result is
/// explicitly marked incomplete; stale/invalid never become success.
fn terminal_state_for(state: RunDisplayState) -> RunState {
    match state {
        RunDisplayState::Succeeded | RunDisplayState::Truncated => RunState::Succeeded,
        RunDisplayState::Failed => RunState::Failed,
        RunDisplayState::Incomplete => RunState::Incomplete,
        // Stale/Unsupported/NotRun/StaticOnly/Running are never produced by a
        // finished JSONL stream; treat any of them as an unusable terminal.
        RunDisplayState::Stale
        | RunDisplayState::Unsupported
        | RunDisplayState::NotRun
        | RunDisplayState::StaticOnly
        | RunDisplayState::Running => RunState::Unavailable,
    }
}

/// Observation binding status exposed to the browser. A failed *run* can still
/// have a fully verified observation stream; truncation is called out first.
fn observation_status_for(projection: &RunProjection) -> &'static str {
    if projection.truncated.is_some() {
        "truncated"
    } else {
        match projection.state {
            RunDisplayState::Succeeded | RunDisplayState::Failed => "verified",
            _ => "invalid",
        }
    }
}

/// Truncated observations are terminal but never complete.
fn output_is_complete(projection: &RunProjection) -> bool {
    projection.output_complete && projection.truncated.is_none()
}

fn terminal_from_projection(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
    projection: RunProjection,
) -> TerminalOutcome {
    let state = terminal_state_for(projection.state);
    let observation_status = observation_status_for(&projection);
    let result = observation_result(
        plan,
        static_doc,
        duration_ms,
        &projection,
        observation_status,
    );
    let summary = observation_v1::summary_json(&projection, &static_doc.verified.projection, false);
    let (error_code, error_message) = match state {
        RunState::Failed => (
            Some(
                projection
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "view.run_failed".to_string()),
            ),
            Some(first_warning(&projection)),
        ),
        RunState::Incomplete => (
            Some(
                projection
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "view.observation_incomplete".to_string()),
            ),
            Some(first_warning(&projection)),
        ),
        _ => (None, None),
    };
    TerminalOutcome {
        state,
        result,
        observation_summary: Some(summary),
        observation: Some(projection),
        error_code,
        error_message,
    }
}

fn first_warning(projection: &RunProjection) -> String {
    projection
        .warnings
        .first()
        .cloned()
        .unwrap_or_else(|| "运行未成功完成".to_string())
}

fn observation_result(
    plan: &ExecutionPlan,
    static_doc: &StaticDocument,
    duration_ms: u64,
    projection: &RunProjection,
    observation_status: &str,
) -> serde_json::Value {
    let mut result = base_result(plan, static_doc, duration_ms);
    result["command"] = serde_json::json!(command_for(plan));
    result["success"] = serde_json::json!(projection.is_success());
    result["exit_code"] = serde_json::json!(projection.exit_code);
    let (value, value_type, value_text) = observed_value(projection.result_value.as_ref());
    result["value"] = value;
    result["value_type"] = serde_json::json!(value_type);
    result["value_text"] = serde_json::json!(value_text);
    result["stdout"] = serde_json::json!(projection.stdout);
    result["stderr"] = serde_json::json!(projection.stderr);
    result["stdout_bytes"] = serde_json::json!(projection.stdout.len());
    result["stderr_bytes"] = serde_json::json!(projection.stderr.len());
    result["diagnostics"] = serde_json::json!([]);
    result["error"] = match projection.state {
        RunDisplayState::Failed | RunDisplayState::Incomplete => {
            let code = projection
                .error_code
                .clone()
                .unwrap_or_else(|| "view.run_failed".to_string());
            serde_json::json!({"code": code, "message": first_warning(projection)})
        }
        _ => serde_json::Value::Null,
    };
    result["core_run_id"] = serde_json::json!(projection.run_id);
    result["capture_level"] = serde_json::json!(projection.capture_level);
    result["observation_status"] = serde_json::json!(observation_status);
    result["observation_protocol"] = serde_json::json!(tool_v1::OBSERVATION_REVISION);
    result["observation_truncated"] = serde_json::json!(projection.truncated.is_some());
    result["source_verified"] = serde_json::json!(projection.source_verified);
    result["output_complete"] = serde_json::json!(output_is_complete(projection));
    result["warnings"] = serde_json::json!(projection.warnings);
    result
}

/// Safe Wire Value summary: recognized scalars are rendered as text; everything
/// else stays inert. The decoder already applied value consent, so View never
/// re-derives or widens what the user requested.
fn observed_value(value: Option<&RunValue>) -> (serde_json::Value, String, String) {
    match value {
        None => (
            serde_json::Value::Null,
            "none".to_string(),
            "无返回值".to_string(),
        ),
        Some(value) => (
            serde_json::Value::Null,
            value.ty.clone(),
            value.display.clone(),
        ),
    }
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
    use crate::core_client::Grants;
    use crate::protocol::observation_v1::{EventCounts, Truncation};
    use crate::reader::source::SafeRelativePath;

    fn plan() -> ExecutionPlan {
        ExecutionPlan {
            document: SafeRelativePath::parse("index.mq.md").expect("valid relative path"),
            target: ExecutionTarget::Entry,
            function_id: "root:t@0.1.0::index.mq.md::main".to_string(),
            arguments: Vec::new(),
            mode: RunMode::Pure,
            grants: Grants::default(),
            analysis_digest: format!("sha256:{}", "a".repeat(64)),
            client_request_id: "req-1".to_string(),
        }
    }

    fn projection(state: RunDisplayState, truncated: bool) -> RunProjection {
        RunProjection {
            run_id: "core-run".to_string(),
            entry: None,
            run_status: "ok".to_string(),
            state,
            error_code: None,
            analysis_digest: format!("sha256:{}", "a".repeat(64)),
            source_verified: true,
            events: Vec::new(),
            truncated: truncated.then(|| Truncation {
                reason: "event_budget".to_string(),
                limit: 10,
                written: 10,
                dropped: Some(1),
            }),
            capture_level: "structural".to_string(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            result_value: None,
            counts: EventCounts::default(),
            entry_event_count: 0,
            other_event_count: 0,
            warnings: Vec::new(),
            output_complete: true,
        }
    }

    fn argv_text(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn exit_code_two_is_a_hard_option_rejection_never_a_retry() {
        assert!(classify_exit_code(0, "").is_ok());
        assert!(classify_exit_code(1, "error: refusing to run").is_ok());
        let err = classify_exit_code(
            2,
            "error[strict.unsupported_grant]: strict mode rejects live destinations\n",
        )
        .expect_err("exit 2 must be a hard error");
        assert_eq!(err.code(), "view.core_option_rejected");
        assert!(err.detail().contains("strict.unsupported_grant"));
        assert!(err.detail().contains("不会移除参数重试"));
    }

    #[test]
    fn run_argv_is_observation_jsonl_and_never_retired_trace_flags() {
        let plan = plan();
        let args = build_argv(
            &plan,
            Path::new("/p/index.mq.md"),
            Path::new("/tmp/observation.json"),
            &build_run_options(&plan, &ExecutionConfig::default()),
        );
        let argv = argv_text(&args);
        assert_eq!(argv[0], "run");
        assert!(argv.contains(&"--output".to_string()));
        assert!(argv.contains(&"jsonl".to_string()));
        assert!(argv.contains(&"--observation-file".to_string()));
        for retired in [
            "--trace-file",
            "--event-stream",
            "--trace-max-events",
            "--trace-max-bytes",
            "--json",
        ] {
            assert!(
                !argv.contains(&retired.to_string()),
                "retired flag {retired} must never be built"
            );
        }
        assert!(!argv
            .iter()
            .any(|arg| arg.contains(';') || arg.contains('|')));
    }

    #[test]
    fn call_argv_carries_explicit_args_and_observation_file() {
        let mut plan = plan();
        plan.target = ExecutionTarget::Function {
            cli_name: "f".to_string(),
            label: "N.f".to_string(),
        };
        plan.arguments = vec![("n".to_string(), "5".to_string())];
        let args = build_argv(
            &plan,
            Path::new("/p/index.mq.md"),
            Path::new("/tmp/observation.json"),
            &build_run_options(&plan, &ExecutionConfig::default()),
        );
        let argv = argv_text(&args);
        assert_eq!(argv[0], "call");
        assert!(argv.contains(&"f".to_string()));
        assert!(argv.contains(&"n=5".to_string()));
        assert!(argv.contains(&"--observation-file".to_string()));
    }

    #[test]
    fn run_options_never_request_variable_values() {
        let options = build_run_options(&plan(), &ExecutionConfig::default());
        assert!(options.observe_variables.is_empty());
        assert!(options.observe_parameters.is_empty());
        assert!(!options.observe_text_values);
        assert_eq!(options.observe_max_events, DEFAULT_OBSERVE_MAX_EVENTS);
        assert_eq!(options.observe_max_bytes, DEFAULT_OBSERVE_MAX_BYTES);
    }

    #[test]
    fn truncated_observation_is_terminal_but_never_labelled_complete() {
        let truncated = projection(RunDisplayState::Truncated, true);
        assert_eq!(
            terminal_state_for(truncated.state),
            RunState::Succeeded,
            "a truncated observation still has a real terminal"
        );
        assert_eq!(observation_status_for(&truncated), "truncated");
        assert!(!output_is_complete(&truncated));

        let complete = projection(RunDisplayState::Succeeded, false);
        assert_eq!(terminal_state_for(complete.state), RunState::Succeeded);
        assert_eq!(observation_status_for(&complete), "verified");
        assert!(output_is_complete(&complete));
    }

    #[test]
    fn stale_and_incomplete_never_map_to_success() {
        assert_eq!(
            terminal_state_for(RunDisplayState::Stale),
            RunState::Unavailable
        );
        assert_eq!(
            terminal_state_for(RunDisplayState::Incomplete),
            RunState::Incomplete
        );
        assert_eq!(
            observation_status_for(&projection(RunDisplayState::Incomplete, false)),
            "invalid"
        );
        let stale = ViewError::ObservationStale {
            detail: "source changed during the run".to_string(),
        };
        assert_eq!(
            error_terminal_state(&stale),
            (RunState::Failed, "stale", true)
        );
        let invalid = ViewError::ObservationInvalid {
            detail: "mixed run_id".to_string(),
        };
        assert_eq!(
            error_terminal_state(&invalid),
            (RunState::Failed, "invalid", false)
        );
        let rejected = ViewError::CoreOptionRejected {
            detail: "exit 2".to_string(),
        };
        assert_eq!(
            error_terminal_state(&rejected),
            (RunState::Failed, "unavailable", false)
        );
    }

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
    fn run_ids_are_unguessable_and_unique() {
        let a = new_run_id();
        let b = new_run_id();
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
