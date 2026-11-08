// ChefDAV end-to-end smoke test.
//
// Prerequisites:
//   - dev stack running:  podman compose -f docker/docker-compose.yml up -d
//   - app running:        trunk serve   (http://127.0.0.1:3100)
//   - a calendar exists for alice (see README quickstart)
//
// Run:
//   cd e2e && npm install && node smoke.mjs
//
// Environment overrides:
//   APP_URL   (default http://127.0.0.1:3100)
//   DAV_URL   (default http://127.0.0.1:8080)
//   DAV_USER  (default alice)
//   DAV_PASS  (default secret)

import { chromium } from "playwright";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";


const run = Date.now().toString(36);
const EVENT = `E2E ${run}`;
const EVENT_EDITED = `${EVENT} edited`;

const log = (...a) => console.log("[e2e]", ...a);
let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const browser = await chromium.launch();
const page = await browser.newPage();

const consoleErrors = [];
page.on("console", (m) => {
  if (m.type() === "error") consoleErrors.push(m.text());
});
page.on("pageerror", (e) => consoleErrors.push(String(e)));

async function login() {
  await page.goto(APP, { waitUntil: "networkidle" });
  await page.waitForSelector(".login-card form", { timeout: 20000 });
  await page.fill('input[type="url"]', DAV);
  await page.fill('input[autocomplete="username"]', USER);
  await page.fill('input[autocomplete="current-password"]', PASS);
  await page.click('button[type="submit"]');
  try {
    await page.waitForSelector(".month-grid", { timeout: 20000 });
  } catch (e) {
    // Diagnostics: make intermittent failures debuggable instead of opaque.
    const formError = (await page.locator(".form-error").count())
      ? await page.textContent(".form-error")
      : "(none)";
    console.error("[e2e] login failed");
    console.error("  url:", page.url());
    console.error("  form error:", formError);
    console.error("  console errors:", consoleErrors);
    console.error("  has login card:", await page.locator(".login-card").count());
    console.error("  has month grid:", await page.locator(".month-grid").count());
    await page.screenshot({ path: "login-failure.png", fullPage: true });
    throw e;
  }
}

const chips = () => page.$$eval(".chip .chip-title", (els) => els.map((e) => e.textContent.trim()));

// ---------- login ----------
await login();
check("login", true, await page.textContent(".month-title"));

// the calendar list arrives with the first PROPFIND; wait for it
await page.waitForSelector(".cal-item", { timeout: 15000 });
const calendars = await page.$$eval(".cal-item .cal-name", (els) => els.map((e) => e.textContent.trim()));
check("calendar list", calendars.length > 0, calendars.join(", "));

// ---------- views ----------
await page.click('.segmented-btn:has-text("Week")');
await page.waitForSelector(".week-view", { timeout: 10000 });
const dayHeads = await page.locator(".week-day-head").count();
const hourLabels = await page.locator(".hour-label").count();
check("week view renders", dayHeads === 7 && hourLabels === 24, `${dayHeads} days, ${hourLabels} hours`);

await page.click('.segmented-btn:has-text("Day")');
await page.waitForSelector(".week-view", { timeout: 10000 });
const dayCols = await page.locator(".week-day-head").count();
check("day view renders", dayCols === 1, `${dayCols} day column`);

await page.click('.segmented-btn:has-text("List")');
await page.waitForSelector(".agenda", { timeout: 10000 });
await page.waitForSelector(".agenda-day, .empty-state", { timeout: 10000 });
check("agenda view renders", true, await page.textContent(".month-title"));

await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid", { timeout: 10000 });

// ---------- create ----------
await page.click("button.new-event");
await page.waitForSelector(".modal");
await page.fill('.modal input[type="text"]', EVENT);
  await page.fill('.modal input[type="date"]', "2026-11-22");
await page.fill('.modal input[type="time"] >> nth=0', "12:30");
await page.fill('.modal input[type="time"] >> nth=1', "13:30");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("create event", (await chips()).includes(EVENT));

// ---------- details first, then edit ----------
await page.locator(".chip", { hasText: EVENT }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
check("event click opens details", (await page.locator(".details-modal").count()) === 1);
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
await page.fill('.modal input[type="text"]', EVENT_EDITED);
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
const afterEdit = await chips();
check("edit event", afterEdit.includes(EVENT_EDITED) && !afterEdit.includes(EVENT));

// ---------- delete ----------
await page.locator(".chip", { hasText: EVENT_EDITED }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
await page.click('.modal button:has-text("Delete")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("delete event", !(await chips()).includes(EVENT_EDITED));

// ---------- stay logged in ----------
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
check("reload stays logged in", (await page.locator(".login-card form").count()) === 0);

// logout forgets the password but keeps server/username for prefill
await page.click('button:has-text("Log out")');
await page.waitForSelector(".login-card form", { timeout: 15000 });
const prefilledUser = await page.inputValue('input[autocomplete="username"]');
const prefilledPass = await page.inputValue('input[autocomplete="current-password"]');
check(
  "logout prefills username but not password",
  prefilledUser === USER && prefilledPass === "",
  `${prefilledUser} / ${prefilledPass ? "***" : "(empty)"}`
);

// ---------- wrong password ----------
await page.fill('input[autocomplete="current-password"]', "definitely-wrong");
await page.click('button[type="submit"]');
await page.waitForSelector(".form-error", { timeout: 15000 });
const err = (await page.textContent(".form-error")).trim();
check("wrong password rejected", err.toLowerCase().includes("unauthorized"), err);

// the wrong-password check intentionally triggers a 401 resource error
const unexpectedErrors = consoleErrors.filter((e) => !e.includes("401 (Unauthorized)"));
check("no console errors", unexpectedErrors.length === 0, unexpectedErrors.join(" | "));

await browser.close();
console.log(failures === 0 ? "\nALL CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
