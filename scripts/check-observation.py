#!/usr/bin/env python3
"""Machine checks for the current observation/analysis contracts (G06/G07/G08).

These probes use only the frozen CLI's public interface and assert the exact
contract marqdo-view 0.3.0 consumes. They do not run the View binary; View-side
regressions are covered by the cargo test suite owned by the migration.

Usage:
  MARQDO_BIN=/abs/path/marqdo python3 scripts/check-observation.py stream-snapshot
  MARQDO_BIN=/abs/path/marqdo python3 scripts/check-observation.py binding
  MARQDO_BIN=/abs/path/marqdo python3 scripts/check-observation.py fail-closed
"""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = pathlib.Path(os.environ.get("MARQDO_BIN", "")).resolve()


def fail(message):
    print(f"observation probe: FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def run(args, cwd, expect=0):
    result = subprocess.run([str(BIN), *args], cwd=cwd, capture_output=True, text=True)
    if result.returncode != expect:
        fail(f"{' '.join(args)} exited {result.returncode}, expected {expect}\n{result.stderr}")
    return result


def sha256_prefixed(data):
    return "sha256:" + hashlib.sha256(data).hexdigest()


def workspace():
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    directory = tempfile.TemporaryDirectory(prefix="observation-contract-", dir=dist)
    root = pathlib.Path(directory.name)
    for source in (
        "examples/function-params.mq.md",
        "examples/branch-loop.mq.md",
        "examples/knowledge-only.mq.md",
    ):
        shutil.copy(ROOT / source, root / pathlib.Path(source).name)
    return directory, root


def load_snapshot(path):
    snapshot = json.loads(pathlib.Path(path).read_text())
    if snapshot.get("schema") != "marqdo.observation" or snapshot.get("schema_version") != 1:
        fail(f"not an Observation v1 snapshot: {path}")
    events = snapshot["events"]
    run_id = snapshot["run"]["id"]
    if [event["seq"] for event in events] != list(range(len(events))):
        fail("snapshot seq is not zero-based contiguous")
    if any(event["run_id"] != run_id for event in events):
        fail("snapshot mixes run_id values")
    return snapshot, events


def stream_snapshot():
    directory, root = workspace()
    with directory:
        stream = run(
            ["call", "function-params.mq.md", "double", "--arg", "n=5", "--mode", "pure",
             "--output", "jsonl", "--observation-file", "obs.json"],
            root,
        ).stdout
        lines = [json.loads(line) for line in stream.splitlines() if line.strip()]
        snapshot, events = load_snapshot(root / "obs.json")
        if lines != events:
            fail("JSONL stream and atomic snapshot disagree for the same execution")
        if snapshot["run"]["status"] != "ok":
            fail(f"unexpected run.status {snapshot['run']['status']}")
        if events[-1]["kind"] != "run.completed":
            fail("terminal event is not run.completed")
        if events[-1]["payload"]["value"] != {"type": "int", "decimal": "10"}:
            fail("call result is not Int 10")
        print("stream/snapshot equivalence: PASS (%d events, one run_id, contiguous seq, one terminal)" % len(events))

        truncated_out = run(
            ["run", "branch-loop.mq.md", "--mode", "pure", "--observe-max-events", "2",
             "--output", "jsonl", "--observation-file", "truncated.json"],
            root,
        ).stdout
        truncated_lines = [json.loads(line) for line in truncated_out.splitlines() if line.strip()]
        truncated_snapshot, truncated_events = load_snapshot(root / "truncated.json")
        if truncated_lines != truncated_events:
            fail("truncated stream and snapshot disagree")
        truncated = truncated_snapshot["capture"]["truncated"]
        if not truncated or truncated.get("reason") != "event_budget":
            fail("missing observation.truncated capture metadata")
        if truncated_events[-1]["kind"] != "run.completed":
            fail("truncated run has no truthful terminal")
        print("observation truncation: PASS (capture.truncated=%s, terminal=%s)"
              % (truncated.get("reason"), truncated_events[-1]["kind"]))


def span_ok(span, length):
    return (
        isinstance(span, dict)
        and isinstance(span.get("start"), int)
        and isinstance(span.get("end"), int)
        and 0 <= span["start"] <= span["end"] <= length
    )


def binding():
    directory, root = workspace()
    with directory:
        run(["check", "function-params.mq.md", "--emit", "analysis.json", "--json"], root)
        analysis_bytes = (root / "analysis.json").read_bytes()
        analysis = json.loads(analysis_bytes)
        if analysis.get("schema") != "marqdo.analysis" or analysis.get("schema_version") != 3:
            fail("check --emit did not produce Analysis revision 3")
        expected_digest = sha256_prefixed(analysis_bytes)
        module = analysis["modules"][0]
        source = module["source"].encode("utf-8")
        if module["source_digest"] != hashlib.sha256(source).hexdigest():
            fail("module.source_digest does not hash the exact module.source bytes")
        run(
            ["call", "function-params.mq.md", "double", "--arg", "n=5", "--mode", "pure",
             "--output", "jsonl", "--observation-file", "obs.json"],
            root,
        )
        snapshot, events = load_snapshot(root / "obs.json")
        started = next((event for event in events if event["kind"] == "run.started"), None)
        if started is None:
            fail("no run.started event")
        observed = started["payload"]["source"]
        if observed.get("analysis_digest") != expected_digest:
            fail("run.started analysis_digest does not bind the exact check --emit bytes")
        if observed.get("root_id") != analysis["project"]["root_id"]:
            fail("run.started root_id does not match Analysis project.root_id")
        if snapshot["run"]["source"] != observed:
            fail("snapshot run.source disagrees with run.started source")
        module_ids = {candidate["id"] for candidate in analysis["modules"]}
        for event in events:
            payload = event["payload"]
            if payload.get("module_id") is not None and payload["module_id"] not in module_ids:
                fail(f"event {event['seq']} references unknown module_id")
            span = payload.get("span")
            if span is not None and not span_ok(span, len(source)):
                fail(f"event {event['seq']} span is outside the exact source bytes")
        # A source edit must invalidate the binding: a new check emits a different digest.
        original = (root / "function-params.mq.md").read_bytes()
        (root / "function-params.mq.md").write_bytes(original + b"\nsource changed\n")
        result = subprocess.run(
            [str(BIN), "check", "function-params.mq.md", "--emit", "analysis-edited.json", "--json"],
            cwd=root, capture_output=True, text=True,
        )
        (root / "function-params.mq.md").write_bytes(original)
        if result.returncode == 0:
            edited = (root / "analysis-edited.json").read_bytes()
            if sha256_prefixed(edited) == expected_digest:
                fail("source edit did not change the Analysis digest")
        print("exact Analysis binding: PASS (digest=%s, root/module/span checks, source edit invalidates)" % expected_digest)


def fail_closed():
    directory, root = workspace()
    with directory:
        cases = [
            (["run", "branch-loop.mq.md", "--mode", "pure", "--json"], "retired --json"),
            (["run", "branch-loop.mq.md", "--mode", "pure", "--trace-file", "x.json"], "retired --trace-file"),
            (["run", "branch-loop.mq.md", "--mode", "pure", "--event-stream", "jsonl"], "retired --event-stream"),
        ]
        for args, label in cases:
            result = subprocess.run([str(BIN), *args], cwd=root, capture_output=True, text=True)
            if result.returncode != 2:
                fail(f"{label}: expected exit 2, got {result.returncode}")
            print(f"fail-closed {label}: PASS (exit 2, no fallback retry)")
        strict = subprocess.run(
            [str(BIN), "run", "branch-loop.mq.md", "--mode", "strict", "--output", "jsonl"],
            cwd=root, capture_output=True, text=True,
        )
        if strict.returncode != 2 or "strict.unsupported_grant" not in strict.stderr:
            fail(f"strict + jsonl: expected exit 2 strict.unsupported_grant, got {strict.returncode}: {strict.stderr}")
        print("fail-closed strict live destination: PASS (exit 2, strict.unsupported_grant)")

        # Retired manifest fixture: used only to prove the current CLI rejects it.
        old = root / "old-tool"
        (old / "bin").mkdir(parents=True)
        (old / "bin" / "probe").write_text("#!/usr/bin/env bash\nexit 0\n")
        os.chmod(old / "bin" / "probe", 0o755)
        (old / "tool.json").write_text(json.dumps({
            "schema": "marqdo.cli-tool",
            "schema_version": 1,
            "name": "old-probe",
            "version": "0.2.0",
            "command": "old-probe",
            "entrypoint": "bin/probe",
            "compatibility": {"schema": "retired-fixture", "versions": ["fixture-only"]},
        }))
        env = {**os.environ, "MARQDO_TOOL_HOME": str(root / "tool-home")}
        legacy = subprocess.run(
            [str(BIN), "tool", "install", str(old / "tool.json")],
            cwd=root, capture_output=True, text=True, env=env,
        )
        if legacy.returncode == 0 or "compatibility" not in legacy.stderr:
            fail(f"retired tool.json was not rejected: {legacy.returncode} {legacy.stderr}")
        print("fail-closed retired tool.json: PASS (unknown field `compatibility`, no migrator)")

        # Retired install record (record_version 1): dispatching the tool must
        # fail with a remove+reinstall instruction; there is no migrator.
        record_home = root / "record-home"
        record_dir = record_home / "view"
        record_dir.mkdir(parents=True)
        (record_dir / "record.json").write_text(json.dumps({
            "record_version": 1,
            "command": "view",
            "name": "marqdo-view",
            "version": "0.2.0",
            "entrypoint": "bin/marqdo-view",
            "bundle": [],
            "bundle_sha256": "0" * 64,
            "source": "retired-fixture",
            "schema_version": 1,
            "requires": {"api_revision": 1},
        }))
        retired_env = {
            **os.environ,
            "MARQDO_TOOL_HOME": str(record_home),
            "MARQDO_BIN": str(BIN),
        }
        retired = subprocess.run(
            [str(BIN), "view", "--version"],
            cwd=root, capture_output=True, text=True, env=retired_env,
        )
        message = retired.stderr + retired.stdout
        if retired.returncode == 0 or "reinstall" not in message:
            fail(f"retired record_version 1 was not rejected: {retired.returncode} {message}")
        print("fail-closed retired record_version 1: PASS (dispatch refused, remove+reinstall hint)")


def main():
    if not BIN.is_file() or not os.access(BIN, os.X_OK):
        fail("set MARQDO_BIN to the unpacked frozen marqdo binary")
    if len(sys.argv) != 2:
        fail(__doc__)
    mode = sys.argv[1]
    if mode == "stream-snapshot":
        stream_snapshot()
    elif mode == "binding":
        binding()
    elif mode == "fail-closed":
        fail_closed()
    else:
        fail(f"unknown mode {mode}")


if __name__ == "__main__":
    main()
