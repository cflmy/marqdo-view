#!/usr/bin/env bash
# RFC-VIEW-0002 §13.3/§13.4: prove the five 3.0 example documents against the
# frozen released CLI. Never rebuilds the Core; runs in a copied temp project so
# the repository examples are never modified.
#
# Usage: MARQDO_BIN=/abs/path/marqdo scripts/check-examples.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
core_sha_before="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"

work="$repo_root/dist/examples-check"
rm -rf "$work"
mkdir -p "$work"
cp examples/welcome.mq.md examples/branch-loop.mq.md examples/function-params.mq.md \
  examples/knowledge-only.mq.md examples/unsafe-side-effect.mq.md "$work/"
cp examples/view.json "$work/"

cd "$work"

echo "==> check all five example documents (no execution)"
for file in welcome.mq.md branch-loop.mq.md function-params.mq.md knowledge-only.mq.md unsafe-side-effect.mq.md; do
  if ! "$MARQDO_BIN" check "$file" --emit "analysis-$file.json" --json > "check-$file.json"; then
    echo "error: frozen CLI refused $file" >&2
    cat "check-$file.json" >&2
    exit 1
  fi
  test -s "analysis-$file.json"
done

echo "==> call function-params.double(n=5) -> 10"
call_json="$("$MARQDO_BIN" call function-params.mq.md double --arg n=5 --mode pure --json --trace-file call.trace.json)"
python3 - "$call_json" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data["success"] is True, data
assert data["value"] == {"type": "int", "decimal": "10"}, data["value"]
print("call double(5): PASS (Int 10)")
PY

echo "==> run branch-loop main -> Int 0 with loop/return trace"
run_json="$("$MARQDO_BIN" run branch-loop.mq.md --mode pure --json --trace-file run.trace.json)"
python3 - "$run_json" "run.trace.json" <<'PY'
import json, sys
data = json.loads(sys.argv[1])
assert data["success"] is True, data
assert data["value"] == {"type": "int", "decimal": "0"}, data["value"]
trace = json.load(open(sys.argv[2]))
kinds = {event["kind"] for event in trace["events"]}
assert "loop.iteration" in kinds and "return" in kinds, kinds
print("run branch-loop: PASS (value 0, events: %s)" % ",".join(sorted(kinds)))
PY

echo "==> knowledge-only has no main (truthful failure, no fallback)"
set +e
"$MARQDO_BIN" run knowledge-only.mq.md --mode pure --json > knowledge.json 2>&1
status=$?
set -e
test "$status" -ne 0
python3 - knowledge.json <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
assert data["success"] is False, data
assert data["code"] == "sem.no_entry_function", data
print("knowledge-only: PASS (explicit sem.no_entry_function)")
PY

echo "==> unsafe-side-effect writes nothing in pure mode"
set +e
"$MARQDO_BIN" run unsafe-side-effect.mq.md --mode pure --json > unsafe-pure.json 2>&1
status=$?
set -e
test "$status" -ne 0
test ! -e out/view-sentinel.txt
python3 - unsafe-pure.json <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
assert data["success"] is False, data
assert data["error"]["code"] == "permission.denied", data["error"]
print("unsafe-side-effect pure: PASS (permission.denied, no file)")
PY

echo "==> unsafe-side-effect writes only with explicit project grants"
"$MARQDO_BIN" run unsafe-side-effect.mq.md --mode project \
  --allow-write out --allow-write 'out/**' --json --trace-file unsafe.trace.json >/dev/null
test -f out/view-sentinel.txt
python3 - <<'PY'
content = open("out/view-sentinel.txt", "rb").read()
assert content == b"explicit run only\n", content
print("unsafe-side-effect project+grants: PASS (sentinel written explicitly)")
PY
rm -rf out

core_sha_after="$(sha256sum "$MARQDO_BIN" | cut -d' ' -f1)"
test "$core_sha_before" = "$core_sha_after"
echo "examples: PASS (frozen CLI sha256 $core_sha_before unchanged)"
