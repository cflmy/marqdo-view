//! Local-session, CSRF, Origin/Host and response-header security.
//!
//! The Live server is loopback-only but still guards the one execution endpoint
//! (RFC-VIEW-0002 §6.8): an HttpOnly `SameSite=Strict` session cookie, a
//! per-session CSRF token required as `X-View-CSRF`, an exact same-origin
//! `Origin`, a valid loopback `Host`, JSON-only bounded POSTs and a strict CSP.

use crate::error::{ViewError, ViewResult};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tiny_http::Request;

pub const SESSION_COOKIE: &str = "mq_view_session";
pub const CSRF_HEADER: &str = "X-View-CSRF";
pub const MAX_SESSIONS: usize = 64;
pub const SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);
pub const MAX_POST_BODY_BYTES: usize = 64 * 1024;
pub const MAX_URL_BYTES: usize = 2048;
pub const MAX_QUERY_BYTES: usize = 4096;

#[derive(Debug, Clone)]
pub struct SessionView {
    pub id: String,
    pub csrf: String,
}

struct Session {
    csrf: String,
    last_seen: Instant,
}

pub struct SecurityState {
    sessions: Mutex<HashMap<String, Session>>,
    allowed_hosts: Vec<String>,
    origin: String,
}

impl SecurityState {
    pub fn new(port: u16) -> Self {
        let host = format!("127.0.0.1:{port}");
        Self {
            sessions: Mutex::new(HashMap::new()),
            allowed_hosts: vec![host.clone(), format!("localhost:{port}")],
            origin: format!("http://{host}"),
        }
    }

    pub fn allowed_hosts(&self) -> &[String] {
        &self.allowed_hosts
    }

    /// Validate `Host` for every request (DNS-rebinding defence).
    pub fn check_host(&self, request: &Request) -> ViewResult<()> {
        let host = header(request, "Host").ok_or_else(|| ViewError::RunDenied {
            detail: "missing Host header".to_string(),
        })?;
        if self
            .allowed_hosts
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(host.trim()))
        {
            Ok(())
        } else {
            Err(ViewError::RunDenied {
                detail: "invalid Host header".to_string(),
            })
        }
    }

    /// Browser page load: reuse a valid cookie or mint a new session. Returns
    /// the session plus the `Set-Cookie` value when a new one was created.
    pub fn page_session(&self, request: &Request) -> (SessionView, Option<String>) {
        let now = Instant::now();
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        sessions.retain(|_, session| now.duration_since(session.last_seen) < SESSION_TTL);
        if let Some(id) = cookie_value(request, SESSION_COOKIE) {
            if let Some(session) = sessions.get_mut(&id) {
                session.last_seen = now;
                return (
                    SessionView {
                        id,
                        csrf: session.csrf.clone(),
                    },
                    None,
                );
            }
        }
        let id = random_hex(32);
        let csrf = random_hex(32);
        sessions.insert(
            id.clone(),
            Session {
                csrf: csrf.clone(),
                last_seen: now,
            },
        );
        while sessions.len() > MAX_SESSIONS {
            let oldest = sessions
                .iter()
                .min_by_key(|(_, session)| session.last_seen)
                .map(|(id, _)| id.clone());
            match oldest {
                Some(id) => {
                    sessions.remove(&id);
                }
                None => break,
            }
        }
        let cookie =
            format!("{SESSION_COOKIE}={id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=43200");
        (SessionView { id, csrf }, Some(cookie))
    }

    /// Authenticated GET (page/SSE): an existing session cookie is required.
    pub fn require_session(&self, request: &Request) -> ViewResult<SessionView> {
        let id =
            cookie_value(request, SESSION_COOKIE).ok_or_else(|| ViewError::SessionRequired {
                detail: "缺少本地会话 Cookie；请刷新阅读页面后重试".to_string(),
            })?;
        let now = Instant::now();
        let mut sessions = self
            .sessions
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let session = sessions
            .get_mut(&id)
            .ok_or_else(|| ViewError::SessionRequired {
                detail: "本地会话已过期；请刷新阅读页面".to_string(),
            })?;
        if now.duration_since(session.last_seen) >= SESSION_TTL {
            sessions.remove(&id);
            return Err(ViewError::SessionRequired {
                detail: "本地会话已过期；请刷新阅读页面".to_string(),
            });
        }
        session.last_seen = now;
        Ok(SessionView {
            id,
            csrf: session.csrf.clone(),
        })
    }

    /// Authenticated JSON POST (run submission).
    pub fn authenticate_post(&self, request: &Request) -> ViewResult<SessionView> {
        self.authenticate_post_impl(request, true)
    }

    /// Authenticated control POST without a JSON body (cancel is a bare POST).
    pub fn authenticate_control(&self, request: &Request) -> ViewResult<SessionView> {
        self.authenticate_post_impl(request, false)
    }

    fn authenticate_post_impl(
        &self,
        request: &Request,
        require_json: bool,
    ) -> ViewResult<SessionView> {
        self.check_host(request)?;
        let origin = header(request, "Origin").ok_or_else(|| ViewError::RunDenied {
            detail: "缺少 Origin，跨站来源被拒绝".to_string(),
        })?;
        if !origin
            .trim_end_matches('/')
            .eq_ignore_ascii_case(&self.origin)
        {
            return Err(ViewError::RunDenied {
                detail: "Origin 与本地服务不一致，跨站请求被拒绝".to_string(),
            });
        }
        if let Some(site) = header(request, "Sec-Fetch-Site") {
            if site.eq_ignore_ascii_case("cross-site") {
                return Err(ViewError::RunDenied {
                    detail: "Sec-Fetch-Site 表示跨站请求，已拒绝".to_string(),
                });
            }
        }
        if require_json
            && !header(request, "Content-Type")
                .map(|value| value.to_ascii_lowercase().starts_with("application/json"))
                .unwrap_or(false)
        {
            return Err(ViewError::Unsupported {
                detail: "运行请求必须是 application/json".to_string(),
            });
        }
        let session = self.require_session(request)?;
        let csrf = header(request, CSRF_HEADER).ok_or_else(|| ViewError::RunDenied {
            detail: "缺少 X-View-CSRF，运行被拒绝".to_string(),
        })?;
        if !constant_time_eq(csrf.as_bytes(), session.csrf.as_bytes()) {
            return Err(ViewError::RunDenied {
                detail: "X-View-CSRF 无效，运行被拒绝".to_string(),
            });
        }
        Ok(session)
    }

    /// Origin check for read-only JSON APIs (no CSRF needed, no state change).
    pub fn check_same_origin(&self, request: &Request) -> ViewResult<()> {
        if let Some(origin) = header(request, "Origin") {
            if !origin
                .trim_end_matches('/')
                .eq_ignore_ascii_case(&self.origin)
            {
                return Err(ViewError::RunDenied {
                    detail: "跨站读取被拒绝".to_string(),
                });
            }
        }
        Ok(())
    }
}

pub fn header(request: &Request, name: &'static str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv(name))
        .map(|header| header.value.as_str().trim().to_string())
}

pub fn cookie_value(request: &Request, name: &str) -> Option<String> {
    let raw = header(request, "Cookie")?;
    for pair in raw.split(';') {
        let (key, value) = pair.split_once('=')?;
        if key.trim() == name {
            let value = value.trim();
            if !value.is_empty()
                && value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            {
                return Some(value.to_string());
            }
            return None;
        }
    }
    None
}

pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Random lowercase hex from `/dev/urandom` (with a non-constant fallback).
pub fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    #[cfg(unix)]
    {
        use std::io::Read as _;
        if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
            if file.read_exact(&mut buffer).is_ok() {
                return hex_encode(&buffer);
            }
        }
    }
    let thread = format!("{:?}", std::thread::current().id());
    let material = format!(
        "{}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0),
        thread,
        fallback_counter()
    );
    crate::reader::source::sha256_hex(material.as_bytes())[..bytes * 2].to_string()
}

fn fallback_counter() -> u64 {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// HTTP security headers for every response (RFC-VIEW-0002 §6.8).
pub fn security_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Content-Security-Policy",
            "default-src 'self'; connect-src 'self'; style-src 'self'; script-src 'self'; img-src 'self' data:; font-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        ),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Frame-Options", "DENY"),
        ("Cross-Origin-Resource-Policy", "same-origin"),
        ("Cross-Origin-Opener-Policy", "same-origin"),
        ("Cache-Control", "no-store"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_is_strict() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }

    #[test]
    fn random_tokens_are_lower_hex_and_unique() {
        let a = random_hex(32);
        let b = random_hex(32);
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
        assert!(a.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn session_cookie_is_parsed_from_a_real_request() {
        let request: tiny_http::Request = tiny_http::TestRequest::new()
            .with_header(
                tiny_http::Header::from_bytes(
                    &b"Cookie"[..],
                    &b"a=b; mq_view_session=ok_1; other=x"[..],
                )
                .unwrap(),
            )
            .into();
        assert_eq!(
            cookie_value(&request, SESSION_COOKIE).as_deref(),
            Some("ok_1")
        );
    }
}
