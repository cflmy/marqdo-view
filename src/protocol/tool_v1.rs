//! Current CLI Tool protocol constants (revision 1) and product identity.
//!
//! A CLI Tool declares exact contract revisions in `tool.json`
//! (`requires.api_revision`, optional `analysis`/`observation`). There is no
//! negotiation: the installed registration injects only `MARQDO_BIN` and
//! `MARQDO_API_REVISION`, and a mismatch is a hard, actionable error.

use crate::error::{ViewError, ViewResult};
use semver::Version;

/// Product range this View build consumes. The rc.3 contract family is the
/// first public pre-release with Observation v1 and Tool revision 1.
pub const PRODUCT_RANGE: &str = ">=3.0.0-rc.3,<4.0.0";

/// Lowest compatible product version (inclusive).
pub const MIN_PRODUCT: &str = "3.0.0-rc.3";

/// CLI Tool API revision implemented by this View build.
pub const API_REVISION: u32 = 1;
/// Public Analysis revision consumed by this View build.
pub const ANALYSIS_REVISION: u32 = 3;
/// Public Observation revision consumed by this View build.
pub const OBSERVATION_REVISION: u32 = 1;

/// Environment variables injected by the CLI Tool registration.
pub const ENV_MARQDO_BIN: &str = "MARQDO_BIN";
pub const ENV_MARQDO_API_REVISION: &str = "MARQDO_API_REVISION";

#[derive(Debug, Clone)]
pub struct CoreVersion {
    pub version: Version,
    /// Exact `marqdo --version` first line, kept for evidence output.
    pub raw: String,
}

/// Parse the output of `marqdo --version` (`marqdo 3.0.0-rc.4`).
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

/// True when `version` is inside the supported product range.
pub fn product_is_compatible(version: &Version) -> bool {
    let min = Version::parse(MIN_PRODUCT).expect("MIN_PRODUCT is valid SemVer");
    version.major == 3 && version >= &min
}

/// Validate the CLI-injected API revision, if present.
///
/// Absence is allowed for a directly-run development binary (`--marqdo-bin`);
/// a present-but-different revision is a hard error. The value is only a
/// static expectation check, never a negotiation input.
pub fn validate_api_revision_env(raw: Option<&str>) -> ViewResult<()> {
    let Some(raw) = raw else {
        return Ok(());
    };
    let value: u32 = raw
        .trim()
        .parse()
        .map_err(|_| ViewError::ProtocolIncompatible {
            detail: format!("{ENV_MARQDO_API_REVISION}={raw:?} is not a positive integer"),
        })?;
    if value != API_REVISION {
        return Err(ViewError::ProtocolIncompatible {
            detail: format!(
                "{ENV_MARQDO_API_REVISION}={value} is not supported; this View implements CLI Tool \
                 API revision {API_REVISION}. Reinstall the rebuilt Tool bundle."
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_version_line() {
        let version = parse_version_output("marqdo 3.0.0-rc.4\n").unwrap();
        assert_eq!(version.version.to_string(), "3.0.0-rc.4");
        assert!(product_is_compatible(&version.version));
    }

    #[test]
    fn rejects_retired_and_future_product_lines() {
        for accepted in ["3.0.0-rc.3", "3.0.0-rc.4", "3.0.0", "3.1.0-beta.1"] {
            assert!(
                product_is_compatible(&Version::parse(accepted).unwrap()),
                "{accepted} must be accepted"
            );
        }
        for rejected in ["2.9.1", "3.0.0-beta.1", "3.0.0-rc.2", "4.0.0"] {
            assert!(
                !product_is_compatible(&Version::parse(rejected).unwrap()),
                "{rejected} must be rejected"
            );
        }
    }

    #[test]
    fn api_revision_is_an_exact_expectation() {
        assert!(validate_api_revision_env(None).is_ok());
        assert!(validate_api_revision_env(Some("1")).is_ok());
        assert!(validate_api_revision_env(Some("2")).is_err());
        assert!(validate_api_revision_env(Some("one")).is_err());
    }
}
