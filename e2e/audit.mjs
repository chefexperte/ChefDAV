// Screenshot + layout + translation audit for ChefDAV.
// Saves screenshots to /tmp/opencode/screenshots and prints layout/translation
// findings. Run: cd e2e && node audit.mjs

import { chromium } from "playwright";
import fs from "fs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";
const OUT = "/tmp/opencode/screenshots";
fs.mkdirSync(OUT, { recursive: true });

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const englishUiWords = [
  "New event", "New task", "New calendar", "Save", "Cancel", "Delete", "Today",
  "Month", "Week", "Day", "Tasks", "Search events", "Calendar server", "Username",
  "Password", "Log in", "Log out", "Dark", "Light", "All day", "Location", "Notes",
  "Repeat", "Every", "Ends", "Never", "Until", "Apply to", "This event",
  "All events", "Close", "Edit event", "Sharing", "Copy", "Your access",
  "No events", "More events", "Read-only", "No date", "selected", "pending",
  "syncing", "Title", "Date", "Start", "End", "Calendar", "Task", "Done",
  "Priority", "Due date", "Creating", "Saving", "Deleting", "Connecting",
  "Rename calendar", "Delete calendar", "Calendar URL", "Custom color", "Color",
  "No matching events", "Nothing here", "All clear", "No tasks yet",
  "No due date", "What needs doing", "Edit task", "This calendar is read-only",
  "Go to day", "No events at this time", "New all-day event", "Unknown",
];
function englishIn(text) {
  return englishUiWords.filter((w) => new RegExp(`\\b${w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`, "i").test(text));
}
async function scanDe(selector, name) {
  const text = await page.locator(selector).innerText();
  const found = englishIn(text);
  check(`German ${name} has no English labels`, found.length === 0, found.join(", "));
}

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => {
  if (m.type() === "error" && !m.text().includes("401 (Unauthorized)")) errors.push(m.text());
});

const shot = (name, opts = {}) => page.screenshot({ path: `${OUT}/${name}.png`, ...opts });

await page.goto(APP, { waitUntil: "networkidle" });
await page.waitForSelector(".login-card", { timeout: 20000 });
await shot("login-en");

await page.fill('input[type="url"]', DAV);
await page.fill('input[autocomplete="username"]', USER);
await page.fill('input[autocomplete="current-password"]', PASS);
await page.click('button[type="submit"]');
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForSelector(".cal-item", { timeout: 15000 });
await page.waitForTimeout(1800);
await shot("month-en");

// ---------- layout: equal month cells / no chip overflow ----------
const dayHeights = await page.$$eval(".day", (els) =>
  els.map((e) => Math.round(e.getBoundingClientRect().height * 10) / 10)
);
const uniqueHeights = [...new Set(dayHeights)];
check("month cells equal height", uniqueHeights.length === 1, uniqueHeights.join(", "));

const chipOverflow = await page.$$eval(".chip", (els) =>
  els.map((e) => {
    const d = e.closest(".day").getBoundingClientRect();
    const r = e.getBoundingClientRect();
    return { right: r.right - d.right, bottom: r.bottom - d.bottom };
  })
);
const maxRight = Math.max(...chipOverflow.map((x) => x.right), 0);
const maxBottom = Math.max(...chipOverflow.map((x) => x.bottom), 0);
check("chips stay inside day cells", maxRight <= 1 && maxBottom <= 1, `right+${maxRight.toFixed(1)} bottom+${maxBottom.toFixed(1)}`);

// ---------- views ----------
await page.click('.segmented-btn:has-text("Week")');
await page.waitForSelector(".week-view", { timeout: 10000 });
await page.waitForTimeout(800);
await shot("week-en");

await page.click('.segmented-btn:has-text("Day")');
await page.waitForSelector(".week-view", { timeout: 10000 });
await page.waitForTimeout(800);
await shot("day-en");

await page.click('.segmented-btn:has-text("Agenda")');
await page.waitForSelector(".agenda", { timeout: 10000 });
await page.waitForTimeout(800);
await shot("agenda-en");

await page.click('.segmented-btn:has-text("Tasks")');
await page.waitForSelector(".tasks-view", { timeout: 10000 });
await page.waitForTimeout(800);
await shot("tasks-en");

// ---------- event details + editor ----------
await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid", { timeout: 10000 });
await page.waitForTimeout(800);
await page.locator(".chip", { hasText: "Lunch with Sam" }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await shot("details-en");
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });

// all-day switch alignment vs date input
const align = await page.evaluate(() => {
  const date = document.querySelector('.modal input[type="date"]');
  const sw = document.querySelector(".modal .switch");
  if (!date || !sw) return null;
  const d = date.getBoundingClientRect();
  const s = sw.getBoundingClientRect();
  return { dh: d.height, sh: s.height, db: d.bottom, sb: s.bottom, dt: d.top, st: s.top };
});
check(
  "all-day switch matches input height/alignment",
  align && Math.abs(align.dh - align.sh) <= 1 && Math.abs(align.db - align.sb) <= 2,
  JSON.stringify(align)
);
await shot("editor-en");
await page.click('.modal button:has-text("Cancel")');
await page.waitForSelector(".modal", { state: "detached" });

// ---------- calendar menu (sharing/color) ----------
await page.locator(".cal-item").first().locator(".cal-menu-btn").click();
await page.waitForSelector(".modal-small");
await shot("calendar-menu-en");
await page.click('.modal-small button:has-text("Cancel")');

// ---------- day popover ----------
const day10 = page
  .locator(".day")
  .filter({ has: page.locator(".day-num", { hasText: /^10$/ }) })
  .first();
await day10.locator(".day-num").click();
await page.waitForSelector(".popover", { timeout: 10000 });
await page.waitForTimeout(300);
const popBox = await page.locator(".popover").boundingBox();
const vp = page.viewportSize();
check(
  "day popover inside viewport",
  popBox && popBox.x >= 0 && popBox.y >= 0 && popBox.x + popBox.width <= vp.width + 1 && popBox.y + popBox.height <= vp.height + 1,
  JSON.stringify(popBox)
);
await shot("day-popover-en");
await page.click(".popover-backdrop", { position: { x: 5, y: 5 } });

// ---------- slot popover (week) ----------
await page.click('.segmented-btn:has-text("Week")');
await page.waitForSelector(".week-view", { timeout: 10000 });
await page.waitForTimeout(600);
await page.locator(".day-column").nth(2).click({ position: { x: 20, y: 14 * 42 + 5 } });
await page.waitForSelector(".popover", { timeout: 10000 });
await page.waitForTimeout(300);
await shot("slot-popover-en");
await page.click(".popover-backdrop", { position: { x: 5, y: 5 } });

// ---------- search ----------
await page.fill(".search-box input", "review");
await page.waitForSelector(".search-panel", { timeout: 10000 });
await page.waitForTimeout(300);
await shot("search-en");
await page.fill(".search-box input", "");
await page.waitForTimeout(300);

// ---------- dark theme ----------
await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid", { timeout: 10000 });
await page.click('.prefs-row button:has-text("Dark")');
await page.waitForTimeout(500);
await shot("month-dark");
await page.click('.prefs-row button:has-text("Light")');
await page.waitForTimeout(300);

// ---------- German ----------
await page.click('.prefs-row button:has-text("EN")');
await page.waitForTimeout(500);
await shot("month-de");
await page.locator(".chip", { hasText: "Lunch with Sam" }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await shot("details-de");
await scanDe(".details-modal", "event details");
await page.click('.details-modal button:has-text("Bearbeiten")').catch(async () => {
  await page.click('.details-modal button:has-text("Edit")');
});
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
await shot("editor-de");
await scanDe('.modal:not(.details-modal)', "event editor");
await page.click('.modal button:has-text("Abbrechen")');
await page.waitForSelector(".modal", { state: "detached" });
await page.locator(".cal-item").first().locator(".cal-menu-btn").click();
await page.waitForSelector(".modal-small");
await scanDe(".modal-small", "calendar menu");
await page.click('.modal-small button:has-text("Abbrechen")');
await page.waitForSelector(".modal-small", { state: "detached" });
await page.locator(".day").filter({ has: page.locator(".day-num", { hasText: /^10$/ }) }).first().locator(".day-num").click();
await page.waitForSelector(".popover", { timeout: 10000 });
await scanDe(".popover", "day popover");
await page.click(".popover-backdrop", { position: { x: 5, y: 5 } });
await page.click('.segmented-btn:has-text("Aufgaben")');
await page.waitForSelector(".tasks-view", { timeout: 10000 });
await page.waitForTimeout(500);
await shot("tasks-de");

// translation scan on German chrome
const deText = await page.evaluate(() => {
  const parts = [".toolbar", ".sidebar", ".tasks-view", ".agenda", ".week-view"];
  return parts
    .map((sel) => document.querySelector(sel)?.innerText ?? "")
    .join("\n");
});
const englishWords = [
  "New event", "New task", "New calendar", "Save", "Cancel", "Delete", "Today",
  "Month", "Week", "Day", "Tasks", "Search events", "Calendar server", "Username",
  "Password", "Log in", "Log out", "Dark", "Light", "All day", "Location", "Notes",
  "Repeat", "Every", "Ends", "Never", "Until", "Apply to", "This event",
  "All events", "Close", "Edit event", "Sharing", "Copy", "Your access",
  "No events", "More events", "Read-only", "No date", "selected", "pending",
];
const found = englishWords.filter((w) => deText.includes(w));
check("German chrome has no English UI labels", found.length === 0, found.join(", "));

// ---------- mobile layout ----------
await page.setViewportSize({ width: 390, height: 844 });
await page.click('.segmented-btn:has-text("Monat")');
await page.waitForSelector(".month-grid", { timeout: 10000 });
await page.waitForTimeout(600);
await shot("month-mobile", { fullPage: true });
const mobileOverflow = await page.evaluate(() => ({
  sw: document.documentElement.scrollWidth,
  iw: window.innerWidth,
}));
check(
  "mobile page has no horizontal overflow",
  mobileOverflow.sw <= mobileOverflow.iw + 1,
  JSON.stringify(mobileOverflow)
);
await page.locator(".day").filter({ has: page.locator(".day-num", { hasText: /^10$/ }) }).first().locator(".day-num").click();
await page.waitForSelector(".popover", { timeout: 10000 });
await page.waitForTimeout(300);
await shot("day-popover-mobile");
const mobilePop = await page.locator(".popover").boundingBox();
check(
  "mobile popover inside viewport",
  mobilePop && mobilePop.x >= 0 && mobilePop.y >= 0 && mobilePop.x + mobilePop.width <= 391 && mobilePop.y + mobilePop.height <= 845,
  JSON.stringify(mobilePop)
);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nAUDIT PASSED" : `\n${failures} AUDIT CHECK(S) FAILED`);
console.log("screenshots:", OUT);
process.exit(failures === 0 ? 0 : 1);
