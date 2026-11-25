#!/usr/bin/env bash
# Regenerate committed Analysis v3 / Observation v1 goldens from the frozen CLI
# release binary.
#
# The current public contracts have exactly one static source (`check --emit`
# Analysis v3) and one runtime sequence (Observation v1, either `--output jsonl`
# or the atomic `--observation-file` snapshot of the same execution).
#
# Usage: MARQDO_BIN=/abs/path/marqdo scripts/gen-golden.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
if [[ ! -x "$MARQDO_BIN" ]]; then
  echo "error: MARQDO_BIN is not executable: $MARQDO_BIN" >&2
  exit 1
fi
echo "golden core sha256: $(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"

out="tests/golden/analysis-v3"
mkdir -p "$out"
"$MARQDO_BIN" check examples/mixed-modes/index.mq.md --emit "$out/mixed-modes.json"
"$MARQDO_BIN" check examples/control-flow/index.mq.md --emit "$out/control-flow.json"
"$MARQDO_BIN" check examples/tables-math/index.mq.md --emit "$out/tables-math.json"
"$MARQDO_BIN" check examples/welcome/00-welcome.mq.md --emit "$out/welcome.json"
"$MARQDO_BIN" check examples/unicode-spans/index.mq.md --emit "$out/unicode-spans.json"
"$MARQDO_BIN" check examples/unicode-spans/crlf.mq.md --emit "$out/unicode-spans-crlf.json"

# Observation v1 goldens come from the same frozen binary (pure mode, no grants).
out_obs="tests/golden/observation-v1"
mkdir -p "$out_obs"
( cd examples/control-flow \
  && "$MARQDO_BIN" run index.mq.md --mode pure --output jsonl --observation-file "$repo_root/$out_obs/control-flow.snapshot.json" \
       > "$repo_root/$out_obs/control-flow.jsonl" )
( cd examples/control-flow \
  && "$MARQDO_BIN" run index.mq.md --mode pure --observe-max-events 5 \
       --output jsonl --observation-file "$repo_root/$out_obs/control-flow.truncated.snapshot.json" \
       > "$repo_root/$out_obs/control-flow.truncated.jsonl" )
if ( cd examples/mixed-modes \
  && "$MARQDO_BIN" run index.mq.md --mode pure --output jsonl \
       --observation-file "$repo_root/$out_obs/mixed-modes.arity.snapshot.json" \
       > "$repo_root/$out_obs/mixed-modes.arity.jsonl" 2>/dev/null ); then
  echo "error: mixed-modes without params unexpectedly ran" >&2
  exit 1
fi
# Diagnostics fixture must fail: verify the failure shape stays as expected.
if "$MARQDO_BIN" check examples/diagnostics/index.mq.md --json >/dev/null 2>&1; then
  echo "error: examples/diagnostics unexpectedly passed check" >&2
  exit 1
fi
echo "goldens: PASS (Analysis v3 + Observation v1 stream/snapshot)"
