//! Execution layer: the only path from an explicit user request to a Core
//! subprocess.
//!
//! `DocumentService` and the renderer never depend on this module; only the
//! `POST /api/runs` server handler can reach it (RFC-VIEW-0002 §5.1).
//!
//! - `request`: schema/target/parameter/mode/grant validation
//! - `process`: the one cancellable Core process adapter with bounded capture
//! - `events`: bounded, sequenced View-owned run events and SSE frames
//! - `controller`: single-job lifecycle, dedupe, cancel, Observation v1 binding

pub mod controller;
pub mod events;
pub mod export_candidate;
pub mod process;
pub mod request;
