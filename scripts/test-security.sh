#!/usr/bin/env bash
# Security/contract suite: HTTP boundary, CSRF/Origin, path escapes, XSS
# sanitization, export-no-exec, architectural coupling gates.
#
# Usage: scripts/test-security.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo test --locked --test security
cargo test --locked --test integration
cargo test --locked --test contract

if [[ -n "${MARQDO_FROZEN_BIN:-}" ]]; then
  MARQDO_BIN="$MARQDO_FROZEN_BIN" ./scripts/test-export.sh
else
  echo "note: MARQDO_FROZEN_BIN not set; skipped real-core export script"
fi
echo "security: PASS"
