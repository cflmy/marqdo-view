//! Live server smoke tests over a raw TCP HTTP/1.1 client (no extra deps).
//! The server is a real child process so startup, URL output, routing and
//! security headers are all exercised end to end.

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
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let core = FakeCore::new(&format!("live-core-{seq}"));
    let golden = std::fs::read(repo_root.join("tests/golden/analysis-v2/welcome.json")).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());

    let mut child = Command::new(common::view_bin())
        .current_dir(&repo_root)
        .args([
            "examples/welcome",
            "--port",
            "0",
            "--no-open",
            "--marqdo-bin",
        ])
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
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let host = host
        .map(str::to_string)
        .unwrap_or_else(|| format!("127.0.0.1:{port}"));
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nAccept: */*\r\n\r\n");
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
    assert!(!page.body.contains("data-live=\"false\""));
    // CSRF token meta is present for the future explicit-run API.
    assert!(page.body.contains("mq-csrf-token"));

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
    assert!(health.body.contains("\"schema\": \"marqdo.view-health\""));
    assert!(health.body.contains("\"analysis\": 2"));

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
    // POST is not exposed yet.
    let mut stream = TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    let request = format!(
        "POST /api/run HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}",
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
