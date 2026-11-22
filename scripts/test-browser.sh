#!/usr/bin/env bash
# Browser acceptance for the Live reader: real Chromium, real frozen core.
#
# Usage:
#   MARQDO_BIN=/abs/path/marqdo scripts/test-browser.sh
#
# Requires `cd tests/browser && npm install` once (Playwright + Chromium).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
if [[ ! -x "$MARQDO_BIN" ]]; then
  echo "error: MARQDO_BIN is not executable: $MARQDO_BIN" >&2
  exit 1
fi
core_before="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"

export PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-$repo_root/tests/.cache/pw-browsers}"
export MQ_SCREENSHOT_DIR="${MQ_SCREENSHOT_DIR:-$repo_root/tests/browser/artifacts}"

if [[ ! -d "$repo_root/tests/browser/node_modules/playwright" ]]; then
  echo "error: Playwright is not installed; run: (cd tests/browser && npm install)" >&2
  exit 1
fi
if [[ ! -d "$PLAYWRIGHT_BROWSERS_PATH" ]]; then
  echo "error: Chromium is not installed; run: (cd tests/browser && npx playwright install chromium)" >&2
  exit 1
fi

if [[ "${VIEW_USE_INSTALLED_TOOL:-0}" == 1 ]]; then
  view=("$MARQDO_BIN" view)
else
  binary="$repo_root/target/release/marqdo-view"
  [[ -x "$binary" ]] || cargo build --release --locked
  view=("$binary")
fi

server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill -- "-$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
    server_pid=""
  fi
}
trap cleanup EXIT

run_suite() {
  local project="$1"
  local log
  log="$(mktemp "$repo_root/tests/.cache/browser-server.XXXXXX.log")"
  echo "==> browser suite: $project"
  setsid "${view[@]}" "$project" --port 0 --no-open >"$log" 2>&1 &
  server_pid=$!
  local url=""
  for _ in $(seq 1 100); do
    url="$(grep -o 'http://127.0.0.1:[0-9]*/' "$log" | head -1 || true)"
    [[ -n "$url" ]] && break
    if ! kill -0 "$server_pid" 2>/dev/null; then
      echo "error: server exited during startup:" >&2
      cat "$log" >&2
      exit 1
    fi
    sleep 0.1
  done
  if [[ -z "$url" ]]; then
    echo "error: server did not print a URL" >&2
    cat "$log" >&2
    exit 1
  fi
  MQ_URL="$url" MQ_PROJECT="$project" node tests/browser/reader.browser.cjs
  cleanup
  if curl -fsS --max-time 2 "$url" >/dev/null 2>&1; then
    echo "error: test server remained active after cleanup" >&2
    exit 1
  fi
  rm -f "$log"
}

mkdir -p "$repo_root/tests/.cache" "$MQ_SCREENSHOT_DIR"
run_suite examples/welcome
run_suite examples/control-flow
run_suite examples/function-params.mq.md
run_suite tests/fixtures/security-project

core_after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
if [[ "$core_before" != "$core_after" ]]; then
  echo "error: frozen core binary changed during browser tests" >&2
  exit 1
fi
echo "browser: PASS (core sha256 $core_before unchanged; screenshots: $MQ_SCREENSHOT_DIR)"
