//! Shared test helpers for integration and security suites.
//!
//! Fake cores are real executables (never mocked Rust traits at the process
//! boundary) so argv, exit codes, stderr and temp artifacts are all exercised.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn view_bin() -> &'static str {
    env!("CARGO_BIN_EXE_marqdo-view")
}

pub fn temp_root(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test temp dir");
    dir
}

pub struct TempProject {
    pub dir: PathBuf,
}

impl TempProject {
    pub fn new(name: &str) -> Self {
        Self {
            dir: temp_root(name),
        }
    }

    pub fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }

    pub fn marqdo_project(&self, source: &str) {
        self.write(
            "marqdo.yaml",
            "project:\n  name: fixture\n  version: 0.1.0\n",
        );
        self.write("index.mq.md", source);
    }
}

/// A scriptable shell-based fake core. Every behaviour is controlled by files
/// in its directory, so tests never mutate global environment variables.
pub struct FakeCore {
    pub dir: PathBuf,
    pub script: PathBuf,
}

impl FakeCore {
    pub fn new(name: &str) -> Self {
        let dir = temp_root(name);
        let script = dir.join("marqdo");
        let body = r#"#!/bin/sh
dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
{
  printf 'invocation\n'
  for arg in "$@"; do printf 'arg=%s\n' "$arg"; done
} >> "$dir/argv.txt"
case "$1" in
  --version)
    if [ -f "$dir/version" ]; then cat "$dir/version"; else printf 'marqdo 3.0.0-beta.1\n'; fi
    exit 0
    ;;
  check)
    if [ -f "$dir/check_stderr" ]; then cat "$dir/check_stderr" >&2; fi
    if [ -f "$dir/check_json" ]; then cat "$dir/check_json"; fi
    emit=""
    prev=""
    for a in "$@"; do
      if [ "$prev" = "--emit" ]; then emit="$a"; fi
      prev="$a"
    done
    if [ -f "$dir/analysis.json" ] && [ -n "$emit" ]; then cp "$dir/analysis.json" "$emit"; fi
    if [ -f "$dir/check_exit" ]; then exit "$(cat "$dir/check_exit")"; fi
    exit 0
    ;;
  run|call)
    : > "$dir/run-was-called"
    if [ -f "$dir/sleep" ]; then
      sleep "$(cat "$dir/sleep")"
    fi
    trace=""
    prev=""
    for a in "$@"; do
      if [ "$prev" = "--trace-file" ]; then trace="$a"; fi
      prev="$a"
    done
    if [ -f "$dir/trace.json" ] && [ -n "$trace" ]; then cp "$dir/trace.json" "$trace"; fi
    if [ -f "$dir/run.json" ]; then cat "$dir/run.json"; else printf '{"command":"run","success":true}\n'; fi
    if [ -f "$dir/run_exit" ]; then exit "$(cat "$dir/run_exit")"; fi
    exit 0
    ;;
  *)
    printf 'fake core: unknown command %s\n' "$1" >&2
    exit 64
    ;;
esac
"#;
        std::fs::write(&script, body).expect("write fake core");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("chmod fake core");
        }
        Self { dir, script }
    }

    pub fn set_version(&self, text: &str) {
        std::fs::write(self.dir.join("version"), text).expect("write version");
    }

    pub fn set_check(&self, exit_code: i32, stderr: &str) {
        std::fs::write(self.dir.join("check_exit"), exit_code.to_string()).expect("write exit");
        std::fs::write(self.dir.join("check_stderr"), stderr).expect("write stderr");
    }

    pub fn set_check_json(&self, json: &str) {
        std::fs::write(self.dir.join("check_json"), json).expect("write check json");
    }

    pub fn set_analysis(&self, analysis: &str) {
        std::fs::write(self.dir.join("analysis.json"), analysis).expect("write analysis");
    }

    pub fn set_run(&self, exit_code: i32, json: &str, trace: Option<&str>) {
        std::fs::write(self.dir.join("run_exit"), exit_code.to_string()).expect("write run exit");
        std::fs::write(self.dir.join("run.json"), json).expect("write run json");
        match trace {
            Some(trace) => {
                std::fs::write(self.dir.join("trace.json"), trace).expect("write trace");
            }
            None => {
                let _ = std::fs::remove_file(self.dir.join("trace.json"));
            }
        }
    }

    pub fn set_sleep_seconds(&self, seconds: u32) {
        std::fs::write(self.dir.join("sleep"), seconds.to_string()).expect("write sleep");
    }

    pub fn argv_lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("argv.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// All argument values for the most recent invocation.
    pub fn last_args(&self) -> Vec<String> {
        self.argv_lines()
            .into_iter()
            .filter_map(|line| line.strip_prefix("arg=").map(str::to_string))
            .collect()
    }
}

pub fn run_view(args: &[&str], cwd: &Path, envs: &[(&str, Option<&str>)]) -> Output {
    let mut command = Command::new(view_bin());
    command.args(args).current_dir(cwd);
    for (key, value) in envs {
        match value {
            Some(value) => {
                command.env(key, value);
            }
            None => {
                command.env_remove(key);
            }
        }
    }
    command.output().expect("run marqdo-view")
}

/// Minimal valid Analysis v2 JSON for a single module, with a caller-provided
/// source digest and schema version.
pub fn minimal_analysis(relative_path: &str, source_digest: &str, schema_version: u32) -> String {
    serde_json::json!({
        "schema": "marqdo.analysis",
        "schema_version": schema_version,
        "producer": {
            "name": "marqdo",
            "version": "3.0.0-beta.1",
            "markdown_profile": "marqdo.md.v1"
        },
        "project": {
            "root_id": "root:fixture@0.1.0",
            "identity_kind": "manifest",
            "lock_digest": "",
            "stdlib": null
        },
        "modules": [{
            "id": "root:fixture@0.1.0::index.mq.md",
            "package_instance_id": "root:fixture@0.1.0",
            "relative_path": relative_path,
            "source_digest": source_digest,
            "units": [],
            "functions": [],
            "imports": [],
            "references": [],
            "calls": [],
            "resources": []
        }],
        "diagnostics": []
    })
    .to_string()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub mod http;
