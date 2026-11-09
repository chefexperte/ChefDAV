// Regression test: toggling a calendar's visibility must hide/show its events
// even after a calendar-list refresh (which used to replace the visibility
// signal while the sidebar row kept the old one).
//
// Run: cd e2e && node visibility.mjs

import { chromium } from "playwright";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";
const run = Date.now().toString(36);
const EVENT = `Visibility ${run}`;
const DUMMY = `Dummy ${run}`;

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
await page.waitForSelector(".cal-item", { timeout: 15000 });
await page.waitForTimeout(1200);

const chips = () => page.$$eval(".chip .chip-title", (els) => els.map((e) => e.textContent.trim()));
const personal = page.locator(".cal-item", { hasText: "Personal" });

// 1. create an event in Personal (Nov 21 = Saturday, no seed/test events there,
//    so the chip is never pushed behind the cell's "Show all" overflow)
await page.click("button.new-event");
await page.waitForSelector(".modal");
await page.fill('.modal input[type="text"]', EVENT);
await page.fill('.modal input[type="date"]', "2026-11-21");
await page.click('.modal button:has-text("Save")');
await page.waitForSelector(".modal", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("event created", (await chips()).includes(EVENT));

// 2. create a dummy calendar -> forces a calendar-list refresh
await page.click('.btn-icon-plain[title="New calendar"]');
await page.waitForSelector(".modal-small");
await page.fill('.modal-small input[type="text"]', DUMMY);
await page.click('.modal-small button:has-text("Create")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
check("dummy calendar created", (await page.locator(".cal-name", { hasText: DUMMY }).count()) === 1);

// 3. toggle Personal OFF after the refresh -> its event must disappear
await personal.locator('input[type="checkbox"]').uncheck();
await page.waitForTimeout(1800);
check("unchecking hides events after a calendar refresh", !(await chips()).includes(EVENT));

// 4. toggle back ON (wait for the chip, not a fixed sleep — the refresh after
// toggling is async and must not race the assertion)
await personal.locator('input[type="checkbox"]').check();
let recheckOk = false;
try {
  await page.waitForFunction(
    (t) => [...document.querySelectorAll(".chip .chip-title")].some((c) => c.textContent.trim() === t),
    EVENT,
    { timeout: 10000 }
  );
  recheckOk = true;
} catch {}
check("rechecking shows events again", recheckOk, (await chips()).join(", "));

// 5. rename the dummy calendar (another refresh), then toggle Personal again
await page.locator(".cal-item", { hasText: DUMMY }).locator(".cal-menu-btn").click();
await page.waitForSelector(".modal-small");
await page.fill('.modal-small input[type="text"]', `${DUMMY} X`);
await page.click('.modal-small button:has-text("Save")');
await page.waitForSelector(".modal-small", { state: "detached", timeout: 15000 });
await page.waitForTimeout(1200);
await personal.locator('input[type="checkbox"]').uncheck();
await page.waitForTimeout(1800);
check("unchecking still works after a rename refresh", !(await chips()).includes(EVENT));
await personal.locator('input[type="checkbox"]').check();
await page.waitForTimeout(1200);

// cleanup: delete the event and the dummy calendar directly via CalDAV, so a
// failed run cannot leave leftovers that hide later runs' chips behind the
// month cell's "Show all" overflow
async function cleanupViaDav() {
  const auth = "Basic " + Buffer.from(`${USER}:${PASS}`).toString("base64");
  for (const cal of ["personal", "Arbeit"]) {
    const pf = await fetch(`${DAV}/alice/${cal}/`, {
      method: "PROPFIND",
      headers: { Authorization: auth, Depth: "1" },
    });
    const xml = await pf.text();
    const hrefs = [...xml.matchAll(/href>([^<]*\.ics)/g)].map((m) => m[1]);
    for (const h of hrefs) {
      const ics = await (await fetch(`${DAV}${h}`, { headers: { Authorization: auth } })).text();
      if (ics.includes(run)) {
        await fetch(`${DAV}${h}`, { method: "DELETE", headers: { Authorization: auth } });
      }
    }
  }
  // remove the dummy calendar itself (created/renamed during the test)
  for (const cal of [DUMMY, `${DUMMY} X`]) {
    await fetch(`${DAV}/alice/${cal}/`, { method: "DELETE", headers: { Authorization: auth } });
  }
}
await cleanupViaDav();

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL VISIBILITY CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
