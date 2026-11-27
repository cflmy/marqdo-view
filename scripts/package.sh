#!/bin/sh
# Deterministic source archive + release-evidence.json for this component.
#
# Required env:
#   COMPONENT_ID   catalog id (e.g. std-text)
#   REPOSITORY     owner/name (e.g. cflmy/marqdo-std-text)
# Optional:
#   VERSION        without leading v (default: derived from CI_COMMIT_TAG)
#   SOURCE_SHA     full commit (default: HEAD)
#   OUT_DIR        default dist
#   DEPENDENCY     repeatable write_evidence dependency specs via env DEP_ARGS
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
COMPONENT_ID=${COMPONENT_ID:?COMPONENT_ID is required}
REPOSITORY=${REPOSITORY:?REPOSITORY is required}
SOURCE_SHA=${SOURCE_SHA:-$(git -C "$ROOT" rev-parse HEAD)}
OUT_DIR=${OUT_DIR:-dist}
TAG=${CI_COMMIT_TAG:-${TAG:-}}
if [ -n "${VERSION:-}" ]; then
  :
elif [ -n "$TAG" ]; then
  VERSION=${TAG#v}
else
  echo "package.sh: set VERSION or CI_COMMIT_TAG" >&2
  exit 1
fi
TAG=${TAG:-v$VERSION}

python3 "$ROOT/scripts/release/package_source.py" \
  --root "$ROOT" \
  --component-id "$COMPONENT_ID" \
  --version "$VERSION" \
  --out-dir "$OUT_DIR"

ARCHIVE=$(ls "$ROOT/$OUT_DIR"/marqdo-"$COMPONENT_ID"-"$VERSION"-source.tar.gz)
SHAFILE="$ARCHIVE.sha256"

EVIDENCE_ARGS="--component-id $COMPONENT_ID --repository $REPOSITORY --tag $TAG --source-sha $SOURCE_SHA --asset $ARCHIVE --asset $SHAFILE --out $ROOT/$OUT_DIR/release-evidence.json"
if [ -n "${CI_PIPELINE_NUMBER:-}" ]; then
  EVIDENCE_ARGS="$EVIDENCE_ARGS --pipeline-number $CI_PIPELINE_NUMBER"
fi
if [ -n "${CI_PIPELINE_URL:-}" ]; then
  EVIDENCE_ARGS="$EVIDENCE_ARGS --pipeline-url $CI_PIPELINE_URL"
fi
# shellcheck disable=SC2086
eval "python3 \"$ROOT/scripts/release/write_evidence.py\" $EVIDENCE_ARGS ${DEP_ARGS:-}"
echo "$ARCHIVE"
echo "$SHAFILE"
echo "$ROOT/$OUT_DIR/release-evidence.json"
