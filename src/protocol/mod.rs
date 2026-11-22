//! Public protocol adapters consumed by View.
//!
//! These modules are a **consumer** of the published CLI formats, never a
//! second publisher or implementation of them (RFC-VIEW-0002 §5.3).

pub mod analysis_v2;
pub mod tool_v1;
pub mod trace_v2;
