import { allowPostOnly, safeBody, sendUpstream, setSessionCookie, upstream } from "./_shared.js";

export default async function handler(request, response) {
  if (!allowPostOnly(request, response)) return;
  try {
    const body = safeBody(request.body);
    const result = await upstream("/v1/activate", { body });
    const token = result.data?.device_token;
    if (result.response.ok && (typeof token !== "string" || token.length < 32 || token.length > 4096)) {
      return sendUpstream(response, 502, { message: "FindOut returned an invalid activation token" });
    }
    if (result.response.ok) {
      setSessionCookie(response, token);
      return sendUpstream(response, 200, { activated: true });
    }
    return sendUpstream(response, result.response.status, result.data);
  } catch (error) {
    return sendUpstream(response, error.message === "Invalid request body" ? 400 : 502, { message: error.message });
  }
}
