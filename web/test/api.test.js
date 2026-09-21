import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import activate from "../api/activate.js";
import query from "../api/query.js";

function mockResponse() {
  return {
    headers: {}, statusCode: 200, body: null,
    setHeader(name, value) { this.headers[name.toLowerCase()] = value; },
    status(code) { this.statusCode = code; return this; },
    json(body) { this.body = body; return this; },
  };
}

async function withUpstream(callback, queryResponse = {}) {
  const seen = [];
  const server = createServer(async (request, response) => {
    let raw = "";
    for await (const chunk of request) raw += chunk;
    seen.push({ url: request.url, headers: request.headers, body: JSON.parse(raw) });
    response.setHeader("Content-Type", "application/json");
    if (request.url === "/v1/activate") response.end(JSON.stringify({ device_token: "t".repeat(64) }));
    else {
      const {
        status = 200,
        headers = {
          "X-FindOut-Daily-Limit": "37",
          "X-FindOut-Daily-Remaining": "12",
          "X-FindOut-Daily-Reset": "2026-09-08T00:00:00.000Z",
        },
        body = { answer: "Test answer", searched: false },
      } = queryResponse;
      for (const [name, value] of Object.entries(headers)) response.setHeader(name, value);
      response.statusCode = status;
      response.end(JSON.stringify(body));
    }
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const oldOrigin = process.env.FINDOUT_API_ORIGIN;
  const oldNodeEnv = process.env.NODE_ENV;
  process.env.FINDOUT_API_ORIGIN = `http://127.0.0.1:${server.address().port}`;
  process.env.NODE_ENV = "test";
  try { await callback(seen); }
  finally {
    process.env.FINDOUT_API_ORIGIN = oldOrigin;
    process.env.NODE_ENV = oldNodeEnv;
    await new Promise((resolve) => server.close(resolve));
  }
}

test("activation hides the upstream bearer token in an HttpOnly cookie", async () => withUpstream(async (seen) => {
  const response = mockResponse();
  await activate({ method: "POST", body: { activation_key: "trial", device_id: "a".repeat(64) } }, response);
  assert.equal(response.statusCode, 200);
  assert.deepEqual(response.body, { activated: true });
  assert.match(response.headers["set-cookie"], /HttpOnly; Secure; SameSite=Strict/);
  assert.equal(seen[0].headers["x-findout-protocol"], "1");
}));

test("query restores authorization and forwards server-provided usage headers", async () => withUpstream(async (seen) => {
  const response = mockResponse();
  await query({ method: "POST", cookies: { findout_session: "secret-token" }, body: { query: "Why?", previous_turns: [], force_search: false } }, response);
  assert.equal(response.statusCode, 200);
  assert.equal(response.body.answer, "Test answer");
  assert.equal(response.headers["x-findout-daily-limit"], "37");
  assert.equal(response.headers["x-findout-daily-remaining"], "12");
  assert.equal(response.headers["x-findout-daily-reset"], "2026-09-08T00:00:00.000Z");
  assert.equal(seen[0].headers.authorization, "Bearer secret-token");
}));

test("quota-exhaustion responses preserve authoritative headers without clearing the session", async () => withUpstream(async () => {
  const response = mockResponse();
  await query({ method: "POST", cookies: { findout_session: "secret-token" }, body: { query: "Why?" } }, response);
  assert.equal(response.statusCode, 429);
  assert.deepEqual({
    limit: response.headers["x-findout-daily-limit"],
    remaining: response.headers["x-findout-daily-remaining"],
    reset: response.headers["x-findout-daily-reset"],
  }, {
    limit: "37",
    remaining: "0",
    reset: "2026-09-08T00:00:00.000Z",
  });
  assert.equal(response.headers["set-cookie"], undefined);
}, {
  status: 429,
  headers: {
    "X-FindOut-Daily-Limit": "37",
    "X-FindOut-Daily-Remaining": "0",
    "X-FindOut-Daily-Reset": "2026-09-08T00:00:00.000Z",
  },
  body: { message: "Daily free limit reached; try again after midnight UTC" },
}));

test("query requires an activation cookie", async () => {
  const response = mockResponse();
  await query({ method: "POST", cookies: {}, body: {} }, response);
  assert.equal(response.statusCode, 401);
});

test("feedback works without activation and forwards cookie only with consent", async () => withUpstream(async (seen) => {
  const { default: feedback } = await import("../api/feedback.js");
  for (const consent of [false, true]) {
    const response = mockResponse();
    await feedback({ method: "POST", headers: { "content-type": "application/json" }, cookies: { findout_session: "secret-token" }, body: { message: "Help", include_license: consent } }, response);
    assert.equal(response.statusCode, 200);
    assert.equal(seen.at(-1).url, "/v1/feedback");
    assert.equal(seen.at(-1).headers.authorization, consent ? "Bearer secret-token" : undefined);
    assert.equal(response.headers["set-cookie"], undefined);
  }
  const response = mockResponse();
  await feedback({ method: "POST", headers: { "content-type": "application/json" }, cookies: {}, body: { message: "Cannot activate", include_license: true } }, response);
  assert.equal(response.statusCode, 200);
  assert.equal(seen.at(-1).headers.authorization, undefined);
}));

test("feedback rejects cross-site simple form submissions", async () => {
  const { default: feedback } = await import("../api/feedback.js");
  const response = mockResponse();
  await feedback({ method: "POST", headers: { "content-type": "text/plain" }, body: { message: "Help" } }, response);
  assert.equal(response.statusCode, 415);
});
