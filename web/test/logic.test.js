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

test("recent threads retain full answers and per-answer sources while request context stays bounded", async () => {
  const { updateThread, normalizeThreads, threadText } = await import("../logic.js");
  const turns = Array.from({ length: 8 }, (_, i) => ({ query: `q${i}`, answer: "ä".repeat(3000), searched: i === 1, hadImage: i === 2 }));
  let threads = updateThread([], "one", turns);
  assert.equal(threads[0].turns.length, 8);
  assert.equal(threads[0].turns[0].answer.length, 3000);
  assert.match(threadText(threads[0].turns), /🌐/); assert.match(threadText(threads[0].turns), /📷/);
  assert.equal(previousTurns(threads[0].turns).length, 4);
  assert.equal(previousTurns(threads[0].turns)[0].answer.length, 2000);
  for (let i = 2; i <= 6; i++) threads = updateThread(threads, String(i), [{ query: `q${i}`, answer: "a" }]);
  assert.equal(threads.length, 5); assert.equal(threads[0].id, "2");
  assert.deepEqual(normalizeThreads([null, {}, { id: "bad", turns: [null] }]), []);
});
