// App-shell service worker regression: after one online visit the frontend
// (HTML, JS, WASM, CSS and config.json) must boot from CacheStorage even when
// the server hosting the frontend is completely unreachable.
//
// Prerequisites: same as smoke.mjs (dev stack + app served from dist/).
// Run: cd e2e && node offline-shell.mjs

import { launchPinned } from "./clock.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const { browser, page } = await launchPinned();
const context = page.context();
const errors = [];
page.on("console", (m) => {
  if (
    m.type() === "error" &&
    !m.text().includes("net::ERR_INTERNET_DISCONNECTED") &&
    !m.text().includes("net::ERR_FAILED")
  ) {
    errors.push(m.text());
  }
});
page.on("pageerror", (e) => errors.push(String(e)));

async function login() {
  await page.goto(APP, { waitUntil: "networkidle" });
  await page.waitForSelector(".login-card form", { timeout: 20000 });
  await page.fill('input[type="url"]', DAV);
  await page.fill('input[autocomplete="username"]', USER);
  await page.fill('input[autocomplete="current-password"]', PASS);
  await page.click('button[type="submit"]');
  await page.waitForSelector(".month-grid", { timeout: 20000 });
}

await login();
await page.waitForSelector(".chip", { timeout: 20000 });
await page.waitForTimeout(800); // let the INDEXEDDB event cache write finish

// The first visit installs the worker and precaches the shell. Wait until it
// controls the page and the cache really contains the executable assets.
await page.evaluate(() => navigator.serviceWorker.ready.then(() => true));
await page.waitForFunction(
  () =>
    new Promise((resolve) => {
      if (navigator.serviceWorker.controller) return resolve(true);
      navigator.serviceWorker.addEventListener("controllerchange", () => resolve(true), {
        once: true,
      });
    }),
  null,
  { timeout: 60000 }
);
const shellCache = await page.evaluate(async () => {
  const name = (await caches.keys()).find((key) => key.startsWith("chefdav-shell-"));
  if (!name) return { name: null, urls: [] };
  const cache = await caches.open(name);
  return { name, urls: (await cache.keys()).map((r) => r.url) };
});
check("app-shell cache exists", Boolean(shellCache.name), shellCache.name ?? "(none)");
check(
  "app-shell cache has the JS bundle",
  shellCache.urls.some((url) => /(?:^|\/)chef-dav[^/]*\.js$/.test(url)),
  shellCache.urls.slice(0, 6).join(", ")
);
check(
  "app-shell cache has the WASM bundle",
  shellCache.urls.some((url) => /_bg\.wasm$/.test(url)),
  shellCache.urls.slice(0, 6).join(", ")
);
check(
  "app-shell cache has the fonts",
  shellCache.urls.some((url) => /\.woff2$/.test(url)),
  shellCache.urls.slice(0, 6).join(", ")
);

// Cut *all* network access: no frontend server and no CalDAV server.
await context.setOffline(true);
await page.reload({ waitUntil: "domcontentloaded" });
await page.waitForSelector(".month-grid", { timeout: 30000 });
check(
  "offline reload boots the cached frontend",
  (await page.locator(".login-card form").count()) === 0
);
check(
  "offline reload still shows the cached calendar",
  (await page.locator(".cal-item").count()) > 0
);
check(
  "offline reload warns about the server",
  (await page.locator(".backend-banner").count()) === 1
);
await page.goto(new URL("month/2026-11", APP).href, { waitUntil: "domcontentloaded" });
await page.waitForSelector(".month-grid", { timeout: 30000 });
check(
  "offline deep link boots the cached shell",
  page.url().includes("/month/2026-11")
);

await context.setOffline(false);
await page.click(".backend-banner .backend-retry");
await page.waitForSelector(".backend-banner", { state: "detached", timeout: 30000 });
check(
  "reconnect restores the live session",
  !(await page.locator("button.new-event").isDisabled())
);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(
  failures === 0 ? "\nALL OFFLINE-SHELL CHECKS PASSED" : `\n${failures} OFFLINE-SHELL CHECK(S) FAILED`
);
process.exit(failures === 0 ? 0 : 1);
