#!/usr/bin/env bash
# G01 (issue #5 §4): fail if the current tree still contains retired public
# contracts, retired run/call argv, legacy Tool negotiation or dual-state
# compatibility shims.
#
# Scope:
#   * src/                     one decoder, one validator, no version branch;
#   * tool.json                CLI Tool revision 1 only (no `compatibility`);
#   * current contract docs    README/COMPATIBILITY/STATUS/ACCEPTANCE/
#                              CONFORMANCE/architecture/current audit/ADR-0012.
#
# Historical records are exempt by design and are never rewritten:
#   docs/evidence/** (except README), docs/decisions/0001..0011,
#   docs/architecture/RFC-VIEW-0001*, RFC-VIEW-0002.md, docs/audits/legacy-*,
#   docs/audits/protocol-conformance.md, CHANGELOG.md (history is validated by
#   scripts/check-docs.sh instead).
#
# Usage: scripts/check-no-legacy.sh
# Exit: 0 = 0 hits; 1 = at least one retired reference (actionable output).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail=0
hit() {
  printf 'FAIL[%s] %s\n' "$1" "$2"
  fail=1
}

# ---------------------------------------------------------------- src/ ------
src_files() {
  find src -type f \( -name '*.rs' -o -name '*.js' -o -name '*.css' \) -print0
}

# Negative unit tests may name a retired flag to assert it is never emitted.
# Everything from the conventional `#[cfg(test)]` attribute to EOF is test code
# and is not a production compatibility path, so scans skip it.
scan_src_pattern() {
  local label="$1" pattern="$2" file line text
  while IFS= read -r -d '' file; do
    while IFS=: read -r line text; do
      [[ -n "$line" ]] && hit "$label" "$file:$line: $text"
    done < <(awk -v pat="$pattern" -v file="$file" '
      /^[[:space:]]*#\[cfg\(test\)\]/ { intests = 1 }
      intests { next }
      $0 ~ pat { print FNR ":" $0 }
    ' "$file" || true)
  done < <(src_files)
}

scan_src_pattern "legacy protocol module/name" 'analysis_v2|trace_v2|Trace v2|Execution Events'
scan_src_pattern "retired run/call flag" '--trace-file|--event-stream|trace_max|trace_path|trace_bytes'
scan_src_pattern "legacy Tool negotiation/migration" 'compatibility|record_version:[[:space:]]*1|PROTOCOL_ANALYSIS|PROTOCOL_TRACE|TOOL_PROTOCOL'
scan_src_pattern "dual-state/legacy branch" 'Option<Legacy|enum Either|legacy_mode|map_v1'
# Version branches belong to protocol modules only; the reader never sees them.
while IFS= read -r -d '' file; do
  while IFS=: read -r line text; do
    [[ -n "$line" ]] && hit "reader branches on schema_version" "$file:$line: $text"
  done < <(awk '
    /^[[:space:]]*#\[cfg\(test\)\]/ { intests = 1 }
    intests { next }
    /schema_version/ { print FNR ":" $0 }
  ' "$file" || true)
done < <(find src/reader -type f -name '*.rs' -print0 2>/dev/null || true)

# `fallback` is legitimate only as a comment phrase ("never decoded through a
# fallback") or for the unrelated OS-random fallback helper. Flag real code.
while IFS= read -r -d '' file; do
  while IFS=: read -r line text; do
    [[ -z "$line" ]] && continue
    code="${text%%//*}"
    [[ "$code" == *fallback* ]] || continue
    case "$code" in
      *fallback_counter*) ;;
      *) hit "protocol fallback branch" "$file:$line: $text" ;;
    esac
  done < <(awk '
    /^[[:space:]]*#\[cfg\(test\)\]/ { intests = 1 }
    intests { next }
    /fallback/ { print FNR ":" $0 }
  ' "$file" || true)
done < <(src_files)

# Retired `run`/`call --json` argv: the only accepted `--json` is the `check`
# diagnostic summary. Heuristic: find the enclosing Rust function; argv
# builders for check-like commands are the only permitted owners.
while IFS= read -r -d '' file; do
  while IFS=: read -r line_no text; do
    [[ -z "$line_no" ]] && continue
    case "$text" in
      //*|///*) continue ;;
    esac
    fn="$(awk -v target="$line_no" '
      /^[[:space:]]*(pub[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]+[A-Za-z0-9_]+/ {
        s=$0; sub(/^[[:space:]]*(pub[[:space:]]+)?(async[[:space:]]+)?fn[[:space:]]+/, "", s); sub(/[^A-Za-z0-9_].*/, "", s); current=s
      }
      NR==target { print current; exit }
    ' "$file")"
    case "$fn" in
      *check*|*diagnostic*|*analysis*) ;;
      *) hit "retired run/call --json argv" "$file:$line_no: $text (enclosing fn: ${fn:-<top-level>})" ;;
    esac
  done < <(awk '
    /^[[:space:]]*#\[cfg\(test\)\]/ { intests = 1 }
    intests { next }
    /--json/ { print FNR ":" $0 }
  ' "$file" || true)
done < <(src_files)

# ------------------------------------------------------------- tool.json ----
if grep -qE '"compatibility"' tool.json; then
  while IFS=: read -r line text; do
    hit "tool.json compatibility field" "tool.json:$line: $text"
  done < <(grep -nE '"compatibility"' tool.json)
fi
if grep -qE 'record_version|PROTOCOL_ANALYSIS|PROTOCOL_TRACE|TOOL_PROTOCOL' tool.json; then
  while IFS=: read -r line text; do
    hit "legacy tool metadata" "tool.json:$line: $text"
  done < <(grep -nE 'record_version|PROTOCOL_ANALYSIS|PROTOCOL_TRACE|TOOL_PROTOCOL' tool.json)
fi

# -------------------------------------------------------- current docs ------
current_docs=(
  README.md
  COMPATIBILITY.md
  docs/STATUS.md
  docs/ACCEPTANCE.md
  docs/CONFORMANCE.md
  docs/audits/protocol-contract-0002.md
  docs/architecture/observation-v1-and-analysis-v3.md
  docs/decisions/0012-rc4-current-contracts.md
  docs/design/complete-reader.md
  docs/design/state-matrix.md
  docs/design/visual-tokens.md
  docs/evidence/README.md
)
doc_pattern='Analysis v2|Trace v2|Execution Events|--trace-file|--event-stream|PROTOCOL_ANALYSIS|PROTOCOL_TRACE|TOOL_PROTOCOL|3\.0\.0-beta|>=3\.0\.0-rc\.2'
run_json_pattern='run[^.。\n]{0,20}--json|call[^.。\n]{0,20}--json|--json[^.。\n]{0,20}run|--json[^.。\n]{0,20}call'
for doc in "${current_docs[@]}"; do
  if [[ ! -f "$doc" ]]; then
    hit "missing current doc" "$doc"
    continue
  fi
  while IFS=: read -r line text; do
    [[ -n "$line" ]] && hit "legacy contract in current doc" "$doc:$line: $text"
  done < <(grep -nE "$doc_pattern" "$doc" || true)
  while IFS=: read -r line text; do
    [[ -n "$line" ]] && hit "retired run --json in current doc" "$doc:$line: $text"
  done < <(grep -nE "$run_json_pattern" "$doc" || true)
done

if [[ "$fail" -ne 0 ]]; then
  cat >&2 <<'EOF'

check-no-legacy: FAIL
  Remove the retired reference from the current tree (delete the code path,
  rewrite the doc claim, or move history into CHANGELOG.md under its explicit
  `## History` heading). Do not add a compatibility branch, fallback decoder or
  migration shim: the rc.4 baseline has exactly one current contract per
  boundary. Historical evidence under docs/evidence/** and historical ADRs are
  exempt and untouched.
  Current baseline: marqdo 3.0.0-rc.4, Analysis revision 3 (ADR-0069),
  Observation revision 1, CLI Tool revision 1.
EOF
  exit 1
fi

echo "check-no-legacy: PASS (0 retired references in src/, tool.json and current docs; rc.4 baseline)"
