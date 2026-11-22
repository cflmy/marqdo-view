//! The only way View talks to the frozen CLI.
//!
//! Invariants (RFC-VIEW-0001 §4.2, §10.3, §14.1):
//! * absolute `MARQDO_BIN`, argv arrays, never `sh -c`, never shell strings;
//! * `check --emit` is the only static channel; `run --trace-file --json` the
//!   only dynamic one; private Core IR dumps are never used;
//! * stdout/stderr/exit status are handled separately; a stderr JSON blob is
//!   never promoted to a successful Analysis;
//! * temporary artifacts live in a private 0700 directory and are removed on
//!   drop.

use crate::error::{ViewError, ViewResult};
use crate::project::{Project, SafeRelativePath};
use crate::source_map::{sha256_hex, sha256_prefixed};
use crate::version::{self, CoreVersion};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Default View-level wall clock cap for one Core `run`.
pub const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(120);
/// Default View-level wall clock cap for one Core `check`.
pub const DEFAULT_CHECK_TIMEOUT: Duration = Duration::from_secs(120);

const ENV_MARQDO_BIN: &str = "MARQDO_BIN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    Pure,
    Project,
    Strict,
}

impl SandboxMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxMode::Pure => "pure",
            SandboxMode::Project => "project",
            SandboxMode::Strict => "strict",
        }
    }
}

/// Explicit grants assembled only from a user-approved run request. View never
/// reads project capability declarations to derive grants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Grants {
    pub allow_read: Vec<String>,
    pub allow_write: Vec<String>,
    pub allow_net: Vec<String>,
    pub allow_env: Vec<String>,
    pub allow_process: Vec<String>,
    pub allow_random: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// `--refs` also validates Markdown link targets; it is a stricter check
    /// and may fail where plain syntax succeeds.
    pub refs: bool,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub mode: SandboxMode,
    pub grants: Grants,
    pub fuel: Option<u64>,
    pub stack: Option<u64>,
    pub max_output: Option<u64>,
    pub timeout_seconds: Option<u64>,
    pub view_timeout: Option<Duration>,
    pub trace_max_events: Option<u64>,
    pub trace_max_bytes: Option<u64>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            mode: SandboxMode::Pure,
            grants: Grants::default(),
            fuel: None,
            stack: None,
            max_output: None,
            timeout_seconds: None,
            view_timeout: Some(DEFAULT_RUN_TIMEOUT),
            trace_max_events: None,
            trace_max_bytes: None,
        }
    }
}

#[derive(Debug)]
pub struct CheckOutput {
    /// Exact bytes written by `check --emit`, the digest source of truth.
    pub analysis_bytes: Vec<u8>,
    /// Lowercase hex SHA-256 of `analysis_bytes`.
    pub analysis_sha256: String,
    /// Prefixed digest (`sha256:...`) for comparison with Trace v2.
    pub analysis_digest: String,
    pub stdout: String,
    pub stderr: String,
}

/// One compile diagnostic from `check --json`, normalized so no absolute
/// machine path can leak into pages, exports or logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawCheckDiagnostic {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub severity: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub column: Option<u64>,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub hints: Vec<String>,
}

/// Result of one `check` invocation. A failed check is **not** an I/O error:
/// the prose stays readable and diagnostics must be shown (RFC §12.3).
#[derive(Debug)]
pub struct CheckResponse {
    pub success: bool,
    pub analysis: Option<CheckOutput>,
    pub diagnostics: Vec<RawCheckDiagnostic>,
    pub exit_code: i32,
    pub stderr: String,
}

#[derive(Debug)]
pub struct RunOutput {
    /// Exactly one JSON document emitted on stdout by the Core.
    pub json: serde_json::Value,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    /// Raw Trace v2 bytes when the Core wrote them (even for a failed run).
    pub trace_bytes: Option<Vec<u8>>,
    pub trace_sha256: Option<String>,
}

/// A private temporary directory for Core artifacts.
pub struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    pub fn create(label: &str) -> ViewResult<Self> {
        let base = std::env::temp_dir();
        let unique = format!(
            "marqdo-view-{}-{}-{}",
            std::process::id(),
            unique_counter(),
            sanitize_label(label)
        );
        let path = base.join(unique);
        std::fs::create_dir_all(&path)
            .map_err(|err| ViewError::io(&format!("create scratch {}", path.display()), err))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
                .map_err(|err| ViewError::io(&format!("chmod scratch {}", path.display()), err))?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn unique_counter() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    seq ^ nanos.rotate_left(17)
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .take(32)
        .collect()
}

/// Negotiated public-client for one frozen core binary.
#[derive(Debug, Clone)]
pub struct CoreClient {
    bin: PathBuf,
    protocol_env: ProtocolEnv,
}

#[derive(Debug, Clone, Default)]
struct ProtocolEnv {
    tool: Option<String>,
    analysis: Option<String>,
    trace: Option<String>,
}

impl CoreClient {
    /// Build a client from `--marqdo-bin` (development) or `MARQDO_BIN`
    /// (installed Tool Protocol). Never guesses a `PATH` binary: a wrong
    /// version must fail loudly, not silently.
    pub fn from_env(explicit: Option<&Path>) -> ViewResult<Self> {
        let bin = locate_bin(explicit)?;
        Self::with_bin(bin)
    }

    /// Construct from an explicit binary path, still reading any negotiated
    /// protocol environment. Useful for `--marqdo-bin` and integration tests;
    /// production dispatch should prefer [`CoreClient::from_env`].
    pub fn with_bin(bin: impl Into<PathBuf>) -> ViewResult<Self> {
        let bin = validate_bin(&bin.into(), "core binary")?;
        Ok(Self {
            bin,
            protocol_env: ProtocolEnv {
                tool: std::env::var(version::ENV_TOOL_PROTOCOL).ok(),
                analysis: std::env::var(version::ENV_PROTOCOL_ANALYSIS).ok(),
                trace: std::env::var(version::ENV_PROTOCOL_TRACE).ok(),
            },
        })
    }

    /// Test-only constructor with an explicit binary and no environment
    /// negotiation input.
    #[cfg(test)]
    pub fn for_test(bin: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            protocol_env: ProtocolEnv::default(),
        }
    }

    pub fn bin(&self) -> &Path {
        &self.bin
    }

    /// Run `--version`, validate the product range and any negotiated protocol
    /// environment variables. A missing Development-mode env is allowed; an
    /// unknown major is a hard error.
    pub fn negotiate(&self) -> ViewResult<CoreVersion> {
        version::negotiate_protocol_env(
            self.protocol_env.tool.as_deref(),
            self.protocol_env.analysis.as_deref(),
            self.protocol_env.trace.as_deref(),
        )?;
        let output = self.execute(
            &[OsString::from("--version")],
            &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            DEFAULT_CHECK_TIMEOUT,
        )?;
        if output.exit_code != 0 {
            return Err(ViewError::CoreMissing {
                detail: format!(
                    "{} --version failed (exit {}): {}",
                    self.bin.display(),
                    output.exit_code,
                    output.stderr.trim()
                ),
            });
        }
        let version = version::parse_version_output(&output.stdout)?;
        if !version::product_is_compatible(&version.version) {
            return Err(ViewError::ProtocolIncompatible {
                detail: format!(
                    "core {} is outside the supported range {}",
                    version.version,
                    version::PRODUCT_RANGE
                ),
            });
        }
        Ok(version)
    }

    /// `check --json --emit`. On success the exact Analysis v2 bytes are
    /// returned; on failure the structured diagnostics are returned without
    /// any absolute path. Only spawn/timeout/IO failures are hard errors.
    pub fn check(
        &self,
        project: &Project,
        relative: &SafeRelativePath,
        options: &CheckOptions,
    ) -> ViewResult<CheckResponse> {
        let entry = project.absolute_for(relative)?;
        let scratch = ScratchDir::create("check")?;
        let emit = scratch.file("analysis.json");
        let mut args = build_check_args(&entry);
        args.push(OsString::from("--json"));
        args.push(OsString::from("--emit"));
        args.push(emit.clone().into_os_string());
        if options.refs {
            args.push(OsString::from("--refs"));
        }

        let output = self.execute(
            &args,
            project.root(),
            options.timeout.unwrap_or(DEFAULT_CHECK_TIMEOUT),
        )?;
        let check_json: Option<serde_json::Value> =
            serde_json::from_slice(&output.stdout_bytes).ok();
        let success = output.exit_code == 0
            && check_json
                .as_ref()
                .and_then(|json| json.get("success"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
        let diagnostics = check_json
            .as_ref()
            .and_then(|json| json.get("diagnostics"))
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| serde_json::from_value(item.clone()).ok())
                    .collect::<Vec<RawCheckDiagnostic>>()
            })
            .unwrap_or_default();
        if !success {
            return Ok(CheckResponse {
                success: false,
                analysis: None,
                diagnostics,
                exit_code: output.exit_code,
                stderr: output.stderr,
            });
        }
        let analysis_bytes = std::fs::read(&emit).map_err(|err| ViewError::AnalysisFailed {
            detail: format!(
                "`marqdo check` succeeded but emitted no Analysis at {}: {err}",
                emit.display()
            ),
        })?;
        if analysis_bytes.is_empty() {
            return Err(ViewError::AnalysisFailed {
                detail: "`marqdo check` emitted an empty Analysis file".to_string(),
            });
        }
        Ok(CheckResponse {
            success: true,
            analysis: Some(CheckOutput {
                analysis_sha256: sha256_hex(&analysis_bytes),
                analysis_digest: sha256_prefixed(&analysis_bytes),
                analysis_bytes,
                stdout: output.stdout,
                stderr: output.stderr,
            }),
            diagnostics,
            exit_code: 0,
            stderr: String::new(),
        })
    }

    /// `run --trace-file --json`, only after an explicit user confirmation in
    /// the calling layer. Returns the CLI JSON document and raw Trace bytes.
    pub fn run(
        &self,
        project: &Project,
        relative: &SafeRelativePath,
        options: &RunOptions,
    ) -> ViewResult<RunOutput> {
        let entry = project.absolute_for(relative)?;
        let scratch = ScratchDir::create("run")?;
        let trace_path = scratch.file("trace.json");
        let args = build_run_args(&entry, &trace_path, options);
        let output = self.execute(
            &args,
            project.root(),
            options.view_timeout.unwrap_or(DEFAULT_RUN_TIMEOUT),
        )?;

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout_bytes).map_err(|err| ViewError::Io {
                detail: format!(
                    "`marqdo run --json` did not emit exactly one JSON document: {err}; stderr: {}",
                    summarize_stderr(&output.stderr)
                ),
            })?;
        if !json.is_object() {
            return Err(ViewError::Io {
                detail: "`marqdo run --json` emitted a non-object JSON document".to_string(),
            });
        }
        let trace_bytes = match std::fs::read(&trace_path) {
            Ok(bytes) => {
                if bytes.is_empty() {
                    None
                } else {
                    Some(bytes)
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
            Err(err) => {
                return Err(ViewError::io(
                    &format!("read trace {}", trace_path.display()),
                    err,
                ))
            }
        };
        Ok(RunOutput {
            json,
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
            trace_sha256: trace_bytes.as_deref().map(sha256_hex),
            trace_bytes,
        })
    }

    /// Spawn the core with an explicit argv, private cwd and a wall-clock cap.
    fn execute(&self, args: &[OsString], cwd: &Path, timeout: Duration) -> ViewResult<ExecOutput> {
        let mut command = Command::new(&self.bin);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Put the core in its own process group so a View-level timeout kills
        // the whole subtree, not just the immediate shell-less child.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: `setpgid` is async-signal-safe and only touches the child
            // before exec.
            unsafe {
                command.pre_exec(|| {
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let mut child = command.spawn().map_err(|err| ViewError::CoreSpawn {
            detail: format!("spawn {}: {err}", self.bin.display()),
        })?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stdout_thread = std::thread::spawn(move || read_all(stdout));
        let stderr_thread = std::thread::spawn(move || read_all(stderr));

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        kill_process_tree(&mut child);
                        let _ = child.wait();
                        let _ = stdout_thread.join();
                        let _ = stderr_thread.join();
                        return Err(ViewError::CoreTimeout {
                            detail: format!(
                                "core did not finish within {}s and was killed",
                                timeout.as_secs()
                            ),
                        });
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(err) => {
                    let _ = child.kill();
                    return Err(ViewError::CoreSpawn {
                        detail: format!("wait for {}: {err}", self.bin.display()),
                    });
                }
            }
        };

        let stdout_bytes = stdout_thread.join().unwrap_or_default();
        let stderr_bytes = stderr_thread.join().unwrap_or_default();
        let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
        let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
        Ok(ExecOutput {
            exit_code: status.code().unwrap_or(-1),
            stdout_bytes,
            stdout,
            stderr,
        })
    }
}

struct ExecOutput {
    exit_code: i32,
    stdout_bytes: Vec<u8>,
    stdout: String,
    stderr: String,
}

/// Kill the core and every descendant in its process group (Unix), or just the
/// direct child elsewhere. Never leaves a runaway subprocess behind.
fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        if pid > 0 {
            // SAFETY: negative pid targets the process group created by
            // pre_exec(setpgid); ESRCH is harmless when it already exited.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}

fn read_all(mut reader: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = reader.read_to_end(&mut bytes);
    bytes
}

/// Build the base `check` arguments. The caller appends `--emit <path>` and
/// optionally `--refs`; this function exists so the argv shape is unit-testable.
fn build_check_args(entry: &Path) -> Vec<OsString> {
    vec![OsString::from("check"), entry.as_os_str().to_os_string()]
}

/// Build `run` arguments from an already user-approved request.
pub fn build_run_args(entry: &Path, trace_path: &Path, options: &RunOptions) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("run"),
        entry.as_os_str().to_os_string(),
        OsString::from("--mode"),
        OsString::from(options.mode.as_str()),
        OsString::from("--json"),
        OsString::from("--trace-file"),
        trace_path.as_os_str().to_os_string(),
    ];
    fn push_pair(args: &mut Vec<OsString>, flag: &str, value: &str) {
        args.push(OsString::from(flag));
        args.push(OsString::from(value));
    }
    for glob in &options.grants.allow_read {
        push_pair(&mut args, "--allow-read", glob);
    }
    for glob in &options.grants.allow_write {
        push_pair(&mut args, "--allow-write", glob);
    }
    for host in &options.grants.allow_net {
        push_pair(&mut args, "--allow-net", host);
    }
    for name in &options.grants.allow_env {
        push_pair(&mut args, "--allow-env", name);
    }
    for program in &options.grants.allow_process {
        push_pair(&mut args, "--allow-process", program);
    }
    if options.grants.allow_random {
        args.push(OsString::from("--allow-random"));
    }
    if let Some(fuel) = options.fuel {
        push_pair(&mut args, "--fuel", &fuel.to_string());
    }
    if let Some(stack) = options.stack {
        push_pair(&mut args, "--stack", &stack.to_string());
    }
    if let Some(max_output) = options.max_output {
        push_pair(&mut args, "--max-output", &max_output.to_string());
    }
    if let Some(timeout) = options.timeout_seconds {
        push_pair(&mut args, "--timeout", &timeout.to_string());
    }
    if let Some(max_events) = options.trace_max_events {
        push_pair(&mut args, "--trace-max-events", &max_events.to_string());
    }
    if let Some(max_bytes) = options.trace_max_bytes {
        push_pair(&mut args, "--trace-max-bytes", &max_bytes.to_string());
    }
    args
}

fn locate_bin(explicit: Option<&Path>) -> ViewResult<PathBuf> {
    if let Some(path) = explicit {
        return validate_bin(path, "--marqdo-bin");
    }
    if let Some(raw) = std::env::var_os(ENV_MARQDO_BIN) {
        if raw.is_empty() {
            return Err(ViewError::CoreMissing {
                detail: format!("{ENV_MARQDO_BIN} is set but empty"),
            });
        }
        return validate_bin(Path::new(&raw), ENV_MARQDO_BIN);
    }
    Err(ViewError::CoreMissing {
        detail: format!(
            "{ENV_MARQDO_BIN} is not set. Install this tool with `marqdo tool install <bundle>/tool.json`, \
             or pass --marqdo-bin <path> when running marqdo-view directly."
        ),
    })
}

fn validate_bin(path: &Path, origin: &str) -> ViewResult<PathBuf> {
    let canonical = path.canonicalize().map_err(|err| ViewError::CoreMissing {
        detail: format!("{origin} points to {}: {err}", path.display()),
    })?;
    let metadata = std::fs::metadata(&canonical).map_err(|err| ViewError::CoreMissing {
        detail: format!("{origin} points to {}: {err}", canonical.display()),
    })?;
    if !metadata.is_file() {
        return Err(ViewError::CoreMissing {
            detail: format!("{origin} {} is not a regular file", canonical.display()),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(ViewError::CoreMissing {
                detail: format!("{origin} {} is not executable", canonical.display()),
            });
        }
    }
    Ok(canonical)
}

fn summarize_stderr(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        "(no stderr)".to_string()
    } else {
        let mut text: String = trimmed.chars().take(1200).collect();
        if trimmed.chars().count() > 1200 {
            text.push('…');
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_args_are_explicit_argv_never_shell() {
        let args = build_run_args(
            Path::new("/p/index.mq.md"),
            Path::new("/t/trace.json"),
            &RunOptions {
                mode: SandboxMode::Pure,
                grants: Grants {
                    allow_read: vec!["data/**".into()],
                    allow_net: vec!["example.com".into()],
                    ..Default::default()
                },
                fuel: Some(1000),
                trace_max_events: Some(20),
                ..Default::default()
            },
        );
        let text: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        assert_eq!(text[0], "run");
        assert_eq!(text[1], "/p/index.mq.md");
        assert!(text.contains(&"pure".to_string()));
        assert!(text.contains(&"--json".to_string()));
        assert!(text.contains(&"--trace-file".to_string()));
        assert!(text.contains(&"data/**".to_string()));
        assert!(text.contains(&"example.com".to_string()));
        assert!(text.contains(&"1000".to_string()));
        assert!(text.contains(&"20".to_string()));
        // No token is a shell fragment: every argument is a separate value.
        assert!(!text
            .iter()
            .any(|t| t.contains(';') || t.contains('|') || t.contains('&')));
    }

    #[test]
    fn check_args_are_plain_argv() {
        let args = build_check_args(Path::new("/p/index.mq.md"));
        assert_eq!(args[0], OsString::from("check"));
        assert_eq!(args[1], OsString::from("/p/index.mq.md"));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn missing_bin_has_stable_code() {
        let err = locate_bin(Some(Path::new("/definitely/not/here/marqdo"))).unwrap_err();
        assert_eq!(err.code(), "view.core_missing");
        assert_eq!(err.exit_code(), 2);
    }

    #[test]
    fn scratch_dir_is_private_and_removed() {
        let scratch = ScratchDir::create("unit").unwrap();
        let path = scratch.path().to_path_buf();
        assert!(path.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        drop(scratch);
        assert!(!path.exists());
    }

    #[test]
    fn stderr_summary_is_bounded() {
        let text = summarize_stderr(&"x".repeat(5000));
        assert!(text.chars().count() <= 1201);
        assert_eq!(summarize_stderr("  "), "(no stderr)");
    }
}
