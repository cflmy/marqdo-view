//! Contract suite: the frozen CLI black-box process contract, Analysis v3 /
//! Observation v1 validation, the View CLI surface, and architecture/asset gates.

#[path = "../common/mod.rs"]
mod common;

mod asset_integrity;
mod cli_surface;
mod core_contract;
mod no_coupling;
mod observation_contract;
mod presentation_states;
