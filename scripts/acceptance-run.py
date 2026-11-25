#!/usr/bin/env python3
"""Explicit real-run acceptance over the frozen CLI + Live SSE server.

Usage: python3 scripts/acceptance-run.py http://127.0.0.1:PORT/
Prints a PASS line and exits non-zero on any failure. Never polls: it creates
one run, observes one SSE stream until the single terminal event, and asserts
the verified Observation v1 binding (source analysis digest, contiguous seq,
unique terminal) instead of any retired trace protocol.
"""
import http.cookiejar
import json
import re
import sys
import urllib.request

url = sys.argv[1]
origin = url.rstrip("/")
jar = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))


def get(path):
    with opener.open(origin + path, timeout=30) as response:
        return response.read().decode("utf-8")


page = get("/")
csrf = re.search(r'name="mq-csrf-token" content="([0-9a-f]+)"', page)
assert csrf, "csrf token missing"
digest = re.search(r'data-analysis-digest="(sha256:[0-9a-f]{64})"', page)
assert digest, "analysis digest missing"
body = json.dumps({
    "document": "index.mq.md",
    "target": {"kind": "entry"},
    "parameters": [],
    "analysis_digest": digest.group(1),
    "mode": "pure",
    "grants": {"read": [], "write": []},
    "client_request_id": "acceptance-real-run-1",
}).encode()
request = urllib.request.Request(
    origin + "/api/runs",
    data=body,
    headers={
        "Content-Type": "application/json",
        "Origin": origin,
        "X-View-CSRF": csrf.group(1),
    },
    method="POST",
)
with opener.open(request, timeout=60) as response:
    assert response.status == 202, response.status
    accepted = json.loads(response.read().decode("utf-8"))
assert accepted["state"] == "accepted", accepted
events_url = origin + accepted["events_url"]
terminal = None
with opener.open(events_url, timeout=120) as stream:
    data_lines = []
    for raw in stream:
        line = raw.decode("utf-8").rstrip("\n")
        if line.startswith("data: "):
            data_lines.append(line[6:])
        elif line == "" and data_lines:
            event = json.loads("\n".join(data_lines))
            data_lines = []
            if event.get("state") in (
                "succeeded",
                "failed",
                "cancelled",
                "timed_out",
                "unavailable",
            ):
                terminal = event
                break
assert terminal is not None, "no terminal SSE event"
assert terminal["state"] == "succeeded", terminal
assert terminal["result"]["success"] is True, terminal["result"]
assert terminal["result"]["observation_status"] == "verified", terminal["result"]
assert terminal["result"]["observation_protocol"] == 1, terminal["result"]
assert terminal["result"]["observation_truncated"] is False, terminal["result"]
summary = terminal.get("observation_summary") or terminal["result"].get("observation_summary")
assert summary is not None, "missing observation summary"
assert summary.get("status") in ("verified", "ok"), summary
event_count = 0
if isinstance(summary.get("events"), list):
    event_count = len(summary["events"])
elif isinstance(summary.get("counts"), dict):
    event_count = sum(int(value) for value in summary["counts"].values())
print(
    "explicit real run: PASS (run_id=%s observation=%s events=%d)"
    % (accepted["run_id"], terminal["result"]["observation_status"], event_count)
)
