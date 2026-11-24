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
  return page.evaluate(() => document.querySelector(".main-scroll").scrollTop);
}

await login();

// Noon on the pinned day: the week contains today and it is after 06:00.
await page.click('.segmented-btn:has-text("Week")');
const noonScroll = await weekScroll();
check("week starts at 06:00 after the morning", noonScroll > 200, `${noonScroll}px`);

// Opening an event must not re-run the initial scroll.
await page.locator(".timed-event", { hasText: "Weekly sync" }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
const afterClick = await page.evaluate(
  () => document.querySelector(".main-scroll").scrollTop
);
check(
  "opening an event does not jump the timeline",
  Math.abs(afterClick - noonScroll) <= 1,
  `${noonScroll}px → ${afterClick}px`
);
await page.keyboard.press("Escape");
await page.waitForSelector(".details-modal", { state: "detached", timeout: 10000 });

// Same day at 04:30: do not jump to 06:00.
await page.click('.segmented-btn:has-text("Month")');
await page.waitForSelector(".month-grid", { timeout: 10000 });
await page.evaluate(() => document.querySelector(".main-scroll").scrollTo(0, 0));
await page.clock.setFixedTime(new Date(2026, 10, 9, 4, 30, 0));
await page.click('.segmented-btn:has-text("Week")');
const earlyScroll = await weekScroll();
check("today before 06:00 starts at midnight", earlyScroll < 50, `${earlyScroll}px`);

// A different week at 04:30, loaded fresh, no longer contains today.
await page.goto(new URL("week/2026-11-16", APP).href, { waitUntil: "networkidle" });
const otherWeekScroll = await weekScroll();
check("a week without today still starts at 06:00", otherWeekScroll > 200, `${otherWeekScroll}px`);

// The old window-scroll implementation landed at ~02:45 on a phone; the
// inner scroll container must give the same 06:00 line on a narrow viewport.
await page.clock.setFixedTime(new Date(2026, 10, 9, 12, 0, 0));
await page.setViewportSize({ width: 390, height: 844 });
await page.goto(new URL("week/2026-11-09", APP).href, { waitUntil: "networkidle" });
const mobileScroll = await weekScroll();
check("mobile week also starts at 06:00", mobileScroll > 200, `${mobileScroll}px`);
await page.evaluate(() => document.querySelector(".main-scroll").scrollBy(0, 150));
await page.waitForTimeout(50);
const mobileSticky = await page.evaluate(() => {
  const scroller = document.querySelector(".main-scroll").getBoundingClientRect().top;
  return Math.round(
    document.querySelector(".week-sticky").getBoundingClientRect().top - scroller
  );
});
check("mobile header and all-day row are sticky", Math.abs(mobileSticky) <= 1, `${mobileSticky}px`);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(
  failures === 0 ? "\nALL TIMELINE CHECKS PASSED" : `\n${failures} TIMELINE CHECK(S) FAILED`
);
process.exit(failures === 0 ? 0 : 1);
