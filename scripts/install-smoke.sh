#!/usr/bin/env bash
# Install the built bundle with a frozen marqdo CLI into an isolated tool home,
# then prove the command is served by this program (not by the Core).
#
# The frozen CLI is the current Tool revision 1 baseline: it injects
# MARQDO_API_REVISION=1 and MARQDO_BIN only, with no legacy
# MARQDO_PROTOCOL_* negotiation variables.
#
# Usage: MARQDO_BIN=/abs/path/marqdo scripts/install-smoke.sh [tool.json]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest="${1:-$repo_root/dist/marqdo-view/tool.json}"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"

if [[ ! -x "$MARQDO_BIN" ]]; then
  echo "error: MARQDO_BIN is not executable: $MARQDO_BIN" >&2
  exit 1
fi
before="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"

# The packaged manifest must declare the current exact contracts.
python3 - "$manifest" <<'PY'
import json
import sys

tool = json.load(open(sys.argv[1]))
assert tool["schema"] == "marqdo.cli-tool", tool
assert tool["schema_version"] == 1, tool
assert "compatibility" not in tool, "retired compatibility field present"
assert tool["requires"] == {"api_revision": 1, "analysis": 3, "observation": 1}, tool["requires"]
print("manifest: PASS (revision 1 requires api_revision=1 analysis=3 observation=1)")
PY

mkdir -p "$repo_root/dist"
tool_home="$(mktemp -d "$repo_root/dist/smoke-tool-home.XXXXXX")"
cleanup() {
  rm -rf "$tool_home"
}
trap cleanup EXIT

export MARQDO_TOOL_HOME="$tool_home"
"$MARQDO_BIN" tool install "$manifest" >/dev/null
"$MARQDO_BIN" tool list | grep -q '^view '
help="$("$MARQDO_BIN" view --help)"
grep -q 'marqdo-view' <<<"$help"
grep -q 'export' <<<"$help"
grep -q 'verify' <<<"$help"
version="$("$MARQDO_BIN" view --version)"
grep -q 'marqdo-view' <<<"$version"

# Prove the current dispatch environment contract: the CLI injects
# MARQDO_API_REVISION=1 (and MARQDO_BIN), never the retired negotiation vars.
probe_dir="$tool_home/probe"
mkdir -p "$probe_dir/bin"
cat >"$probe_dir/bin/api-probe" <<'PROBE'
#!/usr/bin/env bash
printf 'MARQDO_API_REVISION=%s\n' "${MARQDO_API_REVISION:-<unset>}"
printf 'MARQDO_BIN=%s\n' "${MARQDO_BIN:-<unset>}"
printf 'retired=%s\n' "${MARQDO_PROTOCOL_ANALYSIS:-}${MARQDO_PROTOCOL_TRACE:-}${MARQDO_TOOL_PROTOCOL:-}"
PROBE
chmod 755 "$probe_dir/bin/api-probe"
cat >"$probe_dir/tool.json" <<'PROBE_JSON'
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "api-probe",
  "version": "0.0.1",
  "command": "api-probe",
  "entrypoint": "bin/api-probe",
  "requires": {"api_revision": 1}
}
PROBE_JSON
"$MARQDO_BIN" tool install "$probe_dir/tool.json" >/dev/null
probe="$("$MARQDO_BIN" api-probe)"
grep -qx 'MARQDO_API_REVISION=1' <<<"$probe"
grep -q '^MARQDO_BIN=/' <<<"$probe"
grep -qx 'retired=' <<<"$probe"
"$MARQDO_BIN" tool remove api-probe >/dev/null

after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
if [[ "$before" != "$after" ]]; then
  echo "error: frozen core binary changed during install smoke test" >&2
  exit 1
fi

echo "install-smoke: PASS (view installed/dispatched, api_revision 1 injected, core sha256 $before unchanged, tool home $tool_home)"
