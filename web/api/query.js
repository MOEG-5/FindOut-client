import { allowPostOnly, clearSessionCookie, passUsageHeaders, safeBody, sendUpstream, sessionToken, upstream } from "./_shared.js";

export default async function handler(request, response) {
  if (!allowPostOnly(request, response)) return;
  const token = sessionToken(request);
  if (!token) return sendUpstream(response, 401, { message: "Activation required" });
  try {
    const body = safeBody(request.body);
    const result = await upstream("/v1/query", { body, token });
    passUsageHeaders(result.response, response);
    if (result.response.status === 401) clearSessionCookie(response);
    return sendUpstream(response, result.response.status, result.data);
  } catch (error) {
    return sendUpstream(response, error.message === "Invalid request body" ? 400 : 502, { message: error.message });
  }
}
