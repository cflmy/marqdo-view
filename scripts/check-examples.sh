#!/usr/bin/env bash
# Prove the five 3.0 example documents against the frozen released CLI using
# the current public contracts: one `check --emit` Analysis v3 source and one
# Observation v1 sequence per `run`/`call`. Never rebuilds the Core; runs in a
# copied temp project so the repository examples are never modified.
#
# Usage: MARQDO_BIN=/abs/path/marqdo scripts/check-examples.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
core="${MARQDO_BIN}"
core_sha_before="$(sha256sum "$core" | cut -d' ' -f1)"

work="$repo_root/dist/examples-check"
rm -rf "$work"
mkdir -p "$work"
cp examples/welcome.mq.md examples/branch-loop.mq.md examples/function-params.mq.md \
  examples/knowledge-only.mq.md examples/unsafe-side-effect.mq.md "$work/"
cp examples/view.json "$work/"

cd "$work"

echo "==> check all five example documents (one static source: check --emit)"
for file in welcome.mq.md branch-loop.mq.md function-params.mq.md knowledge-only.mq.md unsafe-side-effect.mq.md; do
  if ! "$core" check "$file" --emit "analysis-$file.json" --json > "check-$file.json"; then
    echo "error: frozen CLI refused $file" >&2
    cat "check-$file.json" >&2
    exit 1
  fi
  test -s "analysis-$file.json"
done
python3 - <<'PY'
import json
for name in ("welcome", "branch-loop", "function-params", "knowledge-only", "unsafe-side-effect"):
    artifact = json.load(open(f"analysis-{name}.mq.md.json"))
    assert artifact["schema"] == "marqdo.analysis", artifact["schema"]
    assert artifact["schema_version"] == 3, artifact["schema_version"]
    assert artifact["modules"] and artifact["modules"][0]["source"], name
    assert artifact["modules"][0]["document"], name
    assert artifact["project"]["entry_module_id"], name
print("Analysis v3: PASS (schema_version 3, exact source/document/entry present)")
PY

echo "==> call function-params.double(n=5) -> 10 via Observation v1"
"$core" call function-params.mq.md double --arg n=5 --mode pure --output json > call.json
python3 - <<'PY'
import json
snapshot = json.load(open("call.json"))
assert snapshot["schema"] == "marqdo.observation" and snapshot["schema_version"] == 1
assert snapshot["run"]["status"] == "ok", snapshot["run"]
terminal = snapshot["events"][-1]
assert terminal["kind"] == "run.completed", terminal
assert terminal["payload"]["value"] == {"type": "int", "decimal": "10"}, terminal["payload"]
print("call double(5): PASS (Observation v1, Int 10)")
PY

echo "==> run branch-loop main -> Int 0 with loop/return events"
"$core" run branch-loop.mq.md --mode pure --output json > run.json
python3 - <<'PY'
import json
snapshot = json.load(open("run.json"))
assert snapshot["run"]["status"] == "ok", snapshot["run"]
terminal = snapshot["events"][-1]
assert terminal["kind"] == "run.completed", terminal
assert terminal["payload"]["value"] == {"type": "int", "decimal": "0"}, terminal["payload"]
kinds = [event["kind"] for event in snapshot["events"]]
assert "loop.iteration" in kinds and "return" in kinds, kinds
seqs = [event["seq"] for event in snapshot["events"]]
assert seqs == list(range(len(seqs))), seqs
assert len({event["run_id"] for event in snapshot["events"]}) == 1
print("run branch-loop: PASS (Int 0, contiguous seq, kinds: %s)" % ",".join(kinds))
PY

echo "==> knowledge-only has no main (truthful rejection, no fallback)"
set +e
"$core" run knowledge-only.mq.md --mode pure --output json > knowledge.json 2>/dev/null
status=$?
set -e
test "$status" -ne 0
python3 - <<'PY'
import json
snapshot = json.load(open("knowledge.json"))
terminal = snapshot["events"][-1]
assert terminal["kind"] == "run.rejected", terminal
assert terminal["payload"]["error"]["code"] == "sem.no_entry_function", terminal["payload"]
print("knowledge-only: PASS (explicit sem.no_entry_function)")
PY

echo "==> unsafe-side-effect writes nothing in pure mode"
set +e
"$core" run unsafe-side-effect.mq.md --mode pure --output json > unsafe-pure.json 2>/dev/null
status=$?
set -e
test "$status" -ne 0
test ! -e out/view-sentinel.txt
python3 - <<'PY'
import json
snapshot = json.load(open("unsafe-pure.json"))
terminal = snapshot["events"][-1]
assert terminal["kind"] == "run.failed", terminal
assert terminal["payload"]["error"]["code"] == "permission.denied", terminal["payload"]
print("unsafe-side-effect pure: PASS (permission.denied, no file)")
PY

echo "==> unsafe-side-effect writes only with explicit project grants"
"$core" run unsafe-side-effect.mq.md --mode project \
  --allow-write out --allow-write 'out/**' --output json >/dev/null
test -f out/view-sentinel.txt
python3 - <<'PY'
content = open("out/view-sentinel.txt", "rb").read()
assert content == b"explicit run only\n", content
print("unsafe-side-effect project+grants: PASS (sentinel written explicitly)")
PY
rm -rf out

core_sha_after="$(sha256sum "$core" | cut -d' ' -f1)"
test "$core_sha_before" = "$core_sha_after"
echo "examples: PASS (frozen CLI sha256 $core_sha_before unchanged)"
