// ChefDAV app-shell service worker.
//
// This caches the *frontend* (index.html, JS, WASM, CSS, fonts and the runtime
// config.json), so a reload still boots when the web server that serves the
// frontend is down. Calendar data is not handled here: it already lives in
// IndexedDB (src/store.rs).
//
// Registered from index.html with the mount path as its scope. At "/ui/" the
// release bundle uses relative asset URLs plus a runtime <base href>, so the
// install step resolves every asset against the base found in index.html.

const CACHE = "chefdav-shell-v1";
const SCOPE = self.registration.scope;
const SCOPE_PATH = new URL(SCOPE).pathname;
const CONFIG_PATH = new URL("config.json", SCOPE).pathname;
// Trunk's release filenames (chef-dav-<hash>.js, main-<hash>.css,
// chef-dav-<hash>_bg.wasm) are immutable. Development builds are unhashed and
// take the network-first path instead, so `trunk serve` never gets stuck on an
// old bundle.
const HASHED_ASSET = /-[0-9a-f]{8,}[._-]/i;
const STATIC_ASSET = /\.(?:woff2?|ttf|otf|svg|png|ico)$/i;

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      await cacheShell();
      // A new worker should take over immediately; the cached shell is already
      // consistent because every asset is either hashed or revalidated.
      await self.skipWaiting();
    })()
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter((key) => key.startsWith("chefdav-shell-") && key !== CACHE)
          .map((key) => caches.delete(key))
      );
      await self.clients.claim();
    })()
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  if (!url.pathname.startsWith(SCOPE_PATH)) return;

  if (request.mode === "navigate") {
    event.respondWith(navigation(request));
    return;
  }

  if (url.pathname === CONFIG_PATH) {
    event.respondWith(networkFirst(request));
    return;
  }

  if (HASHED_ASSET.test(url.pathname)) {
    event.respondWith(cacheFirst(request));
    return;
  }

  if (STATIC_ASSET.test(url.pathname) || /\/(?:tokens|fonts)\.css$/i.test(url.pathname)) {
    event.respondWith(staleWhileRevalidate(request));
    return;
  }

  event.respondWith(networkFirst(request));
});

// Precache the entry document and everything it references (JS, WASM, CSS,
// icons, modulepreload/preload hints) plus the runtime config. This runs on
// the first visit, before the app itself is controlled, so a later offline
// reload has a complete shell even if the browser's HTTP cache was evicted.
async function cacheShell() {
  const cache = await caches.open(CACHE);

  let response;
  try {
    response = await fetch(SCOPE, { cache: "no-cache" });
  } catch (_) {
    return;
  }
  if (!response.ok) return;

  await cache.put(SCOPE, response.clone());
  let html = "";
  try {
    html = await response.text();
  } catch (_) {
    return;
  }

  const baseMatch = html.match(/<base\s[^>]*href=["']([^"']+)["']/i);
  let base = SCOPE;
  if (baseMatch) {
    try {
      base = new URL(baseMatch[1], SCOPE).href;
    } catch (_) {
      base = SCOPE;
    }
  }

  const urls = new Set();
  const refs = /<(?:script|link)\b[^>]*\b(?:src|href)=["']([^"']+)["'][^>]*>/gi;
  for (const match of html.matchAll(refs)) {
    let url;
    try {
      url = new URL(match[1], base);
    } catch (_) {
      continue;
    }
    if (url.origin !== self.location.origin) continue;
    if (!url.pathname.startsWith(SCOPE_PATH)) continue;
    urls.add(url.href);
  }

  const seen = new Set();
  await Promise.all([...urls].map((url) => cacheAsset(cache, url, seen)));

  // Stable files the Rust UI asks for by name (config.rs asset_url), not
  // through a hashed reference in index.html.
  await Promise.all(
    ["logo.svg", "favicon.svg", "tokens.css", "fonts.css"].map((file) =>
      cacheAsset(cache, new URL(file, SCOPE).href, seen)
    )
  );

  try {
    const config = await fetch(new URL("config.json", SCOPE), { cache: "no-cache" });
    if (config.ok) await cache.put(new URL("config.json", SCOPE).href, config);
  } catch (_) {
    // No config is fine: the app falls back to build-time defaults.
  }

  // Do not activate a shell that cannot boot. A failed install is retried on
  // the next online visit; an apparently-installed but incomplete cache would
  // only fail later, offline. Check the exact assets this index.html needs, so
  // an old cached bundle cannot mask a failed update.
  const cached = new Set((await cache.keys()).map((request) => request.url));
  const essential = [...urls].filter(
    (url) => /\.js$/i.test(url) || /\.css$/i.test(url) || /_bg\.wasm$/i.test(url)
  );
  if (!essential.length || essential.some((url) => !cached.has(url))) {
    throw new Error("ChefDAV app shell is incomplete");
  }
}

// Cache one asset. CSS also pulls in its `url(...)` dependencies, which is how
// the self-hosted woff2 files become available offline (they are not
// referenced from index.html directly).
async function cacheAsset(cache, url, seen) {
  if (seen.has(url)) return;
  seen.add(url);
  try {
    const asset = await fetch(url);
    if (!asset.ok) return;
    if (/\.css$/i.test(new URL(url).pathname)) {
      const css = await asset.clone().text();
      await cache.put(url, asset);
      const refs = css.matchAll(/url\(\s*["']?([^"')]+)["']?\s*\)/gi);
      await Promise.all(
        [...refs].map((match) => {
          let ref;
          try {
            ref = new URL(match[1], url);
          } catch (_) {
            return undefined;
          }
          if (ref.origin !== self.location.origin) return undefined;
          if (!ref.pathname.startsWith(SCOPE_PATH)) return undefined;
          return cacheAsset(cache, ref.href, seen);
        })
      );
    } else {
      await cache.put(url, asset);
    }
  } catch (_) {
    // A missing optional asset must not fail the whole install.
  }
}

// Online navigations update the cached shell from the network. Offline (or a
// 5xx from a proxy without a backend) gets the last good entry document, which
// keeps every SPA route usable.
async function navigation(request) {
  const cache = await caches.open(CACHE);
  try {
    const response = await fetch(request);
    if (response.ok) {
      await cache.put(SCOPE, response.clone());
      return response;
    }
    const cached = await cache.match(SCOPE, { ignoreVary: true });
    return cached || response;
  } catch (_) {
    const cached = await cache.match(SCOPE, { ignoreVary: true });
    if (cached) return cached;
    return new Response(
      "<!doctype html><meta charset=utf-8><title>ChefDAV</title>" +
        "<p>ChefDAV has not been stored on this device yet. Open it once while the server is reachable.</p>",
      { status: 503, headers: { "Content-Type": "text/html; charset=utf-8" } }
    );
  }
}

async function cacheFirst(request) {
  const cache = await caches.open(CACHE);
  const cached = await cache.match(request, { ignoreVary: true });
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) await cache.put(request, response.clone());
  return response;
}

async function networkFirst(request) {
  const cache = await caches.open(CACHE);
  try {
    const response = await fetch(request);
    if (response.ok) await cache.put(request, response.clone());
    return response;
  } catch (_) {
    return (await cache.match(request, { ignoreVary: true })) || Response.error();
  }
}

async function staleWhileRevalidate(request) {
  const cache = await caches.open(CACHE);
  const cached = await cache.match(request, { ignoreVary: true });
  const update = fetch(request).then(async (response) => {
    if (response.ok) await cache.put(request, response.clone());
    return response;
  });
  if (cached) {
    // Refresh in the background; do not leave an unhandled rejection when the
    // network is gone.
    update.catch(() => {});
    return cached;
  }
  try {
    return await update;
  } catch (_) {
    return Response.error();
  }
}
