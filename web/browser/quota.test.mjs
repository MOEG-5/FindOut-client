import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { chromium } from "playwright-core";

test("PWA activation, quota, Monolith themes and focus recovery", { timeout: 30000 }, async (t) => {
  let quota = { limit: 37, remaining: 12, reset: "2026-09-22T00:00:00.000Z" };
  let status = 200;
  let queryCount = 0;
  const queries = [];
  const upstream = createServer(async (req, res) => {
    let raw = "";
    for await (const chunk of req) raw += chunk;
    res.setHeader("Content-Type", "application/json");
    if (req.url === "/v1/activate") {
      res.end(JSON.stringify({ device_token: "t".repeat(64) }));
      return;
    }
    queryCount++;
    queries.push(JSON.parse(raw));
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
    page.setDefaultTimeout(5000);
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

    await t.test("Monolith themes persist across reloads and the privacy page", async () => {
      assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), "light");
      await page.locator("#settingsButton").click();
      await page.locator("#themeButton").click();
      await page.reload();
      assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), "dark");
      assert.equal(await page.locator('meta[name="theme-color"]').getAttribute("content"), "#1a1a1a");
      await page.goto(`http://127.0.0.1:${port}/privacy.html`);
      assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), "dark");
      await page.locator(".back-link").click();
      for (const theme of ["dark", "light"]) {
        if (theme === "light") {
          await page.locator("#settingsButton").click();
          await page.locator("#themeButton").click();
          await page.locator("#settingsButton").click();
        }
        for (const width of [320, 390, 768, 1280]) {
          await page.setViewportSize({ width, height: 844 });
          const layout = await page.evaluate(() => ({
            width: innerWidth,
            scrollWidth: document.documentElement.scrollWidth,
            surface: getComputedStyle(document.body).backgroundColor,
            strip: getComputedStyle(document.querySelector(".composer")).backgroundColor,
          }));
          assert.ok(layout.scrollWidth <= layout.width, `${theme} layout fits ${width}px`);
          assert.equal(layout.surface, theme === "light" ? "rgb(255, 255, 255)" : "rgb(26, 26, 26)");
          assert.equal(layout.strip, theme === "light" ? "rgb(26, 26, 26)" : "rgb(255, 255, 255)");
        }
      }
    });

    await t.test("focus grace keeps quick returns and starts a fresh context after ten seconds", async () => {
      await page.locator("#queryInput").fill("Keep this draft");
      await page.locator(".search-toggle").click();
      await page.locator("#imageInput").setInputFiles(new URL("../icons/findout-192.png", import.meta.url).pathname);
      await page.locator("#imagePreview").waitFor({ state: "visible" });
      const saved = await page.evaluate(() => localStorage.getItem("findout.threads.v2"));
      const result = await page.evaluate(() => {
        const originalNow = Date.now;
        let now = originalNow();
        Date.now = () => now;
        try {
          window.dispatchEvent(new Event("blur"));
          now += 9999;
          window.dispatchEvent(new Event("focus"));
          const quick = {
            draft: document.querySelector("#queryInput").value,
            image: !document.querySelector("#imagePreview").hidden,
            answer: document.querySelector("#answerText").textContent,
          };
          window.dispatchEvent(new Event("blur"));
          now += 10000;
          window.dispatchEvent(new Event("focus"));
          const expired = {
            draft: document.querySelector("#queryInput").value,
            image: !document.querySelector("#imagePreview").hidden,
            answer: document.querySelector("#answerText").textContent,
            status: document.querySelector("#queryStatus").textContent,
            search: document.querySelector("#forceSearchInput").checked,
          };
          return { quick, expired };
        } finally { Date.now = originalNow; }
      });
      assert.deepEqual(result.quick, { draft: "Keep this draft", image: true, answer: "Test answer" });
      assert.deepEqual(result.expired, { draft: "", image: false, answer: "Your answer will appear here.", status: "", search: false });
      assert.equal(await page.evaluate(() => localStorage.getItem("findout.threads.v2")), saved);
      status = 200;
      await ask();
      assert.deepEqual(queries.at(-1).previous_turns, [], "the next request must start a new thread");
      const olderThread = page.locator(".history-item").nth(1);
      await olderThread.locator(".history-question").click();
      await olderThread.getByRole("button", { name: "CONTINUE THREAD" }).click();
      await ask();
      assert.equal(queries.at(-1).previous_turns.length, 2, "Recent can explicitly resume the saved context");
    });

    await t.test("late answers cannot overwrite or unlock a newer request after focus reset", async () => {
      await page.evaluate(() => {
        window.pendingAnswers = [];
        window.requestAborted = false;
        const originalFetch = window.fetch;
        window.fetch = (url, options) => url === "/api/query"
          ? new Promise(resolve => {
            window.pendingAnswers.push(resolve);
            options.signal.addEventListener("abort", () => { window.requestAborted = true; });
          })
          : originalFetch(url, options);
      });
      await page.locator("#queryInput").fill("Slow question");
      await page.locator("#sendButton").click();
      await page.evaluate(() => {
        const originalNow = Date.now;
        const now = originalNow();
        window.dispatchEvent(new Event("blur"));
        Date.now = () => now + 10000;
        window.dispatchEvent(new Event("focus"));
        Date.now = originalNow;
      });
      assert.equal(await page.evaluate(() => window.requestAborted), true);
      await page.locator("#queryInput").fill("Fresh question");
      await page.locator("#sendButton").click();
      await page.evaluate(async () => {
        window.pendingAnswers[0](new Response(JSON.stringify({ answer: "Old answer", searched: false })));
        await new Promise(resolve => setTimeout(resolve, 20));
      });
      assert.equal(await page.locator("#answerText").innerText(), "Your answer will appear here.");
      assert.equal(await page.locator("#sendButton").isDisabled(), true);
      await page.evaluate(() => window.pendingAnswers[1](new Response(JSON.stringify({ answer: "Fresh answer", searched: false }))));
      await page.waitForFunction(() => !document.querySelector("#sendButton").disabled);
      assert.equal(await page.locator("#answerText").innerText(), "Fresh answer");
    });
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
