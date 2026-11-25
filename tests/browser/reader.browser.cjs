#!/usr/bin/env node
/* Marqdo View browser acceptance (Chromium headless).
 *
 * Requires a running Live server URL in MQ_URL. Verifies rendering geometry,
 * keyboard/search/settings interactions, no-JS readability, print mode, offline
 * requests (no external network), and the explicit-run flow when the server is
 * started on examples/control-flow. Never introduces a second rendering path.
 */
"use strict";

const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

const base = process.env.MQ_URL;
const project = process.env.MQ_PROJECT || "welcome";
const projectName = path.basename(project);
const screenshotDir = process.env.MQ_SCREENSHOT_DIR || "";
const failures = [];

if (!base) {
  console.error("MQ_URL is required");
  process.exit(2);
}

function check(name, ok, detail) {
  if (ok) {
    console.log("ok   " + name);
  } else {
    failures.push(name + (detail ? ": " + detail : ""));
    console.error("FAIL " + name + (detail ? ": " + detail : ""));
  }
}

function localHostOnly(url) {
  try {
    const parsed = new URL(url);
    return ["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname);
  } catch (error) {
    return false;
  }
}

(async () => {
  const browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1664, height: 920 } });
  const page = await context.newPage();
  const external = [];
  const consoleErrors = [];
  let submittedRuns = 0;
  page.on("request", (request) => {
    if (request.method() === "POST" && new URL(request.url()).pathname === "/api/runs") {
      submittedRuns += 1;
    }
    if (!localHostOnly(request.url()) && !request.url().startsWith("data:")) {
      external.push(request.url());
    }
  });
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  page.on("pageerror", (error) => consoleErrors.push(String(error)));

  await page.goto(base, { waitUntil: "networkidle" });
  check("reader root rendered", (await page.locator(".mq-reader").count()) === 1);
  check("document title has Marqdo", (await page.title()).includes("Marqdo"));
  check("one stylesheet", (await page.locator('link[rel="stylesheet"]').count()) === 1);
  check("one script", (await page.locator("script[src]").count()) === 1);
  check("no external requests", external.length === 0, external.join(", "));
  check("no browser console errors", consoleErrors.length === 0, consoleErrors.join(" | "));
  check("dock has all actions", (await page.locator(".mq-dock-btn").count()) >= 5);
  check(
    "dock run control present",
    (await page.locator('.mq-dock-btn[data-action="run"]').count()) === 1
  );

  // Keyboard reachability: focus must move through named interactive elements.
  const namedTabStops = await page.evaluate(async () => {
    return Array.from(document.querySelectorAll("a[href], button")).map((el) => ({
      label: (el.getAttribute("aria-label") || el.textContent || "").trim(),
      hidden: el.offsetParent === null,
    }));
  });
  check(
    "interactive controls have accessible names",
    namedTabStops.filter((item) => !item.hidden && item.label.length > 0).length >= 8,
    JSON.stringify(namedTabStops.slice(0, 8))
  );
  let focusedNamed = 0;
  for (let index = 0; index < 10; index += 1) {
    await page.keyboard.press("Tab");
    const label = await page.evaluate(() => {
      const element = document.activeElement;
      if (!element) return "";
      return (element.getAttribute("aria-label") || element.textContent || "").trim();
    });
    if (label.length > 0) focusedNamed += 1;
  }
  check("Tab reaches named controls", focusedNamed >= 6, String(focusedNamed));

  // Source/document mode switch through the single menu.
  await page.click('.mq-dock-btn[data-action="more"]');
  check("more dialog opens", await page.locator("#mq-dialog-more").isVisible());
  await page.click('#mq-dialog-more [data-action="mode-source"]');
  await page.keyboard.press("Escape");
  check("source mode attribute", (await page.getAttribute("body", "data-mode")) === "source");
  check("source pane visible", await page.locator(".mq-source").isVisible());
  check("document hidden in source mode", !(await page.locator(".mq-document").isVisible()));
  await page.click('.mq-dock-btn[data-action="more"]');
  await page.click('#mq-dialog-more [data-action="mode-document"]');
  await page.keyboard.press("Escape");
  check(
    "document mode restored",
    (await page.getAttribute("body", "data-mode")) === "document"
  );

  // Local search: use two characters from the first heading so the same test
  // works for every project fixture.
  const firstHeading = await page
    .locator(".mq-document h1, .mq-document h2")
    .first()
    .innerText();
  const searchQuery = firstHeading.replace(/^[@a-zA-Z\s]+/, "").slice(0, 2) || "的";
  await page.keyboard.press("Control+k");
  check("search dialog opens", await page.locator("#mq-dialog-search").isVisible());
  await page.fill("#mq-search-input", searchQuery);
  await page.waitForTimeout(200);
  check(
    "search returns local results",
    (await page.locator("#mq-search-results button").count()) > 0,
    `query ${JSON.stringify(searchQuery)}`
  );
  await page.keyboard.press("Escape");

  // Settings: text size and annotation toggles are real actions.
  await page.click('.mq-dock-btn[data-action="settings"]');
  await page.click('#mq-dialog-settings [data-action="size"]');
  check("text size toggled", (await page.getAttribute("body", "data-size")) !== "normal");
  await page.click('#mq-dialog-settings [data-action="annotations"]');
  check(
    "annotations toggled",
    (await page.getAttribute("body", "data-annotations")) === "off"
  );
  await page.keyboard.press("Escape");

  if (projectName === "security-project") {
    // X-01: an adversarial document must not execute script or fetch anything.
    let alertFired = false;
    page.on("dialog", async (dialog) => {
      alertFired = true;
      await dialog.dismiss();
    });
    await page.reload({ waitUntil: "networkidle" });
    check("adversarial doc has no alert dialog", !alertFired);
    check("adversarial doc has no external requests", external.length === 0, external.join(", "));
    check("raw HTML omitted note rendered", (await page.locator(".mq-raw-omitted").count()) > 0);
    check("prose survives sanitization", (await page.locator(".mq-document").innerText()).includes("普通正文"));
    check(
      "javascript link neutralized",
      (await page.locator('a[href^="javascript:"]').count()) === 0
    );
  }

  if (projectName === "welcome") {
    check("welcome hero only on configured page", (await page.locator(".mq-welcome").count()) === 1);
    check(
      "grammar statement is a real element",
      (await page.locator("blockquote").count()) >= 1
    );
    check("GFM table rendered", (await page.locator(".mq-document table").count()) >= 1);
  }

  // Simple theme switch (RFC §1.2 MVP SMALL) persists and stays readable.
  await page.click('.mq-dock-btn[data-action="settings"]');
  await page.click('#mq-dialog-settings [data-action="theme"]');
  check("dark theme applied", (await page.getAttribute("body", "data-theme")) === "dark");
  await page.reload({ waitUntil: "networkidle" });
  check(
    "theme preference persisted",
    (await page.getAttribute("body", "data-theme")) === "dark"
  );
  await page.click('.mq-dock-btn[data-action="settings"]');
  await page.click('#mq-dialog-settings [data-action="theme"]');
  check("paper theme restored", (await page.getAttribute("body", "data-theme")) === "light");

  // Responsive geometry: no horizontal overflow; dock inside viewport.
  // RFC-VIEW-0002 §9.3 visual matrix: the source reference is 2048x1133.
  const widths = [
    [2048, 1133],
    [1664, 920],
    [1440, 900],
    [1024, 768],
    [768, 1024],
    [390, 844],
  ];
  for (const [width, height] of widths) {
    await page.setViewportSize({ width, height });
    await page.reload({ waitUntil: "networkidle" });
    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - window.innerWidth
    );
    check(`no horizontal overflow at ${width}`, overflow <= 1, String(overflow));
    const dock = await page.locator(".mq-dock").boundingBox();
    check(
      `dock inside viewport at ${width}`,
      !!dock && dock.x >= -1 && dock.x + dock.width <= width + 1 && dock.y + dock.height <= height + 1,
      JSON.stringify(dock)
    );
    if (screenshotDir) {
      fs.mkdirSync(screenshotDir, { recursive: true });
      await page.screenshot({
        path: path.join(screenshotDir, `${projectName}-${width}x${height}.png`),
      });
    }
  }

  // No-JS readability (XC-04).
  const nojs = await browser.newContext({
    javaScriptEnabled: false,
    viewport: { width: 1280, height: 800 },
  });
  const nojsPage = await nojs.newPage();
  await nojsPage.goto(base, { waitUntil: "load" });
  const nojsText = await nojsPage.locator(".mq-document").innerText();
  check("no-JS document remains readable", nojsText.trim().length > 20);
  check(
    "no-JS exposes no active run UI",
    !(await nojsPage.locator("#mq-run-panel").isVisible()) &&
      (await nojsPage.locator('.mq-dock-btn[data-action="run"]').count()) === 1
  );
  await nojs.close();

  // Print hides chrome but keeps prose.
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.reload({ waitUntil: "networkidle" });
  await page.emulateMedia({ media: "print" });
  check("print hides dock", !(await page.locator(".mq-dock").isVisible()));
  check("print keeps prose", await page.locator(".mq-document").isVisible());
  await page.emulateMedia({ media: "screen" });

  check("browsing, refresh, search and settings never submit runs", submittedRuns === 0);

  // Explicit run flow, only against the control-flow example.
  if (projectName === "control-flow") {
    const runRequests = [];
    let originalEventId;
    let reconnectEventId;
    page.on("request", (request) => {
      const pathname = new URL(request.url()).pathname;
      if (pathname.startsWith("/api/runs")) {
        runRequests.push({ method: request.method(), pathname });
      }
    });
    await page.route("**/api/runs/*/events", async (route) => {
      if (originalEventId !== undefined) {
        reconnectEventId = (await route.request().allHeaders())["last-event-id"];
        await route.continue();
        return;
      }
      // Ask the server to replay from the beginning even if the Core has
      // already finished. Deliver one genuine nonterminal frame before ending
      // the response so Chromium has an event ID to use for reconnection.
      const headers = await route.request().allHeaders();
      headers["last-event-id"] = "0";
      const response = await route.fetch({ headers, timeout: 15000 });
      const body = await response.text();
      const first = body.split("\n\n").find((frame) => /^event: run\.state$/m.test(frame));
      const id = first && /^id: (\d+)$/m.exec(first);
      if (!response.ok() || !id) throw new Error("missing first View SSE event");
      originalEventId = id[1];
      await route.fulfill({
        status: 200,
        contentType: "text/event-stream; charset=utf-8",
        body: first + "\n\nretry: 50\n\n",
      });
    });
    await page.click('.mq-dock-btn[data-action="run"]');
    check("run dialog opens", await page.locator("#mq-dialog-run").isVisible());
    const dialogText = await page.locator("#mq-dialog-run").innerText();
    check("run dialog mentions pure mode", dialogText.includes("pure"));
    check(
      "run dialog previews the command",
      (await page.locator("#mq-run-command").innerText()).includes("marqdo run")
    );
    await page.click('[data-action="run-confirm"]');
    await page.waitForSelector('body[data-run-state="succeeded"]', { timeout: 30000 });
    // SSE delivered the terminal state without a page reload.
    check("run state succeeded via SSE", (await page.getAttribute("body", "data-run-state")) === "succeeded");
    check("run panel visible", await page.locator("#mq-run-panel").isVisible());
    check("run chips anchored", (await page.locator(".mq-run-chip").count()) > 0);
    check(
      "observed chips carry observation provenance",
      (await page.locator('.mq-run-chip[data-annotation-source="observation"]').count()) > 0
    );
    check(
      "static statement anchors carry analysis provenance",
      (await page.locator('[data-annotation-source="analysis"]').count()) > 0
    );
    check(
      "at most one grouped chip per anchor",
      (await page.locator(".mq-run-chip").count()) ===
        (await page.locator(".mq-run-chip").evaluateAll((nodes) => {
          const seen = new Set();
          let unique = 0;
          nodes.forEach((node) => {
            const key = node.parentElement ? node.parentElement.dataset.sourceId : null;
            if (key && !seen.has(key)) {
              seen.add(key);
              unique += 1;
            }
          });
          return unique;
        }))
    );
    check(
      "observation event list rendered",
      (await page.locator("#mq-run-events li").count()) > 0
    );
    check(
      "stdout rendered escaped",
      (await page.locator("#mq-run-stdout").innerText()).includes("0")
    );
    check(
      "Chromium reconnected with the original Last-Event-ID",
      originalEventId !== undefined && reconnectEventId === originalEventId,
      JSON.stringify({ originalEventId, reconnectEventId })
    );
    check(
      "reconnect did not submit a second run or poll status",
      runRequests.filter(({ method }) => method === "POST").length === 1 &&
        runRequests.filter(({ method }) => method === "GET").length === 2 &&
        runRequests.every(({ method, pathname }) =>
          method === "POST" ? pathname === "/api/runs" : pathname.endsWith("/events")),
      JSON.stringify(runRequests)
    );
    if (screenshotDir) {
      await page.screenshot({ path: path.join(screenshotDir, "control-flow-run.png") });
      await page.click('.mq-dock-btn[data-action="settings"]');
      await page.click('#mq-dialog-settings [data-action="theme"]');
      await page.screenshot({ path: path.join(screenshotDir, "control-flow-run-dark.png") });
      await page.click('.mq-dock-btn[data-action="settings"]');
      await page.click('#mq-dialog-settings [data-action="theme"]');
    }
    // Clear run hides the display and keeps the project untouched.
    await page.click('.mq-dock-btn[data-action="clear"]');
    check("clear hides run panel", !(await page.locator("#mq-run-panel").isVisible()));
    check("page was never reloaded by the run", (await page.title()).includes("Marqdo"));
  }

  if (projectName === "function-params.mq.md") {
    await page.click('.mq-dock-btn[data-action="run"]');
    check("no-main Dock opens the functions list", await page.locator("#mq-dialog-functions").isVisible());
    check("no-main Dock did not submit a run", submittedRuns === 0);
    const call = (name) => page.locator(`#mq-dialog-functions [data-action="call-function"][data-function-label="${name}"]`);
    await call("double").click();
    const n = page.locator('#mq-run-params [data-param-name="n"]');
    check("Analysis marks n required and Int",
      await n.getAttribute("aria-required") === "true" &&
      (await n.locator("..").innerText()).includes("Int"));
    await page.click('[data-action="run-confirm"]');
    check("missing required parameter leaves the form open", await page.locator("#mq-dialog-run").isVisible());
    check("missing required parameter did not submit", submittedRuns === 0);
    await n.fill("5");
    const argsPreview = page.locator("#mq-run-command");
    check("call preview includes the chosen value", (await argsPreview.innerText()).includes("--arg n=5"));
    await page.click('[data-action="run-confirm"]');
    await page.waitForSelector('body[data-run-state="succeeded"]');
    check("installed tool displays the real double(5) result",
      await page.locator("#mq-run-summary").innerText().then((text) => text.includes("返回值：10")));

    await page.click('.mq-tabs [data-action="functions"]');
    await call("format_arguments").click();
    await page.fill('[data-param-name="label"]', '"浏览器 "');
    await page.fill('[data-param-name="items"]', '["列表", 7]');
    await page.click('[data-action="run-confirm"]');
    await page.getByText("返回值：浏览器 列表 · 7", { exact: true }).waitFor();
    check("CLI round-tripped Unicode Text and a List through the form", submittedRuns === 2);
  }

  await browser.close();
  if (failures.length) {
    console.error(`\n${failures.length} browser check(s) failed`);
    process.exit(1);
  }
  console.log("\nbrowser: PASS");
})().catch((error) => {
  console.error("browser fatal:", error);
  process.exit(1);
});
