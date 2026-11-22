#!/usr/bin/env bash
# Prove a frozen marqdo release binary is byte-identical before and after a
# command sequence (RFC-VIEW-0002 §2 / §9.4 / §10.2).
#
# Usage:
#   scripts/verify-frozen-cli.sh /abs/path/marqdo [-- EXPECTED_SHA256] -- CMD [ARGS...]
#   EXPECTED_SHA256=<hex> scripts/verify-frozen-cli.sh /abs/path/marqdo -- CMD [ARGS...]
#   scripts/verify-frozen-cli.sh /abs/path/marqdo          # version/SHA report only
#
# The script never rebuilds or modifies the CLI. It records the version, the
# SHA-256 before/after, and exits non-zero if the SHA changes or if an expected
# SHA (argument or EXPECTED_SHA256 env) does not match.
set -euo pipefail

core="${1:-}"
if [[ -z "$core" ]]; then
  echo "usage: $0 /abs/path/marqdo [-- EXPECTED_SHA256] -- CMD [ARGS...]" >&2
  exit 2
fi
shift

expected="${EXPECTED_SHA256:-}"
if [[ "${1:-}" == "--" && ( "${2:-}" =~ ^[0-9a-f]{64}$ ) ]]; then
  expected="$2"
  shift 2
fi
if [[ "${1:-}" == "--" ]]; then
  shift
fi

if [[ ! -x "$core" ]]; then
  echo "error: frozen CLI is not executable: $core" >&2
  exit 2
fi

sha_before="$(sha256sum "$core" | cut -d' ' -f1)"
version_line="$("$core" --version 2>&1 | head -1)"

if [[ -n "$expected" && "$expected" != "$sha_before" ]]; then
  echo "error: frozen CLI SHA-256 mismatch" >&2
  echo "  expected: $expected" >&2
  echo "  actual:   $sha_before" >&2
  exit 1
fi

if [[ $# -gt 0 ]]; then
  "$@"
fi

sha_after="$(sha256sum "$core" | cut -d' ' -f1)"
if [[ "$sha_before" != "$sha_after" ]]; then
  echo "error: frozen CLI binary changed during the command" >&2
  echo "  before: $sha_before" >&2
  echo "  after:  $sha_after" >&2
  exit 1
fi

printf 'verify-frozen-cli: PASS\n'
printf '  path:    %s\n' "$core"
printf '  version: %s\n' "$version_line"
printf '  sha256:  %s (unchanged)\n' "$sha_before"
