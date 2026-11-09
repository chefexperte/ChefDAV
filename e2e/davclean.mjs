// Shared e2e cleanup: delete every calendar resource whose ICS contains the
// run tag (and any calendar whose name contains it), directly via CalDAV.
// UI-based cleanup is unreliable — a chip hidden behind a cell's "Show all"
// overflow can't be clicked, so interrupted/failed runs leave leftovers that
// pollute later runs. Always clean via DAV instead.
//
// Usage: import { cleanupRun } from "./davclean.mjs";
//        await cleanupRun(run);   // run = Date.now().toString(36)

const DAV = process.env.DAV_URL ?? "http://127.0.0.1:8080";
const USER = process.env.DAV_USER ?? "alice";
const PASS = process.env.DAV_PASS ?? "secret";

export async function cleanupRun(tag, extraCalNames = []) {
  const auth = "Basic " + Buffer.from(`${USER}:${PASS}`).toString("base64");
  const h = { Authorization: auth };

  // discover all calendar collections of the user
  const pf = await fetch(`${DAV}/${USER}/`, { method: "PROPFIND", headers: { ...h, Depth: "1" } });
  const xml = await pf.text();
  const calHrefs = [...xml.matchAll(/<href>(\/[^<]*\/)<\/href>/g)]
    .map((m) => m[1])
    .filter((p) => p !== `/${USER}/` && p !== "/");

  for (const cal of calHrefs) {
    const name = decodeURIComponent(cal.replace(`/${USER}/`, "").replace(/\/$/, ""));
    const isTestCal = extraCalNames.some((n) => name.includes(n)) || name.includes(tag);
    if (isTestCal) {
      await fetch(`${DAV}${cal}`, { method: "DELETE", headers: h });
      continue;
    }
    const list = await fetch(`${DAV}${cal}`, { method: "PROPFIND", headers: { ...h, Depth: "1" } });
    const lxml = await list.text();
    for (const m of lxml.matchAll(/<href>([^<]*\.ics)<\/href>/g)) {
      const ics = await (await fetch(`${DAV}${m[1]}`, { headers: h })).text();
      if (ics.includes(tag)) {
        await fetch(`${DAV}${m[1]}`, { method: "DELETE", headers: h });
      }
    }
  }
}
