# ChefDAV end-to-end tests

Playwright suites that drive the real app against the real dev stack:

- `smoke.mjs` — login → calendar list → create → edit → delete → stay-logged-in
  reload → logout → wrong-password rejection.
- `features.mjs` — popups, search, multi-select, tasks, recurrence, calendar
  management, theme/language, keyboard, offline outbox.
- `recurrence.mjs` — RECURRENCE-ID exceptions (this event / this and following
  / cancellation).
- `visibility.mjs` — calendar show/hide after a calendar-list refresh.
- `session.mjs` — auto-login, logout, opt-out, and the login-page theme/language
  controls.
- `location.mjs` — OpenStreetMap preview for coordinates and the search
  fallback for plain venue names.
- `audit.mjs` — screenshots plus layout/translation checks (expects the seeded
  dev data).

## Pinned clock

The suites and the seeded demo data are pinned to November 2026, and the
nav-based checks assume "today" is in the week of Mon 2026-11-09 – Sun
2026-11-15 (toolbar "Next" must land on `/week/2026-11-16`, "Today" on
`/week/2026-11-09`, created events must show up in the current month view).
To keep that valid forever, every suite pins the browser clock to a fixed
"now" inside that week via `clock.mjs` (`page.clock.setFixedTime` — the app
reads the clock only through the JS `Date` object, and timers keep running in
real time). Use `launchPinned()` (or `pinClock(page)` for your own context)
in new suites instead of `chromium.launch()`, and create test data on fixed
November 2026 dates like the existing ones. `node clock-sanity.mjs` is a
5-second standalone check that the pin is seen by the app and survives a
reload.

## Run

```sh
# 1. dev stack
systemctl --user start podman.socket
podman compose -f docker/docker-compose.yml up -d
# on a fresh volume: seed the demo calendars + events the suites expect
../scripts/seed-dev.sh
# after interrupted/failed runs: remove test leftovers (keeps the seed)
../scripts/clean-dev.sh

# 2. deterministic build + SPA-fallback server (not `trunk serve`: it can
#    serve a half-applied bundle mid-rebuild)
trunk build
python3 scripts/serve.py 3100 dist &

# 3. tests
cd e2e
npm install
npx playwright install chromium   # first time only
npm test
node features.mjs
node recurrence.mjs
node visibility.mjs
node session.mjs
node location.mjs
node links.mjs
node multiday.mjs
node audit.mjs
```

Environment overrides: `APP_URL`, `DAV_URL`, `DAV_USER`, `DAV_PASS`.

The suites create uniquely-named events and delete what they created, so they
can be re-run against the same server.
