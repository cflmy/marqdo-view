//! Reader layer: controlled source access, Markdown rendering, the one
//! presentation model, assembly from public protocols, and the one renderer.
//!
//! Live and Export share every type here; only link policy and interaction
//! availability differ (RFC-VIEW-0002 §4.3/§5.4).

pub mod assemble;
pub mod assets;
pub mod links;
pub mod markdown;
pub mod math;
pub mod media;
pub mod model;
pub mod render;
pub mod snapshot;
pub mod source;
