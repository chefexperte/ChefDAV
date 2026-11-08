// Recurrence exception smoke test: RECURRENCE-ID overrides for
// "this event" / "this and following", plus cancellation.
//
// Run: cd e2e && node recurrence.mjs

import { chromium } from "playwright";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";
const run = Date.now().toString(36);
const TITLE = `Series ${run}`;
const EXCEPTION = `Exception ${run}`;
const FUTURE = `Future ${run}`;

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => {
  if (m.type() === "error" && !m.text().includes("401 (Unauthorized)")) errors.push(m.text());
});

await page.goto(APP, { waitUntil: "networkidle" });
await page.waitForSelector(".login-card form", { timeout: 20000 });
await page.fill('input[type="url"]', DAV);
await page.fill('input[autocomplete="username"]', USER);
await page.fill('input[autocomplete="current-password"]', PASS);
await page.click('button[type="submit"]');
await page.waitForSelector(".month-grid", { timeout: 20000 });
await page.waitForTimeout(3500);

const chips = () => page.$$eval(".chip .chip-title", (els) => els.map((e) => e.textContent.trim()));
const chipTime = (title) =>
  page.locator(".chip", { hasText: title }).first().locator(".chip-time").textContent();

// ---------- create a weekly series with 3 occurrences ----------
await page.click("button.new-event");
await page.waitForSelector(".modal");
await page.fill('.modal input[type="text"]', TITLE);
await page.fill('.modal input[type="date"]', "2026-11-15");
await page.selectOption('.modal select >> nth=1', "WEEKLY");
await page.waitForTimeout(150);
await page.selectOption('.modal select >> nth=2', "count");
await page.fill('.modal input[type="number"] >> nth=1', "3");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(3500);
check("series created with 3 occurrences", (await chips()).filter((c) => c === TITLE).length === 3);

// ---------- edit the second occurrence only ----------
await page.locator(".chip", { hasText: TITLE }).nth(1).click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
const scope = page.locator('.modal label:has-text("Apply to") select');
check("scope selector visible", (await scope.count()) === 1);
check("scope defaults to this event", (await scope.inputValue()) === "this");
await page.fill('.modal input[type="text"]', EXCEPTION);
await page.fill('.modal input[type="time"] >> nth=0', "14:00");
await page.fill('.modal input[type="time"] >> nth=1', "15:00");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(3500);
const afterSingle = await chips();
check(
  "single override replaces one occurrence",
  afterSingle.filter((c) => c === TITLE).length === 2 &&
    afterSingle.filter((c) => c === EXCEPTION).length === 1,
  afterSingle.join(", ")
);
check("override has the new time", (await chipTime(EXCEPTION)).trim() === "14:00");

// ---------- edit this and following ----------
await page.locator(".chip", { hasText: EXCEPTION }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
await page.locator('.modal label:has-text("Apply to") select').selectOption("future");
await page.fill('.modal input[type="text"]', FUTURE);
await page.fill('.modal input[type="time"] >> nth=0', "12:00");
await page.fill('.modal input[type="time"] >> nth=1', "13:00");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(3500);
const afterFuture = await chips();
check(
  "this-and-following shifts later occurrences",
  afterFuture.filter((c) => c === TITLE).length === 1 &&
    afterFuture.filter((c) => c === FUTURE).length === 2,
  afterFuture.join(", ")
);
check("future override has the new time", (await chipTime(FUTURE)).trim() === "12:00");

// ---------- cancel one occurrence ----------
await page.locator(".chip", { hasText: FUTURE }).nth(1).click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
await page.locator('.modal label:has-text("Apply to") select').selectOption("this");
await page.click('.modal button:has-text("Delete")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(3500);
const afterCancel = await chips();
check(
  "cancelled occurrence disappears",
  afterCancel.filter((c) => c === FUTURE).length === 1,
  afterCancel.join(", ")
);

// ---------- cleanup: delete the whole series ----------
await page.locator(".chip", { hasText: FUTURE }).first().click();
await page.waitForSelector(".details-modal", { timeout: 10000 });
await page.click('.details-modal button:has-text("Edit")');
await page.waitForSelector('.modal:not(.details-modal)', { timeout: 10000 });
await page.locator('.modal label:has-text("Apply to") select').selectOption("all");
await page.click('.modal button:has-text("Delete")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(3500);
const finalChips = await chips();
check(
  "whole series deleted",
  !finalChips.some((c) => c === TITLE || c === FUTURE || c === EXCEPTION),
  finalChips.join(", ")
);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL RECURRENCE CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
