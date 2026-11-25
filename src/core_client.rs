//! The only way View talks to the frozen CLI.
//!
//! Invariants:
//! * absolute `MARQDO_BIN`, argv arrays, never `sh -c`, never shell strings;
//! * `check --emit` is the only static channel; `run`/`call --output jsonl`
//!   (optionally with `--observation-file`) is the only dynamic channel, and
//!   both dynamic destinations come from the same execution;
//! * private Core IR dumps, second parsers and protocol probing are never used;
//! * stdout/stderr/exit status are handled separately; a stderr JSON blob is
//!   never promoted to a successful Analysis;
//! * temporary artifacts live in a private 0700 directory and are removed on
//!   drop.

use crate::error::{ViewError, ViewResult};
use crate::execution::process::{run_bounded, ProcessBudget};
use crate::protocol::tool_v1::{self, CoreVersion};
use crate::reader::source::{sha256_hex, sha256_prefixed};
use crate::reader::source::{Project, SafeRelativePath};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Default View-level wall clock cap for one Core `run`.
pub const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(120);
/// Default View-level wall clock cap for one Core `check`.
pub const DEFAULT_CHECK_TIMEOUT: Duration = Duration::from_secs(120);

/// Bounded observation budget sent on every explicit run. The protocol
/// expresses exhaustion through `observation.truncated`; View never silently
/// drops or amplifies events itself.
pub const DEFAULT_OBSERVE_MAX_EVENTS: u64 = 10_000;
pub const DEFAULT_OBSERVE_MAX_BYTES: u64 = 4_194_304;
pub const DEFAULT_OBSERVE_MAX_VALUE_BYTES: u64 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    Pure,
    Project,
}

impl SandboxMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxMode::Pure => "pure",
            SandboxMode::Project => "project",
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
    /// and its artifact must never be used as the Observation binding input.
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
    pub observe_max_events: u64,
    pub observe_max_bytes: u64,
    pub observe_max_value_bytes: u64,
    /// Explicit value selection. Empty by default: View never requests values
    /// unless the user opts in (and Text additionally needs a second consent).
    pub observe_variables: Vec<String>,
    pub observe_parameters: Vec<String>,
    pub observe_text_values: bool,
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
            observe_max_events: DEFAULT_OBSERVE_MAX_EVENTS,
            observe_max_bytes: DEFAULT_OBSERVE_MAX_BYTES,
            observe_max_value_bytes: DEFAULT_OBSERVE_MAX_VALUE_BYTES,
            observe_variables: Vec::new(),
            observe_parameters: Vec::new(),
            observe_text_values: false,
        }
    }
}

#[derive(Debug)]
pub struct CheckOutput {
    /// Exact bytes written by `check --emit`, the digest source of truth.
    pub analysis_bytes: Vec<u8>,
    /// Lowercase hex SHA-256 of `analysis_bytes`.
    pub analysis_sha256: String,
    /// Prefixed digest (`sha256:...`) used by the Observation binding.
    pub analysis_digest: String,
    pub stdout: String,
    pub stderr: String,
}

/// One check diagnostic from `check --json`, normalized so no absolute
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
/// the prose stays readable and diagnostics must be shown.
#[derive(Debug)]
pub struct CheckResponse {
    pub success: bool,
    pub analysis: Option<CheckOutput>,
    pub diagnostics: Vec<RawCheckDiagnostic>,
    pub exit_code: i32,
    pub stderr: String,
}

pub use crate::scratch::ScratchDir;

/// The public client for one frozen core binary.
#[derive(Debug, Clone)]
pub struct CoreClient {
    bin: PathBuf,
    api_revision: Option<String>,
}

impl CoreClient {
    /// Build a client from `--marqdo-bin` (development) or `MARQDO_BIN`
    /// (installed Tool). Never guesses a `PATH` binary: a wrong version must
    /// fail loudly, not silently.
    pub fn from_env(explicit: Option<&Path>) -> ViewResult<Self> {
        let bin = locate_bin(explicit)?;
        Self::with_bin(bin)
    }

    /// Construct from an explicit binary path, still reading the injected API
    /// revision expectation. Useful for `--marqdo-bin` and integration tests.
    pub fn with_bin(bin: impl Into<PathBuf>) -> ViewResult<Self> {
        let bin = validate_bin(&bin.into(), "core binary")?;
        let api_revision = std::env::var(tool_v1::ENV_MARQDO_API_REVISION).ok();
        tool_v1::validate_api_revision_env(api_revision.as_deref())?;
        Ok(Self { bin, api_revision })
    }

    /// Test-only constructor with no injected environment expectation.
    #[cfg(test)]
    pub fn for_test(bin: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            api_revision: None,
        }
    }

    pub fn bin(&self) -> &Path {
        &self.bin
    }

    pub fn api_revision(&self) -> Option<&str> {
        self.api_revision.as_deref()
    }

    /// Run `--version`, validate the product range and the injected API
    /// revision. A missing Development-mode environment is allowed; an
    /// unknown revision is a hard error.
    pub fn negotiate(&self) -> ViewResult<CoreVersion> {
        tool_v1::validate_api_revision_env(self.api_revision.as_deref())?;
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
        let version = tool_v1::parse_version_output(&output.stdout)?;
        if !tool_v1::product_is_compatible(&version.version) {
            return Err(ViewError::ProtocolIncompatible {
                detail: format!(
                    "core {} is outside the supported range {}; install the current rc.3+ core",
                    version.version,
                    tool_v1::PRODUCT_RANGE
                ),
            });
        }
        Ok(version)
    }

    /// `check --json --emit`. `--json` only selects the diagnostic summary on
    /// stdout; it does not change the emitted Analysis bytes. On success the
    /// exact Analysis bytes are returned; on failure the structured
    /// diagnostics are returned without any absolute path. Only
    /// spawn/timeout/IO failures are hard errors.
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

    /// Spawn the core through the single process adapter with an explicit
    /// argv, private cwd, bounded capture and a wall-clock cap.
    fn execute(&self, args: &[OsString], cwd: &Path, timeout: Duration) -> ViewResult<ExecOutput> {
        let budget = ProcessBudget {
            wall_clock: timeout,
            ..Default::default()
        };
        let outcome = run_bounded(&self.bin, args, cwd, &budget, None)?;
        if outcome.timed_out {
            return Err(ViewError::CoreTimeout {
                detail: format!(
                    "core did not finish within {}s and was killed",
                    timeout.as_secs()
                ),
            });
        }
        let stdout = outcome.stdout.text_lossy();
        let stderr = outcome.stderr.text_lossy();
        Ok(ExecOutput {
            exit_code: outcome.exit_code.unwrap_or(-1),
            stdout_bytes: outcome.stdout.bytes,
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

/// Build the base `check` arguments. The caller appends `--json`,
/// `--emit <path>` and optionally `--refs`; this function exists so the argv
/// shape is unit-testable and cannot drift to a retired flag.
fn build_check_args(entry: &Path) -> Vec<OsString> {
    vec![OsString::from("check"), entry.as_os_str().to_os_string()]
}

/// Common explicit flags shared by `run` and `call`.
fn append_common_args(args: &mut Vec<OsString>, observation_file: &Path, options: &RunOptions) {
    fn push_pair(args: &mut Vec<OsString>, flag: &str, value: &str) {
        args.push(OsString::from(flag));
        args.push(OsString::from(value));
    }
    args.push(OsString::from("--mode"));
    args.push(OsString::from(options.mode.as_str()));
    push_pair(args, "--output", "jsonl");
    args.push(OsString::from("--observation-file"));
    args.push(observation_file.as_os_str().to_os_string());
    push_pair(
        args,
        "--observe-max-events",
        &options.observe_max_events.to_string(),
    );
    push_pair(
        args,
        "--observe-max-bytes",
        &options.observe_max_bytes.to_string(),
    );
    push_pair(
        args,
        "--observe-max-value-bytes",
        &options.observe_max_value_bytes.to_string(),
    );
    for name in &options.observe_variables {
        push_pair(args, "--observe-variable", name);
    }
    for name in &options.observe_parameters {
        push_pair(args, "--observe-parameter", name);
    }
    if options.observe_text_values {
        args.push(OsString::from("--observe-text-values"));
    }
    for glob in &options.grants.allow_read {
        push_pair(args, "--allow-read", glob);
    }
    for glob in &options.grants.allow_write {
        push_pair(args, "--allow-write", glob);
    }
    for host in &options.grants.allow_net {
        push_pair(args, "--allow-net", host);
    }
    for name in &options.grants.allow_env {
        push_pair(args, "--allow-env", name);
    }
    for program in &options.grants.allow_process {
        push_pair(args, "--allow-process", program);
    }
    if options.grants.allow_random {
        args.push(OsString::from("--allow-random"));
    }
    if let Some(fuel) = options.fuel {
        push_pair(args, "--fuel", &fuel.to_string());
    }
    if let Some(stack) = options.stack {
        push_pair(args, "--stack", &stack.to_string());
    }
    if let Some(max_output) = options.max_output {
        push_pair(args, "--max-output", &max_output.to_string());
    }
    if let Some(timeout) = options.timeout_seconds {
        push_pair(args, "--timeout", &timeout.to_string());
    }
}

/// Build `run` arguments from an already user-approved request.
pub fn build_run_args(
    entry: &Path,
    observation_file: &Path,
    options: &RunOptions,
) -> Vec<OsString> {
    let mut args = vec![OsString::from("run"), entry.as_os_str().to_os_string()];
    append_common_args(&mut args, observation_file, options);
    args
}

/// Build `call` arguments: explicit argv, one `--arg name=expression` per
/// parameter, never a shell string and never client-supplied flags.
pub fn build_call_args(
    entry: &Path,
    function: &str,
    arguments: &[(String, String)],
    observation_file: &Path,
    options: &RunOptions,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("call"),
        entry.as_os_str().to_os_string(),
        OsString::from(function),
    ];
    for (name, expression) in arguments {
        args.push(OsString::from("--arg"));
        args.push(OsString::from(format!("{name}={expression}")));
    }
    append_common_args(&mut args, observation_file, options);
    args
}

fn locate_bin(explicit: Option<&Path>) -> ViewResult<PathBuf> {
    if let Some(path) = explicit {
        return validate_bin(path, "--marqdo-bin");
    }
    if let Some(raw) = std::env::var_os(tool_v1::ENV_MARQDO_BIN) {
        if raw.is_empty() {
            return Err(ViewError::CoreMissing {
                detail: format!("{} is set but empty", tool_v1::ENV_MARQDO_BIN),
            });
        }
        return validate_bin(Path::new(&raw), tool_v1::ENV_MARQDO_BIN);
    }
    Err(ViewError::CoreMissing {
        detail: format!(
            "{} is not set. Install this tool with `marqdo tool install <bundle>/tool.json`, \
             or pass --marqdo-bin <path> when running marqdo-view directly.",
            tool_v1::ENV_MARQDO_BIN
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

// ---------------------------------------------------------------------------
// `marqdo view verify`: static Analysis/source binding check. Never executes.
// ---------------------------------------------------------------------------

use crate::cli::{RuntimeOpts, VerifyArgs};
use crate::protocol::analysis_v3;

pub fn verify_command(opts: &RuntimeOpts, args: &VerifyArgs) -> ViewResult<i32> {
    let path = args
        .path
        .clone()
        .or_else(|| opts.path.clone())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let project = Project::open(&path)?;
    let entry = project.entry().relative.clone();
    let source_bytes = project.read_source(&entry)?;
    let source = String::from_utf8(source_bytes).map_err(|err| ViewError::Io {
        detail: format!("{entry} is not valid UTF-8: {err}"),
    })?;

    let client = CoreClient::from_env(opts.marqdo_bin.as_deref())?;
    let core_version = client.negotiate()?;
    let response = client.check(
        &project,
        &entry,
        &CheckOptions {
            refs: args.refs,
            timeout: None,
        },
    )?;
    let check = match response {
        CheckResponse {
            success: false,
            diagnostics,
            exit_code,
            ..
        } => {
            let first = diagnostics
                .first()
                .map(|diagnostic| {
                    format!(
                        "{}: {}",
                        if diagnostic.code.is_empty() {
                            "diagnostic"
                        } else {
                            &diagnostic.code
                        },
                        diagnostic.message
                    )
                })
                .unwrap_or_else(|| "no diagnostic detail".to_string());
            return Err(ViewError::AnalysisFailed {
                detail: format!(
                    "`marqdo check` failed with exit code {exit_code} ({} diagnostic(s)); first: {first}",
                    diagnostics.len()
                ),
            });
        }
        CheckResponse {
            analysis: Some(check),
            ..
        } => check,
        CheckResponse { analysis: None, .. } => {
            return Err(ViewError::AnalysisFailed {
                detail: "`marqdo check` succeeded but emitted no Analysis".to_string(),
            })
        }
    };
    let verified =
        analysis_v3::verify_analysis(&check.analysis_bytes, entry.as_str(), Some(&source))?;
    let projection = &verified.projection;

    if args.json {
        let doc = serde_json::json!({
            "schema": "marqdo.view-verify",
            "schema_version": 1,
            "ok": true,
            "core": {
                "version": core_version.version.to_string(),
                "supported_range": tool_v1::PRODUCT_RANGE,
                "api_revision": tool_v1::API_REVISION,
            },
            "analysis": {
                "schema": "marqdo.analysis",
                "schema_version": projection.schema_version,
                "digest": projection.analysis_digest,
                "producer_version": projection.producer_version,
                "markdown_profile": projection.markdown_profile,
                "functions": projection.functions.len(),
                "units": projection.units.len(),
                "references": projection.references.len(),
                "diagnostics": projection.diagnostics.len(),
                "requires": {
                    "analysis": tool_v1::ANALYSIS_REVISION,
                    "observation": tool_v1::OBSERVATION_REVISION,
                },
            },
            "project": {
                "root_id": projection.project_root_id,
                "identity_kind": projection.identity_kind,
                "entry_module_id": projection.entry_module_id,
                "lock_digest": projection.lock_digest,
                "manifest_digest": projection.manifest_digest,
                "stdlib_version": projection.stdlib_version,
                "extensions": projection.extensions.len(),
            },
            "document": {
                "relative_path": entry.as_str(),
                "module_id": projection.module_id,
                "source_digest": projection.source_digest,
                "source_digest_matches": true,
            },
            "result": "PASS",
            "executed": false,
            "note": "Static verification only; the document was not executed.",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        println!(
            "core: marqdo {} (supported {}, api revision {})",
            core_version.version,
            tool_v1::PRODUCT_RANGE,
            tool_v1::API_REVISION
        );
        println!(
            "project: {} ({})",
            projection.project_root_id, projection.identity_kind
        );
        println!("document: {entry}");
        println!(
            "analysis: schema=marqdo.analysis v{} modules={} functions={} references={} digest={}",
            projection.schema_version,
            projection.modules.len(),
            projection.functions.len(),
            projection.references.len(),
            projection.analysis_digest
        );
        println!("source digest: match");
        println!("result: PASS (static only; document not executed)");
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn run_args_use_the_current_observation_flags_only() {
        let args = build_run_args(
            Path::new("/p/index.mq.md"),
            Path::new("/t/observation.json"),
            &RunOptions {
                mode: SandboxMode::Project,
                grants: Grants {
                    allow_read: vec!["data/**".into()],
                    allow_net: vec!["example.com".into()],
                    ..Default::default()
                },
                fuel: Some(1000),
                observe_max_events: 20,
                ..Default::default()
            },
        );
        let text = text(&args);
        assert_eq!(text[0], "run");
        assert_eq!(text[1], "/p/index.mq.md");
        assert!(text.contains(&"project".to_string()));
        assert!(text.contains(&"--output".to_string()));
        assert!(text.contains(&"jsonl".to_string()));
        assert!(text.contains(&"--observation-file".to_string()));
        assert!(text.contains(&"--observe-max-events".to_string()));
        assert!(text.contains(&"--observe-max-bytes".to_string()));
        assert!(text.contains(&"--observe-max-value-bytes".to_string()));
        assert!(text.contains(&"data/**".to_string()));
        assert!(text.contains(&"example.com".to_string()));
        // Retired flags and any shell fragment must never appear.
        for retired in [
            "--json",
            "--trace-file",
            "--event-stream",
            "--trace-max-events",
            "--trace-max-bytes",
        ] {
            assert!(
                !text.contains(&retired.to_string()),
                "retired flag {retired}"
            );
        }
        assert!(!text
            .iter()
            .any(|token| token.contains(';') || token.contains('|') || token.contains('&')));
    }

    #[test]
    fn run_defaults_do_not_request_values() {
        let args = build_run_args(
            Path::new("/p/index.mq.md"),
            Path::new("/t/observation.json"),
            &RunOptions::default(),
        );
        let text = text(&args);
        assert!(!text.iter().any(|token| token == "--observe-variable"));
        assert!(!text.iter().any(|token| token == "--observe-parameter"));
        assert!(!text.iter().any(|token| token == "--observe-text-values"));
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
}
