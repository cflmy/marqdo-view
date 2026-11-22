#!/usr/bin/env bash
# Identity of the released CLI used by the RFC-VIEW-0002 acceptance gate.
set -euo pipefail

version=3.0.0-beta.1
source_sha=2c7c4f31e7a388d3fcfa717a77067e696ed13854
binary_sha=062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f
archive_sha=507e4cd358f75f517433935ce90d5fc2eb5cecda4f19121a68b136c83b15f317
artifact="marqdo-$version-x86_64-unknown-linux-gnu"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

verify() {
  local binary="$1"
  local install
  install="$(dirname "$binary")"
  if [[ ! -f "$install/SOURCE_SHA" || "$(cat "$install/SOURCE_SHA")" != "$source_sha" ]]; then
    echo "error: expected the official marqdo $version installation ($source_sha)" >&2
    return 1
  fi
  "$root/verify-frozen-cli.sh" "$binary" -- "$binary_sha" >&2
  if [[ "$("$binary" --version)" != "marqdo $version" ]]; then
    echo "error: frozen marqdo version does not match $version" >&2
    return 1
  fi
}

if [[ $# != 2 ]]; then
  echo "usage: $0 fetch EMPTY_DIRECTORY | $0 verify /absolute/path/to/marqdo" >&2
  exit 2
fi
case "$1" in
  fetch)
    mkdir -p "$2"
    destination="$(cd "$2" && pwd)"
    if [[ -n "$(ls -A "$destination")" ]]; then
      echo "error: frozen CLI destination must be empty: $destination" >&2
      exit 2
    fi
    archive="$destination/$artifact.tar.gz"
    curl -fsSL --retry 3 --connect-timeout 15 --max-time 90 \
      "https://git.cflmy.cn/cflmy/marqdo-cli/releases/download/v$version/$artifact.tar.gz" -o "$archive"
    printf '%s  %s\n' "$archive_sha" "$archive" | sha256sum -c - >&2
    tar -xzf "$archive" -C "$destination"
    verify "$destination/$artifact/marqdo"
    printf '%s\n' "$destination/$artifact/marqdo"
    ;;
  verify)
    verify "$2"
    ;;
  *)
    echo "unknown frozen CLI operation: $1" >&2
    exit 2
    ;;
esac
