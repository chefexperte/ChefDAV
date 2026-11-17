// Shared browser-clock pinning for the e2e suites.
//
// The suites and the seeded dev data (scripts/seed-dev.sh) are pinned to
// November 2026, and the nav-based checks assume "today" lives in the week of
// Mon 2026-11-09 – Sun 2026-11-15 (the week the suites were designed and
// verified around): toolbar "Next" from the default week must land on
// /week/2026-11-16, "Today" on /week/2026-11-09, and events created on fixed
// November dates must show up in the current month view. Once the real date
// moved past that week (2026-11-16+), those checks failed.
//
// Instead of rewriting every date to be relative, the suites pin the browser
// clock to a fixed "now" inside that week. The app reads the clock only via
// the JS Date object (src/browser.rs now_utc -> js_sys::Date::now), so
// page.clock.setFixedTime() covers all of it, while timers (setTimeout,
// rAF, ...) keep running in real time — the offline-outbox retry and the
// modal-animation checks rely on that.
//
// Usage:
//   import { launchPinned } from "./clock.mjs";
//   const { browser, page } = await launchPinned();
//   // ... or, if the suite creates its own context/page:
//   import { pinClock } from "./clock.mjs";
//   await pinClock(page);

import { chromium } from "playwright";

// Mon 2026-11-09, 12:00 — constructed in the runner's *local* timezone (which
// the browser shares), so the in-browser local date is Nov 9 both on the dev
// machine and on a UTC CI runner.
export const PINNED_NOW = new Date(2026, 10, 9, 12, 0, 0);

// Launch a browser + page with the clock pinned. Accepts the same options as
// browser.newPage() (e.g. a viewport).
export async function launchPinned(opts = {}) {
  const browser = await chromium.launch();
  const page = await browser.newPage(opts);
  await page.clock.setFixedTime(PINNED_NOW);
  return { browser, page };
}

// Pin a page the suite created itself (e.g. via browser.newContext()).
export async function pinClock(page) {
  await page.clock.setFixedTime(PINNED_NOW);
}
