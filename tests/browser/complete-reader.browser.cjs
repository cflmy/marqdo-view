"use strict";

const assert = require("node:assert/strict");
const { execFileSync, spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { createInterface } = require("node:readline");
const { pathToFileURL } = require("node:url");
const { chromium } = require("playwright");

const root = path.resolve(__dirname, "../..");
const core = fs.realpathSync(process.env.MARQDO_BIN);
const view = process.env.VIEW_BIN || path.join(root, "target/debug/marqdo-view");
const artifactDir = process.env.MQ_SCREENSHOT_DIR;
const temp = fs.mkdtempSync(path.join(os.tmpdir(), "mq-complete-browser-"));
const project = path.join(temp, "project");
const runCount = path.join(temp, "calls");
const wrapper = path.join(temp, "counted-core");

fs.cpSync(path.join(root, "examples/complete-reader"), project, { recursive: true });
fs.writeFileSync(wrapper, `#!/usr/bin/env python3
import json, os, subprocess, sys, time
core, calls = ${JSON.stringify(core)}, ${JSON.stringify(runCount)}
args = sys.argv[1:]
if args[0] not in ('run','call'):
    os.execv(core, [core, *args])
assert '--event-stream' in args and '--json' not in args
with open(calls, 'a') as f: f.write(args[0]+'\\n')
if 'write_output' not in args:
    os.execv(core, [core, *args])
with subprocess.Popen([core, *args], stdout=subprocess.PIPE) as child:
    for line in child.stdout:
        sys.stdout.buffer.write(line)
        sys.stdout.buffer.flush()
        if json.loads(line)['kind'] == 'stdout.chunk': time.sleep(1.2)
    sys.exit(child.wait())
`, { mode: 0o700 });

async function awaitUrl(server) {
  const logger = createInterface({ input: server.stderr });
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("View startup timeout")), 8000);
    logger.on("line", line => {
      const url = line.match(/http:\/\/127\.0\.0\.1:\d+\//);
      if (url) {
        clearTimeout(timer);
        resolve(url[0]);
      }
    });
    server.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error("View exited " + code));
    });
    server.once("error", reject);
  });
}

async function screenshot(page, file) {
  if (!artifactDir) return;
  fs.mkdirSync(artifactDir, { recursive: true });
  await page.screenshot({ path: path.join(artifactDir, file), animations: "disabled" });
}

async function imagesLoaded(page, count) {
  const loaded = await page.evaluate(async () => {
    const images = Array.from(document.querySelectorAll("img.mq-document-image"));
    await Promise.all(images.map(image => image.decode()));
    return images.filter(image => image.complete && image.naturalWidth > 0).length;
  });
  assert.equal(loaded, count);
}

async function verifyOffline(browser, url) {
  const offline = await browser.newContext({ javaScriptEnabled: false, offline: true });
  const page = await offline.newPage();
  const requests = [];
  page.on("request", request => requests.push(request.url()));
  await page.goto(url);
  await imagesLoaded(page, 1);
  assert.equal(await page.locator("math mfrac").count(), 1);
  assert.equal(await page.locator("script, [data-action=run], [data-action=call-function]").count(), 0);
  assert.match(await page.locator(".mq-snapshot pre[aria-label=返回值]").innerText(), /4\.0|Float\(4\)/);
  assert.equal(await page.locator(".mq-snapshot [aria-label=stdout]").count(), 0);
  await screenshot(page, "offline-snapshot.png");
  await page.getByRole("link", { name: "误差说明", exact: true }).click();
  await imagesLoaded(page, 2);
  assert.equal(decodeURIComponent(new URL(page.url()).hash), "#误差-1");
  assert.equal(await page.locator(":target").textContent(), "误差");
  assert.equal(await page.locator(".mq-snapshot").count(), 0);
  assert.ok(requests.every(request => request.startsWith("file:") || request.startsWith("data:")), requests);
  await offline.close();
}

(async () => {
  const server = spawn(view, ["--marqdo-bin", wrapper, project, "--port", "0", "--no-open"], {
    stdio: ["ignore", "ignore", "pipe"],
  });
  let browser;
  try {
    const url = await awaitUrl(server);
    browser = await chromium.launch({ executablePath: process.env.CHROMIUM_BIN || undefined });
    console.log("Chromium " + browser.version());
    const context = await browser.newContext({ viewport: { width: 1480, height: 960 }, acceptDownloads: true });
    const page = await context.newPage();
    const external = [];
    const errors = [];
    page.on("request", request => {
      if (!request.url().startsWith(url) && !request.url().startsWith("data:") &&
          !request.url().startsWith("blob:")) external.push(request.url());
    });
    page.on("pageerror", error => errors.push(String(error)));
    await page.goto(url);
    await imagesLoaded(page, 1);
    assert.equal(await page.locator("math mfrac").count(), 1);
    await page.getByRole("link", { name: "误差说明", exact: true }).click();
    await imagesLoaded(page, 2);
    assert.equal(await page.locator(":target").textContent(), "误差");
    assert.equal(decodeURIComponent(new URL(page.url()).hash), "#误差-1");
    await page.getByRole("link", { name: "返回计算均值", exact: true }).click();
    assert.match(await page.locator(":target").innerText(), /计算均值/);
    assert.equal(fs.existsSync(runCount), false);

    await page.locator("h2[id=计算均值] [data-action=call-function]").click();
    await page.locator("[data-param-name=a]").fill("3");
    assert.equal(fs.existsSync(runCount), false);
    await page.locator("[data-action=run-confirm]").click();
    const result = page.locator(".mq-result-slot #mq-run-panel");
    await result.locator("[data-action=export-result]").waitFor();
    assert.equal(await result.getAttribute("data-run-state"), "succeeded");
    assert.match(await result.locator("#mq-run-summary").innerText(), /Float\(4\.0\)/);
    assert.equal(fs.readFileSync(runCount, "utf8"), "call\n");
    await result.scrollIntoViewIfNeeded();
    await screenshot(page, "live-result.png");

    await result.locator("[data-action=export-result]").click();
    assert.equal(await page.locator("#mq-export-download").isDisabled(), true);
    assert.equal(await page.locator("#mq-export-fields [name=arguments]").isChecked(), false);
    assert.equal(await page.locator("#mq-export-fields [name=stdout]").isChecked(), false);
    await screenshot(page, "export-preview.png");
    await page.locator("#mq-export-confirm").check();
    const downloading = page.waitForEvent("download");
    await page.locator("#mq-export-download").click();
    const download = await downloading;
    assert.equal(fs.readFileSync(runCount, "utf8"), "call\n");
    const zip = path.join(temp, "snapshot.zip");
    await download.saveAs(zip);
    const extracted = path.join(temp, "offline");
    execFileSync("python3", ["-m", "zipfile", "-e", zip, extracted]);

    await page.locator("h2[id=write_output] [data-action=call-function]").click();
    await page.locator("[data-action=run-confirm]").click();
    await page.locator("#mq-run-stdout").waitFor();
    assert.equal(await result.getAttribute("data-run-state"), "running");
    assert.equal((await page.locator("#mq-run-stdout").innerText()).trim(), "4");
    await page.locator("[data-action=export-result]").waitFor();
    assert.equal(fs.readFileSync(runCount, "utf8"), "call\ncall\n");
    assert.deepEqual(external, []);
    assert.deepEqual(errors, []);

    const mobile = await context.newPage();
    await mobile.setViewportSize({ width: 390, height: 844 });
    await mobile.goto(url);
    await imagesLoaded(mobile, 1);
    assert.equal(await mobile.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await screenshot(mobile, "mobile-reader.png");
    await context.close();
    server.kill();
    await verifyOffline(browser, pathToFileURL(path.join(extracted, "index.html")).href);
    console.log("Live media, cross-document anchors, confirmed call, pre-terminal stdout, private download and offline ZIP: PASS");
  } finally {
    if (browser) await browser.close();
    server.kill();
    fs.rmSync(temp, { recursive: true, force: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
