//! Contract suite: the frozen CLI black-box process contract, Analysis v2 /
//! Trace v2 validation, the View CLI surface, and architecture/asset gates.

#[path = "../common/mod.rs"]
mod common;

mod asset_integrity;
mod cli_surface;
mod core_contract;
mod no_coupling;
mod presentation_states;
mod trace_contract;
