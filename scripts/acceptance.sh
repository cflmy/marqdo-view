#!/usr/bin/env bash
# RFC-VIEW-0001 §18.1 one-command acceptance.
#
# Usage:
#   MARQDO_BIN=/abs/path/to/frozen/marqdo scripts/acceptance.sh
#
# Optional:
#   CLI_SOURCE_SHA=<sha>   override the CLI source SHA for the evidence manifest
#   SKIP_BROWSER=1         skip Chromium suite (recorded as SKIP, not PASS)
#
# The script never modifies or recompiles the frozen CLI; it records its
# SHA-256 before and after and fails if it changes.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo release binary}"
[[ -x "$MARQDO_BIN" ]] || { echo "error: MARQDO_BIN not executable" >&2; exit 1; }

work="$repo_root/dist/acceptance"
rm -rf "$work"
mkdir -p "$work"

statuses=""
record() { statuses="${statuses}$1=$2\n"; echo "[acceptance] $1: $2"; }

run_step() {
  local name="$1"
  shift
  echo "==> $name"
  if "$@"; then
    record "$name" PASS
  else
    record "$name" FAIL
    return 1
  fi
}

core_before="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
cli_source_sha="${CLI_SOURCE_SHA:-}"
if [[ -z "$cli_source_sha" && -f "$(dirname "$MARQDO_BIN")/SOURCE_SHA" ]]; then
  cli_source_sha="$(tr -d '[:space:]' < "$(dirname "$MARQDO_BIN")/SOURCE_SHA")"
fi
cli_source_sha="${cli_source_sha:-unknown}"
view_source_sha="$(git rev-parse HEAD 2>/dev/null || echo unknown)"

export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_HOME="${CARGO_HOME:-$repo_root/.cargo-home}"

# 0. code quality / unit / contract tests
run_step fmt cargo fmt --all -- --check
run_step clippy cargo clippy --all-targets -- -D warnings
run_step test cargo test --locked
run_step build cargo build --release --locked

# 1. controlled Tool bundle + frozen-CLI install smoke
run_step bundle env MARQDO_BIN="$MARQDO_BIN" ./scripts/package.sh
bundle="$repo_root/dist/marqdo-view"
archive="$(ls -t "$repo_root"/dist/marqdo-view-*-x86_64-unknown-linux-gnu.tar.gz | head -1)"
archive_sha="$(sha256sum "$archive" | cut -d' ' -f1)"

# 2. clean install from the *archive*, in a moved temporary directory
mkdir -p "$work/extracted"
tar -xzf "$archive" -C "$work/extracted"
extracted_bundle="$work/extracted/marqdo-view"
tool_home="$work/tool-home"
mkdir -p "$tool_home"
export MARQDO_TOOL_HOME="$tool_home"
run_step archive_install "$MARQDO_BIN" tool install "$extracted_bundle/tool.json"
run_step tool_list bash -c "'$MARQDO_BIN' tool list | grep -q view"
run_step view_help bash -c "'$MARQDO_BIN' view --help | grep -q 'marqdo-view'"
run_step verify "$MARQDO_BIN" view verify examples/mixed-modes/index.mq.md
run_step export "$MARQDO_BIN" view export examples/control-flow --out "$work/site"
test -f "$work/site/manifest.json"

# 3. Live browse smoke (no execution on open)
browse_log="$work/browse.log"
"$MARQDO_BIN" view examples/welcome --port 0 --no-open >"$browse_log" 2>&1 &
browse_pid=$!
trap 'kill "$browse_pid" 2>/dev/null || true' EXIT
browse_url=""
for _ in $(seq 1 100); do
  browse_url="$(grep -o 'http://127.0.0.1:[0-9]*/' "$browse_log" | head -1 || true)"
  [[ -n "$browse_url" ]] && break
  sleep 0.1
done
if [[ -n "$browse_url" ]] && curl -sf "${browse_url}api/health" | grep -q '"ok": true'; then
  record browse PASS
else
  record browse FAIL
  cat "$browse_log" >&2
  exit 1
fi
kill "$browse_pid" 2>/dev/null || true
wait "$browse_pid" 2>/dev/null || true
trap - EXIT

# 4. Explicit real run through the installed tool + CSRF flow
run_log="$work/run.log"
"$MARQDO_BIN" view examples/control-flow --port 0 --no-open >"$run_log" 2>&1 &
run_pid=$!
trap 'kill "$run_pid" 2>/dev/null || true' EXIT
run_url=""
for _ in $(seq 1 100); do
  run_url="$(grep -o 'http://127.0.0.1:[0-9]*/' "$run_log" | head -1 || true)"
  [[ -n "$run_url" ]] && break
  sleep 0.1
done
if [[ -n "$run_url" ]]; then
  if python3 - "$run_url" <<'PY'
import json
import re
import sys
import urllib.request

url = sys.argv[1]

def get(path):
    with urllib.request.urlopen(url.rstrip("/") + path, timeout=30) as response:
        return response.read().decode("utf-8")

page = get("/")
match = re.search(r'name="mq-csrf-token" content="([0-9a-f]+)"', page)
assert match, "csrf token missing"
token = match.group(1)
body = json.dumps({"path": "index.mq.md", "mode": "pure", "confirm": True}).encode()
request = urllib.request.Request(
    url.rstrip("/") + "/api/run",
    data=body,
    headers={"Content-Type": "application/json", "X-MQ-CSRF": token},
    method="POST",
)
with urllib.request.urlopen(request, timeout=120) as response:
    result = json.loads(response.read().decode("utf-8"))
assert result["ok"] is True, result
assert result["state"] == "succeeded", result
run_page = get(result["redirect"])
assert 'id="mq-run-panel"' in run_page, "run panel missing"
assert 'data-run-state="succeeded"' in run_page, "succeeded state missing"
assert "mq-run-chip" in run_page, "trace chips missing"
print("explicit real run: PASS (state=%s events=%s)" % (result["state"], result["events"]))
PY
  then
    record real_run PASS
  else
    record real_run FAIL
    exit 1
  fi
else
  record real_run FAIL
  cat "$run_log" >&2
  exit 1
fi
kill "$run_pid" 2>/dev/null || true
wait "$run_pid" 2>/dev/null || true
trap - EXIT

# 5. project-local verification suites
run_step export_suite env MARQDO_BIN="$MARQDO_BIN" ./scripts/test-export.sh
run_step security_suite ./scripts/test-security.sh
if [[ "${SKIP_BROWSER:-0}" == "1" ]]; then
  record browser SKIP
elif [[ -d "$repo_root/tests/browser/node_modules/playwright" && -d "$repo_root/tests/.cache/pw-browsers" ]]; then
  run_step browser env MARQDO_BIN="$MARQDO_BIN" ./scripts/test-browser.sh
else
  record browser SKIP
fi

# 6. frozen core must be byte-identical
core_after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
if [[ "$core_before" != "$core_after" ]]; then
  record core_sha FAIL
  echo "error: frozen core binary changed during acceptance" >&2
  exit 1
fi
record core_sha PASS

view_binary_sha="$(sha256sum "$bundle/bin/marqdo-view" | cut -d' ' -f1)"
target="$(python3 -c 'import json;print(json.load(open("dist/marqdo-view/RELEASE_INFO.json"))["target"])')"
evidence_dir="$repo_root/docs/evidence/$cli_source_sha"
mkdir -p "$evidence_dir"
python3 - "$evidence_dir/manifest.json" <<PY
import datetime
import json
import pathlib

path = pathlib.Path("$evidence_dir/manifest.json")
statuses = {}
for line in """${statuses}""".splitlines():
    if "=" in line:
        name, value = line.split("=", 1)
        statuses[name] = value
manifest = {
    "schema": "marqdo.view-conformance",
    "schema_version": 1,
    "view_source_sha": "$view_source_sha",
    "view_binary_sha256": "$view_binary_sha",
    "cli_source_sha": "$cli_source_sha",
    "cli_binary_sha256_before": "$core_before",
    "cli_binary_sha256_after": "$core_after",
    "tool_protocol": 1,
    "analysis_protocol": 2,
    "trace_protocol": 2,
    "target": "$target",
    "archive_sha256": "$archive_sha",
    "generated_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "tests": {
        "contract": statuses.get("test", "FAIL"),
        "browser": statuses.get("browser", "SKIP"),
        "export": statuses.get("export_suite", "FAIL"),
        "security": statuses.get("security_suite", "FAIL"),
    },
    "gates": statuses,
    "commands": {
        "script": "scripts/acceptance.sh",
        "notes": "Generated from real command exit statuses; install/verify/export/run use the frozen CLI archive copied to an isolated tool home.",
    },
}
path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\\n")
print("evidence manifest:", path)
PY

echo
echo "acceptance: PASS"
echo "evidence: $evidence_dir/manifest.json"
echo "archive sha256: $archive_sha"
