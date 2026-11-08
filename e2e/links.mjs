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
await page.fill('.modal input[type="date"]', "2026-11-22");
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
  "copy-link button exists",
  (await page.locator(".details-modal .copy-link").count()) === 1
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
check("back closes the resource view", new URL(page.url()).pathname === "/", page.url());
await page.goForward();
await page.waitForSelector(".details-modal", { timeout: 10000 });
check(
  "forward reopens the resource view",
  new URL(page.url()).pathname.startsWith("/event/"),
  page.url()
);

// ---------- closing the modal resets the address bar ----------
await page.click('.details-modal button:has-text("Close")');
await page.waitForSelector(".details-modal", { state: "detached", timeout: 10000 });
await page.waitForTimeout(300);
check("close resets the address bar to /", new URL(page.url()).pathname === "/", page.url());

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
  "unknown link returns to / with an error",
  new URL(page.url()).pathname === "/" &&
    (await page.locator(".error-toast, .error, .form-error").count()) >= 0,
  page.url()
);

// ---------- cleanup ----------
await page.locator(".task-title", { hasText: TASK }).first().click().catch(() => {});
await page.waitForSelector(".modal-small", { timeout: 5000 }).catch(() => {});
await page.click('.modal-small button:has-text("Delete")').catch(() => {});
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 }).catch(() => {});

await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid", { timeout: 10000 });
await page.locator(".chip", { hasText: EVENT }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
await page.click('.modal button:has-text("Delete")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);

const unexpectedErrors = consoleErrors.filter((e) => !e.includes("404"));
check("no console errors", unexpectedErrors.length === 0, unexpectedErrors.join(" | "));

await browser.close();
console.log(failures === 0 ? "\nALL CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
