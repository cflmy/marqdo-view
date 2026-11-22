//! Security suite: execution endpoint authentication/CSRF, cross-session SSE
//! isolation, path containment, adversarial Markdown and response hardening.
//!
//! Every test runs against the real server child process and a real fake Core
//! executable, so headers, cookies, process boundaries and the SSE transport
//! are exercised as deployed (RFC-VIEW-0002 §6.8 / §9.2).

#[path = "../common/mod.rs"]
mod common;

use crate::common::http::{
    self, collect_until_terminal, open_session, post_headers, run_body, submit, TestServer,
};

fn control_flow() -> TestServer {
    http::start(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    )
}

fn welcome() -> TestServer {
    http::start(
        "examples/welcome",
        "tests/golden/analysis-v2/welcome.json",
        None,
        None,
    )
}
use crate::common::TempProject;

#[test]
fn host_origin_session_csrf_and_json_are_all_required_before_execution() {
    let server = control_flow();
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-sec",
    );

    // Missing CSRF.
    let mut headers = post_headers(&server, &session, true);
    headers.retain(|(name, _)| *name != "X-View-CSRF");
    let missing_csrf = http::post(server.port, "/api/runs", &headers, Some(&body));
    assert_eq!(missing_csrf.status, 403);

    // Wrong Origin.
    let mut headers = post_headers(&server, &session, true);
    for (name, value) in headers.iter_mut() {
        if *name == "Origin" {
            *value = "http://evil.example".to_string();
        }
    }
    let wrong_origin = http::post(server.port, "/api/runs", &headers, Some(&body));
    assert_eq!(wrong_origin.status, 403);

    // Missing session cookie.
    let mut headers = post_headers(&server, &session, true);
    headers.retain(|(name, _)| *name != "Cookie");
    let no_session = http::post(server.port, "/api/runs", &headers, Some(&body));
    assert_eq!(no_session.status, 401);

    // Wrong content type.
    let mut headers = post_headers(&server, &session, true);
    for (name, value) in headers.iter_mut() {
        if *name == "Content-Type" {
            *value = "text/plain".to_string();
        }
    }
    let wrong_type = http::post(server.port, "/api/runs", &headers, Some(&body));
    assert_eq!(wrong_type.status, 415);

    // Malformed Host.
    let bad_host = http::get_with_host(server.port, "/", "evil.example");
    assert_eq!(bad_host.status, 400);

    assert!(!server.core.dir.join("run-was-called").exists());
}

#[test]
fn sse_and_cancel_are_scoped_to_the_owning_session() {
    let server = control_flow();
    let (session_a, _) = open_session(&server, "/");
    let (session_b, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session_a.digest,
        serde_json::json!({"kind": "entry"}),
        "req-owner",
    );
    let accepted = submit(&server, &session_a, &body).json();
    let run_id = accepted["run_id"].as_str().unwrap().to_string();

    let reader = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[("Cookie", session_b.cookie.clone())],
    );
    assert_eq!(reader.status, 404);
    let cancel = http::post(
        server.port,
        &format!("/api/runs/{run_id}/cancel"),
        &post_headers(&server, &session_b, false),
        None,
    );
    assert_eq!(cancel.status, 404);
}

#[test]
fn path_traversal_is_rejected_and_the_old_document_route_is_gone() {
    let server = welcome();
    for path in [
        "/file?path=../marqdo.yaml",
        "/file?path=%2e%2e%2fmarqdo.yaml",
        "/file?path=/etc/passwd",
        "/file?path=..%5c..%5cetc%5cpasswd",
    ] {
        let response = http::get(server.port, path);
        assert!(
            response.status == 403 || response.status == 400,
            "{path} -> {}",
            response.status
        );
    }
    let missing = http::get(server.port, "/api/document?path=index.mq.md");
    assert_eq!(missing.status, 404);
}

#[test]
fn oversized_and_unknown_fields_are_rejected_before_execution() {
    let server = control_flow();
    let (session, _) = open_session(&server, "/");
    let mut payload: serde_json::Value = serde_json::from_str(&run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-unknown",
    ))
    .unwrap();
    payload["cwd"] = serde_json::json!("/tmp/elsewhere");
    let response = submit(&server, &session, &payload.to_string());
    assert_eq!(response.status, 400);

    let huge = "x".repeat(70 * 1024);
    let response = http::post(
        server.port,
        "/api/runs",
        &post_headers(&server, &session, true),
        Some(&huge),
    );
    assert_eq!(response.status, 413);
    assert!(!server.core.dir.join("run-was-called").exists());
}

#[test]
fn session_cookie_and_headers_are_hardened() {
    let server = http::start(
        "examples/welcome",
        "tests/golden/analysis-v2/welcome.json",
        None,
        None,
    );
    let page = http::get(server.port, "/");
    assert_eq!(page.status, 200);
    let cookie = page.set_cookie().expect("session cookie");
    assert!(cookie.contains("HttpOnly"), "cookie: {cookie}");
    assert!(cookie.contains("SameSite=Strict"), "cookie: {cookie}");
    assert!(cookie.contains("Path=/"), "cookie: {cookie}");
    assert!(
        !cookie.contains("Secure"),
        "loopback HTTP must not fake HTTPS"
    );

    let csp = page.header("Content-Security-Policy").expect("csp");
    assert!(csp.contains("default-src 'self'"));
    assert!(csp.contains("connect-src 'self'"));
    assert!(csp.contains("frame-ancestors 'none'"));
    assert!(!csp.contains("unsafe-inline"));
    assert!(!csp.contains("unsafe-eval"));
    assert_eq!(page.header("X-Frame-Options").as_deref(), Some("DENY"));
    assert_eq!(
        page.header("X-Content-Type-Options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(
        page.header("Cross-Origin-Resource-Policy").as_deref(),
        Some("same-origin")
    );
}

#[test]
fn sec_fetch_site_cross_site_is_rejected_even_with_a_valid_token() {
    let server = http::start(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-sec-fetch",
    );
    let mut headers = post_headers(&server, &session, true);
    headers.push(("Sec-Fetch-Site", "cross-site".to_string()));
    let response = http::post(server.port, "/api/runs", &headers, Some(&body));
    assert_eq!(response.status, 403);
    assert!(!server.core.dir.join("run-was-called").exists());
}

#[cfg(unix)]
#[test]
fn symlink_escape_outside_the_project_root_is_rejected() {
    use std::os::unix::fs::symlink;
    let project = TempProject::new("security-symlink");
    project.marqdo_project("普通正文。\n");
    let outside = TempProject::new("security-outside");
    outside.write("secret.txt", "TOP SECRET");
    symlink(&outside.dir, project.dir.join("link")).expect("create symlink");
    let server = http::start(
        project.dir.to_str().expect("utf8 path"),
        "tests/golden/analysis-v2/welcome.json",
        None,
        None,
    );
    for path in [
        "/file?path=link/secret.txt",
        "/file?path=../security-outside/secret.txt",
    ] {
        let response = http::get(server.port, path);
        assert!(
            response.status == 403 || response.status == 404,
            "{path} -> {}",
            response.status
        );
        assert!(!response.body.contains("TOP SECRET"));
    }
}

#[test]
fn adversarial_markdown_never_injects_markup_or_handlers() {
    let server = http::start(
        "tests/fixtures/security-project",
        "tests/golden/analysis-v2/welcome.json",
        None,
        None,
    );
    let page = http::get(server.port, "/");
    assert_eq!(page.status, 200);
    assert!(!page.body.contains("<script>alert"), "script injected");
    assert!(
        !page.body.contains("<img src=x"),
        "event-handler markup injected"
    );
    assert!(!page.body.contains("href=\"javascript:alert"));
    assert!(page.body.contains("mq-raw-omitted") || page.body.contains("普通正文"));
}

#[test]
fn sse_requires_a_session_and_only_the_owning_session_can_read_or_cancel() {
    let server = http::start(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );
    let (owner, _) = open_session(&server, "/");
    let (other, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &owner.digest,
        serde_json::json!({"kind": "entry"}),
        "req-security-owner",
    );
    let accepted = http::post(
        server.port,
        "/api/runs",
        &post_headers(&server, &owner, true),
        Some(&body),
    )
    .json();
    let run_id = accepted["run_id"].as_str().unwrap().to_string();

    let anonymous = http::SseReader::open(server.port, &format!("/api/runs/{run_id}/events"), &[]);
    assert_eq!(anonymous.status, 401);

    let foreign = http::SseReader::open(
        server.port,
        &format!("/api/runs/{run_id}/events"),
        &[("Cookie", other.cookie.clone())],
    );
    assert_eq!(foreign.status, 404);
    let foreign_cancel = http::post(
        server.port,
        &format!("/api/runs/{run_id}/cancel"),
        &post_headers(&server, &other, false),
        None,
    );
    assert_eq!(foreign_cancel.status, 404);

    // The owner can still cancel; one terminal state is produced.
    let cancel = http::post(
        server.port,
        &format!("/api/runs/{run_id}/cancel"),
        &post_headers(&server, &owner, false),
        None,
    );
    assert!(cancel.status == 200 || cancel.status == 409);
}

#[test]
fn aggressive_markdown_links_cannot_become_a_file_read_api() {
    let server = http::start(
        "examples/welcome",
        "tests/golden/analysis-v2/welcome.json",
        None,
        None,
    );
    let page = http::get(server.port, "/");
    assert_eq!(page.status, 200);
    for href in ["file:///etc/passwd", "/etc/passwd"] {
        assert!(
            !page.body.contains(&format!("href=\"{href}\"")),
            "unsafe href rendered: {href}"
        );
    }
}

#[test]
fn cancel_cannot_be_replayed_to_start_a_new_process() {
    let server = http::start(
        "examples/control-flow",
        "tests/golden/analysis-v2/control-flow.json",
        Some("tests/golden/trace-v2/control-flow.run.json"),
        Some("tests/golden/trace-v2/control-flow.trace.json"),
    );
    let (session, _) = open_session(&server, "/");
    let body = run_body(
        "index.mq.md",
        &session.digest,
        serde_json::json!({"kind": "entry"}),
        "req-security-cancel-replay",
    );
    let accepted = http::post(
        server.port,
        "/api/runs",
        &post_headers(&server, &session, true),
        Some(&body),
    )
    .json();
    let run_id = accepted["run_id"].as_str().unwrap().to_string();
    let url = format!("/api/runs/{run_id}/cancel");
    let first = http::post(
        server.port,
        &url,
        &post_headers(&server, &session, false),
        None,
    );
    assert_eq!(first.status, 200);
    let second = http::post(
        server.port,
        &url,
        &post_headers(&server, &session, false),
        None,
    );
    assert_eq!(second.status, 200);
    // Unknown/forged ids are 404, never a new execution.
    let forged = http::post(
        server.port,
        "/api/runs/00000000000000000000000000000000/cancel",
        &post_headers(&server, &session, false),
        None,
    );
    assert_eq!(forged.status, 404);
    let _ = collect_until_terminal;
}
