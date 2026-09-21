function nonNegativeInteger(value) {
  if (typeof value !== "string" || !/^\d+$/.test(value)) return null;
  const number = Number(value);
  return Number.isSafeInteger(number) ? number : null;
}

/**
 * Quota metadata is optional on the wire. Accept it only as a complete,
 * internally consistent server response so missing or malformed headers never
 * turn into a misleading local allowance.
 */
export function quotaMetadata(headers) {
  const limit = nonNegativeInteger(headers?.get("x-findout-daily-limit"));
  const remaining = nonNegativeInteger(headers?.get("x-findout-daily-remaining"));
  const reset = headers?.get("x-findout-daily-reset");
  const resetAt = typeof reset === "string" ? new Date(reset) : null;
  if (limit === null || remaining === null || remaining > limit || !resetAt || Number.isNaN(resetAt.getTime())) return null;
  return { limit, remaining, resetAt };
}

function resetLabel(resetAt) {
  return `${resetAt.toISOString().slice(0, 16).replace("T", " ")} UTC`;
}

export function quotaStatus(headers) {
  const quota = quotaMetadata(headers);
  return quota ? `${quota.remaining} OF ${quota.limit} LEFT TODAY · RESETS ${resetLabel(quota.resetAt)}` : "";
}

/** A quota 429 is distinguished by the authoritative zero-remaining headers. */
export function quotaExhaustionStatus(headers) {
  const quota = quotaMetadata(headers);
  return quota?.remaining === 0 ? `DAILY LIMIT REACHED · ${quotaStatus(headers)}` : "";
}
