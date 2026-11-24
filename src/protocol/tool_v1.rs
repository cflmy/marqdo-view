//! Product/protocol version negotiation with the frozen CLI.
//!
//! Version axes are independent (RFC-VIEW-0001 §4): product SemVer,
//! Analysis major, Trace major and Tool protocol. We never infer one from
//! another and never silently fall back to a latest-looking JSON.

use crate::error::{ViewError, ViewResult};
use semver::Version;

/// Product range this View build targets. Must match `tool.json`.
pub const PRODUCT_RANGE: &str = ">=3.0.0-beta.1,<4.0.0";

/// Lowest compatible product version (inclusive).
pub const MIN_PRODUCT: &str = "3.0.0-beta.1";

pub const TOOL_PROTOCOL: u32 = 1;
pub const ANALYSIS_PROTOCOL: u32 = 2;
pub const ANALYSIS_PROTOCOLS: [u32; 2] = [2, 3];
pub const TRACE_PROTOCOL: u32 = 2;

/// Environment variables injected by CLI Tool Protocol v1.
pub const ENV_MARQDO_BIN: &str = "MARQDO_BIN";
pub const ENV_TOOL_PROTOCOL: &str = "MARQDO_TOOL_PROTOCOL";
pub const ENV_PROTOCOL_ANALYSIS: &str = "MARQDO_PROTOCOL_ANALYSIS";
pub const ENV_PROTOCOL_TRACE: &str = "MARQDO_PROTOCOL_TRACE";

#[derive(Debug, Clone)]
pub struct CoreVersion {
    pub version: Version,
    /// Exact `marqdo --version` first line, kept for evidence output.
    pub raw: String,
}

/// Parse the output of `marqdo --version` (`marqdo 3.0.0-beta.1`).
pub fn parse_version_output(output: &str) -> ViewResult<CoreVersion> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| ViewError::ProtocolIncompatible {
            detail: "core printed no version text".to_string(),
        })?;
    let token = line
        .split_whitespace()
        .last()
        .ok_or_else(|| ViewError::ProtocolIncompatible {
            detail: format!("cannot parse core version from {line:?}"),
        })?;
    let version = Version::parse(token).map_err(|err| ViewError::ProtocolIncompatible {
        detail: format!("core version {token:?} is not SemVer: {err}"),
    })?;
    Ok(CoreVersion {
        version,
        raw: line.to_string(),
    })
}

/// True when `version` is inside the product range without relying on
/// prerelease quirks of `semver::VersionReq`: any 3.x at or above the first
/// beta is accepted, so a later `3.1.0-beta.1` keeps working.
pub fn product_is_compatible(version: &Version) -> bool {
    let min = Version::parse(MIN_PRODUCT).expect("MIN_PRODUCT is valid SemVer");
    version.major == 3 && version >= &min
}

/// Validate the negotiated environment for Tool Protocol v1.
///
/// Absence of `MARQDO_PROTOCOL_*` is allowed for a directly-run development
/// binary (`--marqdo-bin`), but presence of an unknown major is a hard error.
pub fn negotiate_protocol_env(
    tool_protocol: Option<&str>,
    analysis_protocol: Option<&str>,
    trace_protocol: Option<&str>,
) -> ViewResult<()> {
    if let Some(raw) = tool_protocol {
        let value = parse_env_u32(ENV_TOOL_PROTOCOL, raw)?;
        if value != TOOL_PROTOCOL {
            return Err(ViewError::ProtocolIncompatible {
                detail: format!(
                    "{ENV_TOOL_PROTOCOL}={value} is not supported (this View implements protocol {TOOL_PROTOCOL})"
                ),
            });
        }
    }
    if let Some(raw) = analysis_protocol {
        let value = parse_env_u32(ENV_PROTOCOL_ANALYSIS, raw)?;
        if !ANALYSIS_PROTOCOLS.contains(&value) {
            return Err(ViewError::ProtocolIncompatible {
                detail: format!(
                    "{ENV_PROTOCOL_ANALYSIS}={value} is not supported (this View implements Analysis v2/v3)"
                ),
            });
        }
    }
    if let Some(raw) = trace_protocol {
        let value = parse_env_u32(ENV_PROTOCOL_TRACE, raw)?;
        if value != TRACE_PROTOCOL {
            return Err(ViewError::ProtocolIncompatible {
                detail: format!(
                    "{ENV_PROTOCOL_TRACE}={value} is not supported (this View requires Trace v{TRACE_PROTOCOL})"
                ),
            });
        }
    }
    Ok(())
}

fn parse_env_u32(name: &str, raw: &str) -> ViewResult<u32> {
    raw.trim()
        .parse::<u32>()
        .map_err(|_| ViewError::ProtocolIncompatible {
            detail: format!("{name}={raw:?} is not a positive integer protocol version"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_version_line() {
        let version = parse_version_output("marqdo 3.0.0-beta.1\n").unwrap();
        assert_eq!(version.version.to_string(), "3.0.0-beta.1");
        assert!(product_is_compatible(&version.version));
    }

    #[test]
    fn rejects_stable_only_reinterpretation() {
        // 2.9.1 is the old product line and must never be accepted.
        let old = Version::parse("2.9.1").unwrap();
        assert!(!product_is_compatible(&old));
        // A future major is rejected too.
        assert!(!product_is_compatible(&Version::parse("4.0.0").unwrap()));
        // Later prereleases in the 3.x line remain compatible.
        assert!(product_is_compatible(
            &Version::parse("3.1.0-beta.1").unwrap()
        ));
        assert!(product_is_compatible(&Version::parse("3.0.0").unwrap()));
    }

    #[test]
    fn negotiates_known_protocols_only() {
        assert!(negotiate_protocol_env(Some("1"), Some("2"), Some("2")).is_ok());
        assert!(negotiate_protocol_env(None, None, None).is_ok());
        assert!(negotiate_protocol_env(Some("1"), Some("3"), Some("2")).is_ok());
        assert!(negotiate_protocol_env(Some("2"), None, None).is_err());
        assert!(negotiate_protocol_env(None, Some("1"), None).is_err());
        assert!(negotiate_protocol_env(None, None, Some("3")).is_err());
        assert!(negotiate_protocol_env(None, Some("two"), None).is_err());
    }
}
