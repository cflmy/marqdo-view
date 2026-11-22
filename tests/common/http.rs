//! Raw HTTP/1.1 test client for the Live server (no extra dependencies).
//!
//! Provides one-shot requests and a chunked SSE reader, so integration and
//! security suites exercise the real server process, real cookies, real
//! headers and the real `text/event-stream` transport.

#![allow(dead_code)]

use super::{view_bin, FakeCore};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct TestServer {
    pub child: Child,
    pub port: u16,
    pub core: FakeCore,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start the real `marqdo-view` binary on an OS-chosen loopback port with a
/// scripted fake Core. Golden Analysis must match the project's exact source.
pub fn start(
    project: &str,
    analysis_golden: &str,
    run_golden: Option<&str>,
    trace_golden: Option<&str>,
) -> TestServer {
    start_with_env(project, analysis_golden, run_golden, trace_golden, &[])
}

pub fn start_with_env(
    project: &str,
    analysis_golden: &str,
    run_golden: Option<&str>,
    trace_golden: Option<&str>,
    envs: &[(&str, &str)],
) -> TestServer {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let seq = NEXT.fetch_add(1, Ordering::Relaxed);
    let repo_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let core = FakeCore::new(&format!("http-core-{seq}"));
    let golden = std::fs::read(repo_root.join(analysis_golden)).unwrap();
    core.set_analysis(std::str::from_utf8(&golden).unwrap());
    if let Some(run_golden) = run_golden {
        let run = std::fs::read_to_string(repo_root.join(run_golden)).unwrap();
        let trace =
            trace_golden.map(|trace| std::fs::read_to_string(repo_root.join(trace)).unwrap());
        core.set_run(0, &run, trace.as_deref());
    }

    let mut command = Command::new(view_bin());
    command
        .current_dir(&repo_root)
        .args([project, "--port", "0", "--no-open", "--marqdo-bin"])
        .arg(&core.script)
        .env_remove("MARQDO_BIN")
        .env_remove("MARQDO_TOOL_PROTOCOL")
        .env_remove("MARQDO_PROTOCOL_ANALYSIS")
        .env_remove("MARQDO_PROTOCOL_TRACE")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("spawn marqdo-view");
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
    let debug = std::env::var("MQ_TEST_DEBUG").is_ok();
    std::thread::spawn(move || {
        if debug {
            for line in reader.lines().map_while(Result::ok) {
                eprintln!("[server] {line}");
            }
        } else {
            let mut sink = String::new();
            let _ = reader.read_to_string(&mut sink);
        }
    });
    TestServer { child, port, core }
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: String,
    pub body: String,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.eq_ignore_ascii_case(name) {
                Some(value.trim().to_string())
            } else {
                None
            }
        })
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|err| panic!("json body {err}: {}", self.body))
    }

    pub fn set_cookie(&self) -> Option<String> {
        self.header("Set-Cookie")
    }
}

pub fn get(port: u16, path: &str) -> HttpResponse {
    request(port, "GET", path, &[], None)
}

pub fn get_with_host(port: u16, path: &str, host: &str) -> HttpResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let head = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(head.as_bytes()).expect("write request");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    parse_response(&raw)
}

pub fn post(port: u16, path: &str, headers: &[(&str, String)], body: Option<&str>) -> HttpResponse {
    request(port, "POST", path, headers, body)
}

pub fn request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Option<&str>,
) -> HttpResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let host = format!("127.0.0.1:{port}");
    write_request(&mut stream, &host, method, path, headers, body, true);
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read response");
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> HttpResponse {
    let text = String::from_utf8_lossy(raw).to_string();
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

fn write_request(
    stream: &mut TcpStream,
    host: &str,
    method: &str,
    path: &str,
    headers: &[(&str, String)],
    body: Option<&str>,
    close: bool,
) {
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nAccept: */*\r\n");
    if close {
        head.push_str("Connection: close\r\n");
    }
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if let Some(body) = body {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).expect("write headers");
    if let Some(body) = body {
        stream.write_all(body.as_bytes()).expect("write body");
    }
    stream.flush().expect("flush request");
}

/// One parsed SSE frame (or heartbeat comment).
#[derive(Debug, Clone)]
pub struct SseFrame {
    pub event: String,
    pub id: Option<u64>,
    pub data: Option<serde_json::Value>,
}

pub struct SseReader {
    reader: BufReader<TcpStream>,
    pending: String,
    pub headers: String,
    pub status: u16,
}

impl SseReader {
    pub fn open(port: u16, path: &str, headers: &[(&str, String)]) -> SseReader {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect sse");
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .expect("read timeout");
        let host = format!("127.0.0.1:{port}");
        write_request(&mut stream, &host, "GET", path, headers, None, false);
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read sse header");
            if line == "\r\n" || line == "\n" {
                break;
            }
            head.push_str(&line);
        }
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        SseReader {
            reader,
            pending: String::new(),
            headers: head,
            status,
        }
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.eq_ignore_ascii_case(name) {
                Some(value.trim().to_string())
            } else {
                None
            }
        })
    }

    /// Next frame/heartbeat; `None` on stream end or read failure.
    pub fn next_frame(&mut self) -> Option<SseFrame> {
        loop {
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {}
                Err(_) => return None,
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if self.pending.is_empty() {
                    continue;
                }
                let frame = std::mem::take(&mut self.pending);
                let mut event = "message".to_string();
                let mut id = None;
                let mut data_lines: Vec<String> = Vec::new();
                for line in frame.lines() {
                    if let Some(value) = line.strip_prefix("event: ") {
                        event = value.trim().to_string();
                    } else if let Some(value) = line.strip_prefix("id: ") {
                        id = value.trim().parse::<u64>().ok();
                    } else if let Some(value) = line.strip_prefix("data: ") {
                        data_lines.push(value.to_string());
                    } else if line.starts_with(':') {
                        event = "heartbeat".to_string();
                    }
                }
                let data = if data_lines.is_empty() {
                    None
                } else {
                    serde_json::from_str(&data_lines.join("\n")).ok()
                };
                return Some(SseFrame { event, id, data });
            }
            self.pending.push_str(&line);
        }
    }
}

/// One authenticated browser session: cookie + CSRF + current Analysis digest.
pub struct Session {
    pub cookie: String,
    pub csrf: String,
    pub digest: String,
}

pub fn open_session(server: &TestServer, path: &str) -> (Session, HttpResponse) {
    let page = get(server.port, path);
    assert_eq!(page.status, 200, "body: {}", page.body);
    let cookie = page
        .set_cookie()
        .and_then(|cookie| cookie.split(';').next().map(str::to_string))
        .expect("Set-Cookie");
    let csrf = page
        .body
        .split("name=\"mq-csrf-token\" content=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("csrf meta")
        .to_string();
    let digest = page
        .body
        .split("data-analysis-digest=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("analysis digest attr")
        .to_string();
    (
        Session {
            cookie,
            csrf,
            digest,
        },
        page,
    )
}

pub fn run_body(
    document: &str,
    digest: &str,
    target: serde_json::Value,
    client_id: &str,
) -> String {
    serde_json::json!({
        "document": document,
        "target": target,
        "parameters": [],
        "analysis_digest": digest,
        "mode": "pure",
        "grants": {"read": [], "write": []},
        "client_request_id": client_id,
    })
    .to_string()
}

pub fn post_headers(
    server: &TestServer,
    session: &Session,
    json: bool,
) -> Vec<(&'static str, String)> {
    let mut headers = vec![
        ("Cookie", session.cookie.clone()),
        ("Origin", format!("http://127.0.0.1:{}", server.port)),
        ("X-View-CSRF", session.csrf.clone()),
    ];
    if json {
        headers.push(("Content-Type", "application/json".to_string()));
    }
    headers
}

pub fn submit(server: &TestServer, session: &Session, body: &str) -> HttpResponse {
    post(
        server.port,
        "/api/runs",
        &post_headers(server, session, true),
        Some(body),
    )
}

pub fn collect_until_terminal(reader: &mut SseReader) -> Vec<SseFrame> {
    let mut frames = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let Some(frame) = reader.next_frame() else {
            break;
        };
        let terminal = frame
            .data
            .as_ref()
            .and_then(|data| data["state"].as_str())
            .map(|state| {
                matches!(
                    state,
                    "succeeded" | "failed" | "cancelled" | "timed_out" | "unavailable"
                )
            })
            .unwrap_or(false);
        frames.push(frame);
        if terminal {
            break;
        }
    }
    frames
}
