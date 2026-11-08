// Session persistence regression: auto-login, logout, opt-out, and the
// login-page theme/language controls.
//
// Prerequisites: same as smoke.mjs (dev stack + app at APP_URL).
// Run: cd e2e && node session.mjs

import { chromium } from "playwright";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
}

const browser = await chromium.launch();
const context = await browser.newContext();
const page = await context.newPage();
const errors = [];
page.on("console", (m) => {
  if (m.type() === "error" && !m.text().includes("401 (Unauthorized)")) errors.push(m.text());
});
page.on("pageerror", (e) => errors.push(String(e)));

async function loginPage() {
  await page.waitForSelector(".login-card form", { timeout: 20000 });
}

async function login() {
  await page.fill('input[type="url"]', DAV);
  await page.fill('input[autocomplete="username"]', USER);
  await page.fill('input[autocomplete="current-password"]', PASS);
  await page.click('button[type="submit"]');
  await page.waitForSelector(".month-grid", { timeout: 20000 });
}

await page.goto(APP, { waitUntil: "networkidle" });
await loginPage();

// ---------- login page controls ----------
check(
  "login page has theme + language controls",
  (await page.locator(".login-prefs .prefs-row button").count()) === 2
);
check(
  "stay logged in is on by default",
  await page.isChecked('.check-row input[type="checkbox"]')
);

await page.click('.login-prefs .prefs-row button:has-text("Dark")');
const dark = await page.evaluate(() => document.documentElement.getAttribute("data-theme"));
check("login page dark theme applies", dark === "dark", dark ?? "(none)");
await page.click('.login-prefs .prefs-row button:has-text("Light")');

await page.click('.login-prefs .prefs-row button:has-text("EN")');
const deSubmit = (await page.textContent('button[type="submit"]')).trim();
check("login page switches to German", deSubmit === "Anmelden", deSubmit);
await page.click('.login-prefs .prefs-row button:has-text("DE")');
const enSubmit = (await page.textContent('button[type="submit"]')).trim();
check("login page switches back to English", enSubmit === "Log in", enSubmit);

// ---------- auto-login ----------
await login();
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
check("reload auto-logs in", (await page.locator(".login-card form").count()) === 0);

// theme + language persist across the auto-login
await page.click('.prefs-row button:has-text("Dark")');
await page.click('.prefs-row button:has-text("EN")');
await page.waitForTimeout(600);
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
const themeAfter = await page.evaluate(() => document.documentElement.getAttribute("data-theme"));
const toolbar = await page.textContent(".toolbar");
check(
  "theme + language persist across reload",
  themeAfter === "dark" && toolbar.includes("Monat"),
  `${themeAfter} / ${toolbar.slice(0, 40)}`
);
// ---------- logout ----------
await page.click('button:has-text("Abmelden")');
await loginPage();
const user = await page.inputValue('input[autocomplete="username"]');
const pass = await page.inputValue('input[autocomplete="current-password"]');
check(
  "logout clears the saved password",
  user === USER && pass === "",
  `${user} / ${pass ? "***" : "(empty)"}`
);

await page.reload({ waitUntil: "networkidle" });
await loginPage();
check("reload after logout stays logged out", (await page.locator(".month-grid").count()) === 0);

// ---------- opt out, then back in ----------
await page.uncheck('.check-row input[type="checkbox"]');
await login();
await page.reload({ waitUntil: "networkidle" });
await loginPage();
check(
  "unchecked 'stay logged in' does not auto-login",
  (await page.locator(".month-grid").count()) === 0
);

await page.check('.check-row input[type="checkbox"]');
await login();
await page.reload({ waitUntil: "networkidle" });
await page.waitForSelector(".month-grid", { timeout: 20000 });
check(
  "re-enabling 'stay logged in' restores auto-login",
  (await page.locator(".login-card form").count()) === 0
);

check("no console errors", errors.length === 0, errors.join(" | "));
await browser.close();
console.log(failures === 0 ? "\nALL SESSION CHECKS PASSED" : `\n${failures} SESSION CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
