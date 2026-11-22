//! The one cancellable CLI process adapter.
//!
//! Every explicit run/call is spawned here as an explicit argv array (never a
//! shell), in its own process group, with bounded stdout/stderr capture and a
//! View-owned wall-clock cap that does not rely on the Core terminating itself
//! (RFC-VIEW-0002 §6.7).
//!
//! The adapter is also used by `core_client` for read-only `check`/`--version`
//! calls so there is exactly one process-spawning implementation in View.

use crate::error::{ViewError, ViewResult};
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default stdout/JSON budget (4 MiB, RFC-VIEW-0002 §6.7).
pub const MAX_STDOUT_BYTES: usize = 4 * 1024 * 1024;
/// Default stderr budget retained for display.
pub const MAX_STDERR_BYTES: usize = 64 * 1024;
/// Grace period between SIGTERM and SIGKILL when cancelling.
pub const CANCEL_GRACE: Duration = Duration::from_secs(2);
/// Poll interval for child state/cancel/wall-clock checks.
const POLL: Duration = Duration::from_millis(20);

/// Execution budgets sent to the Core and enforced again by View.
#[derive(Debug, Clone)]
pub struct ProcessBudget {
    /// View-owned wall clock. Always <= the Core `--timeout` value.
    pub wall_clock: Duration,
    pub max_stdout: usize,
    pub max_stderr: usize,
}

impl Default for ProcessBudget {
    fn default() -> Self {
        Self {
            wall_clock: Duration::from_secs(30),
            max_stdout: MAX_STDOUT_BYTES,
            max_stderr: MAX_STDERR_BYTES,
        }
    }
}

/// One bounded stream capture. Reading continues past the cap (so the child
/// never blocks on a full pipe) but retained bytes stay bounded.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub bytes: Vec<u8>,
    pub total: usize,
    pub truncated: bool,
}

impl Captured {
    pub fn text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).to_string()
    }
}

#[derive(Debug, Clone)]
pub struct ProcessOutcome {
    pub exit_code: Option<i32>,
    pub stdout: Captured,
    pub stderr: Captured,
    pub cancelled: bool,
    pub timed_out: bool,
    pub duration: Duration,
}

impl ProcessOutcome {
    pub fn status_ok(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Spawn one Core process and wait for it with bounded capture, optional
/// cancellation and a hard wall-clock cap. Never uses a shell.
pub fn run_bounded(
    bin: &Path,
    args: &[OsString],
    cwd: &Path,
    budget: &ProcessBudget,
    cancel: Option<Arc<AtomicBool>>,
) -> ViewResult<ProcessOutcome> {
    run_bounded_with_hook(bin, args, cwd, budget, cancel, Box::new(|| {}))
}

/// Same as [`run_bounded`], with a hook invoked exactly once **after** the
/// child process was successfully spawned (used to publish `running`).
pub fn run_bounded_with_hook(
    bin: &Path,
    args: &[OsString],
    cwd: &Path,
    budget: &ProcessBudget,
    cancel: Option<Arc<AtomicBool>>,
    on_spawn: Box<dyn FnOnce() + Send>,
) -> ViewResult<ProcessOutcome> {
    let started = Instant::now();
    let mut command = Command::new(bin);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: async-signal-safe child-only setup before exec.
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
        detail: format!("spawn {}: {err}", bin.display()),
    })?;
    on_spawn();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_cap = budget.max_stdout;
    let stderr_cap = budget.max_stderr;
    let stdout_thread = std::thread::spawn(move || read_bounded(stdout, stdout_cap));
    let stderr_thread = std::thread::spawn(move || read_bounded(stderr, stderr_cap));

    let deadline = started + budget.wall_clock;
    let mut cancelled = false;
    let mut timed_out = false;
    let mut cancel_sent: Option<Instant> = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(err) => {
                terminate_group(&mut child);
                return Err(ViewError::CoreSpawn {
                    detail: format!("wait for {}: {err}", bin.display()),
                });
            }
        }

        if let Some(flag) = cancel.as_deref() {
            if flag.load(Ordering::SeqCst) && cancel_sent.is_none() {
                cancel_sent = Some(Instant::now());
                signal_group(child.id(), libc::SIGTERM);
            }
        }
        if let Some(since) = cancel_sent {
            if since.elapsed() >= CANCEL_GRACE {
                signal_group(child.id(), libc::SIGKILL);
                cancelled = true;
            }
        }
        if Instant::now() >= deadline {
            signal_group(child.id(), libc::SIGKILL);
            timed_out = true;
        }
        if cancelled || timed_out {
            // Keep polling until the child is reaped; the group may take a
            // moment to die after SIGKILL.
            std::thread::sleep(POLL);
            continue;
        }
        std::thread::sleep(POLL);
    };

    // A cancel request that raced with a natural exit must not fabricate a
    // cancellation: only a kill initiated by this adapter marks it cancelled.
    if cancel_sent.is_some() && !cancelled && !timed_out {
        cancelled = false;
    }
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    Ok(ProcessOutcome {
        exit_code: status.code(),
        stdout,
        stderr,
        cancelled,
        timed_out,
        duration: started.elapsed(),
    })
}

/// Signal a whole process group (Unix) or only the child elsewhere.
pub fn signal_group(pid: u32, signal: i32) {
    #[cfg(unix)]
    {
        let pgid = pid as i32;
        if pgid > 0 {
            // SAFETY: negative pid targets the group created by setpgid; ESRCH
            // is harmless when the process already exited.
            unsafe {
                libc::kill(-pgid, signal);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, signal);
    }
}

fn terminate_group(child: &mut Child) {
    signal_group(child.id(), kill_signal());
    let _ = child.kill();
    let _ = child.wait();
}

fn kill_signal() -> i32 {
    #[cfg(unix)]
    {
        libc::SIGKILL
    }
    #[cfg(not(unix))]
    {
        9
    }
}

fn read_bounded(mut reader: impl Read, cap: usize) -> Captured {
    let mut captured = Captured::default();
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                captured.total += read;
                let remaining = cap.saturating_sub(captured.bytes.len());
                if remaining > 0 {
                    let take = remaining.min(read);
                    captured.bytes.extend_from_slice(&buffer[..take]);
                }
                if captured.total > cap {
                    captured.truncated = true;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    captured
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_capture_keeps_limit_and_drains() {
        let captured = read_bounded(std::io::Cursor::new(vec![b'x'; 100]), 10);
        assert_eq!(captured.bytes.len(), 10);
        assert_eq!(captured.total, 100);
        assert!(captured.truncated);
    }

    #[test]
    fn default_budget_matches_rfc() {
        let budget = ProcessBudget::default();
        assert_eq!(budget.wall_clock, Duration::from_secs(30));
        assert_eq!(budget.max_stdout, 4 * 1024 * 1024);
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_terminates_the_process_group() {
        let flag = Arc::new(AtomicBool::new(true));
        let start = Instant::now();
        let outcome = run_bounded(
            Path::new("/bin/sleep"),
            &[OsString::from("30")],
            Path::new("/"),
            &ProcessBudget {
                wall_clock: Duration::from_secs(20),
                ..Default::default()
            },
            Some(flag),
        )
        .expect("spawn sleep");
        assert_ne!(
            outcome.exit_code,
            Some(0),
            "cancel must not look successful"
        );
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}
