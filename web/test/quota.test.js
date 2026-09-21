import test from "node:test";
import assert from "node:assert/strict";
import { quotaExhaustionStatus, quotaMetadata, quotaStatus } from "../quota.js";

function headers(values) {
  return new Headers(values);
}

test("quota status is always derived from the complete current server headers", () => {
  const first = headers({
    "x-findout-daily-limit": "37",
    "x-findout-daily-remaining": "12",
    "x-findout-daily-reset": "2026-09-08T00:00:00.000Z",
  });
  const restoredSession = headers({
    "x-findout-daily-limit": "9",
    "x-findout-daily-remaining": "4",
    "x-findout-daily-reset": "2026-09-09T00:00:00.000Z",
  });
  assert.equal(quotaStatus(first), "12 OF 37 LEFT TODAY · RESETS 2026-09-08 00:00 UTC");
  assert.equal(quotaStatus(restoredSession), "4 OF 9 LEFT TODAY · RESETS 2026-09-09 00:00 UTC");
});

test("only a complete zero-remaining quota response becomes an exhaustion status", () => {
  const exhausted = headers({
    "x-findout-daily-limit": "37",
    "x-findout-daily-remaining": "0",
    "x-findout-daily-reset": "2026-09-08T00:00:00.000Z",
  });
  assert.equal(quotaExhaustionStatus(exhausted), "DAILY LIMIT REACHED · 0 OF 37 LEFT TODAY · RESETS 2026-09-08 00:00 UTC");
  assert.equal(quotaExhaustionStatus(headers({
    "x-findout-daily-limit": "37",
    "x-findout-daily-remaining": "1",
    "x-findout-daily-reset": "2026-09-08T00:00:00.000Z",
  })), "");
  assert.equal(quotaMetadata(headers({
    "x-findout-daily-limit": "37",
    "x-findout-daily-remaining": "0",
  })), null);
});
