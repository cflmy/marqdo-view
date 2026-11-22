//! marqdo-view library: a single package with one presentation pipeline.
//!
//! Layer direction (RFC-VIEW-0001 §6.1):
//! `CLI/Server → Application → Domain → ProtocolAdapter`, and
//! `Application → Presentation → Renderer → Assets`.
//! `main.rs` is only a dispatcher; integration tests and future server code use
//! this library so there is never a second renderer or a second parser.

pub mod app;
pub mod assets;
pub mod cli;
pub mod core_client;
pub mod document;
pub mod error;
pub mod presentation;
pub mod project;
pub mod render;
pub mod semantic;
pub mod server;
pub mod source_map;
pub mod trace;
pub mod version;

pub use cli::Cli;
pub use error::{ViewError, ViewResult};

/// Dispatch a parsed CLI invocation. Returns the process exit code.
pub fn run(cli: Cli) -> ViewResult<i32> {
    let opts = cli.runtime();
    cli::validate_bind(&opts.host, opts.unsafe_expose)
        .map_err(|detail| ViewError::BindDenied { detail })?;
    match cli.command {
        Some(cli::Command::Verify(args)) => app::verify(&opts, &args),
        Some(cli::Command::Export(args)) => app::export(&opts, &args),
        None => app::browse(&opts),
    }
}
