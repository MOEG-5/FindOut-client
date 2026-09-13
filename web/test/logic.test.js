import test from "node:test";
import assert from "node:assert/strict";
import { appendHistory, makeQueryPayload, normalizeHistory, previousTurns, randomDeviceId } from "../logic.js";

test("history retains five turns while requests send four", () => {
  let history = [];
  for (let index = 1; index <= 6; index += 1) history = appendHistory(history, { query: `q${index}`, answer: `a${index}`, timestamp: index });
  assert.deepEqual(history.map((turn) => turn.query), ["q2", "q3", "q4", "q5", "q6"]);
  assert.deepEqual(previousTurns(history).map((turn) => turn.query), ["q3", "q4", "q5", "q6"]);
});

test("history rejects malformed entries and clips unicode safely", () => {
  const history = normalizeHistory([{ query: "🦊".repeat(2100), answer: "yes" }, null, { query: 4, answer: "no" }]);
  assert.equal(Array.from(history[0].query).length, 2000);
  assert.equal(history.length, 1);
});

test("query payload matches the desktop protocol", () => {
  const payload = makeQueryPayload({ query: "  Why?  ", history: [{ query: "before", answer: "because" }], forceSearch: true, systemContext: "Android | mobile web app" });
  assert.deepEqual(payload, {
    query: "Why?",
    previous_turns: [{ query: "before", answer: "because" }],
    force_search: true,
    system_context: "Android | mobile web app",
  });
});

test("trial device identifier is 64 lowercase hex characters", () => {
  const cryptoObject = { getRandomValues(bytes) { bytes.fill(171); return bytes; } };
  assert.match(randomDeviceId(cryptoObject), /^[a-f0-9]{64}$/);
});
