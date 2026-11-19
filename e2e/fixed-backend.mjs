// A deployment with BACKEND_URL fixes the server: the login screen must ask
// only for credentials, never for a server URL. The dev bundle ships no
// /config.json, so the configured case is simulated by intercepting that
// request — no files on disk, nothing the other suites can trip over.
import { launchPinned, pinClock } from "./clock.mjs";

const APP = process.env.APP_URL ?? "http://127.0.0.1:3100";
const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";

let failures = 0;
const check = (name, ok, detail = "") => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? ` — ${detail}` : ""}`);
  if (!ok) failures++;
};

const { browser } = await launchPinned();

// ---------- configured deployment (BACKEND_URL set) -------------------------
{
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await pinClock(page);
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));

  await page.route("**/config.json", (route) =>
    route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({ backend_url: DAV }),
    })
  );

  await page.goto(APP + "/", { waitUntil: "networkidle" });
  await page.waitForSelector(".login-card form", { timeout: 20000 });

  check(
    "configured: no server field",
    (await page.locator('.login-card input[type="url"]').count()) === 0
  );
  check(
    "configured: username and password still asked",
    (await page.locator('input[autocomplete="username"]').count()) === 1 &&
      (await page.locator('input[autocomplete="current-password"]').count()) === 1
  );

  // The configured server has to be the one that is actually used.
  await page.fill('input[autocomplete="username"]', "alice");
  await page.fill('input[autocomplete="current-password"]', "secret");
  await page.click('button[type="submit"]');
  const loggedIn = await page
    .waitForSelector(".month-grid", { timeout: 20000 })
    .then(() => true)
    .catch(() => false);
  check("configured: login uses BACKEND_URL", loggedIn);
  check("configured: no page errors", errors.length === 0, errors.slice(0, 2).join(" | "));

  await ctx.close();
}

// ---------- no configuration: the user enters the server --------------------
{
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  await pinClock(page);

  await page.goto(APP + "/", { waitUntil: "networkidle" });
  await page.waitForSelector(".login-card form", { timeout: 20000 });

  const field = page.locator('.login-card input[type="url"]');
  check("unconfigured: server field is shown", (await field.count()) === 1);
  check("unconfigured: server field is editable", await field.isEditable());
  check("unconfigured: server field starts empty", (await field.inputValue()) === "");

  await ctx.close();
}

await browser.close();
console.log(failures === 0 ? "\nALL CHECKS PASSED" : `\n${failures} CHECK(S) FAILED`);
process.exit(failures === 0 ? 0 : 1);
