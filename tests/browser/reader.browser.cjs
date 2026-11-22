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
  page.on("request", (request) => {
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

  if (projectName === "welcome") {
    check("welcome hero only on configured page", (await page.locator(".mq-welcome").count()) === 1);
    check(
      "grammar statement is a real element",
      (await page.locator("blockquote").count()) >= 1
    );
    check("GFM table rendered", (await page.locator(".mq-document table").count()) >= 1);
  }

  // Responsive geometry: no horizontal overflow; dock inside viewport.
  const widths = [
    [390, 844],
    [600, 800],
    [900, 768],
    [1280, 800],
    [1664, 920],
    [1920, 1080],
    [2560, 1440],
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
  check("no-JS has no fake run panel", (await nojsPage.locator("#mq-run-panel").count()) === 0);
  await nojs.close();

  // Print hides chrome but keeps prose.
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.reload({ waitUntil: "networkidle" });
  await page.emulateMedia({ media: "print" });
  check("print hides dock", !(await page.locator(".mq-dock").isVisible()));
  check("print keeps prose", await page.locator(".mq-document").isVisible());
  await page.emulateMedia({ media: "screen" });

  // Explicit run flow, only against the control-flow example.
  if (projectName === "control-flow") {
    await page.click('.mq-dock-btn[data-action="run"]');
    check("run confirm dialog opens", await page.locator("#mq-dialog-run-confirm").isVisible());
    const dialogText = await page.locator("#mq-dialog-run-confirm").innerText();
    check("confirm mentions pure mode", dialogText.includes("pure"));
    await page.click('[data-action="run-confirm"]');
    await page.waitForURL(/run=/, { timeout: 30000 });
    await page.waitForLoadState("networkidle");
    check("run state succeeded", (await page.getAttribute("body", "data-run-state")) === "succeeded");
    check("run panel rendered", await page.locator("#mq-run-panel").isVisible());
    check("run chips anchored", (await page.locator(".mq-run-chip").count()) > 0);
    check(
      "trace event list rendered",
      (await page.locator(".mq-run-events li").count()) > 0
    );
    check(
      "stdout rendered escaped",
      (await page.locator(".mq-stdout").first().innerText()).includes("0")
    );
    if (screenshotDir) {
      await page.screenshot({ path: path.join(screenshotDir, "control-flow-run.png") });
    }
    // Clear run removes the display and the URL marker.
    await page.click('[data-action="clear-run"]');
    await page.waitForURL((url) => !url.searchParams.has("run"), { timeout: 15000 });
    check("clear run removes panel", (await page.locator("#mq-run-panel").count()) === 0);
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
