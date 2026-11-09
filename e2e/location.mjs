// Location preview regression: coordinate pins, the OSM link, and the
// search fallback for plain venue names.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node location.mjs

import { chromium } from "playwright";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

const run = Date.now().toString(36);
const TITLE = `Map ${run}`;

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const errors = [];
page.on("console", (m) => {
  if (m.type() === "error" && !m.text().includes("401 (Unauthorized)")) errors.push(m.text());
});
page.on("pageerror", (e) => errors.push(String(e)));

await page.goto(APP, { waitUntil: "networkidle" });
await page.waitForSelector(".login-card form", { timeout: 20000 });
await page.fill('input[type="url"]', DAV);
await page.fill('input[autocomplete="username"]', USER);
await page.fill('input[autocomplete="current-password"]', PASS);
await page.click('button[type="submit"]');
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForTimeout(1500);

async function openEditor() {
  await page.locator(".chip", { hasText: TITLE }).first().click();
  await page.waitForSelector(".details-modal", { timeout: 10000 });
  await page.click('.details-modal button:has-text("Edit")');
  await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
}

async function setLocation(location) {
  await openEditor();
  await page.locator('.modal input[type="text"]').nth(1).fill(location);
  await page.click('.modal button:has-text("Save")');
  await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
  await page.waitForTimeout(1800);
  await page.locator(".chip", { hasText: TITLE }).first().click();
  await page.waitForSelector(".details-modal", { timeout: 10000 });
}

// ---------- create ----------
await page.click("button.new-event");
await page.waitForSelector(".modal");
await page.fill('.modal input[type="text"]', TITLE);
await page.fill('.modal input[type="date"]', "2026-11-26");
await page.fill('.modal input[type="time"] >> nth=0', "15:00");
await page.fill('.modal input[type="time"] >> nth=1', "16:00");
await page.locator('.modal input[type="text"]').nth(1).fill("52.5163, 13.3777");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1800);
check("map event created", (await page.locator(".chip", { hasText: TITLE }).count()) > 0);

// ---------- coordinate pin ----------
await page.locator(".chip", { hasText: TITLE }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.waitForSelector(".details-map iframe", { timeout: 10000 });
// Let the modal pop animation finish and the OSM tiles load before shooting,
// otherwise the screenshot catches the popup half-visible / the map blank.
// NB: polling getAnimations() too early returns an EMPTY list, which would
// pass `.every()` — so also require the computed style to be fully settled.
const settleModal = () =>
  page
    .waitForFunction(
      () => {
        const m = document.querySelector(".modal-overlay .modal");
        if (!m) return false;
        const cs = getComputedStyle(m);
        return (
          cs.opacity === "1" &&
          cs.transform === "none" &&
          m.getAnimations({ subtree: true }).every((a) => a.playState === "finished" || a.playState === "idle")
        );
      },
      { timeout: 10000 }
    )
    .catch(() => {});
// Poll for the OSM embed frame — it may not be registered on the first check,
// in which case the networkidle wait below would be skipped silently.
let mapFrame = null;
for (let i = 0; i < 40 && !mapFrame; i++) {
  mapFrame = page.frames().find((f) => f.url().includes("openstreetmap.org/export/embed.html"));
  if (!mapFrame) await page.waitForTimeout(250);
}
if (mapFrame) {
  await mapFrame.waitForLoadState("load", { timeout: 30000 }).catch(() => {});
  await mapFrame.waitForLoadState("networkidle", { timeout: 30000 }).catch(() => {});
}
// The modal can re-render (restarting `pop`) when the map finishes loading,
// so settle the animation only after the frame is done.
await settleModal();
await page.waitForTimeout(400);
const embed = await page.getAttribute(".details-map iframe", "src");
const link = await page.getAttribute(".details-map-overlay", "href");
check(
  "coordinates render an OSM embed with a marker",
  embed?.includes("openstreetmap.org/export/embed.html") &&
    embed.includes("marker=52.516300%2C13.377700"),
  embed ?? "(none)"
);
check(
  "map overlay links to the OSM page",
  link?.includes("openstreetmap.org/?mlat=52.516300") &&
    link.includes("mlon=13.377700"),
  link ?? "(none)"
);
await page.screenshot({ path: "/tmp/opencode/screenshots/details-map.png" });
await page.click('.details-modal button:has-text("Close")');
await page.waitForSelector(".details-modal", { state: "detached" });

// ---------- plain venue name: search fallback ----------
await setLocation("Café Nord");
check("plain venue name shows the OSM search fallback", (await page.locator(".details-map-search").count()) === 1);
const search = await page.getAttribute(".details-map-search", "href");
check(
  "search fallback points at OSM search",
  search?.includes("openstreetmap.org/search?query=Caf%C3%A9+Nord"),
  search ?? "(none)"
);
await page.click('.details-modal button:has-text("Close")');
await page.waitForSelector(".details-modal", { state: "detached" });

// ---------- cleanup ----------
await openEditor();
await page.click('.modal button:has-text("Delete")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("map event deleted", (await page.locator(".chip", { hasText: TITLE }).count()) === 0);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL LOCATION CHECKS PASSED" : `\n${failures} LOCATION CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
