#!/usr/bin/env bash
# Build and assemble the Tool Protocol v1 release bundle.
#
# Produces:
#   dist/marqdo-view/                                  stable bundle path (RFC §18)
#   dist/marqdo-view-<version>-<target>.tar.gz (+ .sha256)
#
# The bundle contains only the runtime binary, the manifest and the legal /
# compatibility files. CSS/JS/images are compiled into the binary (ADR-0002),
# so there is exactly one asset source for Live and Export.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${CARGO:=cargo}"
: "${MARQDO_BIN:=}"

version="$("$CARGO" metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
target="$("$CARGO" -vV 2>/dev/null | sed -n 's/^host: //p')"
target="${target:-x86_64-unknown-linux-gnu}"

echo "==> cargo build --release --locked"
"$CARGO" build --release --locked

dist="$repo_root/dist"
bundle="$dist/marqdo-view"
rm -rf "$bundle"
mkdir -p "$bundle/bin"

binary="$repo_root/target/release/marqdo-view"
if [[ ! -x "$binary" ]]; then
  echo "error: missing release binary $binary" >&2
  exit 1
fi

cp "$binary" "$bundle/bin/marqdo-view"
chmod 755 "$bundle/bin/marqdo-view"
cp tool.json "$bundle/tool.json"
cp README.md COMPATIBILITY.md CHANGELOG.md LICENSE-MIT LICENSE-APACHE "$bundle/"

# Verify the tool manifest and the binary agree on the version.
manifest_version="$(python3 -c 'import json;print(json.load(open("tool.json"))["version"])')"
if [[ "$manifest_version" != "$version" ]]; then
  echo "error: tool.json version $manifest_version != package version $version" >&2
  exit 1
fi
binary_version="$("$bundle/bin/marqdo-view" --version | awk '{print $2}')"
if [[ "$binary_version" != "$version" ]]; then
  echo "error: binary prints version $binary_version, package is $version" >&2
  exit 1
fi

source_sha="unknown"
if git rev-parse --verify HEAD >/dev/null 2>&1; then
  source_sha="$(git rev-parse HEAD)"
fi
printf '%s\n' "$source_sha" > "$bundle/SOURCE_SHA"

python3 - "$bundle" "$version" "$target" "$source_sha" <<'PY'
import hashlib
import json
import pathlib
import sys

bundle = pathlib.Path(sys.argv[1])
version, target, source_sha = sys.argv[2], sys.argv[3], sys.argv[4]
files = []
for path in sorted(bundle.rglob('*')):
    if not path.is_file():
        continue
    rel = path.relative_to(bundle).as_posix()
    if rel in ('RELEASE_INFO.json', 'SHA256SUMS'):
        continue
    data = path.read_bytes()
    files.append({
        'path': rel,
        'size': len(data),
        'sha256': hashlib.sha256(data).hexdigest(),
    })
info = {
    'schema': 'marqdo.view-release-info',
    'schema_version': 1,
    'tool_name': 'marqdo-view',
    'tool_version': version,
    'command': 'view',
    'target': target,
    'source_sha': source_sha,
    'compatibility': {
        'marqdo': '>=3.0.0-beta.1,<4.0.0',
        'analysis': [2],
        'trace': [2],
        'tool_protocol': 1,
        'markdown_profile': 'marqdo.md.v1',
    },
    'files': files,
}
(bundle / 'RELEASE_INFO.json').write_text(json.dumps(info, indent=2, sort_keys=True) + '\n')
with (bundle / 'SHA256SUMS').open('w') as out:
    for entry in files:
        out.write(f"{entry['sha256']}  {entry['path']}\n")
PY

# Deterministic archive: sorted paths, fixed owner, gzip without timestamps.
archive="marqdo-view-${version}-${target}.tar.gz"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='UTC 2026-01-01' \
  -C "$dist" -cf - marqdo-view | gzip -n > "$dist/$archive"
( cd "$dist" && sha256sum "$archive" > "$archive.sha256" )

if [[ -n "$MARQDO_BIN" ]]; then
  echo "==> smoke install against frozen core"
  MARQDO_BIN="$MARQDO_BIN" "$repo_root/scripts/install-smoke.sh" "$bundle/tool.json"
fi

echo "bundle: $bundle"
echo "archive: $dist/$archive"
echo "sha256: $(cut -d' ' -f1 "$dist/$archive.sha256")"
