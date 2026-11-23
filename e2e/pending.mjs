// Queued offline writes regression: locally created events must survive a
// reload, be inspectable, and be removable one by one from the pending list.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node pending.mjs

import { launchPinned } from "./clock.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";
const run = Date.now().toString(36);
const titleA = `Queued A ${run}`;
const titleB = `Queued B ${run}`;

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const { browser, page } = await launchPinned();
const errors = [];
page.on("console", (m) => {
  if (m.type() === "error" && !m.text().includes("net::ERR_FAILED")) {
    errors.push(m.text());
  }
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

async function createEvent(title, date, start, end) {
  await page.click("button.new-event");
  await page.waitForSelector(".modal");
  await page.fill('.modal input[type="text"]', title);
  await page.fill('.modal input[type="date"]', date);
  await page.fill('.modal input[type="time"] >> nth=0', start);
  await page.fill('.modal input[type="time"] >> nth=1', end);
  await page.click('.modal button:has-text("Save")');
  await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
}

async function hasChip(title) {
  return page.evaluate(
    (t) => [...document.querySelectorAll(".chip .chip-title")].some((el) => el.textContent.trim() === t),
    title
  );
}

async function waitForChip(title, present) {
  await page.waitForFunction(
    ({ title, present }) =>
      [...document.querySelectorAll(".chip .chip-title")].some(
        (el) => el.textContent.trim() === title
      ) === present,
    { title, present },
    { timeout: 20000 }
  );
}

await login();
await page.waitForSelector(".cal-item", { timeout: 15000 });

// Keep reads working; only writes fail, exactly like a CalDAV server that
// rejects PUT while still listing the collection.
await page.route("**/*.ics", (route) => {
  if (route.request().method() === "PUT" || route.request().method() === "DELETE") {
    route.abort();
  } else {
    route.continue();
  }
});

await createEvent(titleA, "2026-11-24", "10:00", "11:00");
await createEvent(titleB, "2026-11-25", "14:00", "15:00");
check(
  "offline creates show the pending badge",
  (await page.locator(".sidebar-footer .pending-badge").count()) === 1
);

// Reload: the writes are still queued, and both events must be painted from
// the outbox on top of the cache/server state.
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
await waitForChip(titleA, true);
await waitForChip(titleB, true);
check("queued events survive a reload", true);

// A queued event is a normal visible event: it can be opened and inspected.
await page.locator(".chip", { hasText: titleA }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
check(
  "queued event details can be opened",
  (await page.textContent(".details-modal")).includes(titleA)
);
await page.keyboard.press("Escape");
await page.waitForSelector(".details-modal", { state: "detached", timeout: 10000 });

// The pending list shows both writes and lets us discard them individually.
await page.click(".sidebar-footer .pending-badge");
await page.waitForSelector(".pending-modal", { timeout: 10000 });
await page.waitForSelector(".pending-item", { timeout: 10000 });
check(
  "pending modal lists both changes",
  (await page.locator(".pending-item").count()) === 2,
  String(await page.locator(".pending-item").count())
);
check(
  "pending row names the event",
  (await page.textContent(".pending-modal")).includes(titleA) &&
    (await page.textContent(".pending-modal")).includes(titleB)
);

await page.locator(".pending-item", { hasText: titleA }).locator(".pending-discard").click();
await page.waitForFunction(
  () => document.querySelectorAll(".pending-item").length === 1,
  null,
  { timeout: 20000 }
);
check("discarding one change keeps the other", (await page.textContent(".pending-modal")).includes(titleB));
await waitForChip(titleA, false);
check("discarded change disappears from the calendar", !(await hasChip(titleA)));

await page.locator(".pending-item", { hasText: titleB }).locator(".pending-discard").click();
await page.waitForSelector(".pending-modal", { state: "detached", timeout: 20000 });
await waitForChip(titleB, false);
check(
  "discarding the last change clears the pending badge",
  (await page.locator(".sidebar-footer .pending-badge").count()) === 0
);

await page.unroute("**/*.ics");
check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL PENDING CHECKS PASSED" : `\n${failures} PENDING CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
