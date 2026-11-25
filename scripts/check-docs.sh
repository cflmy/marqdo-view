#!/usr/bin/env bash
# G12 (issue #5): current documentation must describe the rc.4 contracts as the
# only current capability. Historical CHANGELOG entries may name retired
# interfaces only under an explicit `## History` heading.
#
# Checked files:
#   README.md, COMPATIBILITY.md, CHANGELOG.md, docs/STATUS.md, docs/ACCEPTANCE.md
#
# The script never rewrites files and never claims a gate is PASS: it only
# reports fact drift between the current docs and the rc.4 contract baseline.
#
# Usage: scripts/check-docs.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail=0
hit() {
  printf 'FAIL[%s] %s\n' "$1" "$2"
  fail=1
}

legacy_pattern='Analysis v2|Trace v2|Execution Events|--trace-file|--event-stream|PROTOCOL_ANALYSIS|PROTOCOL_TRACE|TOOL_PROTOCOL|3\.0\.0-beta|>=3\.0\.0-rc\.2'
run_json_pattern='run[^.。\n]{0,20}--json|call[^.。\n]{0,20}--json|--json[^.。\n]{0,20}run|--json[^.。\n]{0,20}call'

# ------------------------------------------------------------- baseline ----
for file in README.md COMPATIBILITY.md docs/STATUS.md; do
  [[ -f "$file" ]] || { hit "missing doc" "$file"; continue; }
  grep -q '0\.3\.0' "$file" || hit "version drift" "$file: does not state View 0.3.0"
done
for file in README.md COMPATIBILITY.md docs/STATUS.md docs/ACCEPTANCE.md; do
  [[ -f "$file" ]] || { hit "missing doc" "$file"; continue; }
  while IFS=: read -r line text; do
    [[ -n "$line" ]] && hit "legacy contract as current capability" "$file:$line: $text"
  done < <(grep -nE "$legacy_pattern" "$file" || true)
  while IFS=: read -r line text; do
    [[ -n "$line" ]] && hit "retired run --json as current output" "$file:$line: $text"
  done < <(grep -nE "$run_json_pattern" "$file" || true)
done

grep -qE 'Analysis[^A-Za-z0-9]+3' COMPATIBILITY.md || hit "contract drift" "COMPATIBILITY.md: no current Analysis 3 row"
grep -qE 'Observation[^A-Za-z0-9]+1' COMPATIBILITY.md || hit "contract drift" "COMPATIBILITY.md: no current Observation 1 row"
grep -qE 'api_revision[^A-Za-z0-9]+1|API revision[^A-Za-z0-9]+1' COMPATIBILITY.md \
  || hit "contract drift" "COMPATIBILITY.md: no current Tool api_revision 1 row"
grep -q '>=3.0.0-rc.3,<4.0.0' COMPATIBILITY.md || hit "product range drift" "COMPATIBILITY.md: expected >=3.0.0-rc.3,<4.0.0"

# ------------------------------------------------------------- CHANGELOG ---
if [[ ! -f CHANGELOG.md ]]; then
  hit "missing doc" "CHANGELOG.md"
else
  history_line="$(grep -nE '^##[[:space:]]+(History|历史)' CHANGELOG.md | head -1 | cut -d: -f1 || true)"
  if [[ -z "$history_line" ]]; then
    if grep -qE "$legacy_pattern" CHANGELOG.md; then
      hit "CHANGELOG history" "CHANGELOG.md: retired interfaces appear without an explicit '## History' heading"
    fi
  else
    # Only the current (pre-history) part may be scanned for legacy claims.
    while IFS=: read -r line text; do
      [[ -n "$line" ]] && hit "legacy contract as current CHANGELOG entry" "CHANGELOG.md:$line: $text"
    done < <(head -n "$((history_line - 1))" CHANGELOG.md | grep -nE "$legacy_pattern" || true)
    while IFS=: read -r line text; do
      [[ -n "$line" ]] && hit "retired run --json in current CHANGELOG entry" "CHANGELOG.md:$line: $text"
    done < <(head -n "$((history_line - 1))" CHANGELOG.md | grep -nE "$run_json_pattern" || true)
    if ! head -n "$((history_line - 1))" CHANGELOG.md | grep -q '0\.3\.0'; then
      hit "CHANGELOG current entry" "CHANGELOG.md: no 0.3.0 entry before '## History'"
    fi
  fi
fi

# --------------------------------------------------------------- report ----
if [[ "$fail" -ne 0 ]]; then
  cat >&2 <<'EOF'

check-docs: FAIL
  Update the current docs to the rc.4 baseline:
    * one static source: `check --emit` Analysis revision 3 (ADR-0069);
    * one runtime sequence: Observation revision 1 (`--output jsonl` /
      `--observation-file`), no retired trace file or event stream;
    * CLI Tool revision 1 with exact `requires` (api_revision 1, analysis 3,
      observation 1), no `compatibility` field;
    * product range >=3.0.0-rc.3,<4.0.0, acceptance pinned to rc.4.
  Old release notes belong under CHANGELOG's explicit `## History` heading.
  Do not claim PASS/UNVERIFIED results that have no generated evidence.
EOF
  exit 1
fi

echo "check-docs: PASS (README/COMPATIBILITY/CHANGELOG/STATUS/ACCEPTANCE describe the rc.4 baseline; CHANGELOG history is explicit)"
