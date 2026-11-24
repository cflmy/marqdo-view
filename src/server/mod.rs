//! Local single-user Live server.
//!
//! Loopback-only, fixed minimal routes, one explicit execution entry. The
//! server module contains no business engine: pages come from the one Reader,
//! run jobs from the ExecutionController, and SSE only transports already
//! sequenced View events (RFC-VIEW-0002 §5.1).

pub mod routes;
pub mod security;
pub mod sse;

use crate::cli::RuntimeOpts;
use crate::error::{ViewError, ViewResult};
use crate::execution::controller::{ExecutionConfig, ExecutionController};
use crate::reader::assemble::DocumentService;
use crate::reader::render::ReaderRenderer;
use routes::ServerState;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tiny_http::Server;

pub fn serve(service: DocumentService, opts: &RuntimeOpts) -> ViewResult<i32> {
    let address = format!("{}:{}", opts.host, opts.port);
    let server = Server::http(&address).map_err(|err| ViewError::Io {
        detail: format!("listen on {address}: {err}"),
    })?;
    let bound = server.server_addr().to_ip().ok_or_else(|| ViewError::Io {
        detail: "server address is not an IP socket".to_string(),
    })?;
    let port = bound.port();
    let display_host = match opts.host.as_str() {
        "localhost" => "localhost".to_string(),
        "::1" | "[::1]" => "[::1]".to_string(),
        _ => "127.0.0.1".to_string(),
    };
    let url = format!("http://{display_host}:{port}/");

    let service = Arc::new(service);
    let controller = Arc::new(ExecutionController::new(
        Arc::clone(&service),
        ExecutionConfig::default().with_timeout(opts.run_timeout_secs),
    ));
    let state = Arc::new(ServerState {
        service,
        controller,
        renderer: ReaderRenderer::new(),
        cache: Mutex::new(HashMap::new()),
        security: crate::server::security::SecurityState::new(port),
        active_requests: AtomicUsize::new(0),
    });

    eprintln!("marqdo view: {url}");
    eprintln!("root: {}", state.service.project().root().display());
    eprintln!(
        "{} .mq.md document(s). Ctrl+C to stop.",
        state
            .service
            .project()
            .list_documents()
            .map(|documents| documents.len())
            .unwrap_or(0)
    );
    if let Some(version) = state.service.core_version() {
        eprintln!("core: marqdo {version} (Analysis v2/v3, Trace v2)");
    } else if let Some(note) = state.service.core_note() {
        eprintln!("core: unavailable ({note})");
    }
    eprintln!(
        "run: explicit only (pure by default, one active job, {}s wall clock)",
        opts.run_timeout_secs
    );
    if !opts.no_open {
        open_browser(&url);
    }

    loop {
        let request = match server.recv() {
            Ok(request) => request,
            Err(err) => {
                eprintln!("view: accept failed: {err}");
                continue;
            }
        };
        let active = state.active_requests.fetch_add(1, Ordering::SeqCst);
        if active >= 48 {
            state.active_requests.fetch_sub(1, Ordering::SeqCst);
            let _ = request.respond(tiny_http::Response::empty(503));
            continue;
        }
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            let result = routes::handle(&state, request);
            state.active_requests.fetch_sub(1, Ordering::SeqCst);
            if let Err(err) = result {
                eprintln!("view request error: {err}");
            }
        });
    }
}

fn open_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "xdg-open"
    };
    let mut command = std::process::Command::new(program);
    if cfg!(target_os = "windows") {
        command.args(["/C", "start", ""]);
    }
    command.arg(url);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if command.spawn().is_err() {
        eprintln!("view: could not open a browser; open {url} manually");
    }
}
