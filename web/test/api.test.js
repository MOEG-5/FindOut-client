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

async function withUpstream(callback) {
  const seen = [];
  const server = createServer(async (request, response) => {
    let raw = "";
    for await (const chunk of request) raw += chunk;
    seen.push({ url: request.url, headers: request.headers, body: JSON.parse(raw) });
    response.setHeader("Content-Type", "application/json");
    if (request.url === "/v1/activate") response.end(JSON.stringify({ device_token: "t".repeat(64) }));
    else {
      response.setHeader("X-FindOut-Daily-Limit", "20");
      response.setHeader("X-FindOut-Daily-Remaining", "19");
      response.end(JSON.stringify({ answer: "Test answer", searched: false }));
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

test("query restores authorization and passes usage headers", async () => withUpstream(async (seen) => {
  const response = mockResponse();
  await query({ method: "POST", cookies: { findout_session: "secret-token" }, body: { query: "Why?", previous_turns: [], force_search: false } }, response);
  assert.equal(response.statusCode, 200);
  assert.equal(response.body.answer, "Test answer");
  assert.equal(response.headers["x-findout-daily-remaining"], "19");
  assert.equal(seen[0].headers.authorization, "Bearer secret-token");
}));

test("query requires an activation cookie", async () => {
  const response = mockResponse();
  await query({ method: "POST", cookies: {}, body: {} }, response);
  assert.equal(response.statusCode, 401);
});
