// Multi-day events e2e: end-date picker in the editor (all-day and timed
// overnight), spanning bars in the month view, and ←/→ continuation hints in
// week/day/all-day rows and the list (agenda) view. Also checks that the edit
// popup no longer has a horizontal scrollbar.
//
// Run: cd e2e && node multiday.mjs   (dev stack + built app on APP_URL)

import { launchPinned } from "./clock.mjs";
import { cleanupRun } from "./davclean.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

const run = Date.now().toString(36);
const ALLDAY = `Multi ${run}`; // Wed 18 – Thu 19 Nov (all-day)
const OVERNIGHT = `Overnight ${run}`; // Wed 18 23:00 – Thu 19 01:00 (timed)
const SPANWK = `SpanWeek ${run}`; // Fri 20 – Mon 23 Nov (all-day, crosses week boundary)

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const { browser, page } = await launchPinned({ viewport: { width: 1280, height: 900 } });
const consoleErrors = [];
page.on("console", (m) => {
  if (m.type() === "error") consoleErrors.push(m.text());
});
page.on("pageerror", (e) => consoleErrors.push(String(e)));

await page.goto(APP, { waitUntil: "networkidle" });
await page.waitForSelector(".login-card form", { timeout: 20000 });
await page.fill('input[type="url"]', DAV);
await page.fill('input[autocomplete="username"]', USER);
await page.fill('input[autocomplete="current-password"]', PASS);
await page.click('button[type="submit"]');
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForSelector(".cal-item", { timeout: 15000 });

async function newEvent() {
  await page.click("button.new-event");
  await page.waitForSelector(".modal");
}
async function saveAndClose(title) {
  await page.click('.modal button:has-text("Save")');
  await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
  // wait until the sync has rendered the new event (chip or spanning bar)
  await page
    .locator(".chip, .span-bar", { hasText: title })
    .first()
    .waitFor({ timeout: 20000 });
}

// Click a toolbar nav button and wait until the URL actually moved — a bare
// waitForTimeout raced the navigation and sometimes checked the old week.
async function navAndWait(title, urlRe) {
  await page.click(`button[title="${title}"]`);
  try {
    await page.waitForURL(urlRe, { timeout: 8000 });
  } catch {
    await page.click(`button[title="${title}"]`); // click sometimes gets lost mid-render
    await page.waitForURL(urlRe, { timeout: 8000 });
  }
  await page.waitForTimeout(900);
}

// ---------- edit popup must not have a horizontal scrollbar ----------
await newEvent();
const noHScroll = await page.evaluate(() => {
  const m = document.querySelector(".modal");
  return m ? { sw: m.scrollWidth, cw: m.clientWidth } : null;
});
check(
  "edit popup has no horizontal scrollbar",
  !!noHScroll && noHScroll.sw <= noHScroll.cw + 1,
  JSON.stringify(noHScroll)
);

// ---------- all-day event with end date (18 → 19 Nov) ----------
await page.fill('.modal input[type="text"]', ALLDAY);
await page.locator('.modal input[type="date"]').nth(0).fill("2026-11-18");
await page.locator('.modal input[type="date"]').nth(1).fill("2026-11-19");
await page.locator(".modal .switch input").click(); // all-day
await saveAndClose(ALLDAY);
check("all-day event created", (await page.locator(".chip, .span-bar", { hasText: ALLDAY }).count()) > 0);

// ---------- timed overnight event (18 Nov 23:00 → 19 Nov 01:00) ----------
await newEvent();
await page.fill('.modal input[type="text"]', OVERNIGHT);
await page.locator('.modal input[type="date"]').nth(0).fill("2026-11-18");
await page.locator('.modal input[type="time"]').nth(0).fill("23:00");
await page.locator('.modal input[type="date"]').nth(1).fill("2026-11-19");
await page.locator('.modal input[type="time"]').nth(1).fill("01:00");
await saveAndClose(OVERNIGHT);
check("overnight event created", (await page.locator(".chip, .span-bar", { hasText: OVERNIGHT }).count()) > 0);

// ---------- all-day event crossing a week boundary (20 → 23 Nov) ----------
await newEvent();
await page.fill('.modal input[type="text"]', SPANWK);
await page.locator('.modal input[type="date"]').nth(0).fill("2026-11-20");
await page.locator('.modal input[type="date"]').nth(1).fill("2026-11-23");
await page.locator(".modal .switch input").click();
await saveAndClose(SPANWK);

// ---------- extra events to force all-day-row and column overflow ----------
for (let i = 0; i < 3; i++) {
  await newEvent();
  await page.fill('.modal input[type="text"]', `AD${i} ${run}`);
  await page.locator('.modal input[type="date"]').nth(0).fill("2026-11-18");
  await page.locator('.modal input[type="date"]').nth(1).fill("2026-11-18");
  await page.locator(".modal .switch input").click();
  await page.click('.modal button:has-text("Save")');
  await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
  await page.waitForTimeout(900);
}
for (let i = 0; i < 3; i++) {
  await newEvent();
  await page.fill('.modal input[type="text"]', `TM${i} ${run}`);
  await page.locator('.modal input[type="date"]').nth(0).fill("2026-11-20");
  await page.locator('.modal input[type="time"]').nth(0).fill("10:00");
  await page.locator('.modal input[type="time"]').nth(1).fill("11:00");
  await page.click('.modal button:has-text("Save")');
  await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
  await page.waitForTimeout(900);
}

// ---------- editor roundtrip: end date is prefilled when editing ----------
await page.locator(".span-bar, .chip", { hasText: ALLDAY }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
const endDateVal = await page.locator('.modal input[type="date"]').nth(1).inputValue();
check("editor prefills the end date", endDateVal === "2026-11-19", endDateVal);
await page.click('.modal button:has-text("Cancel")');
await page.waitForSelector(".modal", { state: "detached" });

// ---------- month view: spanning bars ----------
const barInfo = await page.evaluate((titles) => {
  const dayW = document.querySelector(".day")?.getBoundingClientRect().width ?? 1;
  return [...document.querySelectorAll(".span-bar")].map((b) => ({
    text: b.textContent.trim(),
    w: b.getBoundingClientRect().width,
    dayW,
    contLeft: b.classList.contains("cont-left"),
    contRight: b.classList.contains("cont-right"),
    hasArrL: !!b.querySelector(".bar-arr"),
  }));
}, [ALLDAY, OVERNIGHT, SPANWK]);
const alldayBar = barInfo.find((b) => b.text.includes(ALLDAY));
check(
  "month: all-day multi-day event renders as a spanning bar (~2 cells wide)",
  !!alldayBar && alldayBar.w > alldayBar.dayW * 1.5,
  alldayBar ? `${Math.round(alldayBar.w)}px vs day ${Math.round(alldayBar.dayW)}px` : "(no bar)"
);
const overnightBar = barInfo.find((b) => b.text.includes(OVERNIGHT));
check("month: overnight timed event renders as a bar with its start time", !!overnightBar && overnightBar.text.includes("23:00"), overnightBar?.text ?? "(none)");

// Regression (2026-11-18, user report): a spanning bar ran flush to the day
// column's edge; it must stop at the same right padding a chip keeps.
const barInset = await page.evaluate(() => {
  const bar = [...document.querySelectorAll(".span-bar")][0];
  if (!bar) return null;
  const br = bar.getBoundingClientRect();
  const cell = [...document.querySelectorAll(".day")].find((d) => {
    const q = d.getBoundingClientRect();
    return br.right <= q.right + 1 && br.right >= q.left;
  });
  if (!cell) return null;
  const cr = cell.getBoundingClientRect();
  const cs = getComputedStyle(cell);
  return {
    inset: +(cr.right - br.right).toFixed(1),
    cellPadRight: parseFloat(cs.paddingRight),
  };
});
check(
  "month: spanning bar keeps the day cell's right padding",
  !!barInset && Math.abs(barInset.inset - barInset.cellPadRight) <= 1.5,
  JSON.stringify(barInset)
);
const spanBars = barInfo.filter((b) => b.text.includes(SPANWK));
check(
  "month: week-crossing event is split into continuation segments with arrows",
  spanBars.length === 2 && spanBars.some((b) => b.contRight) && spanBars.some((b) => b.contLeft),
  spanBars.map((b) => `${b.contLeft ? "←" : ""}${b.contRight ? "→" : ""}`).join(" ")
);
// no duplicate chip on the middle day of the all-day event
const dupChips = await page.evaluate((t) => {
  return [...document.querySelectorAll(".chip")].filter((c) => c.textContent.includes(t)).length;
}, ALLDAY);
check("month: spanned days show the bar, not a duplicate chip", dupChips === 0, `${dupChips} chips`);

// ---------- week view: continuation arrows on timed + all-day ----------
await page.click('.segmented-btn:has-text("Week")');
await page.waitForSelector(".week-view", { timeout: 10000 });
await page.waitForTimeout(800);
// the test events live in the week of Nov 16–22
await navAndWait("Next", /\/week\/2026-11-16/);
const contEnd = await page.locator(".timed-event.cont-bottom", { hasText: OVERNIGHT }).count();
const contStart = await page.locator(".timed-event.cont-top", { hasText: OVERNIGHT }).count();
check("week: overnight event fades out at the bottom on the start day", contEnd >= 1, `${contEnd}`);
check("week: overnight event fades in at the top on the next day", contStart >= 1, `${contStart}`);
const adBars = await page.evaluate((t) => {
  return {
    bars: [...document.querySelectorAll(".allday-bar")].filter((c) => c.textContent.includes(t)).length,
    end: [...document.querySelectorAll(".allday-bar.cont-right")].filter((c) => c.textContent.includes(t)).length,
    start: [...document.querySelectorAll(".allday-bar.cont-left")].filter((c) => c.textContent.includes(t)).length,
  };
}, SPANWK);
check(
  "week: week-crossing all-day event fades out at the week boundary (cont-right)",
  adBars.bars >= 1 && adBars.end >= 1,
  JSON.stringify(adBars)
);
// next week: the continuation segment fades in at the left edge
await navAndWait("Next", /\/week\/2026-11-23/);
const adBars2 = await page.evaluate((t) => {
  return {
    bars: [...document.querySelectorAll(".allday-bar")].filter((c) => c.textContent.includes(t)).length,
    start: [...document.querySelectorAll(".allday-bar.cont-left")].filter((c) => c.textContent.includes(t)).length,
  };
}, SPANWK);
check(
  "week: continuation fades in at the start of the next week (cont-left)",
  adBars2.bars >= 1 && adBars2.start >= 1,
  JSON.stringify(adBars2)
);
// back to the week of Nov 16–22 for the row/overflow checks
await navAndWait("Previous", /\/week\/2026-11-16/);
const withinWeek = await page.evaluate((t) => {
  // an all-day event fully inside this week: one spanning bar, no fade
  const bars = [...document.querySelectorAll(".allday-bar")].filter((c) => c.textContent.includes(t));
  return bars.map((b) => ({ cont: b.classList.contains("cont-left") || b.classList.contains("cont-right"), w: b.getBoundingClientRect().width }));
}, ALLDAY);
check(
  "week: all-day event inside the week is a single longer bar without fade",
  withinWeek.length === 1 && !withinWeek[0].cont && withinWeek[0].w > 150,
  JSON.stringify(withinWeek)
);
// ---------- week: all-day row fixed height + "Show all"; column overflow ----------
const rowInfo = await page.evaluate(() => {
  const cells = [...document.querySelectorAll(".allday-cell")];
  return {
    heights: [...new Set(cells.map((c) => Math.round(c.getBoundingClientRect().height)))],
    col2: (() => {
      const c = cells[2];
      return {
        chips: c.querySelectorAll(".allday-chip").length,
        showAll: c.querySelector(".more-chip")?.textContent.trim() ?? null,
      };
    })(),
    col4: (() => {
      const col = document.querySelectorAll(".day-column")[4];
      return {
        events: col.querySelectorAll(".timed-event").length,
        moreBar: col.querySelector(".more-bar")?.textContent.trim() ?? null,
      };
    })(),
  };
});
check(
  "week: all-day row has a fixed height (3 entries)",
  rowInfo.heights.length === 1,
  JSON.stringify(rowInfo.heights)
);
check(
  "week: all-day overflow shows a neutral 'Show all' entry",
  rowInfo.col2.chips === 2 && rowInfo.col2.showAll === "Show all",
  JSON.stringify(rowInfo.col2)
);
check(
  "week: timed column overflow shows 1 event + '+N more'",
  rowInfo.col4.events === 1 && /^\+\d+ more$/.test(rowInfo.col4.moreBar ?? ""),
  JSON.stringify(rowInfo.col4)
);

// ---------- day view: no redundant header, continuation respected ----------
await page.click('.segmented-btn:has-text("Day")');
await page.waitForSelector(".week-view", { timeout: 10000 });
check(
  "day view: no day header, one column",
  (await page.locator(".week-day-head").count()) === 0 && (await page.locator(".day-column").count()) === 1
);

// ---------- list (agenda) view: continuation days listed with arrows ----------
await page.click('.segmented-btn:has-text("List")');
await page.waitForSelector(".agenda", { timeout: 10000 });
await page.waitForTimeout(800);
const agendaText = await page.evaluate(() => document.querySelector(".agenda")?.innerText ?? "");
check(
  "list: multi-day event appears on several days with ←/→ hints",
  agendaText.includes(`${SPANWK} →`) && agendaText.includes(`← ${SPANWK}`),
  agendaText.split("\n").filter((l) => l.includes(SPANWK)).join(" | ")
);

// ---------- month boundary: an event ending in the next month shows there ----------
// Regression (2026-11-18): the RRULE expansion filtered occurrences by start
// only, so a multi-day event starting in one month disappeared from the next.
// Dec 30 → Jan 2: no seeded event shares this window (TaCoS occupies the
// 30 Nov → 2 Dec row, and the month grid only has 2 bar slots per week row).
const MONTHSPAN = `MonthSpan ${run}`; // Wed 30 Dec → Sat 2 Jan (all-day)
await page.goto(`${APP}/month/2026-12`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForTimeout(800);
await newEvent();
await page.fill('.modal input[type="text"]', MONTHSPAN);
await page.locator('.modal input[type="date"]').nth(0).fill("2026-12-30");
await page.locator('.modal input[type="date"]').nth(1).fill("2027-01-02");
await page.locator(".modal .switch input").click(); // all-day
await saveAndClose(MONTHSPAN);
await page.goto(`${APP}/month/2026-12`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForTimeout(1000);
check(
  "month span: event visible in its start month",
  (await page.locator(".span-bar", { hasText: MONTHSPAN }).count()) > 0
);
await page.goto(`${APP}/month/2027-01`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForTimeout(1200);
const jan = await page.evaluate((t) => {
  const bar = [...document.querySelectorAll(".span-bar, .chip")].find((b) => b.textContent.includes(t));
  if (!bar) return null;
  const b = bar.getBoundingClientRect();
  const cells = [...[...document.querySelectorAll(".week")][0].querySelectorAll(".day")];
  // the bar must START in the cell labelled 30 (its true start) and end at the
  // right edge of the cell labelled 2
  const startCell = cells.findIndex((c) => c.querySelector(".day-num")?.textContent.trim() === "30");
  const endCell = cells.findIndex((c) => c.querySelector(".day-num")?.textContent.trim() === "2");
  const cw = cells[0].getBoundingClientRect().width;
  return {
    isBar: bar.classList.contains("span-bar"),
    contLeft: bar.classList.contains("cont-left"),
    startsAtTrueStart: startCell >= 0 && Math.abs(b.left - cells[startCell].getBoundingClientRect().left) < 20,
    spansIntoJan: endCell >= 0 && b.right > cells[endCell].getBoundingClientRect().left - 10,
    coversDays: +(b.width / cw).toFixed(2),
  };
}, MONTHSPAN);
// January's grid starts Mon 28 Dec, so the event starts INSIDE the grid (no
// left fade) but only its 1–2 Jan days belong to January.
check(
  "month span: event visible in the month it ends in",
  !!jan && jan.isBar,
  JSON.stringify(jan) || "(not rendered in January)"
);
check(
  "month span: bar runs from its 30 Dec start into January",
  !!jan && jan.startsAtTrueStart && jan.spansIntoJan && jan.coversDays > 2.5,
  JSON.stringify(jan)
);

// ---------- cleanup (directly via CalDAV, so hidden events are caught too) ----------
await cleanupRun(run);
await page.goto(`${APP}/month/2026-11`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 30000 });
await page.waitForTimeout(1500);
check(
  "cleanup removed the test events",
  (await page.locator(".chip, .span-bar", { hasText: run }).count()) === 0
);

const unexpected = consoleErrors.filter((e) => !e.includes("404"));
check("no console errors", unexpected.length === 0, unexpected.join(" | "));

await browser.close();
console.log(failures === 0 ? "\nALL MULTIDAY CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
