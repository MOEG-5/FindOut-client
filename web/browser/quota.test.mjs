import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chromium } from "playwright-core";

test("activation, exhaustion and restored sessions use current backend quota", { timeout: 30000 }, async () => {
  let quota = { limit: 37, remaining: 12, reset: "2026-09-22T00:00:00.000Z" };
  let status = 200;
  let queryCount = 0;
  const upstream = createServer(async (req, res) => {
    for await (const chunk of req) { /* drain the request */ }
    res.setHeader("Content-Type", "application/json");
    if (req.url === "/v1/activate") {
      res.end(JSON.stringify({ device_token: "t".repeat(64) }));
      return;
    }
    queryCount++;
    if (req.headers.authorization !== `Bearer ${"t".repeat(64)}`) {
      res.writeHead(401).end(JSON.stringify({ message: "Missing session" }));
      return;
    }
    if (quota) {
      res.setHeader("X-FindOut-Daily-Limit", String(quota.limit));
      res.setHeader("X-FindOut-Daily-Remaining", String(quota.remaining));
      res.setHeader("X-FindOut-Daily-Reset", quota.reset);
    }
    res.statusCode = status;
    res.end(JSON.stringify(status === 200
      ? { answer: "Test answer", searched: false }
      : { message: "Please try again later" }));
  });
  await new Promise(resolve => upstream.listen(0, "127.0.0.1", resolve));
  let app;
  let browser;
  try {
    const probe = createServer();
    await new Promise(resolve => probe.listen(0, "127.0.0.1", resolve));
    const port = probe.address().port;
    await new Promise(resolve => probe.close(resolve));
    app = spawn(process.execPath, ["dev-server.js"], {
      cwd: new URL("..", import.meta.url),
      env: { ...process.env, NODE_ENV: "development", PORT: String(port),
        FINDOUT_API_ORIGIN: `http://127.0.0.1:${upstream.address().port}` },
      stdio: ["ignore", "pipe", "pipe"],
    });
    await Promise.race([
      once(app.stdout, "data"),
      once(app, "exit").then(([code]) => { throw new Error(`Dev server exited: ${code}`); }),
    ]);
    browser = await chromium.launch({ headless: true,
      ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}) });
    const context = await browser.newContext({ serviceWorkers: "block" });
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${port}`);
    assert.match(await page.locator(".lede").innerText(), /free daily usage/);
    await page.locator("#activationInput").fill("trial");
    await page.locator("#activationButton").click();
    await page.locator("#mainView").waitFor({ state: "visible" });
    assert.equal(await page.locator("#allowance").innerText(), "");
    assert.equal(queryCount, 0, "activation must not consume a query to discover quota");

    async function ask() {
      await page.locator("#queryInput").fill("Test question");
      await page.locator("#sendButton").click();
      await page.waitForFunction(() => !document.querySelector("#sendButton").disabled);
    }
    await ask();
    assert.equal(await page.locator("#allowance").innerText(), "12 OF 37 LEFT TODAY · RESETS 2026-09-22 00:00 UTC");
    status = 429;
    quota.remaining = 0;
    await ask();
    assert.equal(await page.locator("#allowance").innerText(), "0 OF 37 LEFT TODAY · RESETS 2026-09-22 00:00 UTC");
    assert.equal(await page.locator("#queryStatus").innerText(), "DAILY LIMIT REACHED · 0 OF 37 LEFT TODAY · RESETS 2026-09-22 00:00 UTC");

    await page.reload();
    await page.locator("#mainView").waitFor({ state: "visible" });
    assert.equal(await page.locator("#allowance").innerText(), "", "restored history must not invent or replay quota");
    assert.equal(queryCount, 2, "restoration must not spend quota");
    status = 200;
    quota = { limit: 9, remaining: 4, reset: "2026-09-23T00:00:00.000Z" };
    await ask();
    assert.equal(await page.locator("#allowance").innerText(), "4 OF 9 LEFT TODAY · RESETS 2026-09-23 00:00 UTC");
    status = 429;
    quota = null;
    await ask();
    assert.equal(await page.locator("#allowance").innerText(), "");
    assert.equal(await page.locator("#queryStatus").innerText(), "Please try again later");
  } finally {
    await browser?.close();
    if (app && app.exitCode === null) {
      const exited = once(app, "exit");
      app.kill();
      await exited;
    }
    upstream.closeAllConnections();
    await new Promise(resolve => upstream.close(resolve));
  }
});
