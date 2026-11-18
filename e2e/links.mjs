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

import { launchPinned } from "./clock.mjs";
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
const h3 = (await page.textContent(".details-modal h3")).trim();
check(
  "deep link reopens the event after reload",
  page.url() === eventUrl && h3 === EVENT,
  `url=${page.url()} h3=${JSON.stringify(h3)} expected=${EVENT}`
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
await page.click('.details-modal .modal-close');
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

// ---------- month boundary across navigation (Dec 2026 -> Jan 2027) ----------
// The boundary week row (Mon Dec 28 - Sun Jan 3) is shared between the two
// grids; its cells must recompute the `out` class when the anchor changes.
await page.click('button[title="Next"]');
await page.waitForURL(/\/month\/2027-01/, { timeout: 10000 });
await page.waitForTimeout(800);
const boundary = await page.evaluate(() => {
  const row = document.querySelector(".week");
  return [...row.querySelectorAll(".day")].map((c) => ({
    num: c.querySelector(".day-num")?.textContent.trim(),
    out: c.classList.contains("out"),
  }));
});
check(
  "month nav Dec->Jan: boundary week marks Dec 28-31 as out, Jan 1-3 as in",
  boundary.length === 7 &&
    boundary.slice(0, 4).every((c) => c.out && ["28", "29", "30", "31"].includes(c.num)) &&
    boundary.slice(4).every((c) => !c.out && ["1", "2", "3"].includes(c.num)),
  JSON.stringify(boundary)
);
// and back: Nov <- Dec, the Nov 30 / Dec 1 boundary must be correct too
await page.click('button[title="Previous"]');
await page.waitForURL(/\/month\/2026-12/, { timeout: 10000 });
await page.waitForTimeout(800);
await page.click('button[title="Previous"]');
await page.waitForURL(/\/month\/2026-11/, { timeout: 10000 });
await page.waitForTimeout(800);
const boundary2 = await page.evaluate(() => {
  const rows = [...document.querySelectorAll(".week")];
  const last = rows[rows.length - 1];
  return [...last.querySelectorAll(".day")].map((c) => ({
    num: c.querySelector(".day-num")?.textContent.trim(),
    out: c.classList.contains("out"),
  }));
});
check(
  "month nav Dec->Nov: last week marks Dec 1-6 as out, Nov 30 as in",
  boundary2.length === 7 &&
    boundary2[0].num === "30" && !boundary2[0].out &&
    boundary2.slice(1).every((c) => c.out && ["1", "2", "3", "4", "5", "6"].includes(c.num)),
  JSON.stringify(boundary2)
);
await page.goto(`${APP}/`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 30000 });

// ---------- task link ----------
await page.click('.segmented-btn:has-text("Tasks")');
await page.waitForSelector(".tasks-view", { timeout: 10000 });
await page.click(".tasks-head .btn-primary");
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

// ---------- popovers open at the anchor, at every depth ----------
// Regression (2026-11-18, user report): panels were clamped against a GUESSED
// height, so anything below the middle of the screen was pushed away from the
// cursor. They must open at the pointer (below-right when there is room,
// flipped above when there is not) and stay fully on screen.
const depths = [];
for (const row of [1, 4]) {
  await page.goto(`${APP}/month/2026-11`, { waitUntil: "networkidle" });
  await page.waitForSelector(".month-grid", { timeout: 20000 });
  await page.waitForTimeout(900);
  const chip = page.locator(".week").nth(row).locator(".chip").first();
  if (!(await chip.count())) continue;
  const b = await chip.boundingBox();
  const cx = Math.round(b.x + 20);
  const cy = Math.round(b.y + 8);
  await page.mouse.click(cx, cy);
  await page.waitForSelector(".details-modal", { timeout: 8000 });
  await page.waitForTimeout(500);
  depths.push(
    await page.evaluate(
      ([cx, cy, row]) => {
        const p = document.querySelector(".details-modal");
        const r = p.getBoundingClientRect();
        return {
          row,
          clickY: cy,
          dx: Math.round(r.left - cx),
          dy: Math.round(r.top - cy),
          onScreen: r.top >= 0 && r.bottom <= window.innerHeight && r.left >= 0 && r.right <= window.innerWidth,
        };
      },
      [cx, cy, row]
    )
  );
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
}
check(
  "popover opens at the cursor at every depth (flips up near the bottom)",
  depths.length === 2 &&
    depths.every((d) => Math.abs(d.dx - 8) <= 2 && (Math.abs(d.dy - 8) <= 2 || d.dy < 0) && d.onScreen),
  JSON.stringify(depths)
);

// the month/year menus hang centred under THEIR OWN field
await page.goto(`${APP}/month/2026-11`, { waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForTimeout(800);
const menuChecks = [];
for (const [kind, sel] of [
  ["month", '.title-part:has-text("November")'],
  ["year", '.title-part:has-text("2026")'],
]) {
  const fb = await page.locator(sel).boundingBox();
  await page.click(sel);
  await page.waitForSelector(".jump-popover", { timeout: 8000 });
  await page.waitForTimeout(400);
  // the field is located by its index, not a :has-text() selector (that is
  // Playwright syntax, not valid CSS inside the page)
  const fieldIdx = kind === "month" ? 0 : 1;
  const m = await page.evaluate((fieldIdx) => {
    const p = document.querySelector(".jump-popover");
    const r = p.getBoundingClientRect();
    const f = document.querySelectorAll(".title-part")[fieldIdx].getBoundingClientRect();
    return {
      fieldCentre: Math.round(f.left + f.width / 2),
      menuCentre: Math.round(r.left + r.width / 2),
      gap: Math.round(r.top - f.bottom),
    };
  }, fieldIdx);
  menuChecks.push({ kind, ...m });
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
}
check(
  "month/year menus open centred under their own field",
  menuChecks.length === 2 &&
    menuChecks.every((m) => Math.abs(m.menuCentre - m.fieldCentre) <= 2 && m.gap >= 0 && m.gap <= 12) &&
    menuChecks[0].fieldCentre !== menuChecks[1].fieldCentre,
  JSON.stringify(menuChecks)
);

// ---------- brand (logo + ChefDAV) links home ----------
// From a resource link in another view: click the wordmark -> month view of
// the current month, nothing selected, no modal open.
await page.goto(`${APP}/week/2026-11-16`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 30000 });
await page.waitForTimeout(800);
await page.locator(".timed-event, .chip, .allday-chip").first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.keyboard.press("Escape");
await page.waitForTimeout(500);
await page.click(".brand");
await page.waitForTimeout(1200);
const homeState = {
  url: new URL(page.url()).pathname,
  monthGrid: await page.locator(".month-grid").count(),
  open: await page.locator(".details-modal, .modal, .more-popover, .day-popup, .slot-popup").count(),
};
// pinned clock = Mon 9 Nov 2026, so "current month" is November 2026
check(
  "brand links home to the current month",
  homeState.url === "/month/2026-11" && homeState.monthGrid === 1 && homeState.open === 0,
  JSON.stringify(homeState)
);
check("brand is a real link (href=/)", (await page.getAttribute(".brand", "href")) === "/");

// The URL alone is not proof: the old brand click rewrote the path but left
// the week grid on screen. Start from another view and assert the DOM changed.
await page.goto(`${APP}/week/2026-09-14`, { waitUntil: "networkidle" });
await page.waitForSelector(".week-view", { timeout: 20000 });
await page.waitForTimeout(800);
await page.locator(".brand img").click();
await page.waitForTimeout(1200);
const brandRender = {
  url: new URL(page.url()).pathname,
  weekGone: (await page.locator(".week-view").count()) === 0,
  monthShown: (await page.locator(".month-grid").count()) === 1,
};
check(
  "brand click actually renders the month view (not just the URL)",
  brandRender.url === "/month/2026-11" && brandRender.weekGone && brandRender.monthShown,
  JSON.stringify(brandRender)
);
// and the brand keeps top/bottom breathing room
const brandBox = await page.locator(".brand").boundingBox();
check(
  "brand has top/bottom padding",
  !!brandBox && brandBox.height >= 44 && brandBox.y >= 8,
  JSON.stringify(brandBox)
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
