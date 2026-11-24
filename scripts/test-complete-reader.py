#!/usr/bin/env python3
"""HTTP and ZIP acceptance against an unmodified RC.2 release binary."""

import hashlib
import http.cookiejar
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
import urllib.error
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
CORE = Path(os.environ["MARQDO_BIN"]).resolve(strict=True)
VIEW = Path(os.environ.get("VIEW_BIN", ROOT / "target/debug/marqdo-view")).resolve(strict=True)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def session(base):
    browser = urllib.request.build_opener(
        urllib.request.ProxyHandler({}),
        urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()),
    )
    html = browser.open(base, timeout=10).read().decode()
    token = re.search(r'name="mq-csrf-token" content="([a-f0-9]+)"', html)[1]
    digest = re.search(r'data-analysis-digest="([^"]+)"', html)[1]
    return browser, token, digest, html


def post(browser, base, route, token, body, status=200, origin=None):
    headers = {"Content-Type": "application/json", "Origin": origin or base}
    if token is not None:
        headers["X-View-CSRF"] = token
    request = urllib.request.Request(base + route, json.dumps(body).encode(), headers)
    try:
        with browser.open(request, timeout=20) as response:
            assert response.status == status, (route, response.status, status)
            return response.read(), response.headers
    except urllib.error.HTTPError as error:
        assert error.code == status, (route, error.code, status, error.read())
        return error.read(), error.headers


def verify_zip(payload, destination):
    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        names = archive.namelist()
        assert len(names) == len(set(names))
        for item in archive.infolist():
            path = PurePosixPath(item.filename)
            assert not path.is_absolute() and ".." not in path.parts
            assert item.compress_type == zipfile.ZIP_STORED and item.file_size <= 16 * 1024 ** 2
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["schema_version"] == 2 and not manifest["executed_during_export"]
        assert manifest["execution"]["source_verified"] and manifest["execution"]["status"] == "succeeded"
        assert manifest["execution"]["parameters"] == "omitted"
        assert manifest["source"]["document"] == "index.mq.md"
        assert manifest["execution"]["core_protocol"] == {"analysis": 3, "trace": 2, "events": 1}
        for entry in manifest["files"] + manifest["assets"]:
            data = archive.read(entry["path"])
            assert sha(data) == entry["sha256"] and len(data) == entry["size"]
        assert not any(name.endswith(".js") for name in names)
        html = archive.read("index.html").decode()
        assert "<math" in html and "<script" not in html and 'data-action="call-function"' not in html
        assert "本次执行快照" in html and "Int(4)" in html and "参数未包含" in html
        assert html.count('class="mq-snapshot"') == 1
        other = archive.read("pages/docs/explanation.mq.md.html").decode()
        assert 'class="mq-snapshot"' not in other and '<script' not in other
        if destination:
            Path(destination).write_bytes(payload)
        print("snapshot manifest sha256", sha(archive.read("manifest.json")))


def check(root):
    project = root / "project"
    shutil.copytree(ROOT / "examples/complete-reader", project)
    scratch = root / "scratch"
    scratch.mkdir()
    count = root / "runs"
    wrapper = root / "core"
    wrapper.write_text(
        "#!/usr/bin/env python3\n"
        "import os, sys\n"
        f"core, count = {str(CORE)!r}, {str(count)!r}\n"
        "if sys.argv[1] in ('run', 'call'):\n"
        "    assert '--event-stream' in sys.argv and '--json' not in sys.argv\n"
        "    with open(count, 'a') as f: f.write(sys.argv[1]+'\\n')\n"
        "os.execv(core, [core, *sys.argv[1:]])\n"
    )
    wrapper.chmod(0o700)
    env = {**os.environ, "TMPDIR": str(scratch)}
    args = [str(VIEW), "--marqdo-bin", str(wrapper)]
    with subprocess.Popen(
        [*args, str(project), "--port", "0", "--no-open"],
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    ) as server:
        try:
            base = server.stderr.readline().split()[-1].rstrip("/")
            browser, token, digest, html = session(base)
            assert not count.exists()
            assert "<math" in html and 'alt="样本数据图"' in html
            media = re.search(r'src="(/media/[^"]+)" alt="样本数据图"', html)[1]
            with browser.open(base + media, timeout=10) as response:
                data = response.read()
                assert response.headers.get_content_type() == "image/png"
                assert sha(data) in media
            try:
                urllib.request.build_opener(urllib.request.ProxyHandler({})).open(base + media, timeout=10)
            except urllib.error.HTTPError as error:
                assert error.code == 401
            else:
                raise AssertionError("unauthenticated project media was accessible")
            original = {path: sha(path.read_bytes()) for path in project.rglob("*") if path.is_file()}

            def run(function, request_id, parameters=None):
                body = {
                    "document": "index.mq.md",
                    "target": {"kind": "function", "function_id": f"root:complete-reader@0.1.0::index.mq.md::计算.{function}"},
                    "parameters": parameters or [],
                    "analysis_digest": digest,
                    "client_request_id": request_id,
                }
                payload, _ = post(browser, base, "/api/runs", token, body, 202)
                accepted = json.loads(payload)
                with browser.open(base + accepted["events_url"], timeout=20) as response:
                    events = [json.loads(line[6:]) for line in response.read().decode().splitlines() if line.startswith("data: ")]
                return accepted["run_id"], events[-1]

            run_id, terminal = run("write_output", "one")
            assert terminal["state"] == "succeeded" and terminal["result"]["stdout"] == "4\n", terminal
            assert terminal["result"]["trace_status"] == "verified"
            assert count.read_text().splitlines() == ["call"]
            selection = {
                "schema": "marqdo.view-static-export-request",
                "schema_version": 1,
                "run_id": run_id,
                "document": "index.mq.md",
                "analysis_digest": digest,
                "scope": "document",
                "include": {"result_value": True, "stdout": True, "stderr": False, "arguments": False, "trace_summary": True},
                "confirm_sensitive_output": True,
                "client_export_request_id": "zip-one",
            }
            post(browser, base, "/api/exports", None, selection, 403)
            post(browser, base, "/api/exports", token, selection, 403, "https://example.invalid")
            other, other_token, _, _ = session(base)
            post(other, base, "/api/exports", other_token, selection, 404)
            post(browser, base, "/api/exports", token, {**selection, "schema_version": 2}, 400)
            post(browser, base, "/api/exports", token, {**selection, "confirm_sensitive_output": False}, 400)
            post(browser, base, "/api/exports", token, {**selection, "run_id": "unknown"}, 404)
            payload, headers = post(browser, base, "/api/exports", token, selection)
            assert headers.get_content_type() == "application/zip" and "attachment" in headers["Content-Disposition"]
            assert headers["Cache-Control"] == "no-store" and headers["X-Content-Type-Options"] == "nosniff"
            verify_zip(payload, os.environ.get("SNAPSHOT_ZIP"))
            subprocess.run([*args, "export", str(project), "--out", str(root / "pure")], check=True, env=env)
            assert "本次执行快照" not in (root / "pure/index.html").read_text()
            assert count.read_text().splitlines() == ["call"]
            assert original == {path: sha(path.read_bytes()) for path in project.rglob("*") if path.is_file()}

            source = project / "index.mq.md"
            original_source = source.read_bytes()
            source.write_bytes(original_source + b"\nsource changed\n")
            post(browser, base, "/api/exports", token, selection, 409)
            source.write_bytes(original_source)
            image = project / "media/flow.png"
            original_image = image.read_bytes()
            image.unlink()
            image.symlink_to(root / "external.png")
            (root / "external.png").write_bytes(original_image)
            post(browser, base, "/api/exports", token, selection, 409)
            image.unlink()
            image.write_bytes(original_image)
            assert count.read_text().splitlines() == ["call"]
            print("pure and selected export both leave execution count at 1; source and media staleness denied")
        finally:
            server.terminate()
            server.wait(timeout=5)
    assert not list(scratch.iterdir()), list(scratch.iterdir())


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="marqdo-view-acceptance-") as directory:
        check(Path(directory))
