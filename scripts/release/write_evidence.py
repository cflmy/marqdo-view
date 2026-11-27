#!/usr/bin/env python3
"""Write the external release-evidence.json index for a component release.

The index references already-built assets. It never includes itself in
`assets[]` (no self-referential digest). Downstream discoverers hash the index
bytes separately when they download it.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")
MAX_ASSETS = 128
MAX_DEPENDENCIES = 256
INDEX_NAME = "release-evidence.json"


class EvidenceError(Exception):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def parse_dependency(raw: str) -> dict:
    # id=cli,source_sha=<40hex>[,asset_sha256=<64hex>]
    fields = {}
    for part in raw.split(","):
        if "=" not in part:
            raise EvidenceError(f"invalid dependency field: {raw!r}")
        key, value = part.split("=", 1)
        fields[key.strip()] = value.strip()
    component_id = fields.get("id")
    source_sha = fields.get("source_sha")
    if not component_id or not source_sha or not SHA_RE.match(source_sha):
        raise EvidenceError(f"dependency requires id and 40-hex source_sha: {raw!r}")
    entry = {"id": component_id, "source_sha": source_sha}
    asset = fields.get("asset_sha256")
    if asset:
        if not DIGEST_RE.match(asset):
            raise EvidenceError(f"invalid asset_sha256 in dependency: {raw!r}")
        entry["asset_sha256"] = asset
    return entry


def build_evidence(
    *,
    component_id: str,
    repository: str,
    tag: str,
    source_sha: str,
    assets: list[Path],
    dependencies: list[dict],
    pipeline_number: int | None,
    pipeline_url: str | None,
) -> dict:
    if not component_id:
        raise EvidenceError("component_id is required")
    if "/" not in repository:
        raise EvidenceError("repository must be owner/name")
    if not tag:
        raise EvidenceError("tag is required")
    if not SHA_RE.match(source_sha):
        raise EvidenceError("source_sha must be a 40-hex commit")
    if not assets:
        raise EvidenceError("at least one non-index asset is required")
    if len(assets) > MAX_ASSETS:
        raise EvidenceError(f"more than {MAX_ASSETS} assets")
    if len(dependencies) > MAX_DEPENDENCIES:
        raise EvidenceError(f"more than {MAX_DEPENDENCIES} dependencies")

    asset_entries = []
    seen_names: set[str] = set()
    for path in assets:
        if not path.is_file():
            raise EvidenceError(f"asset missing: {path}")
        name = path.name
        if name == INDEX_NAME:
            raise EvidenceError("assets must not include release-evidence.json itself")
        if name in seen_names or "/" in name or name in (".", ".."):
            raise EvidenceError(f"unsafe or duplicate asset name: {name!r}")
        seen_names.add(name)
        target = "source"
        if name.endswith(".tar.gz") and "linux" in name:
            target = "x86_64-unknown-linux-gnu"
        asset_entries.append(
            {
                "name": name,
                "target": target,
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )

    evidence = {
        "component_id": component_id,
        "repository": repository,
        "tag": tag,
        "source_sha": source_sha,
        "ci": {
            "pipeline_number": pipeline_number,
            "url": pipeline_url,
            "source_sha": source_sha,
        },
        "assets": asset_entries,
        "dependencies": dependencies,
    }
    return evidence


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--component-id", required=True)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--asset", action="append", default=[], required=True)
    parser.add_argument("--dependency", action="append", default=[])
    parser.add_argument("--pipeline-number", type=int, default=None)
    parser.add_argument("--pipeline-url", default=None)
    parser.add_argument("--out", default=f"dist/{INDEX_NAME}")
    args = parser.parse_args(argv)
    try:
        dependencies = [parse_dependency(item) for item in args.dependency]
        evidence = build_evidence(
            component_id=args.component_id,
            repository=args.repository,
            tag=args.tag,
            source_sha=args.source_sha,
            assets=[Path(item) for item in args.asset],
            dependencies=dependencies,
            pipeline_number=args.pipeline_number,
            pipeline_url=args.pipeline_url,
        )
    except EvidenceError as error:
        print(f"write_evidence: FAIL: {error}", file=sys.stderr)
        return 1
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(evidence, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    if len(payload.encode()) > 1024 * 1024:
        print("write_evidence: FAIL: index exceeds 1 MiB", file=sys.stderr)
        return 1
    out.write_text(payload, encoding="utf-8")
    print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
