//! Public protocol adapters consumed by View.
//!
//! These modules are a **consumer** of the published CLI formats, never a
//! second publisher or implementation of them. Each public boundary has
//! exactly one current decoder and one domain projection:
//! Analysis v3 (`marqdo.analysis`) and Observation v1 (`marqdo.observation`).
//! Retired formats are rejected, never decoded through a fallback.

pub mod analysis_v3;
pub mod observation_v1;
pub mod tool_v1;
pub mod value_wire_v1;
