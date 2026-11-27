#!/usr/bin/env python3
"""Publish a Gitea release with a draft transaction and immutable public assets.

Order (issue #10 §4.2):
  create/reuse draft → upload non-index assets → upload release-evidence.json last
  → re-read and verify → mark non-draft → re-confirm.

Never deletes or replaces assets on an already-published (non-draft) release.
Identical published identity is treated as idempotent success.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import mimetypes
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

DEFAULT_API = "https://git.cflmy.cn/api/v1"
INDEX_NAME = "release-evidence.json"


class PublishError(Exception):
    pass


def token() -> str:
    value = (
        os.environ.get("GITEA_PUBLISH_TOKEN")
        or os.environ.get("GITEA_TOKEN")
        or os.environ.get("gitea_token")
    )
    if not value:
        raise PublishError("GITEA_PUBLISH_TOKEN (or GITEA_TOKEN) is not set")
    return value


def request(
    api: str,
    method: str,
    path: str,
    token_value: str,
    payload: dict | None = None,
    raw: bytes | None = None,
    content_type: str = "application/json",
) -> tuple[int, bytes]:
    url = f"{api}{path}"
    headers = {"Authorization": f"token {token_value}", "Accept": "application/json"}
    data = None
    if payload is not None:
        data = json.dumps(payload).encode()
        headers["Content-Type"] = content_type
    elif raw is not None:
        data = raw
        headers["Content-Type"] = content_type
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=120) as response:
            return response.status, response.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


def decode(body: bytes) -> object:
    if not body:
        return None
    try:
        return json.loads(body)
    except Exception:
        return None


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def get_release_by_tag(api: str, repo: str, tag: str, token_value: str) -> dict | None:
    status, body = request(
        api,
        "GET",
        f"/repos/{repo}/releases/tags/{urllib.parse.quote(tag, safe='')}",
        token_value,
    )
    if status == 404:
        return None
    if status != 200:
        raise PublishError(f"lookup release {tag} failed with HTTP {status}")
    release = decode(body)
    if not isinstance(release, dict) or "id" not in release:
        raise PublishError("release lookup response malformed")
    return release


def create_draft(
    api: str,
    repo: str,
    tag: str,
    body: str,
    token_value: str,
    *,
    prerelease: bool,
    target: str,
) -> dict:
    payload = {
        "tag_name": tag,
        "name": tag,
        "body": body,
        "draft": True,
        "prerelease": prerelease,
        "target_commitish": target,
    }
    status, response = request(api, "POST", f"/repos/{repo}/releases", token_value, payload)
    if status not in (200, 201):
        raise PublishError(f"creating draft release failed with HTTP {status}: {response[:200]!r}")
    release = decode(response)
    if not isinstance(release, dict) or "id" not in release:
        raise PublishError("draft create response malformed")
    return release


def patch_release(api: str, repo: str, release_id: int, token_value: str, payload: dict) -> dict:
    status, response = request(
        api, "PATCH", f"/repos/{repo}/releases/{release_id}", token_value, payload
    )
    if status != 200:
        raise PublishError(f"updating release {release_id} failed with HTTP {status}")
    release = decode(response)
    if not isinstance(release, dict):
        raise PublishError("release patch response malformed")
    return release


def list_assets(api: str, repo: str, release_id: int, token_value: str) -> list[dict]:
    status, response = request(
        api, "GET", f"/repos/{repo}/releases/{release_id}/assets", token_value
    )
    if status != 200:
        raise PublishError(f"listing assets failed with HTTP {status}")
    assets = decode(response)
    if not isinstance(assets, list):
        raise PublishError("asset list malformed")
    return assets


def upload_asset(
    api: str, repo: str, release_id: int, path: Path, token_value: str
) -> dict:
    name = path.name
    if not name or "/" in name or name in (".", ".."):
        raise PublishError(f"unsafe asset name: {name!r}")
    existing = list_assets(api, repo, release_id, token_value)
    for asset in existing:
        if asset.get("name") == name:
            raise PublishError(
                f"asset {name} already exists on draft/release {release_id}; "
                "refusing to overwrite"
            )
    boundary = f"----marqdo{uuid.uuid4().hex}"
    content_type = mimetypes.guess_type(name)[0] or "application/octet-stream"
    body = bytearray()
    body.extend(f"--{boundary}\r\n".encode())
    body.extend(
        f'Content-Disposition: form-data; name="attachment"; filename="{name}"\r\n'.encode()
    )
    body.extend(f"Content-Type: {content_type}\r\n\r\n".encode())
    body.extend(path.read_bytes())
    body.extend(f"\r\n--{boundary}--\r\n".encode())
    status, response = request(
        api,
        "POST",
        f"/repos/{repo}/releases/{release_id}/assets?name={urllib.parse.quote(name)}",
        token_value,
        raw=bytes(body),
        content_type=f"multipart/form-data; boundary={boundary}",
    )
    if status not in (200, 201):
        raise PublishError(f"uploading {name} failed with HTTP {status}: {response[:200]!r}")
    result = decode(response)
    if not isinstance(result, dict):
        raise PublishError(f"upload response for {name} malformed")
    return result


def asset_identity(path: Path) -> dict:
    return {
        "name": path.name,
        "size": path.stat().st_size,
        "sha256": sha256_file(path),
    }


def published_matches(release: dict, expected: list[dict]) -> bool:
    remote = {
        asset.get("name"): int(asset.get("size") or -1)
        for asset in (release.get("assets") or [])
    }
    if set(remote) != {item["name"] for item in expected}:
        return False
    for item in expected:
        if remote.get(item["name"]) != item["size"]:
            return False
    return True


def publish(
    *,
    api: str,
    repo: str,
    tag: str,
    body: str,
    target: str,
    assets: list[Path],
    prerelease: bool,
    token_value: str,
) -> dict:
    if not assets:
        raise PublishError("no assets to publish")
    index_assets = [path for path in assets if path.name == INDEX_NAME]
    other_assets = [path for path in assets if path.name != INDEX_NAME]
    if len(index_assets) != 1:
        raise PublishError("exactly one release-evidence.json is required")
    ordered = other_assets + index_assets
    expected = [asset_identity(path) for path in ordered]

    existing = get_release_by_tag(api, repo, tag, token_value)
    if existing and not existing.get("draft"):
        if published_matches(existing, expected):
            return {
                "status": "idempotent",
                "release_id": existing["id"],
                "html_url": existing.get("html_url"),
                "tag": tag,
                "assets": expected,
            }
        raise PublishError(
            f"published release {tag} already exists with different assets; "
            "refusing to overwrite"
        )

    if existing and existing.get("draft"):
        release = existing
        # Keep draft body/target aligned when continuing an interrupted publish.
        release = patch_release(
            api,
            repo,
            int(release["id"]),
            token_value,
            {
                "name": tag,
                "body": body,
                "draft": True,
                "prerelease": prerelease,
                "target_commitish": target,
            },
        )
    else:
        release = create_draft(
            api, repo, tag, body, token_value, prerelease=prerelease, target=target
        )

    release_id = int(release["id"])
    remote_names = {asset.get("name") for asset in list_assets(api, repo, release_id, token_value)}
    for path in ordered:
        if path.name in remote_names:
            # Same-name asset already on this draft: require identical size, else fail.
            match = next(
                asset
                for asset in list_assets(api, repo, release_id, token_value)
                if asset.get("name") == path.name
            )
            if int(match.get("size") or -1) != path.stat().st_size:
                raise PublishError(
                    f"draft asset {path.name} size mismatch; refusing to overwrite"
                )
            continue
        upload_asset(api, repo, release_id, path, token_value)

    refreshed = get_release_by_tag(api, repo, tag, token_value)
    if not refreshed or not published_matches(refreshed, expected):
        raise PublishError("post-upload release asset list does not match local files")

    published = patch_release(
        api,
        repo,
        release_id,
        token_value,
        {"draft": False, "prerelease": prerelease, "name": tag, "body": body},
    )
    if published.get("draft"):
        raise PublishError("release remained draft after publish")
    final = get_release_by_tag(api, repo, tag, token_value)
    if not final or final.get("draft") or not published_matches(final, expected):
        raise PublishError("final release confirmation failed")
    return {
        "status": "published",
        "release_id": final["id"],
        "html_url": final.get("html_url"),
        "tag": tag,
        "assets": expected,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-url", default=os.environ.get("GITEA_API_URL", DEFAULT_API))
    parser.add_argument("--repo", required=True, help="owner/name")
    parser.add_argument("--tag", required=True)
    parser.add_argument("--target", required=True, help="full commit SHA for the tag")
    parser.add_argument("--body-file", default=None)
    parser.add_argument("--body", default="")
    parser.add_argument("--asset", action="append", default=[], required=True)
    parser.add_argument("--prerelease", action="store_true", default=False)
    parser.add_argument("--evidence-out", default="dist/gitea-publish-result.json")
    args = parser.parse_args(argv)
    body = args.body
    if args.body_file:
        body = Path(args.body_file).read_text(encoding="utf-8")
    assets = [Path(item) for item in args.asset]
    for path in assets:
        if not path.is_file():
            print(f"publish_gitea: FAIL: missing asset {path}", file=sys.stderr)
            return 1
    try:
        result = publish(
            api=args.api_url.rstrip("/"),
            repo=args.repo,
            tag=args.tag,
            body=body,
            target=args.target,
            assets=assets,
            prerelease=args.prerelease,
            token_value=token(),
        )
    except PublishError as error:
        print(f"publish_gitea: FAIL: {error}", file=sys.stderr)
        return 1
    out = Path(args.evidence_out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
