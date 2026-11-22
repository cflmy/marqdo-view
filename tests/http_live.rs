//! Live server tests over a raw TCP HTTP/1.1 client (no extra deps).
//! The server is a real child process so startup, URL output, routing, security
//! headers, CSRF and the explicit-run flow are exercised end to end.

mod common;

use common::{run_view, FakeCore};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct LiveServer {
    child: Child,
    port: u16,
    _core: FakeCore,
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_server() -> LiveServer {
    start_server_with(
        "examples/welcome",
        "tests/golden/analysis-v2/welcome.json",
        None,
        None,
    )
}

fn start_server_with(
    project: &str,
    analysis_golden: &str,
    run_golden: Option<&str>,
    trace_golden: Option<&str>,
) -> LiveServer {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let core = FakeCore::new(&format!("live-core-{seq}"));
    let golden = std::fs::read(repo_root.join(analysis_golden)).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());
    if let Some(run_golden) = run_golden {
        let run = std::fs::read_to_string(repo_root.join(run_golden)).unwrap();
        let trace =
            trace_golden.map(|trace| std::fs::read_to_string(repo_root.join(trace)).unwrap());
        core.set_run(0, &run, trace.as_deref());
    }

    let mut child = Command::new(common::view_bin())
        .current_dir(&repo_root)
        .args([project, "--port", "0", "--no-open", "--marqdo-bin"])
        .arg(&core.script)
        .env_remove("MARQDO_BIN")
        .env_remove("MARQDO_TOOL_PROTOCOL")
        .env_remove("MARQDO_PROTOCOL_ANALYSIS")
        .env_remove("MARQDO_PROTOCOL_TRACE")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn marqdo-view");

    let stderr = child.stderr.take().expect("piped stderr");
    let mut reader = BufReader::new(stderr);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut url: Option<String> = None;
    while Instant::now() < deadline {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(rest) = line.trim().strip_prefix("marqdo view: ") {
            url = Some(rest.to_string());
            break;
        }
    }
    let url = url.expect("server printed its URL");
    let port: u16 = url
        .trim_end_matches('/')
        .rsplit(':')
        .next()
        .expect("port in url")
        .parse()
        .expect("numeric port");
    // Keep draining stderr so the child never blocks on a full pipe.
    std::thread::spawn(move || {
        let mut sink = String::new();
        let _ = reader.read_to_string(&mut sink);
    });

    LiveServer {
        child,
        port,
        _core: core,
    }
}

struct HttpResponse {
    status: u16,
    headers: String,
    body: String,
}

fn http_get(port: u16, host: Option<&str>, path: &str) -> HttpResponse {
    http_request(port, host, "GET", path, &[], None)
}

fn http_post(port: u16, path: &str, headers: &[(&str, String)], body: &str) -> HttpResponse {
    http_request(port, None, "POST", path, headers, Some(body))
}

fn http_request(
    port: u16,
    host: Option<&str>,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Option<&str>,
) -> HttpResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let host = host
        .map(str::to_string)
        .unwrap_or_else(|| format!("127.0.0.1:{port}"));
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: */*\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream.write_all(request.as_bytes()).expect("write request");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    HttpResponse {
        status,
        headers: head.to_string(),
        body: body.to_string(),
    }
}

fn csrf_token(page: &str) -> String {
    page.split("name=\"mq-csrf-token\" content=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("csrf meta token")
        .to_string()
}

fn json_body(page: &str) -> serde_json::Value {
    serde_json::from_str(page).expect("json body")
}

#[test]
fn live_server_serves_pages_assets_and_health() {
    let server = start_server();
    let page = http_get(server.port, None, "/");
    assert_eq!(page.status, 200);
    assert!(page.headers.contains("Content-Type: text/html"));
    assert!(page.headers.contains("Content-Security-Policy"));
    assert!(page.headers.contains("X-Content-Type-Options: nosniff"));
    assert!(page.body.contains("mq-reader"));
    assert!(page.body.contains("欢迎来到 Marqdo"));
    assert!(page.body.contains("data-live=\"true\""));
    assert!(page.body.contains("mq-csrf-token"));
    assert!(page.body.contains("data-run-available=\"false\""));

    let css_path = page
        .body
        .split("href=\"/assets/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("css link");
    let css = http_get(server.port, None, &format!("/assets/{css_path}"));
    assert_eq!(css.status, 200);
    assert!(css.headers.contains("text/css"));
    assert!(css.body.contains("@layer tokens"));
    assert!(css.headers.contains("immutable"));

    let health = http_get(server.port, None, "/api/health");
    assert_eq!(health.status, 200);
    let health = json_body(&health.body);
    assert_eq!(health["schema"], "marqdo.view-health");
    assert_eq!(health["protocols"]["analysis"], 2);
    assert_eq!(health["run_api"], true);

    let document = http_get(server.port, None, "/file?path=guide.mq.md");
    assert_eq!(document.status, 200);
    assert!(document.body.contains("阅读指南"));
}

#[test]
fn live_server_rejects_dns_rebinding_and_path_escape() {
    let server = start_server();
    let bad_host = http_get(server.port, Some("attacker.example"), "/");
    assert_eq!(bad_host.status, 400);

    for path in [
        "/file?path=../secret.mq.md",
        "/file?path=%2e%2e%2fsecret.mq.md",
        "/file?path=/etc/passwd",
        "/file?path=..%2f..%2fetc%2fpasswd",
    ] {
        let response = http_get(server.port, None, path);
        assert!(
            response.status == 403 || response.status == 404,
            "path {path} returned {}",
            response.status
        );
    }

    let missing = http_get(server.port, None, "/file?path=missing.mq.md");
    assert_eq!(missing.status, 404);
    let unknown = http_get(server.port, None, "/api/run");
    assert_eq!(unknown.status, 404);
}

#[test]
fn live_server_rejects_unknown_methods_and_bad_assets() {
    let server = start_server();
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    let request = format!(
        "PUT /api/run HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}",
        server.port
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw);
    assert!(text.starts_with("HTTP/1.1 405"), "{text}");

    let bad_asset = http_get(server.port, None, "/assets/../../etc/passwd");
    assert_eq!(bad_asset.status, 404);
}

#[test]
fn opening_and_refreshing_never_runs_the_document() {
    // R-01: ten page loads must produce zero Core `run` invocations.
    let server = start_server_with(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );
    for _ in 0..10 {
        let page = http_get(server.port, None, "/");
        assert_eq!(page.status, 200);
        assert!(page.body.contains("data-run-state=\"not-run\""));
        assert!(!page.body.contains("mq-run-panel"));
    }
    let args = server._core.argv_lines();
    let run_calls = args
        .iter()
        .filter(|line| line.as_str() == "arg=run")
        .count();
    assert_eq!(run_calls, 0, "page loads must never call Core run");
}

fn run_and_get_page(server: &LiveServer) -> (String, String) {
    let page = http_get(server.port, None, "/");
    let token = csrf_token(&page.body);
    let response = http_post(
        server.port,
        "/api/run",
        &[
            ("Content-Type", "application/json".to_string()),
            ("X-MQ-CSRF", token.clone()),
        ],
        r#"{"path":"index.mq.md","mode":"pure","confirm":true}"#,
    );
    assert_eq!(response.status, 200, "body: {}", response.body);
    let json = json_body(&response.body);
    assert_eq!(json["ok"], true);
    let redirect = json["redirect"].as_str().expect("redirect").to_string();
    let run_page = http_get(server.port, None, &redirect);
    assert_eq!(run_page.status, 200);
    (token, run_page.body)
}

#[test]
fn explicit_run_requires_csrf_and_confirm_then_renders_real_trace() {
    let server = start_server_with(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );
    let (_, page) = run_and_get_page(&server);
    assert!(page.contains("mq-run-panel"));
    assert!(page.contains("data-run-state=\"succeeded\""));
    assert!(page.contains("真实 Trace v2 事件"));
    assert!(page.contains("mq-run-chip"), "inline run chips expected");
    assert!(
        page.contains("0\n0\n2"),
        "Core stdout must be shown escaped"
    );
    // Public JSON final value is allowed; per-statement values are not claimed.
    assert!(page.contains("Core 本次返回值"));
    assert!(page.contains("<code>3</code>"));
    assert!(
        !page.contains("变量值"),
        "must not claim per-assignment values"
    );

    let args = server._core.argv_lines();
    let run_calls = args
        .iter()
        .filter(|line| line.as_str() == "arg=run")
        .count();
    assert_eq!(run_calls, 1, "exactly one explicit run expected");
    assert!(args.iter().any(|line| line == "arg=pure"));
    assert!(args.iter().any(|line| line == "arg=--trace-file"));
}

#[test]
fn run_api_rejects_missing_token_wrong_origin_and_bad_requests() {
    let server = start_server_with(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );
    let body = r#"{"path":"index.mq.md","mode":"pure","confirm":true}"#;

    let no_token = http_post(
        server.port,
        "/api/run",
        &[("Content-Type", "application/json".to_string())],
        body,
    );
    assert_eq!(no_token.status, 403);
    assert!(no_token.body.contains("view.run_denied"));

    let page = http_get(server.port, None, "/");
    let token = csrf_token(&page.body);
    let wrong_origin = http_post(
        server.port,
        "/api/run",
        &[
            ("Content-Type", "application/json".to_string()),
            ("X-MQ-CSRF", token.clone()),
            ("Origin", "https://attacker.example".to_string()),
        ],
        body,
    );
    assert_eq!(wrong_origin.status, 403);

    let no_confirm = http_post(
        server.port,
        "/api/run",
        &[
            ("Content-Type", "application/json".to_string()),
            ("X-MQ-CSRF", token.clone()),
        ],
        r#"{"path":"index.mq.md","mode":"pure","confirm":false}"#,
    );
    assert_eq!(no_confirm.status, 400);

    let project_mode = http_post(
        server.port,
        "/api/run",
        &[
            ("Content-Type", "application/json".to_string()),
            ("X-MQ-CSRF", token.clone()),
        ],
        r#"{"path":"index.mq.md","mode":"project","confirm":true}"#,
    );
    assert_eq!(project_mode.status, 403);
    assert!(project_mode.body.contains("view.run_denied"));
    // Public errors never leak absolute paths or raw subprocess text.
    assert!(
        !project_mode.body.contains("/root/"),
        "{}",
        project_mode.body
    );

    let args = server._core.argv_lines();
    let run_calls = args
        .iter()
        .filter(|line| line.as_str() == "arg=run")
        .count();
    assert_eq!(run_calls, 0, "rejected requests must never spawn a run");
}

#[test]
fn run_cache_can_be_cleared_and_stales_after_source_change() {
    // Use a temporary copy so the test never mutates committed examples while
    // other test binaries read them.
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project = common::TempProject::new("stale-run-project");
    project.write(
        "marqdo.yaml",
        "project:\n  name: control-flow\n  version: 0.1.0\n",
    );
    let source =
        std::fs::read_to_string(repo_root.join("examples/control-flow/index.mq.md")).unwrap();
    let source_path = project.write("index.mq.md", &source);
    let project_path = project.dir.to_str().unwrap().to_string();
    let server = start_server_with(
        &project_path,
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );

    let page = http_get(server.port, None, "/");
    let token = csrf_token(&page.body);
    let response = http_post(
        server.port,
        "/api/run",
        &[
            ("Content-Type", "application/json".to_string()),
            ("X-MQ-CSRF", token.clone()),
        ],
        r#"{"path":"index.mq.md","mode":"pure","confirm":true}"#,
    );
    let json = json_body(&response.body);
    let run_id = json["run_id"].as_str().unwrap().to_string();
    let redirect = json["redirect"].as_str().unwrap().to_string();
    let run_page = http_get(server.port, None, &redirect);
    assert!(run_page.body.contains("mq-run-panel"));

    let clear = http_post(
        server.port,
        "/api/clear-run",
        &[
            ("Content-Type", "application/json".to_string()),
            ("X-MQ-CSRF", token.clone()),
        ],
        &format!(r#"{{"run_id":"{run_id}"}}"#),
    );
    assert_eq!(clear.status, 200);
    let after_clear = http_get(server.port, None, &redirect);
    assert!(!after_clear.body.contains("mq-run-panel"));

    // Run again, then change the source bytes.
    let rerun = http_post(
        server.port,
        "/api/run",
        &[
            ("Content-Type", "application/json".to_string()),
            ("X-MQ-CSRF", token),
        ],
        r#"{"path":"index.mq.md","mode":"pure","confirm":true}"#,
    );
    let rerun_json = json_body(&rerun.body);
    let stale_redirect = rerun_json["redirect"].as_str().unwrap().to_string();
    std::fs::write(
        &source_path,
        format!("{source}\n<!-- changed after run -->\n"),
    )
    .unwrap();
    let stale_page = http_get(server.port, None, &stale_redirect);
    assert_eq!(stale_page.status, 200);
    assert!(
        stale_page.body.contains("data-run-state=\"stale\"")
            || stale_page
                .body
                .contains("data-source-status=\"source-mismatch\""),
        "expected stale/source-mismatch marker"
    );
    assert!(!stale_page.body.contains("mq-run-chip"));
}

#[test]
fn truncated_trace_is_marked_not_claimed_complete() {
    let server = start_server_with(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.truncated.run.json"),
        Some("tests/golden/trace-v2/control-flow.truncated.trace.json"),
    );
    let (_, page) = run_and_get_page(&server);
    assert!(page.contains("data-run-state=\"truncated\""));
    assert!(page.contains("Trace 被截断"));
    assert!(!page.contains("data-run-state=\"succeeded\""));
}

#[test]
fn failed_run_reports_core_error_code_without_fake_success() {
    let server = start_server_with(
        "examples/mixed-modes",
        "tests/golden/analysis-v2/mixed-modes.json",
        Some("tests/golden/trace-v2/mixed-modes.arity.run.json"),
        Some("tests/golden/trace-v2/mixed-modes.arity.trace.json"),
    );
    // The fake core exits 0 for run, but the run JSON reports failure with the
    // Core error code; View must surface that instead of a success state.
    let (_, page) = run_and_get_page(&server);
    assert!(page.contains("data-run-state=\"failed\""));
    assert!(page.contains("call.arity"));
    assert!(!page.contains("data-run-state=\"succeeded\""));
}

#[test]
fn program_stdout_is_escaped_not_injected_as_markup() {
    let tmp = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "hostile-stdout-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(
        &tmp,
        r#"{"command":"run","success":true,"stdout":"<script>alert(1)</script>","stderr":"<img src=x onerror=alert(2)>","trace":[],"value":{"type":"int","decimal":"3"}}"#,
    )
    .unwrap();
    let server = start_server_with(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some(tmp.to_str().unwrap()),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );
    let (_, page) = run_and_get_page(&server);
    assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!page.contains("<script>alert(1)"));
    assert!(!page.contains("onerror=\""));
    assert!(!page.contains("<img src=x"));
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn cli_verify_output_contract_still_holds_for_installed_style_invocation() {
    // Regression guard: `view verify` must remain a pure static check.
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let core = FakeCore::new("verify-core-contract");
    let golden = std::fs::read(repo_root.join("tests/golden/analysis-v2/welcome.json")).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());
    let output = run_view(
        &[
            "verify",
            "examples/welcome/00-welcome.mq.md",
            "--json",
            "--marqdo-bin",
            core.script.to_str().unwrap(),
        ],
        &repo_root,
        &[("MARQDO_BIN", None)],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    assert_eq!(json["result"], "PASS");
    assert_eq!(json["executed"], false);
    assert_eq!(json["analysis"]["schema_version"], 2);
}
