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
use crate::trace::VerifiedRun;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const MAX_URL_BYTES: usize = 2048;
const MAX_QUERY_BYTES: usize = 4096;
const MAX_BODY_BYTES: usize = 16 * 1024;

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
        running: AtomicBool::new(false),
        run_cache: Mutex::new(HashMap::new()),
        run_seq: AtomicU64::new(1),
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
    /// One active run per View session.
    running: AtomicBool,
    run_cache: Mutex<HashMap<String, CachedRun>>,
    run_seq: AtomicU64,
}

#[derive(Clone)]
struct CachedRun {
    relative: String,
    verified: VerifiedRun,
}

struct CacheEntry {
    source_digest: String,
    model: PresentationModel,
}

impl ServerState {
    /// Load one document, reusing a cached model only while the exact source
    /// bytes are unchanged (no path-only cache identity, RFC §6.5).
    /// Load a page and attach a cached run when the URL names one. The run is
    /// re-bound against freshly checked Analysis inside `load_with_run`.
    fn load_for_page(
        &self,
        relative: &SafeRelativePath,
        run_id: Option<&str>,
    ) -> ViewResult<PresentationModel> {
        if let Some(run_id) = run_id {
            let cached = self
                .run_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(run_id).cloned());
            if let Some(cached) = cached {
                if cached.relative == relative.as_str() {
                    return self.ctx.load_with_run(relative, Some(&cached.verified));
                }
            }
        }
        self.load(relative)
    }

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
    let is_head = method == Method::Head;
    let is_get = method == Method::Get;

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

    if !host_is_allowed(&request, &state.allowed_hosts) {
        return respond_text(
            request,
            StatusCode(400),
            "text/plain; charset=utf-8",
            "invalid Host header\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }

    if method == Method::Post {
        return match path_part {
            "/api/run" => handle_run_post(state, request),
            "/api/clear-run" => handle_clear_run_post(state, request),
            _ => respond_text(
                request,
                StatusCode(405),
                "text/plain; charset=utf-8",
                "method not allowed\n".as_bytes().to_vec(),
                CacheKind::NoStore,
            ),
        };
    }
    if !is_get && !is_head {
        return respond_text(
            request,
            StatusCode(405),
            "text/plain; charset=utf-8",
            "method not allowed\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }

    if let Some(hashed_name) = path_part.strip_prefix("/assets/") {
        return serve_asset(request, hashed_name, is_head);
    }

    let run_id = query_param(query, "run");
    match path_part {
        "/" | "/index.html" => {
            let entry = state.ctx.project().entry().relative.clone();
            match state.load_for_page(&entry, run_id.as_deref()) {
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
            match state.load_for_page(&relative, run_id.as_deref()) {
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
            match state.load_for_page(&relative, query_param(query, "run").as_deref()) {
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
                "run_api": true,
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

fn handle_run_post(state: &ServerState, mut request: Request) -> ViewResult<()> {
    if !origin_is_allowed(&request, state) {
        return respond_json_error(
            request,
            StatusCode(403),
            &ViewError::RunDenied {
                detail: "跨站来源被拒绝（Origin 校验失败）".to_string(),
            },
        );
    }
    if !content_type_is_json(&request) {
        return respond_text(
            request,
            StatusCode(415),
            "text/plain; charset=utf-8",
            "expected application/json\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }
    let Some(token) = header_value(&request, "X-MQ-CSRF") else {
        return respond_json_error(
            request,
            StatusCode(403),
            &ViewError::RunDenied {
                detail: "缺少 CSRF 令牌，运行被拒绝".to_string(),
            },
        );
    };
    if !constant_time_eq(token.as_bytes(), state.csrf_token.as_bytes()) {
        return respond_json_error(
            request,
            StatusCode(403),
            &ViewError::RunDenied {
                detail: "CSRF 令牌无效，运行被拒绝".to_string(),
            },
        );
    }
    let body = match read_body(&mut request, MAX_BODY_BYTES) {
        Ok(body) => body,
        Err(err) => return respond_json_error(request, StatusCode(413), &err),
    };
    #[derive(serde::Deserialize)]
    struct RunRequest {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        confirm: Option<bool>,
    }
    let parsed: RunRequest = match serde_json::from_slice(&body) {
        Ok(parsed) => parsed,
        Err(err) => {
            return respond_json_error(
                request,
                StatusCode(400),
                &ViewError::RunDenied {
                    detail: format!("运行请求不是合法 JSON: {err}"),
                },
            )
        }
    };
    if parsed.confirm != Some(true) {
        return respond_json_error(
            request,
            StatusCode(400),
            &ViewError::RunDenied {
                detail: "缺少显式用户确认，拒绝运行".to_string(),
            },
        );
    }
    if let Some(mode) = parsed.mode.as_deref() {
        if mode != "pure" {
            return respond_json_error(
                request,
                StatusCode(403),
                &ViewError::RunDenied {
                    detail: format!(
                        "当前版本只允许最低权限 pure 模式（收到 {mode:?}）；project/strict grants 需要单独的授权界面"
                    ),
                },
            );
        }
    }
    let relative = match parsed.path.as_deref() {
        Some(path) => match SafeRelativePath::parse(path) {
            Ok(relative) => relative,
            Err(err) => return respond_json_error(request, StatusCode(403), &err),
        },
        None => state.ctx.project().entry().relative.clone(),
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
    if state.running.swap(true, Ordering::SeqCst) {
        return respond_json_error(
            request,
            StatusCode(409),
            &ViewError::RunDenied {
                detail: "已有一次运行正在进行；首版每个 View 会话只允许一个活动运行".to_string(),
            },
        );
    }
    let guard = RunningGuard(&state.running);
    let outcome = state.ctx.run_document(&relative);
    drop(guard);
    let executed = match outcome {
        Ok(executed) => executed,
        Err(err) => {
            return respond_json_error(request, run_error_status(&err), &err);
        }
    };
    let seq = state.run_seq.fetch_add(1, Ordering::Relaxed);
    let sanitized = executed
        .verified
        .projection
        .run_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .take(40)
        .collect::<String>();
    let run_id = format!("{seq}-{sanitized}");
    let entry = state.ctx.project().entry().relative.clone();
    let redirect = if relative == entry {
        format!("/?run={run_id}")
    } else {
        format!("/file?path={}&run={run_id}", encode_path(relative.as_str()))
    };
    let cached = CachedRun {
        relative: relative.as_str().to_string(),
        verified: executed.verified,
    };
    if let Ok(mut cache) = state.run_cache.lock() {
        if cache.len() >= 8 {
            let oldest = cache
                .keys()
                .min_by_key(|key| {
                    key.split('-')
                        .next()
                        .and_then(|seq| seq.parse::<u64>().ok())
                        .unwrap_or(u64::MAX)
                })
                .cloned();
            if let Some(oldest) = oldest {
                cache.remove(&oldest);
            }
        }
        cache.insert(run_id.clone(), cached.clone());
    }
    let projection = &cached.verified.projection;
    let response = serde_json::json!({
        "ok": true,
        "run_id": run_id,
        "redirect": redirect,
        "state": projection.state.data_value(),
        "label": projection.state.label(),
        "run_status": projection.run_status,
        "exit_code": projection.exit_code,
        "events": projection.counts.total(),
        "entry_events": projection.entry_event_count,
        "truncated": projection.truncated.as_ref().map(|truncated| truncated.label()),
        "note": "本响应只来自冻结核心 run --json 与 Trace v2；参数值/内存/证明状态不提供。",
    });
    respond_json(request, StatusCode(200), &response)
}

fn handle_clear_run_post(state: &ServerState, mut request: Request) -> ViewResult<()> {
    if !origin_is_allowed(&request, state) {
        return respond_json_error(
            request,
            StatusCode(403),
            &ViewError::RunDenied {
                detail: "跨站来源被拒绝（Origin 校验失败）".to_string(),
            },
        );
    }
    if !content_type_is_json(&request) {
        return respond_text(
            request,
            StatusCode(415),
            "text/plain; charset=utf-8",
            "expected application/json\n".as_bytes().to_vec(),
            CacheKind::NoStore,
        );
    }
    let Some(token) = header_value(&request, "X-MQ-CSRF") else {
        return respond_json_error(
            request,
            StatusCode(403),
            &ViewError::RunDenied {
                detail: "缺少 CSRF 令牌，清理缓存被拒绝".to_string(),
            },
        );
    };
    if !constant_time_eq(token.as_bytes(), state.csrf_token.as_bytes()) {
        return respond_json_error(
            request,
            StatusCode(403),
            &ViewError::RunDenied {
                detail: "CSRF 令牌无效，清理缓存被拒绝".to_string(),
            },
        );
    }
    let body = match read_body(&mut request, MAX_BODY_BYTES) {
        Ok(body) => body,
        Err(err) => return respond_json_error(request, StatusCode(413), &err),
    };
    #[derive(serde::Deserialize)]
    struct ClearRequest {
        #[serde(default)]
        run_id: Option<String>,
    }
    let parsed: ClearRequest =
        serde_json::from_slice(&body).unwrap_or(ClearRequest { run_id: None });
    if let Some(run_id) = parsed.run_id {
        if let Ok(mut cache) = state.run_cache.lock() {
            cache.remove(&run_id);
        }
    }
    respond_json(
        request,
        StatusCode(200),
        &serde_json::json!({ "ok": true, "note": "仅清理本 View 会话的运行展示缓存；未修改项目文件。" }),
    )
}

struct RunningGuard<'a>(&'a AtomicBool);

impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn run_error_status(err: &ViewError) -> StatusCode {
    match err.code() {
        "view.run_denied" => StatusCode(403),
        "view.trace_stale" => StatusCode(409),
        "view.core_timeout" => StatusCode(504),
        "view.protocol_incompatible" => StatusCode(502),
        _ => StatusCode(500),
    }
}

fn read_body(request: &mut Request, max: usize) -> Result<Vec<u8>, ViewError> {
    use std::io::Read as _;
    let mut body = Vec::new();
    let mut reader = request.as_reader().take(max as u64 + 1);
    reader.read_to_end(&mut body).map_err(|err| ViewError::Io {
        detail: format!("read request body: {err}"),
    })?;
    if body.len() > max {
        return Err(ViewError::ResourceLimit {
            detail: format!("request body above {max} bytes"),
        });
    }
    Ok(body)
}

fn header_value(request: &Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str().to_string())
}

fn content_type_is_json(request: &Request) -> bool {
    header_value(request, "Content-Type")
        .map(|value| value.to_ascii_lowercase().starts_with("application/json"))
        .unwrap_or(false)
}

fn origin_is_allowed(request: &Request, state: &ServerState) -> bool {
    let Some(origin) = header_value(request, "Origin") else {
        // Non-browser clients and same-origin `fetch` may omit Origin; the
        // CSRF token is still mandatory in every case.
        return true;
    };
    let origin = origin.trim_end_matches('/').to_ascii_lowercase();
    state.allowed_hosts.iter().any(|host| {
        let http = format!("http://{}", host.to_ascii_lowercase());
        origin == http
    })
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn encode_path(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        let unreserved =
            byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~' | b'/');
        if unreserved {
            out.push(*byte as char);
        } else {
            use std::fmt::Write as _;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
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
        run_available: state.ctx.core().is_some() && model.can_offer_run(),
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
        "run": model.run.as_ref().map(|run| serde_json::json!({
            "state": run.state.data_value(),
            "label": run.state.label(),
            "run_id": run.run_id,
            "run_status": run.run_status,
            "exit_code": run.exit_code,
            "event_count": run.counts.total(),
            "entry_event_count": run.entry_event_count,
            "truncated": run.truncated.as_ref().map(|truncated| truncated.label()),
            "has_result_value": run.result_value.is_some(),
        })),
        "can_offer_run": model.can_offer_run(),
        "note": "Read-only PresentationModel summary; this page load never executes the document.",
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
