const PROTOCOL_VERSION = "1";
const COOKIE_NAME = "findout_session";
const MAX_BODY_CHARS = 4_400_000;
const DEFAULT_BACKEND_ORIGIN = "https://findout-backend.vercel.app";

export function backendOrigin() {
  const value = String(process.env.FINDOUT_API_ORIGIN || DEFAULT_BACKEND_ORIGIN).replace(/\/$/, "");
  const parsed = new URL(value);
  if (process.env.NODE_ENV === "production" && parsed.protocol !== "https:") throw new Error("FINDOUT_API_ORIGIN must use HTTPS");
  return value;
}

export function allowPostOnly(request, response) {
  if (request.method === "POST") return true;
  response.setHeader("Allow", "POST");
  response.status(405).json({ message: "Method not allowed" });
  return false;
}

export function safeBody(body) {
  if (!body || typeof body !== "object" || Array.isArray(body)) throw new Error("Invalid request body");
  if (JSON.stringify(body).length > MAX_BODY_CHARS) throw new Error("Request body is too large");
  return body;
}

export async function upstream(path, { body, token } = {}) {
  const response = await fetch(`${backendOrigin()}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-FindOut-Protocol": PROTOCOL_VERSION,
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify(body || {}),
  });
  const text = await response.text();
  let data = {};
  try { data = text ? JSON.parse(text) : {}; } catch { data = {}; }
  return { response, data };
}

export function setSessionCookie(response, token) {
  const secure = process.env.NODE_ENV === "development" ? "" : "; Secure";
  response.setHeader("Set-Cookie", `${COOKIE_NAME}=${encodeURIComponent(token)}; Path=/; Max-Age=31536000; HttpOnly${secure}; SameSite=Strict`);
}

export function clearSessionCookie(response) {
  const secure = process.env.NODE_ENV === "development" ? "" : "; Secure";
  response.setHeader("Set-Cookie", `${COOKIE_NAME}=; Path=/; Max-Age=0; HttpOnly${secure}; SameSite=Strict`);
}

export function sessionToken(request) {
  const raw = request.cookies?.[COOKIE_NAME];
  if (!raw) return null;
  try { return decodeURIComponent(raw); } catch { return null; }
}

export function passUsageHeaders(upstreamResponse, response) {
  for (const name of ["x-findout-daily-limit", "x-findout-daily-remaining", "x-findout-daily-reset"]) {
    const value = upstreamResponse.headers.get(name);
    if (value) response.setHeader(name, value);
  }
}

export function sendUpstream(response, status, data) {
  response.setHeader("Cache-Control", "no-store");
  response.status(status).json(data && typeof data === "object" ? data : { message: "FindOut returned an invalid response" });
}
