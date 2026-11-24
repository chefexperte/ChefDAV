// Week/day timeline start position: normally 06:00; before 06:00 on a day
// that is visible in the range it starts at midnight instead.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node timeline.mjs

import { launchPinned } from "./clock.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const { browser, page } = await launchPinned();
const errors = [];
page.on("console", (m) => {
  if (m.type() === "error" && !m.text().includes("net::ERR_FAILED")) errors.push(m.text());
});
page.on("pageerror", (e) => errors.push(String(e)));

async function login() {
  await page.goto(APP, { waitUntil: "networkidle" });
  await page.waitForSelector(".login-card form", { timeout: 20000 });
  await page.fill('input[type="url"]', DAV);
  await page.fill('input[autocomplete="username"]', USER);
  await page.fill('input[autocomplete="current-password"]', PASS);
  await page.click('button[type="submit"]');
  await page.waitForSelector(".month-grid", { timeout: 20000 });
}

async function weekScroll() {
  await page.waitForSelector(".week-view", { timeout: 10000 });
  await page.waitForTimeout(200);
  return page.evaluate(() => window.scrollY);
}

await login();

// Noon on the pinned day: the week contains today and it is after 06:00.
await page.click('.segmented-btn:has-text("Week")');
const noonScroll = await weekScroll();
check("week starts at 06:00 after the morning", noonScroll > 250, `${noonScroll}px`);

// Same day at 04:30: do not jump to 06:00.
await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid", { timeout: 10000 });
await page.evaluate(() => window.scrollTo(0, 0));
await page.clock.setFixedTime(new Date(2026, 10, 9, 4, 30, 0));
await page.click('.segmented-btn:has-text("Week")');
const earlyScroll = await weekScroll();
check("today before 06:00 starts at midnight", earlyScroll < 250, `${earlyScroll}px`);

// A different week at 04:30 no longer contains today, so 06:00 is used.
await page.click('button[title="Next"]');
const otherWeekScroll = await weekScroll();
check("a week without today still starts at 06:00", otherWeekScroll > 250, `${otherWeekScroll}px`);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(
  failures === 0 ? "\nALL TIMELINE CHECKS PASSED" : `\n${failures} TIMELINE CHECK(S) FAILED`
);
process.exit(failures === 0 ? 0 : 1);
