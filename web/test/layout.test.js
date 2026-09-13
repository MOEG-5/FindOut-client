import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");

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
