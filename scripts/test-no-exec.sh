#!/usr/bin/env bash
# RFC-VIEW-0002 §13.4 / §6 / §10.3: opening, searching, switching and exporting
# must never execute a document. Uses the unsafe-side-effect fixture and checks
# source/lock hashes plus the absence of any write sentinel around the whole
# read-only operation set.
#
# Usage: MARQDO_BIN=/abs/path/marqdo scripts/test-no-exec.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
core_sha_before="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
if [[ "${VIEW_USE_INSTALLED_TOOL:-0}" == 1 ]]; then
  view=("$MARQDO_BIN" view)
else
  binary="$repo_root/target/release/marqdo-view"
  [[ -x "$binary" ]] || cargo build --release --locked
  view=("$binary")
fi

mkdir -p "$repo_root/dist"
work="$(mktemp -d "$repo_root/dist/no-exec.XXXXXX")"
pid=""
cleanup() {
  if [[ -n "$pid" ]]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT

project="$work/project"
mkdir "$project"
cp examples/unsafe-side-effect.mq.md "$project/"
cat > "$project/marqdo.yaml" <<'YAML'
project:
  name: view-no-exec
  version: 0.1.0
dependencies: {}
YAML
(cd "$project" && "$MARQDO_BIN" lock >/dev/null)
doc="$project/unsafe-side-effect.mq.md"
sentinel="$project/out/view-sentinel.txt"

hash_tree() {
  sha256sum "$doc" "$project/marqdo.yaml" "$project/marqdo.lock"
}

before="$(hash_tree)"
test ! -e "$sentinel"

echo "==> Live read-only requests"
"${view[@]}" "$doc" --port 0 --no-open >"$work/server.log" 2>&1 &
pid=$!
url=""
for _ in $(seq 1 100); do
  url="$(grep -o 'http://127.0.0.1:[0-9]*/' "$work/server.log" | head -1 || true)"
  [[ -n "$url" ]] && break
  sleep 0.1
done
test -n "$url"
cookie_jar="$work/cookies.txt"
curl -fsS -c "$cookie_jar" "${url}" > "$work/page.html"
curl -fsS -c "$cookie_jar" -b "$cookie_jar" "${url}file?path=unsafe-side-effect.mq.md" >/dev/null
curl -fsS -b "$cookie_jar" "${url}api/documents" >/dev/null
curl -fsS -b "$cookie_jar" "${url}api/documents?path=unsafe-side-effect.mq.md" >/dev/null
# Reloading the page multiple times must remain side-effect free.
for _ in $(seq 1 5); do curl -fsS -b "$cookie_jar" "${url}" >/dev/null; done
kill "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true
pid=""

echo "==> Static export"
"${view[@]}" export "$doc" --out "$work/site" >/dev/null
test -f "$work/site/manifest.json"
# Export has no execution controls and no CSRF/session material.
if grep -q 'data-action="run"' "$work/site/pages/unsafe-side-effect.mq.md.html"; then
  echo "error: exported page contains a run control" >&2
  exit 1
fi
if grep -q 'mq-csrf-token' "$work/site/pages/unsafe-side-effect.mq.md.html"; then
  echo "error: exported page contains a CSRF token" >&2
  exit 1
fi
after="$(hash_tree)"
if [[ "$before" != "$after" ]]; then
  echo "error: read-only/export operations modified project sources" >&2
  diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") >&2 || true
  exit 1
fi
test ! -e "$sentinel"
test ! -e "$project/out"

# Prove the fixture is capable of writing when an operator grants it.
"$MARQDO_BIN" run "$doc" --mode project --allow-write out --allow-write 'out/**' >/dev/null
grep -qx 'explicit run only' "$sentinel"

core_sha_after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
test "$core_sha_before" = "$core_sha_after"
echo "no-exec: PASS (sources/lock unchanged, no sentinel, no run control in export; core sha unchanged)"
