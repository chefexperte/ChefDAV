// Sanity check for clock.mjs: the pinned time must be seen by the page's JS
// Date (which the WASM app reads via js_sys::Date::now) and must survive a
// reload/navigation (Playwright injects it as an init script).
import { launchPinned, PINNED_NOW } from "./clock.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const { browser, page } = await launchPinned();
await page.goto(APP, { waitUntil: "networkidle" });

const t1 = await page.evaluate(() => Date.now());
const d1 = await page.evaluate(() => new Date().toString());
await page.reload({ waitUntil: "networkidle" });
const t2 = await page.evaluate(() => Date.now());

// timers must still fire in real time (setFixedTime only fakes Date)
const timerOk = await page.evaluate(
  () => new Promise((res) => { const s = Date.now(); setTimeout(() => res(Date.now() >= s), 150); })
);

const ok1 = Math.abs(t1 - PINNED_NOW.getTime()) < 2000;
const ok2 = Math.abs(t2 - PINNED_NOW.getTime()) < 2000;
console.log("pinned Date.now()        :", ok1 ? "PASS" : "FAIL", new Date(t1).toISOString());
console.log("pinned new Date()        :", d1);
console.log("pin survives reload      :", ok2 ? "PASS" : "FAIL", new Date(t2).toISOString());
console.log("real timers still fire   :", timerOk ? "PASS" : "FAIL");
await browser.close();
process.exit(ok1 && ok2 && timerOk ? 0 : 1);
