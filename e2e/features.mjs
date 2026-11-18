// ChefDAV feature smoke test: view interactions, search, multi-select, tasks,
// recurrence, calendar management, theme/language, keyboard and offline outbox.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node features.mjs

import { launchPinned } from "./clock.mjs";
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

const { browser, page } = await launchPinned();
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

// ---------- events render in the browser timezone ----------
// Regression (2026-11-17): browser_tz() silently fell back to UTC, so events
// with a TZID rendered at their UTC times (the 12:00 Europe/Berlin seed
// "Lunch with Sam" showed as 11:00). Needs the seeded dev data; the expected
// chip time is derived from the browser's own zone, so it also holds on a
// UTC CI runner.
await waitFor(
  () => [...document.querySelectorAll(".chip")].some((e) => e.textContent.includes("Lunch with Sam")),
  null,
  15000
);
const lunch = await page.evaluate(() => {
  const chip = [...document.querySelectorAll(".chip")].find((e) =>
    e.textContent.includes("Lunch with Sam")
  );
  if (!chip) return null;
  // Nov 10, 2026 12:00 Europe/Berlin (CET, +1) as an exact instant; the chip
  // must show that instant in the browser's local time.
  const localHour = new Date("2026-11-10T12:00:00+01:00").getHours();
  return { chip: chip.textContent.trim(), expected: `${String(localHour).padStart(2, "0")}:00` };
});
check(
  "events render in the browser timezone (seed: 12:00 Berlin)",
  lunch !== null && lunch.chip.includes(lunch.expected),
  lunch ? `${lunch.chip} (expected ${lunch.expected})` : "(seed chip not found)"
);

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
// Regression (2026-11-18): the generic checkbutton rule out-specified
// `.switch input`, so the All day switch rendered as a 16px dot with a stray
// knob instead of its 44x24 track.
const sw = await page.evaluate(() => {
  const i = document.querySelector(".modal .switch input");
  if (!i) return null;
  const b = i.getBoundingClientRect();
  return { w: Math.round(b.width), h: Math.round(b.height), bg: getComputedStyle(i).backgroundColor };
});
check(
  "all-day switch renders as a 44x24 track",
  !!sw && Math.abs(sw.w - 44) <= 1 && Math.abs(sw.h - 24) <= 1 && sw.bg !== "rgba(0, 0, 0, 0)",
  JSON.stringify(sw)
);
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
await page.click(".tasks-head .btn-primary");
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

// up to 4 side by side, no "+N more" bar
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

// 5th overlapping event -> 3 shown + "+N more" bar spanning the hidden range
await davPutCol(4);
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 3, null, 20000);
await page.waitForTimeout(500);
const dayOverflow = await page.evaluate(() => {
  const bar = document.querySelector(".day-column .more-bar");
  return {
    events: document.querySelectorAll(".day-column .timed-event").length,
    more: document.querySelectorAll(".day-column .more-bar").length,
    moreText: bar?.textContent.trim() ?? null,
  };
});
check(
  "day view: 5 overlapping events show 3 + '+N more'",
  dayOverflow.events === 3 && dayOverflow.more === 1 && /^\+\d+ more$/.test(dayOverflow.moreText ?? ""),
  JSON.stringify(dayOverflow)
);
// the bar must sit exactly at the hidden events' range and count them ALL.
// The expectation is computed *in the page* so it holds on any runner
// timezone (CI is UTC, the dev box is Europe/Berlin): the two hidden events
// are one DAV Col event (10:00-12:00 Europe/Berlin) and the UI-created Col2
// (10:00-12:00 in the runner's local time) — the bar spans their union.
const barGeom = await page.evaluate(() => {
  const bar = document.querySelector(".day-column .more-bar");
  const col = document.querySelector(".day-column");
  const br = bar?.getBoundingClientRect();
  const cr = col?.getBoundingClientRect();
  if (!bar || !br || !cr) return null;
  const minOfDay = (d) => d.getHours() * 60 + d.getMinutes();
  // hidden #1: a DAV Col event — 10:00-12:00 Europe/Berlin (CET, +01:00)
  const bStart = minOfDay(new Date("2026-11-28T10:00:00+01:00"));
  const bEnd = minOfDay(new Date("2026-11-28T12:00:00+01:00"));
  // hidden #2: the UI-created Col2 — 10:00-12:00 in the RUNNER's local time
  const locStart = 10 * 60;
  const locEnd = 12 * 60;
  const s = Math.min(bStart, locStart);
  const e = Math.max(bEnd, locEnd);
  const hourPx = cr.height / 24;
  return {
    text: bar.textContent.trim(),
    top: Math.round(br.top - cr.top),
    h: Math.round(br.height),
    colH: Math.round(cr.height),
    expTop: Math.round((s / 60) * hourPx),
    expH: Math.round(((e - s) / 60) * hourPx),
  };
});
check(
  "day view: '+N more' bar spans only the hidden events' time range, count in label",
  dayOverflow.more === 1 &&
    dayOverflow.moreText === "+2 more" &&
    barGeom !== null &&
    barGeom.text === "+2 more" &&
    Math.abs(barGeom.top - barGeom.expTop) <= 2 &&
    Math.abs(barGeom.h - barGeom.expH) <= 2 &&
    barGeom.h < barGeom.colH,
  JSON.stringify({ dayOverflow, barGeom })
);

// Regression (2026-11-18, user report): the "+N more" popup listed unrelated
// events (one already visible, one never hidden, and events from other days)
// because the bar stored per-day occurrence indices that were looked up in the
// global event list, and the bar's <For> key omitted the hidden set.
await page.locator(".day-column .more-bar").first().click();
await page.waitForSelector(".more-popover", { timeout: 8000 });
await page.waitForTimeout(400);
const popupList = await page.evaluate(() => {
  const norm = (s) => s.replace(/\s+/g, " ").trim();
  const col = document.querySelector(".day-column");
  const visible = [...col.querySelectorAll(".timed-event")].map((e) => norm(e.textContent));
  const rows = [...document.querySelectorAll(".more-popover .popover-event")].map((r) => norm(r.textContent));
  // a row is "already visible" when its title is one of the visible events'
  const titleOf = (s) => s.replace(/^\d{1,2}:\d{2}/, "").trim();
  return {
    rows,
    count: rows.length,
    overlapsVisible: rows.filter((r) => visible.some((v) => titleOf(v) === titleOf(r))),
    barCount: (document.querySelector(".day-column .more-bar")?.textContent.trim() ?? "").match(/\d+/)?.[0],
  };
});
check(
  "'+N more' popup lists exactly the hidden events (none already visible)",
  popupList.count > 0 &&
    popupList.overlapsVisible.length === 0 &&
    Number(popupList.barCount) === popupList.count,
  JSON.stringify(popupList)
);
// clicking a listed event must open THAT event, not the one at its index in
// some other list
const firstRowTitle = popupList.rows[0].replace(/^\d{1,2}:\d{2}/, "").trim();
await page.locator(".more-popover .popover-event").first().click();
await page.waitForSelector(".details-modal", { timeout: 8000 });
await page.waitForTimeout(400);
const openedTitle = (await page.textContent(".details-modal")).replace(/\s+/g, " ");
check(
  "'+N more' popup opens the event it listed",
  openedTitle.includes(firstRowTitle),
  `listed "${firstRowTitle}" but opened "${openedTitle.slice(0, 60)}"`
);
await page.keyboard.press("Escape");
await page.waitForTimeout(400);

// Regression (2026-11-18, user report): the "+N more" popup used to be a
// dialog containing a bordered list — a card inside a card. It is now one
// plain popover: the popover carries the surface, the rows are flat.
await page.locator(".day-column .more-bar").first().click();
await page.waitForSelector(".more-popover", { timeout: 8000 });
await page.waitForTimeout(400);
const moreSurfaces = await page.evaluate(() => {
  const pop = document.querySelector(".more-popover");
  const cs = getComputedStyle(pop);
  const list = pop.querySelector("ul");
  const lcs = getComputedStyle(list);
  const row = pop.querySelector(".popover-event");
  const rcs = row ? getComputedStyle(row) : null;
  return {
    isPopover: pop.classList.contains("popover"),
    noNestedModal: document.querySelectorAll(".more-popover .modal").length === 0,
    popHasSurface: cs.backgroundColor !== "rgba(0, 0, 0, 0)" && cs.boxShadow !== "none",
    listFlat: lcs.backgroundColor === "rgba(0, 0, 0, 0)" && lcs.boxShadow === "none",
    rowFlat: !rcs || (rcs.backgroundColor === "rgba(0, 0, 0, 0)" && rcs.boxShadow === "none"),
    rows: pop.querySelectorAll(".popover-event").length,
  };
});
check(
  "'+N more' popup is a single popover, not a card in a card",
  moreSurfaces.isPopover &&
    moreSurfaces.noNestedModal &&
    moreSurfaces.popHasSurface &&
    moreSurfaces.listFlat &&
    moreSurfaces.rowFlat &&
    moreSurfaces.rows > 0,
  JSON.stringify(moreSurfaces)
);
await page.keyboard.press("Escape");
await page.waitForTimeout(300);
check("'+N more' popup closes with Escape", (await page.locator(".more-popover").count()) === 0);

// the point of the cluster rewrite: a lone event must keep FULL width while
// the overflowing block next to it shrinks — the old day-global layout gave
// every event the same fraction
await fetch(`${DAV}/alice/personal/lone-${run}@chef-dav.ics`, {
  method: "PUT",
  headers: { Authorization: davAuth, "Content-Type": "text/calendar" },
  body: `BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//e2e//EN\r\nBEGIN:VEVENT\r\nUID:lone-${run}@chef-dav\r\nDTSTAMP:20261101T090000Z\r\nDTSTART;TZID=Europe/Berlin:20261128T080000\r\nDTEND;TZID=Europe/Berlin:20261128T090000\r\nSUMMARY:Lone ${run}\r\nEND:VEVENT\r\nEND:VCALENDAR`,
});
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(
  (t) => [...document.querySelectorAll(".day-column .timed-event")].some((e) => e.textContent.includes(t)),
  `Lone ${run}`,
  20000
);
await page.waitForTimeout(500);
const mixed = await page.evaluate(() => {
  const col = document.querySelector(".day-column");
  const cr = col.getBoundingClientRect();
  const frac = (el) => Math.round((el.getBoundingClientRect().width / cr.width) * 100) / 100;
  const lone = [...document.querySelectorAll(".day-column .timed-event")].find((e) =>
    e.textContent.includes("Lone")
  );
  const block = [...document.querySelectorAll(".day-column .timed-event")].find((e) =>
    e.textContent.includes("Col")
  );
  const bar = document.querySelector(".day-column .more-bar");
  return {
    events: document.querySelectorAll(".day-column .timed-event").length,
    loneFrac: lone ? frac(lone) : null,
    blockFrac: block ? frac(block) : null,
    barText: bar?.textContent.trim() ?? null,
  };
});
check(
  "day view: lone event keeps full width beside an overflowing cluster",
  mixed.events === 4 &&
    mixed.loneFrac !== null && mixed.loneFrac > 0.9 &&
    mixed.blockFrac !== null && mixed.blockFrac < 0.3 &&
    mixed.barText === "+2 more",
  JSON.stringify(mixed)
);

// many events that DON'T overlap -> all shown, no "+N more" bar at all
for (let h = 12; h <= 16; h++) {
  await fetch(`${DAV}/alice/personal/nc${h}-${run}@chef-dav.ics`, {
    method: "PUT",
    headers: { Authorization: davAuth, "Content-Type": "text/calendar" },
    body: `BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//e2e//EN\r\nBEGIN:VEVENT\r\nUID:nc${h}-${run}@chef-dav\r\nDTSTAMP:20261101T090000Z\r\nDTSTART;TZID=Europe/Berlin:20261127T${String(h).padStart(2, "0")}0000\r\nDTEND;TZID=Europe/Berlin:20261127T${String(h + 1).padStart(2, "0")}0000\r\nSUMMARY:NonOverlap${h} ${run}\r\nEND:VEVENT\r\nEND:VCALENDAR`,
  });
}
await page.goto(`${APP}/day/2026-11-27`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 5, null, 20000);
await page.waitForTimeout(500);
const noOverlap = await page.evaluate(() => ({
  events: document.querySelectorAll(".day-column .timed-event").length,
  more: document.querySelectorAll(".day-column .more-bar").length,
}));
check(
  "day view: non-overlapping events all shown, no '+N more' bar",
  noOverlap.events >= 5 && noOverlap.more === 0,
  JSON.stringify(noOverlap)
);
await page.goto(`${APP}/day/${DAY28}`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column .timed-event").length >= 3, null, 20000);

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

// week view unchanged: same events -> 1 column + "+N more" in that day
await page.goto(`${APP}/week/2026-11-23`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await waitFor(() => document.querySelectorAll(".day-column")[5]?.querySelectorAll(".timed-event").length >= 1, null, 20000);
await page.waitForTimeout(500);
const wkOverflow = await page.evaluate(() => {
  const sat = document.querySelectorAll(".day-column")[5]; // Sat 28 Nov
  return { events: sat.querySelectorAll(".timed-event").length, more: !!sat.querySelector(".more-bar") };
});
check("week view still caps at 1 event + '+N more' on overflow", wkOverflow.events === 1 && wkOverflow.more, JSON.stringify(wkOverflow));

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
await page.click('.color-swatch[data-color="#e62d42"]');
await page.click('.modal-small button:has-text("Save")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
const dotStyle = await page
  .locator(".cal-item", { hasText: calName })
  .locator(".cal-dot")
  .getAttribute("style");
check(
  "calendar color changed",
  dotStyle.includes("#e62d42") || dotStyle.includes("230, 45, 66"),
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
// the app now defaults to the dark lacquer theme; force dark if a previous
// step left another theme, then verify both directions
const themeNow = await page.getAttribute("html", "data-theme");
if (themeNow !== "dark") {
  await page.click('.prefs-row button:has-text("Dark")');
  await page.waitForTimeout(300);
}
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
