#!/usr/bin/env bash
# RFC-VIEW-0002 §9.4 one-command acceptance.
#
# Usage:
#   MARQDO_BIN=/abs/path/to/frozen/marqdo scripts/acceptance.sh
#
# Optional:
#   VIEW_EVIDENCE_OUT=path   override dist/acceptance/manifest.json
#   SKIP_BROWSER=1         skip Chromium suite (recorded as SKIP, not PASS)
#
# The script never modifies or recompiles the frozen CLI; it records its
# SHA-256 before and after and fails if it changes.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo release binary}"
[[ -x "$MARQDO_BIN" ]] || { echo "error: MARQDO_BIN not executable" >&2; exit 1; }
MARQDO_BIN="$(realpath "$MARQDO_BIN")"
export MARQDO_BIN
"$repo_root/scripts/frozen-beta.sh" verify "$MARQDO_BIN"

work="$repo_root/dist/acceptance"
rm -rf "$work"
mkdir -p "$work"

statuses=""
record() { statuses+="$1=$2"$'\n'; echo "[acceptance] $1: $2"; }

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
cli_source_sha="$(cat "$(dirname "$MARQDO_BIN")/SOURCE_SHA")"
view_source_sha="$(git rev-parse HEAD 2>/dev/null || echo unknown)"

export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_HOME="${CARGO_HOME:-$repo_root/.cargo-home}"

# 0. code quality / unit / contract tests
check_wrong_cli() {
  mkdir -p "$work/wrong-cli"
  cp "$(dirname "$MARQDO_BIN")/SOURCE_SHA" "$work/wrong-cli/SOURCE_SHA"
  cat >"$work/wrong-cli/marqdo" <<'FAKE'
#!/usr/bin/env bash
touch "$(dirname "$0")/executed"
FAKE
  chmod +x "$work/wrong-cli/marqdo"
  if scripts/frozen-beta.sh verify "$work/wrong-cli/marqdo"; then
    echo "error: mismatched CLI passed the baseline check" >&2
    return 1
  fi
  test ! -e "$work/wrong-cli/executed"
}
run_step wrong_cli_refused check_wrong_cli
run_step fmt cargo fmt --all -- --check
run_step clippy cargo clippy --all-targets -- -D warnings
run_step test cargo test --locked
run_step build cargo build --release --locked

# 1. controlled Tool bundle + frozen-CLI install smoke
run_step bundle env MARQDO_BIN="$MARQDO_BIN" ./scripts/package.sh
bundle="$repo_root/dist/marqdo-view"
read -r version target < <(python3 -c 'import json; p=json.load(open("dist/marqdo-view/RELEASE_INFO.json")); print(p["tool_version"], p["target"])')
archive="$repo_root/dist/marqdo-view-$version-$target.tar.gz"
archive_sha="$(sha256sum "$archive" | cut -d' ' -f1)"

# 2. clean install from the *archive*, in a moved temporary directory
mkdir -p "$work/extracted"
tar -xzf "$archive" -C "$work/extracted"
extracted_bundle="$work/extracted/marqdo-view"
(cd "$extracted_bundle" && sha256sum -c SHA256SUMS)
tool_home="$work/tool-home"
mkdir -p "$tool_home"
export MARQDO_TOOL_HOME="$tool_home"
export VIEW_USE_INSTALLED_TOOL=1
run_step archive_install "$MARQDO_BIN" tool install "$extracted_bundle/tool.json"
check_tool_list() {
  grep -q '^view ' <<<"$("$MARQDO_BIN" tool list)"
}
check_view_help() {
  grep -q 'marqdo-view' <<<"$("$MARQDO_BIN" view --help)"
}
run_step tool_list check_tool_list
run_step view_help check_view_help
run_step verify "$MARQDO_BIN" view verify examples/mixed-modes/index.mq.md
run_step export "$MARQDO_BIN" view export examples/control-flow --out "$work/site"
run_step examples_check env MARQDO_BIN="$MARQDO_BIN" ./scripts/check-examples.sh
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
if [[ -n "$browse_url" ]] && curl -sf "${browse_url}api/documents" | grep -q '"schema": "marqdo.view-documents"'; then
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
  if python3 "$repo_root/scripts/acceptance-run.py" "$run_url"
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
run_step no_exec_suite env MARQDO_BIN="$MARQDO_BIN" ./scripts/test-no-exec.sh
run_step security_suite ./scripts/test-security.sh
if [[ "${SKIP_BROWSER:-0}" == "1" ]]; then
  record browser SKIP
else
  run_step browser env MARQDO_BIN="$MARQDO_BIN" ./scripts/test-browser.sh
fi

check_tampered_install() {
  local output
  printf 'x' >> "$tool_home/view/bin/marqdo-view" || return 1
  if output="$("$MARQDO_BIN" view --version 2>&1)"; then
    echo "error: tampered Tool installation was dispatched" >&2
    return 1
  fi
  grep -q 'bundle hash mismatch' <<<"$output"
}
run_step tool_tamper_refused check_tampered_install
run_step tool_remove "$MARQDO_BIN" tool remove view

# 6. frozen core must be byte-identical
core_after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
if [[ "$core_before" != "$core_after" ]]; then
  record core_sha FAIL
  echo "error: frozen core binary changed during acceptance" >&2
  exit 1
fi
record core_sha PASS

view_binary_sha="$(sha256sum "$bundle/bin/marqdo-view" | cut -d' ' -f1)"
evidence_path="${VIEW_EVIDENCE_OUT:-$work/manifest.json}"
mkdir -p "$(dirname "$evidence_path")"
python3 - "$evidence_path" "$statuses" "$view_source_sha" "$view_binary_sha" \
  "$cli_source_sha" "$core_before" "$core_after" "$target" "$archive_sha" <<'PY_EVIDENCE'
import datetime
import json
import pathlib
import sys

(path, gates, view_source_sha, view_binary_sha, cli_source_sha,
 core_before, core_after, target, archive_sha) = sys.argv[1:]
path = pathlib.Path(path)
statuses = dict(line.split("=", 1) for line in gates.splitlines())
manifest = {
    "schema": "marqdo.view-conformance",
    "schema_version": 1,
    "view_source_sha": view_source_sha,
    "view_binary_sha256": view_binary_sha,
    "cli_source_sha": cli_source_sha,
    "cli_binary_sha256_before": core_before,
    "cli_binary_sha256_after": core_after,
    "tool_protocol": 1,
    "analysis_protocol": 2,
    "trace_protocol": 2,
    "target": target,
    "archive_sha256": archive_sha,
    "generated_at_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "tests": {
        "contract": statuses.get("test", "FAIL"),
        "examples": statuses.get("examples_check", "FAIL"),
        "browser": statuses.get("browser", "SKIP"),
        "export": statuses.get("export_suite", "FAIL"),
        "no_exec": statuses.get("no_exec_suite", "FAIL"),
        "security": statuses.get("security_suite", "FAIL"),
    },
    "gates": statuses,
    "commands": {
        "script": "scripts/acceptance.sh",
        "notes": "Generated from real command exit statuses; install/verify/export/run use the frozen CLI archive copied to an isolated tool home.",
    },
}
path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
print("evidence manifest:", path)
PY_EVIDENCE

echo
if [[ "${SKIP_BROWSER:-0}" == "1" ]]; then
  echo "acceptance: PARTIAL (browser skipped)"
else
  echo "acceptance: PASS"
fi
echo "evidence: $evidence_path"
echo "archive sha256: $archive_sha"
