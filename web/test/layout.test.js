import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const privacyHtml = readFileSync(new URL("../privacy.html", import.meta.url), "utf8");
const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
const serviceWorker = readFileSync(new URL("../sw.js", import.meta.url), "utf8");

test("hidden UI cannot be resurrected by component display rules", () => {
  assert.match(css, /\[hidden\]\s*\{\s*display:\s*none\s*!important;/);
});

test("activated layout places answer then composer then recent history", () => {
  const answer = html.indexOf('id="answerView"');
  const composer = html.indexOf('id="queryForm"');
  const recent = html.indexOf('id="historySection"');
  assert.ok(answer > -1 && answer < composer && composer < recent);
  assert.doesNotMatch(html, /id="emptyState"/);
});

test("activation button is the final visible element in its view", () => {
  const view = html.match(/<section id="activationView"[\s\S]*?<\/section>/)?.[0] || "";
  assert.match(view, /<button id="activationButton"[\s\S]*?<\/button>\s*<\/form>\s*<\/section>$/);
});

test("offline shell includes quota and privacy resources", () => {
  assert.match(serviceWorker, /findout-shell-v0\.1\.5-5/);
  assert.match(serviceWorker, /"\/quota\.js"/);
  assert.match(serviceWorker, /"\/privacy\.html"/);
  assert.match(html, /id="feedbackConsent"[^>]*required/);
  assert.match(html, /Exactly what this submission sends/);
  assert.match(html, /href="\/privacy\.html"/);
});

test("published privacy copy matches quota retention and identifier-free logging", () => {
  assert.match(privacyHtml, /normally removed within 49 hours/);
  assert.match(privacyHtml, /aggregate daily usage is kept for 35 days/);
  assert.match(privacyHtml, /They omit license and reservation IDs/);
  assert.doesNotMatch(privacyHtml, /pseudonymous license ID, quota count/);
});
