// ChefDAV feature smoke test: view interactions, search, multi-select, tasks,
// recurrence, calendar management, theme/language, keyboard and offline outbox.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node features.mjs

import { chromium } from "playwright";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";
const run = Date.now().toString(36);

const log = (...a) => console.log("[features]", ...a);
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
  await page.waitForSelector(".cal-item", { timeout: 15000 });
}

async function openNewEventFromSidebar() {
  await page.click("button.new-event");
  await page.waitForSelector(".modal");
}

async function fillEvent(title, start = "12:00", end = "13:00", date = "2026-11-20") {
  await page.fill('.modal input[type="text"]', title);
  await page.fill('.modal input[type="date"]', date);
  await page.fill('.modal input[type="time"] >> nth=0', start);
  await page.fill('.modal input[type="time"] >> nth=1', end);
}

async function saveModal() {
  await page.click('.modal button:has-text("Save")');
  await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
  await page.waitForTimeout(1200);
}

async function chips() {
  return page.$$eval(".chip .chip-title", (els) => els.map((e) => e.textContent.trim()));
}

await login();
check("login", true, await page.textContent(".month-title"));

// ---------- day click popup ----------
const dayCells = page.locator(".day");
await dayCells.nth(10).locator(".day-num").click();
await page.waitForSelector(".popover", { timeout: 10000 });
check("day click opens popover", (await page.locator(".popover .popup-action").count()) >= 2);
await page.click('.popover .popup-action:has-text("New event")');
await page.waitForSelector(".modal");
check("popup opens event editor", await page.locator(".modal h3").textContent() === "New event");
await page.click('.modal button:has-text("Cancel")');
await page.waitForSelector(".modal", { state: "detached" });

// ---------- week click-to-create ----------
await page.click('.segmented-btn:has-text("Week")');
await page.waitForSelector(".week-view", { timeout: 10000 });
await page.waitForTimeout(800);
const col = page.locator(".day-column").nth(2);
await col.click({ position: { x: 20, y: 14 * 42 + 5 } });
await page.waitForSelector(".popover", { timeout: 10000 });
await page.click('.popover .popup-action:has-text("New event")');
await page.waitForSelector(".modal", { timeout: 10000 });
const startVal = await page.locator('.modal input[type="time"]').first().inputValue();
check("week slot click prefills time", startVal.startsWith("14:"), startVal);
await page.click('.modal button:has-text("Cancel")');
await page.waitForSelector(".modal", { state: "detached" });

// ---------- all-day row click ----------
await page.locator(".allday-cell").nth(2).click();
await page.waitForSelector(".popover", { timeout: 10000 });
await page.click('.popover .popup-action:has-text("New all-day event")');
await page.waitForSelector(".modal", { timeout: 10000 });
const allDayChecked = await page.locator('.modal input[type="checkbox"]').first().isChecked();
check("all-day row click creates all-day event", allDayChecked);
await page.click('.modal button:has-text("Cancel")');
await page.waitForSelector(".modal", { state: "detached" });

// ---------- recurrence editor ----------
await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid");
await page.waitForTimeout(800);
const recTitle = `Recurring ${run}`;
await openNewEventFromSidebar();
await fillEvent(recTitle, "12:00", "13:00", "2026-11-15");
await page.selectOption('.modal select >> nth=1', "WEEKLY");
await page.waitForTimeout(200);
await page.fill('.modal input[type="number"] >> nth=0', "1");
await page.selectOption('.modal select >> nth=2', "count");
await page.fill('.modal input[type="number"] >> nth=1', "3");
await saveModal();
const recCount = (await chips()).filter((c) => c === recTitle).length;
check("recurrence editor creates 3 occurrences", recCount === 3, `${recCount} occurrences`);

// click → details first, then edit: recurrence controls should be prefilled
await page.locator(".chip", { hasText: recTitle }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
check("event click opens details first", (await page.locator(".details-modal").count()) === 1);
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
const freqVal = await page.locator('.modal select >> nth=1').inputValue();
check("recurrence editor prefills RRULE", freqVal === "WEEKLY", freqVal);
await page.click('.modal button:has-text("Cancel")');
await page.waitForSelector(".modal", { state: "detached" });

// ---------- tasks ----------
await page.click('.segmented-btn:has-text("Tasks")');
await page.waitForSelector(".tasks-view", { timeout: 10000 });
await page.click('button:has-text("+ New task")');
await page.waitForSelector(".modal-small");
const taskTitle = `Task ${run}`;
await page.fill('.modal-small input[type="text"]', taskTitle);
await page.click('.modal-small button:has-text("Save")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("task created", (await page.locator(".task-item").count()) > 0, await page.textContent(".task-item .task-title").catch(() => ""));
const taskRow = page.locator(".task-item", { hasText: taskTitle }).first();
await taskRow.locator('input[type="checkbox"]').check();
await page.waitForTimeout(1500);
check("task toggled done", await page.locator(".task-item.done", { hasText: taskTitle }).count() === 1);
// delete it
await page.locator(".task-title", { hasText: taskTitle }).first().click();
await page.waitForSelector(".modal-small");
await page.click('.modal-small button:has-text("Delete")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("task deleted", (await page.locator(".task-item", { hasText: taskTitle }).count()) === 0);

// ---------- search ----------
await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid");
await page.waitForTimeout(800);
await page.fill('.search-box input', recTitle);
await page.waitForSelector(".search-panel", { timeout: 10000 });
const hits = await page.locator(".search-hit").count();
check("search finds recurring event", hits >= 1, `${hits} hits`);
await page.locator(".search-hit").first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
check("search result opens event details", true);
await page.click('.details-modal button:has-text("Close")');
await page.waitForSelector(".details-modal", { state: "detached" });
await page.fill('.search-box input', "");
await page.waitForTimeout(300);
// the search result jumped to Day view; go back to Month for chip assertions
await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid", { timeout: 10000 });
await page.waitForTimeout(800);

// ---------- multi-select + bulk delete ----------
const selTitle = `Select ${run}`;
await openNewEventFromSidebar();
await fillEvent(selTitle, "12:00", "13:00", "2026-11-23");
await saveModal();
const selChip = page.locator(".chip", { hasText: selTitle }).first();
await selChip.click({ modifiers: ["Control"] });
await page.waitForSelector(".selection-bar", { timeout: 10000 });
check("ctrl-click selects event", (await page.textContent(".selection-count")).includes("1"));
await page.click('button:has-text("Delete selected")');
await page.waitForTimeout(1800);
check("bulk delete removes event", !(await chips()).includes(selTitle));

// ---------- calendar create / rename / delete ----------
const calName = `Cal ${run}`;
await page.click('.btn-icon-plain[title="New calendar"]');
await page.waitForSelector(".modal-small");
await page.fill('.modal-small input[type="text"]', calName);
await page.click('.modal-small button:has-text("Create")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("calendar created", (await page.locator(".cal-name", { hasText: calName }).count()) === 1);
const calItem = page.locator(".cal-item", { hasText: calName });

// sharing info + change the calendar color
await calItem.locator(".cal-menu-btn").click();
await page.waitForSelector(".modal-small");
const sharingUrl = await page.inputValue(".sharing-url input").catch(() => "");
check(
  "sharing panel shows calendar URL",
  (await page.locator(".sharing-info").count()) === 1 && sharingUrl.includes("/alice/"),
  sharingUrl
);
await page.click('.color-swatch[data-color="#d1495b"]');
await page.click('.modal-small button:has-text("Save")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
const dotStyle = await page
  .locator(".cal-item", { hasText: calName })
  .locator(".cal-dot")
  .getAttribute("style");
check(
  "calendar color changed",
  dotStyle.includes("#d1495b") || dotStyle.includes("209, 73, 91"),
  dotStyle
);

// rename it
await calItem.locator(".cal-menu-btn").click();
await page.waitForSelector(".modal-small");
const renamed = `${calName} X`;
await page.fill('.modal-small input[type="text"]', renamed);
await page.click('.modal-small button:has-text("Save")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("calendar renamed", (await page.locator(".cal-name", { hasText: renamed }).count()) === 1);
await page.locator(".cal-item", { hasText: renamed }).locator(".cal-menu-btn").click();
await page.waitForSelector(".modal-small");
await page.click('.modal-small button:has-text("Delete calendar")'); // switch to confirmation
await page.waitForTimeout(200);
await page.click('.modal-small button:has-text("Delete calendar")'); // confirm
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1500);
check("calendar deleted", (await page.locator(".cal-name", { hasText: renamed }).count()) === 0);

// ---------- theme + language ----------
await page.click('.prefs-row button:has-text("Dark")');
await page.waitForTimeout(300);
check("dark theme applied", (await page.getAttribute("html", "data-theme")) === "dark");
await page.click('.prefs-row button:has-text("Light")');
await page.waitForTimeout(300);
check("light theme applied", (await page.getAttribute("html", "data-theme")) === "light");
await page.click('.prefs-row button:has-text("EN")');
await page.waitForTimeout(300);
check("language toggle (DE)", (await page.textContent("button.new-event")).includes("Neuer"));
await page.click('.segmented-btn:has-text("Tag")');
await page.waitForTimeout(400);
const deTitle = await page.textContent(".month-title");
check("German date label", deTitle.includes(". November"), deTitle);
await page.click('.segmented-btn:has-text("Monat")');
await page.click('.prefs-row button:has-text("DE")');
await page.waitForTimeout(300);
check("language toggle (EN)", (await page.textContent("button.new-event")).includes("New event"));

// ---------- keyboard ----------
await page.keyboard.press("n");
await page.waitForSelector(".modal", { timeout: 10000 });
check("keyboard 'n' opens new event", true);
await page.keyboard.press("Escape");
await page.waitForSelector(".modal", { state: "detached" });
check("Escape closes modal", true);

// ---------- offline outbox ----------
const offlineTitle = `Offline ${run}`;
await page.route("**/*.ics", (route) => {
  if (route.request().method() === "PUT") route.abort();
  else route.continue();
});
await openNewEventFromSidebar();
await fillEvent(offlineTitle, "12:00", "13:00", "2026-11-25");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1500);
check("offline write shows pending badge", (await page.locator(".pending-badge").count()) > 0);
check("offline write shown optimistically", (await chips()).includes(offlineTitle));
await page.unroute("**/*.ics");
// trigger the online handler
await page.evaluate(() => window.dispatchEvent(new Event("online")));
await page.waitForTimeout(3000);
check("outbox flushed after reconnect", (await page.locator(".pending-badge").count()) === 0);

// cleanup: delete the offline event and the recurring series
for (const title of [offlineTitle, recTitle]) {
  const chip = page.locator(".chip", { hasText: title }).first();
  if (await chip.count()) {
    await chip.click();
    await page.waitForSelector(".details-modal", { timeout: 10000 });
    await page.click('.details-modal button:has-text("Edit")');
    await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
    await page.click('.modal button:has-text("Delete")');
    await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
    await page.waitForTimeout(1000);
  }
}

// the offline test intentionally aborts a PUT, which the browser logs
const unexpectedErrors = consoleErrors.filter((e) => !e.includes("net::ERR_FAILED"));
check("no console errors", unexpectedErrors.length === 0, unexpectedErrors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL FEATURE CHECKS PASSED" : `\n${failures} FEATURE CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
