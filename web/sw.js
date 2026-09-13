const CACHE = "findout-shell-v1";
const SHELL = ["/", "/index.html", "/styles.css", "/app.js", "/logic.js", "/manifest.webmanifest", "/icons/findout.svg", "/icons/findout-192.png", "/icons/findout-512.png", "/icons/findout-maskable-512.png", "/icons/apple-touch-icon.png"];

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(SHELL)).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (event) => {
  event.waitUntil(caches.keys().then((keys) => Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key)))).then(() => self.clients.claim()));
});

self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET" || new URL(event.request.url).pathname.startsWith("/api/")) return;
  event.respondWith(fetch(event.request)
    .then((response) => {
      if (response.ok) caches.open(CACHE).then((cache) => cache.put(event.request, response.clone()));
      return response;
    })
    .catch(() => caches.match(event.request).then((cached) => cached || caches.match("/index.html"))));
});
