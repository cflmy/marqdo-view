#!/usr/bin/env python3
"""Build a deterministic source archive for a Marqdo component release.

Only allowlisted relative paths are included. The archive uses fixed mtime,
uid/gid, and sorted member names so the same tree always yields the same
sha256. Output is written under an isolated directory (default: dist/).
"""

from __future__ import annotations

import argparse
import hashlib
import io
import os
import sys
import tarfile
from pathlib import Path

DEFAULT_ALLOWLIST = (
    "modules",
    "index.mq.md",
    "marqdo.yaml",
    "LICENSE",
    "README.md",
)
FIXED_MTIME = 0
EVIDENCE_NAME = "release-evidence.json"


class PackageError(Exception):
    pass


def fail(message: str) -> None:
    raise PackageError(message)


def is_safe_relative(path: str) -> bool:
    if not path or path.startswith("/") or path.startswith("\\"):
        return False
    parts = Path(path).parts
    return not any(part in (".", "..") for part in parts)


def collect_files(root: Path, allowlist: list[str]) -> list[tuple[str, Path]]:
    selected: dict[str, Path] = {}
    for entry in allowlist:
        if not is_safe_relative(entry):
            fail(f"unsafe allowlist entry: {entry!r}")
        target = root / entry
        if not target.exists():
            continue
        if target.is_file():
            selected[entry.replace("\\", "/")] = target
            continue
        if not target.is_dir():
            fail(f"allowlist entry is not a regular file or directory: {entry}")
        for path in sorted(target.rglob("*")):
            if not path.is_file():
                continue
            if path.is_symlink():
                fail(f"symlinks are not publishable: {path.relative_to(root)}")
            relative = path.relative_to(root).as_posix()
            if not is_safe_relative(relative):
                fail(f"unsafe relative path: {relative}")
            selected[relative] = path
    if not selected:
        fail("allowlist matched no files")
    return sorted(selected.items(), key=lambda item: item[0])


def archive_bytes(files: list[tuple[str, Path]]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz", format=tarfile.PAX_FORMAT, compresslevel=9) as archive:
        for relative, path in files:
            data = path.read_bytes()
            info = tarfile.TarInfo(name=relative)
            info.size = len(data)
            info.mtime = FIXED_MTIME
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def write_outputs(
    root: Path,
    out_dir: Path,
    component_id: str,
    version: str,
    allowlist: list[str],
) -> dict:
    files = collect_files(root, allowlist)
    payload = archive_bytes(files)
    digest = hashlib.sha256(payload).hexdigest()
    out_dir.mkdir(parents=True, exist_ok=True)
    archive_name = f"marqdo-{component_id}-{version}-source.tar.gz"
    archive_path = out_dir / archive_name
    sha_path = out_dir / f"{archive_name}.sha256"
    archive_path.write_bytes(payload)
    sha_path.write_text(f"{digest}  {archive_name}\n", encoding="utf-8")
    return {
        "archive": str(archive_path),
        "sha256_file": str(sha_path),
        "name": archive_name,
        "size": len(payload),
        "sha256": digest,
        "files": [relative for relative, _ in files],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".", help="component repository root")
    parser.add_argument("--component-id", required=True)
    parser.add_argument("--version", required=True, help="version without leading v")
    parser.add_argument("--out-dir", default="dist")
    parser.add_argument(
        "--allow",
        action="append",
        default=[],
        help="relative allowlist entry (repeatable); defaults applied when omitted",
    )
    args = parser.parse_args(argv)
    allowlist = args.allow or list(DEFAULT_ALLOWLIST)
    root = Path(args.root).resolve()
    out_dir = Path(args.out_dir)
    if not out_dir.is_absolute():
        out_dir = root / out_dir
    try:
        result = write_outputs(root, out_dir, args.component_id, args.version, allowlist)
    except PackageError as error:
        print(f"package_source: FAIL: {error}", file=sys.stderr)
        return 1
    print(result["archive"])
    print(result["sha256_file"])
    print(f"sha256={result['sha256']} size={result['size']} files={len(result['files'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
