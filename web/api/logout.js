import { allowPostOnly, clearSessionCookie, sendUpstream } from "./_shared.js";

export default function handler(request, response) {
  if (!allowPostOnly(request, response)) return;
  clearSessionCookie(response);
  return sendUpstream(response, 200, { activated: false });
}
