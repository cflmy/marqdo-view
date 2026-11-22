//! One-way SSE transport for View-owned run events.
//!
//! We write the HTTP response directly to the socket (via tiny_http's writer)
//! instead of going through its buffered chunked writer: SSE requires each
//! frame and heartbeat to be flushed immediately, not when a buffer fills.
//! This module only transports already-sequenced events; it never queries the
//! Core, starts work or implements a polling loop (RFC-VIEW-0002 §6.4/§6.5).

use crate::error::ViewResult;
use crate::execution::controller::SseSubscription;
use crate::execution::events::{HEARTBEAT_FRAME, HEARTBEAT_INTERVAL_SECS};
use std::io::Write;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};
use tiny_http::Request;

pub fn heartbeat_interval() -> Duration {
    // Test/dev override only; production default is the RFC 15 seconds.
    if let Ok(raw) = std::env::var("MARQDO_VIEW_SSE_HEARTBEAT_SECS") {
        if let Ok(seconds) = raw.parse::<u64>() {
            if seconds > 0 {
                return Duration::from_secs(seconds);
            }
        }
    }
    Duration::from_secs(HEARTBEAT_INTERVAL_SECS)
}

/// Send a `200 text/event-stream` response and block while the client observes
/// it. Each frame is flushed immediately; the stream closes after the single
/// terminal event so the browser never re-runs anything.
pub fn respond(request: Request, subscription: SseSubscription) -> ViewResult<()> {
    let run_id = subscription.run_id.clone();
    let replay = subscription.replay.clone();
    let terminal = subscription.terminal;
    let receiver = subscription.receiver;
    let mut writer = request.into_writer();

    let head = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream; charset=utf-8\r\n",
        "Cache-Control: no-store\r\n",
        "X-Accel-Buffering: no\r\n",
        "X-Content-Type-Options: nosniff\r\n",
        "Referrer-Policy: no-referrer\r\n",
        "Connection: close\r\n",
        "\r\n"
    );
    if write_frame(&mut writer, head.as_bytes()).is_err() {
        return Ok(());
    }
    for frame in &replay {
        if write_frame(&mut writer, frame.as_bytes()).is_err() {
            return Ok(());
        }
    }
    let _ = run_id;
    if terminal {
        let _ = writer.flush();
        return Ok(());
    }

    let heartbeat = heartbeat_interval();
    let mut next_heartbeat = Instant::now() + heartbeat;
    loop {
        let now = Instant::now();
        let timeout = next_heartbeat.saturating_duration_since(now);
        match receiver.recv_timeout(timeout) {
            Ok(event) => {
                let is_terminal = event.state.is_terminal();
                let frame = event.to_sse(&subscription.run_id);
                if write_frame(&mut writer, frame.as_bytes()).is_err() {
                    return Ok(());
                }
                if is_terminal {
                    let _ = writer.flush();
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if write_frame(&mut writer, HEARTBEAT_FRAME.as_bytes()).is_err() {
                    return Ok(());
                }
                next_heartbeat = Instant::now() + heartbeat;
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = writer.flush();
                return Ok(());
            }
        }
        if Instant::now() >= next_heartbeat {
            if write_frame(&mut writer, HEARTBEAT_FRAME.as_bytes()).is_err() {
                return Ok(());
            }
            next_heartbeat = Instant::now() + heartbeat;
        }
    }
}

fn write_frame(writer: &mut Box<dyn Write + Send + 'static>, frame: &[u8]) -> std::io::Result<()> {
    writer.write_all(frame)?;
    writer.flush()
}

/// Kept as a named constant for the transport tests/documentation.
pub fn headers() -> [(&'static str, &'static str); 4] {
    [
        ("Content-Type", "text/event-stream; charset=utf-8"),
        ("Cache-Control", "no-store"),
        ("X-Accel-Buffering", "no"),
        ("X-Content-Type-Options", "nosniff"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_interval_is_fifteen_seconds_by_default() {
        std::env::remove_var("MARQDO_VIEW_SSE_HEARTBEAT_SECS");
        assert_eq!(heartbeat_interval(), Duration::from_secs(15));
    }

    #[test]
    fn headers_are_the_sse_contract() {
        let headers = headers();
        assert_eq!(headers[0].1, "text/event-stream; charset=utf-8");
        assert_eq!(headers[1].1, "no-store");
        assert_eq!(headers[2].1, "no");
    }
}
