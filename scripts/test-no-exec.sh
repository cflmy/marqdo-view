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
binary="$repo_root/target/release/marqdo-view"
if [[ ! -x "$binary" ]]; then
  cargo build --release --locked
fi

work="$repo_root/dist/no-exec"
rm -rf "$work"
mkdir -p "$work"
doc="examples/unsafe-side-effect.mq.md"
sentinel="examples/out/view-sentinel.txt"

hash_tree() {
  find examples -type f -name '*.mq.md' -print0 | sort -z | xargs -0 sha256sum
  if [[ -f examples/marqdo.yaml ]]; then sha256sum examples/marqdo.yaml; fi
  if [[ -f examples/view.json ]]; then sha256sum examples/view.json; fi
  if [[ -f marqdo.lock ]]; then sha256sum marqdo.lock; fi
}

rm -rf examples/out
before="$(hash_tree)"
test ! -e "$sentinel"

echo "==> Live read-only requests"
"$binary" "$doc" --port 0 --no-open >"$work/server.log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT
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
trap - EXIT

echo "==> Static export"
"$binary" export "$doc" --out "$work/site" >/dev/null
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
if grep -rqE 'https?://(?!127\.0\.0\.1|localhost)' --include='*.html' "$work/site" 2>/dev/null; then
  echo "error: exported site references an external URL" >&2
  exit 1
fi

after="$(hash_tree)"
if [[ "$before" != "$after" ]]; then
  echo "error: read-only/export operations modified project sources" >&2
  diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") >&2 || true
  exit 1
fi
test ! -e "$sentinel"
test ! -e examples/out

core_sha_after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
test "$core_sha_before" = "$core_sha_after"
echo "no-exec: PASS (sources/lock unchanged, no sentinel, no run control in export; core sha unchanged)"
