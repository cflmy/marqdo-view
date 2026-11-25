//! Command-line surface owned entirely by View.
//!
//! The Core registers this program as the `view` tool and passes the arguments
//! after `view` unchanged. View therefore parses its own subcommands, options
//! and help; the Core never knows about `export`, `verify`, `--out` or
//! `--no-open` (RFC-VIEW-0001 §10.1).

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "marqdo-view",
    version,
    about = "Read .mq.md documents as documents: static Analysis v3, optional explicit Observation v1",
    long_about = "Marqdo View renders the original Markdown and attaches verified Analysis v3 facts. \
                  Running is never implicit: it requires an explicit user action. \
                  Export is always a pure read-only operation.",
    disable_help_subcommand = true,
    propagate_version = true
)]
pub struct Cli {
    /// A `.mq.md` document or a project directory (default: `.`).
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Development only: explicit path to the frozen `marqdo` binary.
    /// Installed Tool Protocol runs get the path from `MARQDO_BIN`.
    #[arg(long, value_name = "PATH", global = true)]
    pub marqdo_bin: Option<PathBuf>,

    /// Loopback host for the Live reader (default 127.0.0.1).
    #[arg(long, default_value = "127.0.0.1", global = true)]
    pub host: String,

    /// Port for the Live reader; `0` lets the OS choose and prints the real URL.
    #[arg(long, default_value_t = 7429, global = true)]
    pub port: u16,

    /// Do not open a browser (the full local URL is still printed).
    #[arg(long, global = true)]
    pub no_open: bool,

    /// Wall-clock limit for one explicit run, in seconds (RFC-VIEW-0002 §6.7).
    /// The frozen Core receives the same value via `--timeout`.
    #[arg(
        long,
        default_value_t = 30,
        global = true,
        value_parser = clap::value_parser!(u64).range(1..=120)
    )]
    pub run_timeout: u64,

    /// Advanced: allow a non-loopback bind. No execution API is opened by it.
    #[arg(long, global = true, hide = true)]
    pub unsafe_expose: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Write a static, read-only HTML site (never executes the document).
    Export(ExportArgs),
    /// Check CLI/Analysis/source binding only; never executes.
    Verify(VerifyArgs),
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// A `.mq.md` document or a project directory (default: parent PATH option).
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Output directory for the generated site.
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    pub out: PathBuf,

    /// Disable semantic annotations but keep the prose readable.
    #[arg(long)]
    pub no_annotations: bool,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    /// A `.mq.md` document or a project directory.
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,

    /// Also validate Markdown link targets (`check --refs`); may fail where the
    /// plain syntax check passes.
    #[arg(long)]
    pub refs: bool,

    /// Machine readable JSON on stdout.
    #[arg(long)]
    pub json: bool,
}

/// Options shared by every command after parsing.
#[derive(Debug, Clone)]
pub struct RuntimeOpts {
    pub path: Option<PathBuf>,
    pub marqdo_bin: Option<PathBuf>,
    pub host: String,
    pub port: u16,
    pub no_open: bool,
    pub unsafe_expose: bool,
    pub run_timeout_secs: u64,
}

impl Cli {
    pub fn runtime(&self) -> RuntimeOpts {
        RuntimeOpts {
            path: self.path.clone(),
            marqdo_bin: self.marqdo_bin.clone(),
            host: self.host.clone(),
            port: self.port,
            no_open: self.no_open,
            unsafe_expose: self.unsafe_expose,
            run_timeout_secs: self.run_timeout,
        }
    }
}

/// First-version bind policy: loopback only unless the hidden advanced flag is
/// given. Even then no execution API becomes reachable without a session token.
pub fn validate_bind(host: &str, unsafe_expose: bool) -> Result<(), String> {
    let loopback = matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]");
    if loopback {
        return Ok(());
    }
    if unsafe_expose {
        Ok(())
    } else {
        Err(format!(
            "refusing to bind non-loopback host {host:?}: the Live reader defaults to 127.0.0.1. \
             Use --unsafe-expose only if you understand that it exposes read-only pages on your network."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_browse_defaults() {
        let cli = Cli::parse_from(["marqdo-view"]);
        assert!(cli.path.is_none());
        assert!(cli.command.is_none());
        assert_eq!(cli.host, "127.0.0.1");
        assert_eq!(cli.port, 7429);
        assert_eq!(cli.run_timeout, 30);
    }

    #[test]
    fn parses_export_subcommand() {
        let cli = Cli::parse_from(["marqdo-view", "export", "examples", "--out", "site"]);
        match cli.command {
            Some(Command::Export(args)) => {
                assert_eq!(args.path.unwrap(), PathBuf::from("examples"));
                assert_eq!(args.out, PathBuf::from("site"));
                assert!(!args.no_annotations);
            }
            other => panic!("expected export, got {other:?}"),
        }
    }

    #[test]
    fn export_requires_out() {
        let err = Cli::try_parse_from(["marqdo-view", "export", "examples"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn parses_verify_and_global_options_anywhere() {
        let cli = Cli::parse_from([
            "marqdo-view",
            "verify",
            "index.mq.md",
            "--refs",
            "--json",
            "--marqdo-bin",
            "/opt/marqdo",
        ]);
        assert_eq!(
            cli.marqdo_bin.as_deref(),
            Some(std::path::Path::new("/opt/marqdo"))
        );
        match cli.command {
            Some(Command::Verify(args)) => {
                assert!(args.refs);
                assert!(args.json);
                assert_eq!(args.path.unwrap(), PathBuf::from("index.mq.md"));
            }
            other => panic!("expected verify, got {other:?}"),
        }
    }

    #[test]
    fn run_timeout_is_bounded_to_rfc_range() {
        assert!(Cli::try_parse_from(["marqdo-view", "--run-timeout", "0"]).is_err());
        assert!(Cli::try_parse_from(["marqdo-view", "--run-timeout", "121"]).is_err());
        let cli = Cli::parse_from(["marqdo-view", "--run-timeout", "45"]);
        assert_eq!(cli.runtime().run_timeout_secs, 45);
    }

    #[test]
    fn bind_policy_is_loopback_by_default() {
        assert!(validate_bind("127.0.0.1", false).is_ok());
        assert!(validate_bind("localhost", false).is_ok());
        assert!(validate_bind("0.0.0.0", false).is_err());
        assert!(validate_bind("0.0.0.0", true).is_ok());
    }
}
