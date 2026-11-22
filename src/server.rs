//! Localhost Live reader.
//!
//! Minimal, loopback-only surface (RFC-VIEW-0001 §11.1). Only read APIs are
//! exposed until the explicit-run flow lands; every response carries a strict
//! CSP and the Host header is validated to blunt DNS-rebinding.

use crate::app::AppContext;
use crate::assets::AssetBundle;
use crate::cli::RuntimeOpts;
use crate::error::{ViewError, ViewResult};
use crate::presentation::PresentationModel;
use crate::project::SafeRelativePath;
use crate::render::{ReaderRenderer, RenderContext};
use crate::source_map::sha256_hex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const MAX_URL_BYTES: usize = 2048;
const MAX_QUERY_BYTES: usize = 4096;

pub fn serve(ctx: AppContext, opts: &RuntimeOpts) -> ViewResult<i32> {
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

    eprintln!("marqdo view: {url}");
    eprintln!("root: {}", ctx.project().root().display());
    eprintln!(
        "{} .mq.md document(s). Ctrl+C to stop.",
        ctx.project()
            .list_documents()
            .map(|docs| docs.len())
            .unwrap_or(0)
    );
    if ctx.core_version().is_some() {
        eprintln!(
            "core: marqdo {} (Analysis v2, Trace v2)",
            ctx.core_version().unwrap_or_default()
        );
    } else if let Some(note) = ctx.core_note() {
        eprintln!("core: unavailable ({note})");
    }
    if !opts.no_open {
        open_browser(&url);
    }

    let state = Arc::new(ServerState {
        ctx,
        csrf_token: random_token(),
        renderer: ReaderRenderer::new(),
        cache: Mutex::new(HashMap::new()),
        allowed_hosts: allowed_hosts(port),
    });

    loop {
        let request = match server.recv() {
            Ok(request) => request,
            Err(err) => {
                eprintln!("view: accept failed: {err}");
                continue;
            }
        };
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            if let Err(err) = handle(&state, request) {
                eprintln!("view request error: {err}");
            }
        });
    }
}

struct ServerState {
    ctx: AppContext,
    csrf_token: String,
    renderer: ReaderRenderer,
    cache: Mutex<HashMap<String, CacheEntry>>,
    allowed_hosts: Vec<String>,
}

struct CacheEntry {
    source_digest: String,
    model: PresentationModel,
}

impl ServerState {
    /// Load one document, reusing a cached model only while the exact source
    /// bytes are unchanged (no path-only cache identity, RFC §6.5).
    fn load(&self, relative: &SafeRelativePath) -> ViewResult<PresentationModel> {
        let digest = self.ctx.project().source_digest(relative)?;
        if let Ok(cache) = self.cache.lock() {
            if let Some(entry) = cache.get(relative.as_str()) {
                if entry.source_digest == digest {
                    return Ok(entry.model.clone());
                }
            }
        }
        let model = self.ctx.load(relative)?;
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() < 64 {
                cache.insert(
                    relative.as_str().to_string(),
                    CacheEntry {
                        source_digest: digest,
                        model: model.clone(),
                    },
                );
            }
        }
        Ok(model)
    }
}

fn handle(state: &ServerState, request: Request) -> ViewResult<()> {
    let method = request.method().clone();
    if !matches!(method, Method::Get | Method::Head) {
        return respond_text(
            request,
            StatusCode(405),
            "text/plain; charset=utf-8",
            "method not allowed\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }
    let is_head = method == Method::Head;

    if !host_is_allowed(&request, &state.allowed_hosts) {
        return respond_text(
            request,
            StatusCode(400),
            "text/plain; charset=utf-8",
            "invalid Host header\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }

    let raw_url = request.url().to_string();
    if raw_url.len() > MAX_URL_BYTES {
        return respond_text(
            request,
            StatusCode(414),
            "text/plain; charset=utf-8",
            "request target too long\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }
    let (path_part, query) = split_url(&raw_url);
    if query.len() > MAX_QUERY_BYTES {
        return respond_text(
            request,
            StatusCode(414),
            "text/plain; charset=utf-8",
            "query too long\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }

    if let Some(hashed_name) = path_part.strip_prefix("/assets/") {
        return serve_asset(request, hashed_name, is_head);
    }

    match path_part {
        "/" | "/index.html" => {
            let entry = state.ctx.project().entry().relative.clone();
            match state.load(&entry) {
                Ok(model) => serve_page(state, request, &model, is_head),
                Err(err) => serve_error_page(request, &err, is_head),
            }
        }
        "/file" => {
            let Some(raw_path) = query_param(query, "path") else {
                return respond_text(
                    request,
                    StatusCode(400),
                    "text/plain; charset=utf-8",
                    "missing path\n".as_bytes().to_vec(),
                    CacheKind::NoStore,
                );
            };
            let Some(decoded) = percent_decode(&raw_path) else {
                return respond_text(
                    request,
                    StatusCode(400),
                    "text/plain; charset=utf-8",
                    "malformed path\n".as_bytes().to_vec(),
                    CacheKind::NoStore,
                );
            };
            let relative = match SafeRelativePath::parse(&decoded) {
                Ok(relative) => relative,
                Err(err) => {
                    return respond_text(
                        request,
                        StatusCode(403),
                        "text/plain; charset=utf-8",
                        format!("{}\n", err.code()).into_bytes(),
                        CacheKind::NoStore,
                    )
                }
            };
            if state.ctx.project().read_source(&relative).is_err() {
                return respond_text(
                    request,
                    StatusCode(404),
                    "text/plain; charset=utf-8",
                    "document not found\n".as_bytes().to_vec(),
                    CacheKind::NoStore,
                );
            }
            match state.load(&relative) {
                Ok(model) => serve_page(state, request, &model, is_head),
                Err(err) => serve_error_page(request, &err, is_head),
            }
        }
        "/api/document" => {
            let relative = match query_param(query, "path")
                .and_then(|raw| percent_decode(&raw))
                .map(|decoded| SafeRelativePath::parse(&decoded))
            {
                Some(Ok(relative)) => relative,
                Some(Err(err)) => {
                    return respond_json_error(request, StatusCode(400), &err);
                }
                None => state.ctx.project().entry().relative.clone(),
            };
            match state.load(&relative) {
                Ok(model) => serve_document_json(request, &model),
                Err(err) => respond_json_error(request, StatusCode(500), &err),
            }
        }
        "/api/health" => {
            let documents = state
                .ctx
                .project()
                .list_documents()
                .map(|documents| documents.len())
                .unwrap_or(0);
            let health = serde_json::json!({
                "schema": "marqdo.view-health",
                "schema_version": 1,
                "ok": true,
                "view": {
                    "name": "marqdo-view",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "core": {
                    "version": state.ctx.core_version(),
                },
                "protocols": {
                    "tool": crate::version::TOOL_PROTOCOL,
                    "analysis": crate::version::ANALYSIS_PROTOCOL,
                    "trace": crate::version::TRACE_PROTOCOL,
                },
                "documents": documents,
                "run_api": false,
            });
            respond_json(request, StatusCode(200), &health)
        }
        _ => respond_text(
            request,
            StatusCode(404),
            "text/plain; charset=utf-8",
            "not found\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        ),
    }
}

fn serve_page(
    state: &ServerState,
    request: Request,
    model: &PresentationModel,
    is_head: bool,
) -> ViewResult<()> {
    let ctx = RenderContext {
        links: crate::render::LinkPolicy::LocalServer,
        live: true,
        csrf_token: Some(state.csrf_token.clone()),
        run_available: false,
    };
    let page = state.renderer.render(model, &ctx);
    respond_html(request, page.html.into_bytes(), is_head)
}

fn serve_error_page(request: Request, err: &ViewError, is_head: bool) -> ViewResult<()> {
    let html = format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"/><title>Marqdo View · 错误</title><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/></head><body style=\"font:16px/1.6 system-ui,serif;background:#fcf9f5;color:#29251f;padding:2rem;max-width:46rem;margin:auto\"><h1>无法生成阅读页面</h1><p><code>{code}</code></p><p>{message}</p><p><a href=\"/\">返回入口</a></p></body></html>",
        code = escape_html(err.code()),
        message = escape_html(&err.to_string()),
    );
    respond_html(request, html.into_bytes(), is_head)
}

fn serve_document_json(request: Request, model: &PresentationModel) -> ViewResult<()> {
    let semantic = model.semantic.as_ref();
    let doc = serde_json::json!({
        "schema": "marqdo.view-document",
        "schema_version": 1,
        "title": model.title,
        "document": {
            "relative_path": model.document.identity.relative_path,
            "module_id": model.document.identity.module_id,
            "source_sha256": sha256_hex(model.document.source.as_bytes()),
        },
        "source_status": model.source_status.data_value(),
        "semantics_usable": model.source_status.semantics_usable(),
        "analysis": semantic.map(|projection| serde_json::json!({
            "digest": projection.analysis_digest,
            "schema_version": crate::version::ANALYSIS_PROTOCOL,
            "producer_version": projection.producer_version,
            "project_root_id": projection.project_root_id,
            "functions": projection.functions.len(),
            "units": projection.units.len(),
            "diagnostics": projection.diagnostics.len(),
        })),
        "outline": model.outline.iter().map(|entry| serde_json::json!({
            "label": entry.label,
            "anchor": entry.anchor,
            "detail": entry.detail,
            "kind": match entry.kind {
                crate::presentation::OutlineKind::Function => "function",
                crate::presentation::OutlineKind::Knowledge => "knowledge",
                crate::presentation::OutlineKind::Section => "section",
            },
        })).collect::<Vec<_>>(),
        "navigation": {
            "documents": model.navigation.documents.iter().map(|doc| doc.as_str()).collect::<Vec<_>>(),
            "welcome": model.navigation.welcome,
        },
        "note": "Read-only PresentationModel summary; the document was not executed.",
    });
    respond_json(request, StatusCode(200), &doc)
}

fn serve_asset(request: Request, hashed_name: &str, is_head: bool) -> ViewResult<()> {
    let bundle = AssetBundle::get();
    if hashed_name == bundle.hashed_css_name() {
        return respond_asset(
            request,
            bundle.css_bytes().to_vec(),
            "text/css; charset=utf-8",
            is_head,
        );
    }
    let Some(asset) = bundle.by_path(hashed_name) else {
        return respond_text(
            request,
            StatusCode(404),
            "text/plain; charset=utf-8",
            "asset not found\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    };
    respond_asset(request, asset.bytes.to_vec(), asset.mime, is_head)
}

enum CacheKind {
    NoStore,
    Immutable,
}

fn respond_html(request: Request, body: Vec<u8>, is_head: bool) -> ViewResult<()> {
    respond_with(
        request,
        StatusCode(200),
        "text/html; charset=utf-8",
        if is_head { Vec::new() } else { body },
        CacheKind::NoStore,
    )
}

fn respond_asset(
    request: Request,
    body: Vec<u8>,
    content_type: &str,
    is_head: bool,
) -> ViewResult<()> {
    respond_with(
        request,
        StatusCode(200),
        content_type,
        if is_head { Vec::new() } else { body },
        CacheKind::Immutable,
    )
}

fn respond_json(request: Request, status: StatusCode, value: &serde_json::Value) -> ViewResult<()> {
    let body = serde_json::to_vec_pretty(value).unwrap_or_else(|_| b"{}".to_vec());
    respond_with(
        request,
        status,
        "application/json; charset=utf-8",
        body,
        CacheKind::NoStore,
    )
}

fn respond_json_error(request: Request, status: StatusCode, err: &ViewError) -> ViewResult<()> {
    respond_json(request, status, &err.to_json())
}

fn respond_text(
    request: Request,
    status: StatusCode,
    content_type: &str,
    body: Vec<u8>,
    cache: CacheKind,
) -> ViewResult<()> {
    respond_with(request, status, content_type, body, cache)
}

fn respond_with(
    request: Request,
    status: StatusCode,
    content_type: &str,
    body: Vec<u8>,
    cache: CacheKind,
) -> ViewResult<()> {
    let length = body.len();
    let mut response = Response::from_data(body).with_status_code(status);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()) {
        response.add_header(header);
    }
    for (name, value) in security_headers() {
        if let Ok(header) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            response.add_header(header);
        }
    }
    let cache_value = match cache {
        CacheKind::NoStore => "no-store",
        CacheKind::Immutable => "public, max-age=31536000, immutable",
    };
    if let Ok(header) = Header::from_bytes(&b"Cache-Control"[..], cache_value.as_bytes()) {
        response.add_header(header);
    }
    let _ = length;
    request.respond(response).map_err(|err| ViewError::Io {
        detail: format!("write response: {err}"),
    })
}

fn security_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Content-Security-Policy",
            "default-src 'none'; style-src 'self'; script-src 'self' file:; img-src 'self' data:; font-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; object-src 'none'",
        ),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Frame-Options", "DENY"),
        ("Cross-Origin-Resource-Policy", "same-origin"),
        ("Cross-Origin-Opener-Policy", "same-origin"),
    ]
}

fn split_url(url: &str) -> (&str, &str) {
    match url.split_once('?') {
        Some((path, query)) => (path, query),
        None => (url, ""),
    }
}

fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let (name, value) = match pair.split_once('=') {
            Some(parts) => parts,
            None => (pair, ""),
        };
        if name == key {
            return Some(value.to_string());
        }
    }
    None
}

/// Decode `%XX` exactly once. Rejects malformed escapes, NUL, other control
/// characters and non-UTF-8 results (RFC §11.2.4).
fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                if index + 2 >= bytes.len() {
                    return None;
                }
                let high = hex_value(*bytes.get(index + 1)?)?;
                let low = hex_value(*bytes.get(index + 2)?)?;
                out.push(high * 16 + low);
                index += 3;
            }
            b'+' => {
                // Query strings here are paths, not form data: keep a literal +.
                out.push(b'+');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    if out.contains(&0) || out.iter().any(|byte| *byte < 0x20 || *byte == 0x7f) {
        return None;
    }
    String::from_utf8(out).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn allowed_hosts(port: u16) -> Vec<String> {
    vec![
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
}

fn host_is_allowed(request: &Request, allowed: &[String]) -> bool {
    let mut host: Option<&str> = None;
    for header in request.headers() {
        if header.field.equiv("Host") {
            host = Some(header.value.as_str());
            break;
        }
    }
    match host {
        Some(value) => allowed
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(value)),
        None => false,
    }
}

fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn random_token() -> String {
    #[cfg(unix)]
    {
        use std::io::Read as _;
        if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
            let mut bytes = [0u8; 32];
            if file.read_exact(&mut bytes).is_ok() {
                return hex_encode(&bytes);
            }
        }
    }
    // Fallback: still unique per process/session, never a constant.
    let material = format!(
        "{}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        std::thread::current().id()
    );
    sha256_hex(material.as_bytes())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_is_single_pass_and_strict() {
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("100%25").as_deref(), Some("100%"));
        assert_eq!(percent_decode("a+b").as_deref(), Some("a+b"));
        assert!(percent_decode("%2e%2e%2fsecret").is_some());
        assert!(percent_decode("%zz").is_none());
        assert!(percent_decode("%2").is_none());
        assert!(percent_decode("a%00b").is_none());
        assert!(percent_decode("%0a").is_none());
        assert!(percent_decode("%ff").is_none());
    }

    #[test]
    fn query_params_are_parsed_without_form_plus_rules() {
        assert_eq!(query_param("path=a%2Fb", "path").as_deref(), Some("a%2Fb"));
        assert_eq!(query_param("x=1&path=y", "path").as_deref(), Some("y"));
        assert_eq!(query_param("", "path"), None);
    }

    #[test]
    fn allowed_hosts_include_loopback_and_port() {
        let hosts = allowed_hosts(7429);
        assert!(hosts.contains(&"127.0.0.1:7429".to_string()));
        assert!(hosts.contains(&"localhost:7429".to_string()));
        assert!(!hosts.contains(&"127.0.0.1:7428".to_string()));
    }

    #[test]
    fn random_token_is_64_hex_chars_and_unique() {
        let a = random_token();
        let b = random_token();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
