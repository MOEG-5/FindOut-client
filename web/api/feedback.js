import { allowPostOnly, safeBody, sendUpstream, sessionToken, upstream } from "./_shared.js";
export default async function handler(request, response) {
  if (!allowPostOnly(request, response)) return;
  if (!String(request.headers?.["content-type"] || "").toLowerCase().startsWith("application/json")) return sendUpstream(response, 415, { message: "JSON required" });
  try {
    const body = safeBody(request.body);
    if (JSON.stringify(body).length > 24000) return sendUpstream(response, 413, { message: "Feedback is too large" });
    const result = await upstream("/v1/feedback", { body, token: body.include_license === true ? sessionToken(request) : null });
    const retry = result.response.headers.get("retry-after");
    if (retry) response.setHeader("Retry-After", retry);
    return sendUpstream(response, result.response.status, result.data);
  } catch { return sendUpstream(response, 502, { message: "Feedback unavailable. Keep or save your message and try again later" }); }
}
