import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import activate from "./api/activate.js";
import query from "./api/query.js";
import logout from "./api/logout.js";

const root = new URL(".", import.meta.url).pathname;
process.env.NODE_ENV ||= "development";
const handlers = { "/api/activate": activate, "/api/query": query, "/api/logout": logout };
const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml", ".webmanifest": "application/manifest+json" };

createServer(async (request, response) => {
  const url = new URL(request.url, "http://localhost");
  if (handlers[url.pathname]) {
    let raw = "";
    for await (const chunk of request) raw += chunk;
    request.body = raw ? JSON.parse(raw) : {};
    request.cookies = Object.fromEntries((request.headers.cookie || "").split(";").filter(Boolean).map((part) => part.trim().split(/=(.*)/s).slice(0, 2)));
    response.status = (code) => { response.statusCode = code; return response; };
    response.json = (body) => { response.setHeader("Content-Type", "application/json"); response.end(JSON.stringify(body)); };
    return handlers[url.pathname](request, response);
  }
  const requested = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
  const safePath = normalize(requested).replace(/^(\.\.(\/|\\|$))+/, "");
  try {
    const file = await readFile(join(root, safePath));
    response.setHeader("Content-Type", types[extname(safePath)] || "application/octet-stream");
    response.end(file);
  } catch { response.statusCode = 404; response.end("Not found"); }
}).listen(Number(process.env.PORT || 4173), "127.0.0.1", () => console.log(`FindOut PWA: http://127.0.0.1:${process.env.PORT || 4173}`));
