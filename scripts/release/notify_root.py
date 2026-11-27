#!/usr/bin/env python3
"""Ask the root Marqdo reconcile workflow to scan after a release.

The payload is only a hint. Correctness comes from the root
`scripts/reconcile-releases.py` reading authoritative Gitea releases.
Lost notifications are recovered by the root `release-reconcile` cron.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sys
import time
import urllib.error
import urllib.request

DEFAULT_URL = "https://ci.cflmy.cn/api/repos/6/pipelines"
DEFAULT_BODY = {"branch": "main", "workflow": "reconcile"}


class NotifyError(Exception):
    pass


def token() -> str:
    value = (
        os.environ.get("WOODPECKER_NOTIFY_TOKEN")
        or os.environ.get("WOODPECKER_TOKEN")
        or os.environ.get("woodpecker_token")
    )
    if not value:
        raise NotifyError("WOODPECKER_NOTIFY_TOKEN (or WOODPECKER_TOKEN) is not set")
    return value


def trigger(url: str, token_value: str, body: dict, attempts: int = 3) -> dict:
    payload = json.dumps(body).encode()
    last_error: Exception | None = None
    for attempt in range(1, attempts + 1):
        request = urllib.request.Request(
            url,
            data=payload,
            method="POST",
            headers={
                "Authorization": f"Bearer {token_value}",
                "Content-Type": "application/json",
                "Accept": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                raw = response.read()
                decoded = json.loads(raw) if raw else {}
                return {
                    "status": "triggered",
                    "http_status": response.status,
                    "pipeline_id": decoded.get("id"),
                    "pipeline_number": decoded.get("number"),
                    "attempt": attempt,
                }
        except urllib.error.HTTPError as error:
            # Auth/permission failures must not retry forever.
            if error.code in (401, 403, 404):
                raise NotifyError(f"trigger rejected with HTTP {error.code}") from error
            last_error = error
        except urllib.error.URLError as error:
            last_error = error
        if attempt < attempts:
            delay = (2 ** (attempt - 1)) + random.uniform(0, 0.5)
            time.sleep(delay)
    raise NotifyError(f"trigger failed after {attempts} attempts: {last_error}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default=os.environ.get("MARQDO_RECONCILE_TRIGGER_URL", DEFAULT_URL))
    parser.add_argument("--branch", default="main")
    parser.add_argument("--workflow", default="reconcile")
    parser.add_argument("--hint-component", default="")
    parser.add_argument("--hint-tag", default="")
    parser.add_argument("--attempts", type=int, default=3)
    args = parser.parse_args(argv)
    body = {"branch": args.branch, "workflow": args.workflow}
    # Hints are informational only; the root reconcile never trusts them as commands.
    if args.hint_component or args.hint_tag:
        body["variables"] = {
            "MARQDO_RELEASE_HINT_COMPONENT": args.hint_component,
            "MARQDO_RELEASE_HINT_TAG": args.hint_tag,
        }
    try:
        result = trigger(args.url, token(), body, attempts=max(1, args.attempts))
    except NotifyError as error:
        print(f"notify_root: FAIL: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
