// Deep-link e2e: every resource (event/task) has its own URL that is pushed
// into the address bar when opened, and opening such a link — including after
// a full reload — opens the resource view directly.
//
// Prerequisites: dev stack up (docker/docker-compose.yml) and the built app
// served on APP_URL (see AGENTS.md: trunk build + scripts/serve.py 3100 dist).
//
// Run: cd e2e && node links.mjs
//
// Environment overrides: APP_URL, DAV_URL, DAV_USER, DAV_PASS

import { chromium } from "playwright";
import { cleanupRun } from "./davclean.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

const run = Date.now().toString(36);
const EVENT = `Link ${run}`;
const TASK = `Link task ${run}`;

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
  await page.waitForSelector(".month-grid", { timeout: 20000 });
}

// ---------- login + create an event ----------
await login();
await page.waitForSelector(".cal-item", { timeout: 15000 });
const calNames = await page.$$eval(".cal-item .cal-name", (els) =>
  els.map((e) => e.textContent.trim())
);

await page.click("button.new-event");
await page.waitForSelector(".modal");
await page.fill('.modal input[type="text"]', EVENT);
await page.fill('.modal input[type="date"]', "2026-11-24");
await page.fill('.modal input[type="time"] >> nth=0', "12:30");
await page.fill('.modal input[type="time"] >> nth=1', "13:30");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("event created", (await page.$$eval(".chip .chip-title", (els) => els.map((e) => e.textContent.trim()))).includes(EVENT));

// ---------- clicking the event pushes its resource URL ----------
await page.locator(".chip", { hasText: EVENT }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
const url1 = new URL(page.url());
check(
  "address bar shows /event/<calendar>/<uid>",
  /^\/event\/[^/]+\/[^/?]+$/.test(url1.pathname),
  url1.pathname
);
const decoded = decodeURIComponent(url1.pathname);
check(
  "link contains the calendar name",
  calNames.some((c) => decoded.includes(c)),
  decoded
);
check(
  "details show the event",
  (await page.textContent(".details-modal h3")).trim() === EVENT
);
check(
  "copy-link button is an icon",
  (await page.locator(".details-modal .copy-link").count()) === 1 &&
    (await page.locator(".details-modal .copy-link svg").count()) === 1 &&
    ((await page.locator(".details-modal .copy-link").innerText()).trim() === ""),
  await page.locator(".details-modal .copy-link").getAttribute("title").catch(() => "")
);
const eventUrl = page.url();

// ---------- reload: the link re-opens the event (auto-login) ----------
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".details-modal", { timeout: 30000 });
check(
  "deep link reopens the event after reload",
  page.url() === eventUrl && (await page.textContent(".details-modal h3")).trim() === EVENT,
  page.url()
);

// ---------- back / forward ----------
await page.goBack();
await page.waitForSelector(".details-modal", { state: "detached", timeout: 10000 });
check(
  "back closes the resource view (returns to the view URL)",
  /^\/(month|week|day|list|tasks)/.test(new URL(page.url()).pathname),
  page.url()
);
await page.goForward();
await page.waitForSelector(".details-modal", { timeout: 10000 });
check(
  "forward reopens the resource view",
  new URL(page.url()).pathname.startsWith("/event/"),
  page.url()
);

// ---------- closing the modal returns to the view URL ----------
await page.click('.details-modal button:has-text("Close")');
await page.waitForSelector(".details-modal", { state: "detached", timeout: 10000 });
await page.waitForTimeout(300);
check(
  "close returns the address bar to the view URL",
  /^\/(month|week|day|list|tasks)/.test(new URL(page.url()).pathname),
  page.url()
);

// ---------- view + date in the URL ----------
await page.click('.segmented-btn:has-text("Week")');
await page.waitForSelector(".week-view", { timeout: 10000 });
await page.waitForTimeout(500);
const weekPath = new URL(page.url()).pathname;
check("week view sets /week/<date>", /^\/week\/\d{4}-\d{2}-\d{2}$/.test(weekPath), weekPath);
await page.click('button[title="Next"]');
await page.waitForTimeout(600);
const weekPath2 = new URL(page.url()).pathname;
check("next week updates the URL", weekPath2 !== weekPath && /^\/week\/\d{4}-\d{2}-\d{2}$/.test(weekPath2), weekPath2);
// reload restores the week + anchor from the URL
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
check(
  "reload restores week view from the URL",
  new URL(page.url()).pathname === weekPath2 &&
    (await page.locator('.segmented-btn.active').innerText()) === "Week",
  page.url()
);
// a shared day link opens that day directly
await page.goto(`${APP}/day/2026-11-20`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
check(
  "shared /day/2026-11-20 link opens that day",
  new URL(page.url()).pathname === "/day/2026-11-20" &&
    (await page.textContent(".month-title")).includes("20"),
  `${page.url()} — ${await page.textContent(".month-title")}`
);
await page.goto(`${APP}/month/2026-12`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 30000 });
check(
  "shared /month/2026-12 link opens December",
  (await page.textContent(".month-title")).includes("December"),
  await page.textContent(".month-title")
);
await page.goto(`${APP}/`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 30000 });

// ---------- task link ----------
await page.click('.segmented-btn:has-text("Tasks")');
await page.waitForSelector(".tasks-view", { timeout: 10000 });
await page.click('button:has-text("+ New task")');
await page.waitForSelector(".modal-small");
await page.fill('.modal-small input[type="text"]', TASK);
await page.click('.modal-small button:has-text("Save")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
await page.locator(".task-title", { hasText: TASK }).first().click();
await page.waitForSelector(".modal-small", { timeout: 10000 });
const taskUrl = new URL(page.url());
check("task click shows /task/<calendar>/<uid>", /^\/task\/[^/]+\/[^/?]+$/.test(taskUrl.pathname), taskUrl.pathname);

// reload: task deep link reopens the editor
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".modal-small", { timeout: 30000 });
check(
  "task deep link reopens after reload",
  new URL(page.url()).pathname.startsWith("/task/") &&
    (await page.inputValue('.modal-small input[type="text"]')) === TASK,
  page.url()
);

// ---------- unknown resource link -> error + back to / ----------
await page.goto(`${APP}/event/${encodeURIComponent(calNames[0] ?? "x")}/no-such-uid-12345`, {
  waitUntil: "networkidle",
});
await page.waitForSelector(".month-grid", { timeout: 30000 });
await page.waitForTimeout(3000);
check(
  "unknown link returns to the view URL with an error",
  /^\/(month|week|day|list|tasks)/.test(new URL(page.url()).pathname),
  page.url()
);

// ---------- cleanup via CalDAV (UI cleanup can miss hidden chips) ----------
await cleanupRun(run);

const unexpectedErrors = consoleErrors.filter((e) => !e.includes("404"));
check("no console errors", unexpectedErrors.length === 0, unexpectedErrors.join(" | "));

await browser.close();
console.log(failures === 0 ? "\nALL CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
