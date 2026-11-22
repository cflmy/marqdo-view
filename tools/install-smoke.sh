#!/usr/bin/env bash
# Install the built bundle with a frozen marqdo CLI into an isolated tool home,
# then prove the command is served by this program (not by the Core).
#
# Usage: MARQDO_BIN=/abs/path/marqdo tools/install-smoke.sh [tool.json]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${1:-$repo_root/dist/marqdo-view/tool.json}"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"

if [[ ! -x "$MARQDO_BIN" ]]; then
  echo "error: MARQDO_BIN is not executable: $MARQDO_BIN" >&2
  exit 1
fi
before="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"

mkdir -p "$repo_root/dist"
tool_home="$(mktemp -d "$repo_root/dist/smoke-tool-home.XXXXXX")"
cleanup() {
  rm -rf "$tool_home"
}
trap cleanup EXIT

export MARQDO_TOOL_HOME="$tool_home"
"$MARQDO_BIN" tool install "$manifest" >/dev/null
"$MARQDO_BIN" tool list | grep -q 'view'
help="$("$MARQDO_BIN" view --help)"
grep -q 'marqdo-view' <<<"$help"
grep -q 'export' <<<"$help"
grep -q 'verify' <<<"$help"
version="$("$MARQDO_BIN" view --version)"
grep -q 'marqdo-view' <<<"$version"

after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
if [[ "$before" != "$after" ]]; then
  echo "error: frozen core binary changed during install smoke test" >&2
  exit 1
fi

echo "install-smoke: PASS (core sha256 $before unchanged, tool home $tool_home)"
