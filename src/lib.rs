//! marqdo-view library: one package, one presentation pipeline, one renderer.
//!
//! Dependency direction (RFC-VIEW-0002 §5.1/§5.4):
//! `CLI/Server → Reader(assemble) → Protocol(Analysis/Trace) + CoreClient`,
//! and `Reader(model) → Renderer → Assets`. `Execution` may only be reached
//! from an explicit `POST /api/runs` handler; page loads and export never form
//! an execution edge.

pub mod cli;
pub mod config;
pub mod core_client;
pub mod error;
pub mod execution;
pub mod export;
pub mod protocol;
pub mod reader;
pub mod server;

pub use cli::Cli;
pub use error::{ViewError, ViewResult};

use cli::{ExportArgs, RuntimeOpts};
use export::ExportOptions;
use reader::assemble::DocumentService;

/// Dispatch a parsed CLI invocation. Returns the process exit code.
pub fn run(cli: Cli) -> ViewResult<i32> {
    let opts = cli.runtime();
    cli::validate_bind(&opts.host, opts.unsafe_expose)
        .map_err(|detail| ViewError::BindDenied { detail })?;
    match cli.command {
        Some(cli::Command::Verify(args)) => core_client::verify_command(&opts, &args),
        Some(cli::Command::Export(args)) => export_command(&opts, &args),
        None => browse_command(&opts),
    }
}

/// `marqdo view [PATH]`: open the read-only Live reader. Never executes.
fn browse_command(opts: &RuntimeOpts) -> ViewResult<i32> {
    let path = opts
        .path
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let service = DocumentService::open(&path, opts.marqdo_bin.as_deref())?;
    crate::server::serve(service, opts)
}

/// `marqdo view export PATH --out DIR`: static export through the same Reader
/// model. The export path has no execution capability by type (RFC §5.4).
fn export_command(opts: &RuntimeOpts, args: &ExportArgs) -> ViewResult<i32> {
    let path = args
        .path
        .clone()
        .or_else(|| opts.path.clone())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let service = DocumentService::open(&path, opts.marqdo_bin.as_deref())?;
    let summary = export::export(
        &service,
        &ExportOptions {
            out: args.out.clone(),
            no_annotations: args.no_annotations,
        },
    )?;
    println!(
        "marqdo view export: {} page(s), {} file(s) → {}",
        summary.pages,
        summary.files,
        summary.out.display()
    );
    println!("manifest: sha256:{}", summary.manifest_sha256);
    println!("executed: false (纯静态导出，未运行任何文档)");
    Ok(0)
}
