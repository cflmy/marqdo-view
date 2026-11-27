#!/usr/bin/env bash
# Identity of the released CLI used by the marqdo-view 0.3.0 acceptance gate.
#
# The pinned release is the current public-contract baseline:
#   marqdo 3.0.0-rc.5 (Analysis v3 / ADR-0069, Observation v1, CLI Tool revision 1)
# The script only downloads, unpacks and verifies the official archive; it never
# rebuilds or modifies the CLI.
set -euo pipefail

version=3.0.0-rc.5
source_sha=c44b0f943875d563bb7c170f8ccd3fd56a82360f
binary_sha=a19049ee111f41512a69c5ed748aaa4968eb3a3f5abd585ccdcf9bbb201bd7ee
archive_sha=fd7a313e0ddaf38a9b1901ff2a9aa7b9a0a153e65522a9c0c60f01e2549f583b
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

if [[ "${1:-}" == "metadata" && $# -eq 1 ]]; then
  case "$1" in
    metadata)
      printf 'version=%s\n' "$version"
      printf 'source_sha=%s\n' "$source_sha"
      printf 'binary_sha=%s\n' "$binary_sha"
      printf 'archive_sha=%s\n' "$archive_sha"
      printf 'artifact=%s\n' "$artifact"
      exit 0
      ;;
  esac
fi
if [[ $# != 2 ]]; then
  echo "usage: $0 fetch EMPTY_DIRECTORY | $0 verify /absolute/path/to/marqdo | $0 metadata" >&2
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
