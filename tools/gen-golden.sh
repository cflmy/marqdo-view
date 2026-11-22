#!/usr/bin/env bash
# Regenerate committed Analysis v2 goldens from the frozen CLI release binary.
#
# Usage: MARQDO_BIN=/abs/path/marqdo tools/gen-golden.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
if [[ ! -x "$MARQDO_BIN" ]]; then
  echo "error: MARQDO_BIN is not executable: $MARQDO_BIN" >&2
  exit 1
fi
echo "golden core sha256: $(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"

out="tests/golden/analysis-v2"
mkdir -p "$out"
"$MARQDO_BIN" check examples/mixed-modes/index.mq.md --emit "$out/mixed-modes.json"
"$MARQDO_BIN" check examples/control-flow/index.mq.md --emit "$out/control-flow.json"
"$MARQDO_BIN" check examples/tables-math/index.mq.md --emit "$out/tables-math.json"
"$MARQDO_BIN" check examples/welcome/00-welcome.mq.md --emit "$out/welcome.json"
# Trace v2 goldens come from the same frozen binary (pure mode, no grants).
out_trace="tests/golden/trace-v2"
mkdir -p "$out_trace"
( cd examples/control-flow \
  && "$MARQDO_BIN" run index.mq.md --mode pure --json --trace-file "$repo_root/$out_trace/control-flow.trace.json" > "$repo_root/$out_trace/control-flow.run.json" )
( cd examples/control-flow \
  && "$MARQDO_BIN" run index.mq.md --mode pure --json --trace-max-events 5 --trace-file "$repo_root/$out_trace/control-flow.truncated.trace.json" > "$repo_root/$out_trace/control-flow.truncated.run.json" )
if ( cd examples/mixed-modes \
  && "$MARQDO_BIN" run index.mq.md --mode pure --json --trace-file "$repo_root/$out_trace/mixed-modes.arity.trace.json" > "$repo_root/$out_trace/mixed-modes.arity.run.json" ); then
  echo "error: mixed-modes without params unexpectedly ran" >&2
  exit 1
fi
# Diagnostics fixture must fail: verify the failure shape stays as expected.
if "$MARQDO_BIN" check examples/diagnostics/index.mq.md --json >/dev/null 2>&1; then
  echo "error: examples/diagnostics unexpectedly passed check" >&2
  exit 1
fi
echo "goldens: PASS"
