//! The fixed minimal HTTP route table (RFC-VIEW-0002 §6.2).
//!
//! | route | executes? |
//! |---|---|
//! | `GET /` | no |
//! | `GET /file?path=...` | no |
//! | `GET /assets/...` | no |
//! | `GET /api/documents` | no |
//! | `GET /media/...` | no |
//! | `POST /api/exports` | no (static snapshot) |
//! | `POST /api/runs` | **yes — the only execution entry** |
//! | `GET /api/runs/{id}/events` | no (observe only) |
//! | `POST /api/runs/{id}/cancel` | no (control of an existing job) |
//!
//! Anything else is 404. Page loads, refreshes, searches, document switches and
//! the static export path can never form an execution edge.

use crate::error::{ViewError, ViewResult};
use crate::execution::controller::ExecutionController;
use crate::reader::assemble::DocumentService;
use crate::reader::assets::AssetBundle;
use crate::reader::model::PresentationModel;
use crate::reader::render::{ReaderRenderer, RenderContext};
use crate::reader::source::{sha256_hex, SafeRelativePath};
use crate::server::security::{
    header, SecurityState, MAX_POST_BODY_BYTES, MAX_QUERY_BYTES, MAX_URL_BYTES,
};
use crate::server::sse;
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Request, Response, StatusCode};

pub struct ServerState {
    pub service: Arc<DocumentService>,
    pub controller: Arc<ExecutionController>,
    pub renderer: ReaderRenderer,
    pub cache: Mutex<HashMap<String, CacheEntry>>,
    pub security: SecurityState,
    pub active_requests: AtomicUsize,
    pub snapshot_export: Mutex<()>,
}

#[derive(Clone)]
pub struct CacheEntry {
    source_digest: String,
    model: PresentationModel,
}

impl ServerState {
    /// Load one document, reusing a cached model only while the exact source
    /// bytes are unchanged. This path performs the read-only `check` only.
    pub fn load(&self, relative: &SafeRelativePath) -> ViewResult<PresentationModel> {
        let digest = self.service.project().source_digest(relative)?;
        let cached = self.cache.lock().ok().and_then(|cache| {
            let entry = cache.get(relative.as_str())?;
            (entry.source_digest == digest).then(|| entry.model.clone())
        });
        if let Some(mut model) = cached {
            model.media = crate::reader::media::collect(
                self.service.project(),
                relative,
                &model.document.blocks,
            );
            model.navigation.documents = self.service.project().list_documents()?;
            return Ok(model);
        }
        let model = self.service.load(relative)?;
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() < 64 {
                let mut cached = model.clone();
                cached.media.clear();
                cache.insert(
                    relative.as_str().to_string(),
                    CacheEntry {
                        source_digest: digest,
                        model: cached,
                    },
                );
            }
        }
        Ok(model)
    }
}

pub fn handle(state: &ServerState, request: Request) -> ViewResult<()> {
    let method = request.method().clone();
    let raw_url = request.url().to_string();
    if raw_url.len() > MAX_URL_BYTES {
        return respond_text(
            request,
            StatusCode(414),
            "request target too long\n",
            CacheKind::NoStore,
        );
    }
    let (path, query) = split_url(&raw_url);
    if query.len() > MAX_QUERY_BYTES {
        return respond_text(
            request,
            StatusCode(414),
            "query too long\n",
            CacheKind::NoStore,
        );
    }
    if let Err(err) = state.security.check_host(&request) {
        return respond_error(request, StatusCode(400), &err);
    }

    let is_get = method == Method::Get || method == Method::Head;
    if is_get {
        if let Some(hashed_name) = path.strip_prefix("/assets/") {
            return serve_asset(request, hashed_name);
        }
        if let Some(name) = path.strip_prefix("/media/") {
            return serve_media(state, request, name, query);
        }
        if path == "/" || path == "/index.html" {
            return serve_page(state, request, None);
        }
        if path == "/file" {
            return serve_file(state, request, query);
        }
        if path == "/api/documents" {
            return serve_documents(state, request, query);
        }
        if let Some(run_id) = matched_run_route(path, "/events") {
            return serve_events(state, request, run_id);
        }
        return respond_text(request, StatusCode(404), "not found\n", CacheKind::NoStore);
    }

    if method == Method::Post {
        if path == "/api/exports" {
            return submit_export(state, request);
        }
        if path == "/api/runs" {
            return submit_run(state, request);
        }
        if let Some(run_id) = matched_run_route(path, "/cancel") {
            return cancel_run(state, request, run_id);
        }
        return respond_text(
            request,
            StatusCode(405),
            "method not allowed\n",
            CacheKind::NoStore,
        );
    }
    respond_text(
        request,
        StatusCode(405),
        "method not allowed\n",
        CacheKind::NoStore,
    )
}

/// Match `/api/runs/<id><suffix>` without accepting nested paths.
fn matched_run_route<'a>(path: &'a str, suffix: &str) -> Option<&'a str> {
    let rest = path.strip_prefix("/api/runs/")?;
    let run_id = rest.strip_suffix(suffix)?;
    if run_id.is_empty()
        || run_id.len() > 128
        || !run_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        || path.contains("//")
    {
        return None;
    }
    Some(run_id)
}

fn serve_page(
    state: &ServerState,
    request: Request,
    relative: Option<SafeRelativePath>,
) -> ViewResult<()> {
    let relative = match relative {
        Some(relative) => relative,
        None => state.service.project().entry().relative.clone(),
    };
    let model = match state.load(&relative) {
        Ok(model) => model,
        Err(err) => return serve_error_page(request, &err),
    };
    let (session, set_cookie) = state.security.page_session(&request);
    let ctx = RenderContext {
        links: crate::reader::render::LinkPolicy::LocalServer,
        live: true,
        csrf_token: Some(session.csrf),
        run_available: state.service.core().is_some() && model.can_offer_run(),
        annotations: true,
        run_timeout_secs: state.controller.config().run_timeout_secs(),
        core_available: state.service.core().is_some(),
        snapshot_package: false,
        overlay: None,
    };
    let page = state.renderer.render(&model, &ctx);
    let mut headers = Vec::new();
    if let Some(cookie) = set_cookie {
        if let Ok(header) = Header::from_bytes(&b"Set-Cookie"[..], cookie.as_bytes()) {
            headers.push(header);
        }
    }
    respond_bytes(
        request,
        StatusCode(200),
        "text/html; charset=utf-8",
        page.html.into_bytes(),
        CacheKind::NoStore,
        headers,
    )
}

fn serve_file(state: &ServerState, request: Request, query: &str) -> ViewResult<()> {
    let Some(raw_path) = query_param(query, "path") else {
        return respond_text(
            request,
            StatusCode(400),
            "missing path\n",
            CacheKind::NoStore,
        );
    };
    let Some(decoded) = percent_decode(&raw_path) else {
        return respond_text(
            request,
            StatusCode(400),
            "malformed path\n",
            CacheKind::NoStore,
        );
    };
    let relative = match SafeRelativePath::parse(&decoded) {
        Ok(relative) => relative,
        Err(err) => {
            return respond_text(
                request,
                StatusCode(403),
                &format!("{}\n", err.code()),
                CacheKind::NoStore,
            )
        }
    };
    if state.service.project().read_source(&relative).is_err() {
        return respond_text(
            request,
            StatusCode(404),
            "document not found\n",
            CacheKind::NoStore,
        );
    }
    serve_page(state, request, Some(relative))
}

fn serve_documents(state: &ServerState, request: Request, query: &str) -> ViewResult<()> {
    if let Err(err) = state.security.check_same_origin(&request) {
        return respond_error(request, StatusCode(403), &err);
    }
    let relative = match query_param(query, "path").and_then(|raw| percent_decode(&raw)) {
        Some(decoded) => match SafeRelativePath::parse(&decoded) {
            Ok(relative) => relative,
            Err(err) => return respond_error(request, StatusCode(400), &err),
        },
        None => state.service.project().entry().relative.clone(),
    };
    let model = match state.load(&relative) {
        Ok(model) => model,
        Err(err) => return respond_error(request, StatusCode(500), &err),
    };
    let semantic = model.semantic.as_ref();
    let documents = state
        .service
        .project()
        .list_documents()
        .unwrap_or_default()
        .into_iter()
        .map(|path| {
            serde_json::json!({
                "path": path.as_str(),
                "active": path == relative,
            })
        })
        .collect::<Vec<_>>();
    let functions = model
        .outline
        .iter()
        .filter(|entry| entry.kind == crate::reader::model::OutlineKind::Function)
        .map(|entry| {
            serde_json::json!({
                "label": entry.label,
                "anchor": entry.anchor,
                "detail": entry.detail,
            })
        })
        .collect::<Vec<_>>();
    let doc = serde_json::json!({
        "schema": "marqdo.view-documents",
        "schema_version": 1,
        "entry": state.service.project().entry().relative.as_str(),
        "documents": documents,
        "current": {
            "relative_path": model.document.identity.relative_path,
            "module_id": model.document.identity.module_id,
            "title": model.title,
            "source_sha256": sha256_hex(model.document.source.as_bytes()),
            "source_status": model.source_status.data_value(),
            "semantics_usable": model.source_status.semantics_usable(),
            "analysis": semantic.map(|projection| serde_json::json!({
                "schema_version": projection.schema_version,
                "digest": projection.analysis_digest,
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
                    crate::reader::model::OutlineKind::Function => "function",
                    crate::reader::model::OutlineKind::Knowledge => "knowledge",
                    crate::reader::model::OutlineKind::Section => "section",
                },
            })).collect::<Vec<_>>(),
            "functions": functions,
            "can_offer_run": model.can_offer_run(),
        },
        "note": "Read-only document structure; this request never executes the document.",
    });
    respond_json(request, StatusCode(200), &doc)
}

/// The only execution entry in the whole product.
fn submit_export(state: &ServerState, mut request: Request) -> ViewResult<()> {
    use crate::export_result::{ExportSelection, MAX_EXPORT_REQUEST};
    let result = (|| {
        let session = state.security.authenticate_post(&request)?;
        let selection = ExportSelection::parse(&read_body(&mut request, MAX_EXPORT_REQUEST)?)?;
        let candidate = state
            .controller
            .get_export_candidate(&session.id, &selection.run_id)?;
        let guard = state
            .snapshot_export
            .try_lock()
            .map_err(|_| ViewError::ResourceLimit {
                detail: "a snapshot download is already being prepared or sent".into(),
            })?;
        let (model, overlay) = selection.prepare(&state.service, &candidate)?;
        let archive = crate::export::snapshot(&state.service, &model, overlay)?;
        Ok((guard, archive))
    })();
    let (_guard, archive) = match result {
        Ok(result) => result,
        Err(err) => return respond_error(request, status_for(&err), &err),
    };
    let mut headers = crate::server::security::security_headers();
    headers.retain(|(name, _)| *name != "Content-Security-Policy");
    headers.extend([
        ("Content-Type", "application/zip"),
        (
            "Content-Disposition",
            "attachment; filename=marqdo-execution-snapshot.zip",
        ),
        ("Content-Security-Policy", "default-src 'none'; sandbox"),
    ]);
    let headers = headers
        .into_iter()
        .map(|(name, value)| {
            Header::from_bytes(name, value).expect("valid snapshot download header")
        })
        .collect();
    let response = Response::new(
        StatusCode(200),
        headers,
        archive.file,
        Some(archive.len),
        None,
    );
    request
        .respond(response)
        .map_err(|err| ViewError::io("send snapshot ZIP", err))
}

fn submit_run(state: &ServerState, mut request: Request) -> ViewResult<()> {
    let session = match state.security.authenticate_post(&request) {
        Ok(session) => session,
        Err(err) => return respond_error(request, status_for(&err), &err),
    };
    let body = match read_body(&mut request, MAX_POST_BODY_BYTES) {
        Ok(body) => body,
        Err(err) => {
            let status = match err.code() {
                "view.resource_limit" | "view.output_limit" => StatusCode(413),
                _ => status_for(&err),
            };
            return respond_error(request, status, &err);
        }
    };
    match state.controller.submit(&session.id, &body) {
        Ok(accepted) => respond_json(
            request,
            StatusCode(202),
            &serde_json::json!({
                "schema": "marqdo.view-run-accepted",
                "schema_version": 1,
                "run_id": accepted.run_id,
                "state": accepted.state.data_value(),
                "events_url": accepted.events_url,
                "cancel_url": accepted.cancel_url,
                "reused": accepted.reused,
            }),
        ),
        Err(err) => respond_error(request, status_for(&err), &err),
    }
}

fn serve_events(state: &ServerState, request: Request, run_id: &str) -> ViewResult<()> {
    let session = match state.security.require_session(&request) {
        Ok(session) => session,
        Err(err) => return respond_error(request, status_for(&err), &err),
    };
    if let Err(err) = state.security.check_same_origin(&request) {
        return respond_error(request, StatusCode(403), &err);
    }
    let last_event_id =
        header(&request, "Last-Event-ID").and_then(|value| value.parse::<u64>().ok());
    match state
        .controller
        .subscribe(&session.id, run_id, last_event_id)
    {
        Ok(subscription) => sse::respond(request, subscription),
        Err(err) => respond_error(request, status_for(&err), &err),
    }
}

fn cancel_run(state: &ServerState, request: Request, run_id: &str) -> ViewResult<()> {
    let session = match state.security.authenticate_control(&request) {
        Ok(session) => session,
        Err(err) => return respond_error(request, status_for(&err), &err),
    };
    match state.controller.cancel(&session.id, run_id) {
        Ok(state_value) => respond_json(
            request,
            StatusCode(200),
            &serde_json::json!({
                "schema": "marqdo.view-run-cancel",
                "schema_version": 1,
                "run_id": run_id,
                "state": state_value.data_value(),
            }),
        ),
        Err(err) => respond_error(request, status_for(&err), &err),
    }
}

fn serve_media(state: &ServerState, request: Request, name: &str, query: &str) -> ViewResult<()> {
    if let Err(err) = state.security.require_session(&request) {
        return respond_error(request, status_for(&err), &err);
    }
    let Some(path) =
        query_param(query, "path").and_then(|path| SafeRelativePath::parse(&path).ok())
    else {
        return respond_text(
            request,
            StatusCode(404),
            "media not found\n",
            CacheKind::NoStore,
        );
    };
    let Some(media) = crate::reader::media::load(state.service.project(), &path)
        .filter(|media| media.filename() == name)
    else {
        return respond_text(
            request,
            StatusCode(404),
            "media not found\n",
            CacheKind::NoStore,
        );
    };
    respond_bytes(
        request,
        StatusCode(200),
        media.mime,
        media.bytes,
        CacheKind::NoStore,
        vec![
            Header::from_bytes("Content-Security-Policy", "default-src 'none'; sandbox")
                .expect("valid media CSP"),
        ],
    )
}

fn serve_asset(request: Request, hashed_name: &str) -> ViewResult<()> {
    let bundle = AssetBundle::get();
    if hashed_name == bundle.hashed_css_name() {
        return respond_bytes(
            request,
            StatusCode(200),
            "text/css; charset=utf-8",
            bundle.css_bytes().to_vec(),
            CacheKind::Immutable,
            Vec::new(),
        );
    }
    let Some(asset) = bundle.by_path(hashed_name) else {
        return respond_text(
            request,
            StatusCode(404),
            "asset not found\n",
            CacheKind::NoStore,
        );
    };
    respond_bytes(
        request,
        StatusCode(200),
        asset.mime,
        asset.bytes.to_vec(),
        CacheKind::Immutable,
        Vec::new(),
    )
}

fn serve_error_page(request: Request, err: &ViewError) -> ViewResult<()> {
    let html = format!(
        "<!DOCTYPE html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"/><title>Marqdo View · 错误</title><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"/></head><body><main><h1>无法生成阅读页面</h1><p><code>{code}</code></p><p>{message}</p><p><a href=\"/\">返回入口</a></p></main></body></html>",
        code = escape_html(err.code()),
        message = escape_html(err.public_message()),
    );
    respond_html(request, html.into_bytes())
}

fn status_for(err: &ViewError) -> StatusCode {
    match err.code() {
        "view.session_required" => StatusCode(401),
        "view.run_denied" => StatusCode(403),
        "view.run_invalid" | "view.no_entry_function" | "view.export_invalid" => StatusCode(400),
        "view.run_busy"
        | "view.run_conflict"
        | "view.export_unavailable"
        | "view.export_source_changed" => StatusCode(409),
        "view.run_not_found" => StatusCode(404),
        "view.sse_busy" | "view.resource_limit" => StatusCode(429),
        "view.output_limit" => StatusCode(413),
        "view.analysis_failed" => StatusCode(422),
        "view.core_missing" | "view.core_spawn" => StatusCode(503),
        "view.core_timeout" => StatusCode(504),
        "view.protocol_incompatible" | "view.core_protocol" => StatusCode(502),
        "view.unsupported" => StatusCode(415),
        _ => StatusCode(500),
    }
}

fn read_body(request: &mut Request, max: usize) -> Result<Vec<u8>, ViewError> {
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

enum CacheKind {
    NoStore,
    Immutable,
}

fn respond_html(request: Request, body: Vec<u8>) -> ViewResult<()> {
    respond_bytes(
        request,
        StatusCode(200),
        "text/html; charset=utf-8",
        body,
        CacheKind::NoStore,
        Vec::new(),
    )
}

fn respond_text(
    request: Request,
    status: StatusCode,
    body: &str,
    cache: CacheKind,
) -> ViewResult<()> {
    respond_bytes(
        request,
        status,
        "text/plain; charset=utf-8",
        body.as_bytes().to_vec(),
        cache,
        Vec::new(),
    )
}

fn respond_json(request: Request, status: StatusCode, value: &serde_json::Value) -> ViewResult<()> {
    let body = serde_json::to_vec_pretty(value).unwrap_or_else(|_| b"{}".to_vec());
    respond_bytes(
        request,
        status,
        "application/json; charset=utf-8",
        body,
        CacheKind::NoStore,
        Vec::new(),
    )
}

fn respond_error(request: Request, status: StatusCode, err: &ViewError) -> ViewResult<()> {
    let mut headers = Vec::new();
    if let Ok(header) =
        Header::from_bytes(&b"WWW-Authenticate"[..], b"Session realm=\"marqdo-view\"")
    {
        if err.code() == "view.run_denied" && status.0 == 401 {
            headers.push(header);
        }
    }
    let value = serde_json::json!({
        "schema": "marqdo.view-error",
        "schema_version": 1,
        "code": err.code(),
        "source": "view",
        "message": err.public_message(),
    });
    let body = serde_json::to_vec_pretty(&value).unwrap_or_else(|_| b"{}".to_vec());
    respond_bytes(
        request,
        status,
        "application/json; charset=utf-8",
        body,
        CacheKind::NoStore,
        headers,
    )
}

fn respond_bytes(
    request: Request,
    status: StatusCode,
    content_type: &str,
    body: Vec<u8>,
    cache: CacheKind,
    extra_headers: Vec<Header>,
) -> ViewResult<()> {
    let mut response = Response::from_data(body).with_status_code(status);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()) {
        response.add_header(header);
    }
    for (name, value) in crate::server::security::security_headers() {
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
    for header in extra_headers {
        response.add_header(header);
    }
    request.respond(response).map_err(|err| ViewError::Io {
        detail: format!("write response: {err}"),
    })
}

fn split_url(url: &str) -> (&str, &str) {
    match url.split_once('?') {
        Some((path, query)) => (path, query),
        None => (url, ""),
    }
}

pub fn query_param(query: &str, key: &str) -> Option<String> {
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

/// Decode `%XX` exactly once; reject malformed escapes, NUL, controls and
/// non-UTF-8 results.
pub fn percent_decode(raw: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_route_matching_is_strict() {
        assert_eq!(
            matched_run_route("/api/runs/abc/events", "/events"),
            Some("abc")
        );
        assert_eq!(
            matched_run_route("/api/runs/abc/cancel", "/cancel"),
            Some("abc")
        );
        assert_eq!(matched_run_route("/api/runs//events", "/events"), None);
        assert_eq!(matched_run_route("/api/runs/a/b/events", "/events"), None);
        assert_eq!(matched_run_route("/api/runs/abc/events/", "/events"), None);
    }

    #[test]
    fn percent_decode_is_single_pass_and_strict() {
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("100%25").as_deref(), Some("100%"));
        assert_eq!(percent_decode("a+b").as_deref(), Some("a+b"));
        assert!(percent_decode("%zz").is_none());
        assert!(percent_decode("a%00b").is_none());
        assert!(percent_decode("%ff").is_none());
    }

    #[test]
    fn status_mapping_covers_execution_errors() {
        assert_eq!(
            status_for(&ViewError::RunBusy {
                detail: String::new()
            }),
            StatusCode(409)
        );
        assert_eq!(
            status_for(&ViewError::RunNotFound {
                detail: String::new()
            }),
            StatusCode(404)
        );
        assert_eq!(
            status_for(&ViewError::RunDenied {
                detail: String::new()
            }),
            StatusCode(403)
        );
    }
}
