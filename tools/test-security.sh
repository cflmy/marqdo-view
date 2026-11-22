#!/usr/bin/env bash
# Security/contract suite: HTTP boundary, CSRF/Origin, path escapes, XSS
# sanitization, export-no-exec, architectural coupling gates.
#
# Usage: tools/test-security.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo test --locked --test http_live
cargo test --locked --test rendering
cargo test --locked --test export
cargo test --locked --test no_coupling
cargo test --locked --test asset_integrity

if [[ -n "${MARQDO_FROZEN_BIN:-}" ]]; then
  MARQDO_BIN="$MARQDO_FROZEN_BIN" ./tools/test-export.sh
else
  echo "note: MARQDO_FROZEN_BIN not set; skipped real-core export script"
fi
echo "security: PASS"
