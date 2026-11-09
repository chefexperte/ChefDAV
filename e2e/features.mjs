// ChefDAV feature smoke test: view interactions, search, multi-select, tasks,
// recurrence, calendar management, theme/language, keyboard and offline outbox.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node features.mjs

import { chromium } from "playwright";
import { cleanupRun } from "./davclean.mjs";

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

// Wait until a DOM condition holds (default 10 s); returns false on timeout
// instead of throwing, so the check reports FAIL with detail instead of
// crashing the whole suite and skipping the DAV cleanup.
async function waitFor(condFn, arg, timeout = 10000) {
  try {
    await page.waitForFunction(condFn, arg, { timeout });
    return true;
  } catch {
    return false;
  }
}
// NOTE: conditions passed to waitFor run in the PAGE, so they must be
// self-contained (no references to Node-side helpers).

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
await waitFor(
  (t) => [...document.querySelectorAll(".chip .chip-title")].filter((c) => c.textContent.trim() === t).length >= 3,
  recTitle
);
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
await waitFor((t) => [...document.querySelectorAll(".task-item")].some((r) => r.textContent.includes(t)), taskTitle);
check("task created", (await page.locator(".task-item", { hasText: taskTitle }).count()) > 0, await page.textContent(".task-item .task-title").catch(() => ""));
const taskRow = page.locator(".task-item", { hasText: taskTitle }).first();
await taskRow.locator('input[type="checkbox"]').check();
check(
  "task toggled done",
  await waitFor((t) => [...document.querySelectorAll(".task-item")].some((r) => r.classList.contains("done") && r.textContent.includes(t)), taskTitle)
);
// delete it
await page.locator(".task-title", { hasText: taskTitle }).first().click();
await page.waitForSelector(".modal-small");
await page.click('.modal-small button:has-text("Delete")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
check("task deleted", await waitFor((t) => ![...document.querySelectorAll(".task-item")].some((r) => r.textContent.includes(t)), taskTitle));

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
await page.click('.details-modal .modal-close');
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
check(
  "bulk delete removes event",
  await waitFor((t) => ![...document.querySelectorAll(".chip .chip-title")].some((c) => c.textContent.includes(t)), selTitle),
  (await chips()).join(", ")
);

// ---------- day view: 4 side-by-side columns, overflow at 5+ ----------
// (most created via CalDAV for speed; all tagged with `run` for cleanupRun)
const DAY28 = "2026-11-28"; // a Saturday with no seeded timed events
const davAuth = "Basic " + Buffer.from(`${USER}:${PASS}`).toString("base64");
async function davPutCol(i) {
  await fetch(`${DAV}/alice/personal/col${i}-${run}@chef-dav.ics`, {
    method: "PUT",
    headers: { Authorization: davAuth, "Content-Type": "text/calendar" },
    body: `BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//e2e//EN\r\nBEGIN:VEVENT\r\nUID:col${i}-${run}@chef-dav\r\nDTSTAMP:20261101T090000Z\r\nDTSTART;TZID=Europe/Berlin:20261128T100000\r\nDTEND;TZID=Europe/Berlin:20261128T120000\r\nSUMMARY:Col${i} ${run}\r\nEND:VEVENT\r\nEND:VCALENDAR`,
  });
}
const colWidthFraction = () =>
  page.evaluate(() => {
    const col = document.querySelector(".day-column");
    const ev = document.querySelector(".day-column .timed-event");
    if (!col || !ev) return null;
    return ev.getBoundingClientRect().width / col.getBoundingClientRect().width;
  });

// start with 2 overlapping events (2 columns -> ~50% wide each)
await davPutCol(0);
await davPutCol(1);
await page.goto(`${APP}/day/${DAY28}`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 2, null, 20000);
const frac2 = await colWidthFraction();

// create a 3rd overlapping event via the UI (no reload): columns must resize
// from 2x50% to 3x33% immediately — the <For> key must include col/cols
await page.click("button.new-event");
await page.waitForSelector(".modal");
await page.fill('.modal input[type="text"]', `Col2 ${run}`);
await page.locator('.modal input[type="date"]').nth(0).fill(DAY28);
await page.locator('.modal input[type="time"]').nth(0).fill("10:00");
await page.locator('.modal input[type="time"]').nth(1).fill("12:00");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 3, null, 20000);
await page.waitForTimeout(400);
const frac3 = await colWidthFraction();
check(
  "day view: columns resize when an overlapping event is created (no reload)",
  frac2 !== null && Math.abs(frac2 - 0.5) < 0.03 && frac3 !== null && Math.abs(frac3 - 1 / 3) < 0.03,
  `2 events: ${frac2?.toFixed(3)}, 3 events: ${frac3?.toFixed(3)}`
);

// up to 4 side by side, no "Show all"
await davPutCol(3);
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 4, null, 20000);
check(
  "day view: 4 overlapping events all shown side by side",
  (await page.locator(".day-column .timed-event").count()) === 4 &&
    (await page.locator(".more-bar").count()) === 0,
  `${await page.locator(".day-column .timed-event").count()} events, ${await page.locator(".more-bar").count()} more-bars`
);

// 5th overlapping event -> 3 shown + full-height "Show all"
await davPutCol(4);
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 3, null, 20000);
await page.waitForTimeout(500);
const dayOverflow = await page.evaluate(() => ({
  events: document.querySelectorAll(".day-column .timed-event").length,
  more: document.querySelectorAll(".day-column .more-bar").length,
  moreH: document.querySelector(".day-column .more-bar")?.getBoundingClientRect().height ?? 0,
  moreTop: document.querySelector(".day-column .more-bar")?.getBoundingClientRect().top ?? 0,
  colH: document.querySelector(".day-column")?.getBoundingClientRect().height ?? 0,
  colTop: document.querySelector(".day-column")?.getBoundingClientRect().top ?? 0,
}));
check(
  "day view: 5 overlapping events show 3 + 'Show all'",
  dayOverflow.events === 3 && dayOverflow.more === 1,
  JSON.stringify(dayOverflow)
);
check(
  "day view: 'Show all' bar spans the full column height (top to bottom)",
  dayOverflow.more === 1 &&
    Math.abs(dayOverflow.moreTop - dayOverflow.colTop) <= 2 &&
    Math.abs(dayOverflow.moreH - dayOverflow.colH) <= 2,
  `bar ${Math.round(dayOverflow.moreH)}px of column ${Math.round(dayOverflow.colH)}px, top offset ${Math.round(dayOverflow.moreTop - dayOverflow.colTop)}px`
);

// sticky title: add a long event that can scroll past the viewport top, scroll
// the page to the bottom, and the long event's title must stay pinned at top
await fetch(`${DAV}/alice/personal/long-${run}@chef-dav.ics`, {
  method: "PUT",
  headers: { Authorization: davAuth, "Content-Type": "text/calendar" },
  body: `BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//e2e//EN\r\nBEGIN:VEVENT\r\nUID:long-${run}@chef-dav\r\nDTSTAMP:20261101T090000Z\r\nDTSTART;TZID=Europe/Berlin:20261128T003000\r\nDTEND;TZID=Europe/Berlin:20261128T233000\r\nSUMMARY:Long ${run}\r\nEND:VEVENT\r\nEND:VCALENDAR`,
});
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 3, null, 20000);
const sticky = await page.evaluate(() => {
  const pos = getComputedStyle(document.querySelector(".timed-event .te-title")).position;
  window.scrollTo(0, document.documentElement.scrollHeight); // scroll to the bottom
  // an event whose top scrolled above the viewport (the long 00:30 event)
  const ev = [...document.querySelectorAll(".timed-event")].find(
    (e) => !e.classList.contains("compact") && e.getBoundingClientRect().top < -5
  );
  if (!ev) return { pos, stuck: false };
  const titleTop = ev.querySelector(".te-title").getBoundingClientRect().top;
  const evTop = ev.getBoundingClientRect().top;
  window.scrollTo(0, 0);
  return { pos, stuck: true, evTop, titleTop };
});
check(
  "timed event title sticks to the top while scrolling the timeline",
  !!sticky && sticky.pos === "sticky" && sticky.stuck && sticky.evTop < 0 && Math.abs(sticky.titleTop) <= 3,
  JSON.stringify(sticky)
);

// week view unchanged: same events -> 1 column + "Show all" in that day
await page.goto(`${APP}/week/2026-11-23`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column")[5]?.querySelectorAll(".timed-event").length >= 1, null, 20000);
await page.waitForTimeout(500);
const wkOverflow = await page.evaluate(() => {
  const sat = document.querySelectorAll(".day-column")[5]; // Sat 28 Nov
  return { events: sat.querySelectorAll(".timed-event").length, more: !!sat.querySelector(".more-bar") };
});
check("week view still caps at 1 event + 'Show all' on overflow", wkOverflow.events === 1 && wkOverflow.more, JSON.stringify(wkOverflow));

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
check(
  "offline write shows pending badge",
  await waitFor(() => document.querySelectorAll(".pending-badge").length > 0, null)
);
check(
  "offline write shown optimistically",
  await waitFor((t) => [...document.querySelectorAll(".chip .chip-title")].some((c) => c.textContent.includes(t)), offlineTitle),
  (await chips()).join(", ")
);
await page.unroute("**/*.ics");
// trigger the online handler
await page.evaluate(() => window.dispatchEvent(new Event("online")));
check(
  "outbox flushed after reconnect",
  await waitFor(() => document.querySelectorAll(".pending-badge").length === 0, null, 15000)
);

// cleanup: delete everything tagged with this run directly via CalDAV (a chip
// hidden behind a cell's "Show all" overflow can't be clicked in the UI)
await cleanupRun(run, [calName]);

// the offline test intentionally aborts a PUT, which the browser logs
const unexpectedErrors = consoleErrors.filter((e) => !e.includes("net::ERR_FAILED"));
check("no console errors", unexpectedErrors.length === 0, unexpectedErrors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL FEATURE CHECKS PASSED" : `\n${failures} FEATURE CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
