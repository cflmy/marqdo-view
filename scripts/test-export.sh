#!/usr/bin/env bash
# Static export acceptance: pure read-only, offline assets, safe paths.
#
# Usage: MARQDO_BIN=/abs/path/marqdo scripts/test-export.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
[[ -x "$MARQDO_BIN" ]] || { echo "error: MARQDO_BIN not executable" >&2; exit 1; }
core_before="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"

if [[ "${VIEW_USE_INSTALLED_TOOL:-0}" == 1 ]]; then
  view=("$MARQDO_BIN" view)
else
  binary="$repo_root/target/release/marqdo-view"
  [[ -x "$binary" ]] || cargo build --release --locked
  view=("$binary")
fi

site="$repo_root/dist/test-export-site"
rm -rf "$site"
MARQDO_BIN="$MARQDO_BIN" "${view[@]}" export examples/welcome --out "$site"

python3 - "$site" <<'PY'
import hashlib
import json
import pathlib
import re
import sys

site = pathlib.Path(sys.argv[1])
manifest = json.loads((site / "manifest.json").read_text())
assert manifest["schema"] == "marqdo.view-export"
assert manifest["executed"] is False
assert manifest["pages"], "no pages exported"
for entry in manifest["files"]:
    rel = entry["path"]
    assert not rel.startswith("/") and ".." not in pathlib.PurePosixPath(rel).parts
    data = (site / rel).read_bytes()
    assert hashlib.sha256(data).hexdigest() == entry["sha256"], rel
index = (site / "index.html").read_text()
assert "/file?path=" not in index
assert re.search(r'<link rel="stylesheet" href="assets/reader\.[0-9a-f]{16}\.css"', index)
for pattern in ("<script src=\"http", "<link rel=\"stylesheet\" href=\"http", "url(http"):
    assert pattern not in index, pattern
print("export manifest: PASS (%d pages, %d files)" % (len(manifest["pages"]), len(manifest["files"])))
PY

# X-04: open the exported site from file:// in a real browser (offline).
if [[ -d "$repo_root/tests/browser/node_modules/playwright" ]]; then
  PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-$repo_root/tests/.cache/pw-browsers}" \
  MQ_FILE="$site/index.html" node - "$repo_root/tests/browser" <<'JS'
const path = require("path");
const { chromium } = require(path.join(process.argv[2], "node_modules/playwright"));
(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const external = [];
  const errors = [];
  page.on("request", (request) => {
    if (!request.url().startsWith("file:") && !request.url().startsWith("data:")) {
      external.push(request.url());
    }
  });
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  page.on("pageerror", (error) => errors.push(String(error)));
  await page.goto("file://" + process.env.MQ_FILE, { waitUntil: "load" });
  const text = await page.locator(".mq-document").innerText();
  if (text.trim().length < 20) throw new Error("offline page not readable");
  if (external.length) throw new Error("offline page requested " + external.join(","));
  if (errors.length) throw new Error("offline console errors: " + errors.join(" | "));
  if ((await page.locator("link[rel=stylesheet]").count()) !== 1) throw new Error("stylesheet missing");
  console.log("file:// offline browser check: PASS");
  await browser.close();
})().catch((error) => { console.error(error.message); process.exit(1); });
JS
else
  echo "note: Playwright not installed; skipped file:// browser check"
fi

core_after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
[[ "$core_before" == "$core_after" ]] || { echo "error: frozen core changed" >&2; exit 1; }
echo "export: PASS (offline site at $site; core sha256 $core_before unchanged)"
