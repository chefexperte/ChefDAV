// Location preview regression: an address is geocoded (Photon) into an OSM
// pin — that is also the canonical details-map.png screenshot — plus
// coordinate pins, the OSM link, and the search fallback for plain venue names.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node location.mjs

import { chromium } from "playwright";
import { cleanupRun } from "./davclean.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

// Fixed title/location so the details-map.png screenshot is reproducible.
const TITLE = "Bib-Date ❤️";
const ADDRESS = "Im Neuenheimer Feld 289"; // Heidelberg — geocoded via Photon
const COORDS = "52.5163, 13.3777"; // Berlin — parsed locally

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
await page.locator('.modal input[type="text"]').nth(1).fill(ADDRESS);
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1800);
check("map event created", (await page.locator(".chip", { hasText: TITLE }).count()) > 0);

// ---------- address pin (the details-map.png screenshot) ----------
await page.locator(".chip", { hasText: TITLE }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
// The iframe only appears after the Photon geocoding round-trip.
await page.waitForSelector(".details-map iframe", { timeout: 20000 });
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
  "address is geocoded into an OSM embed with a marker",
  embed?.includes("openstreetmap.org/export/embed.html") && embed.includes("marker="),
  embed ?? "(none)"
);
check(
  "geocoded pin lands in Heidelberg (Im Neuenheimer Feld)",
  link?.includes("openstreetmap.org/?mlat=49.4") && link.includes("&mlon=8.6"),
  link ?? "(none)"
);
check(
  "map overlay links to the OSM page",
  link?.includes("openstreetmap.org/?mlat=") && link.includes("&mlon="),
  link ?? "(none)"
);
check("geocoded label shown under the map", (await page.locator(".details-map-label").count()) === 1);
await page.screenshot({ path: "/tmp/opencode/screenshots/details-map.png" });
await page.click('.details-modal button:has-text("Close")');
await page.waitForSelector(".details-modal", { state: "detached" });

// ---------- coordinate pin (parsed locally, no geocoder) ----------
await setLocation(COORDS);
const coordEmbed = await page.getAttribute(".details-map iframe", "src");
const coordLink = await page.getAttribute(".details-map-overlay", "href");
check(
  "coordinates render an OSM embed with a marker",
  coordEmbed?.includes("openstreetmap.org/export/embed.html") &&
    coordEmbed.includes("marker=52.516300%2C13.377700"),
  coordEmbed ?? "(none)"
);
check(
  "map overlay links to the OSM page",
  coordLink?.includes("openstreetmap.org/?mlat=52.516300") &&
    coordLink.includes("mlon=13.377700"),
  coordLink ?? "(none)"
);
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
// The title is fixed (for reproducible screenshots), so delete every match via
// CalDAV in case an earlier interrupted run left one behind; then reload so
// the app drops it from local state before we verify.
await cleanupRun(TITLE);
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 30000 });
await page.waitForTimeout(1200);
check("map event deleted", (await page.locator(".chip", { hasText: TITLE }).count()) === 0);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL LOCATION CHECKS PASSED" : `\n${failures} LOCATION CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
