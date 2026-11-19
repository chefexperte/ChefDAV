# ChefDAV end-to-end tests

Playwright suites that drive the real app against the real dev stack:

- `smoke.mjs`: login, calendar list, create, edit, delete, reload with a saved
  session, logout, and a wrong password.
- `features.mjs`: popups, search, multi-select, tasks, recurrence, calendar
  management, theme and language, keyboard shortcuts, offline outbox.
- `recurrence.mjs`: RECURRENCE-ID exceptions (this event, this and following,
  cancellation).
- `visibility.mjs`: show/hide a calendar, then reload the calendar list.
- `fixed-backend.mjs`: with `BACKEND_URL` set the login screen has no server
  field; without it the field is back. The configured case intercepts
  `/config.json`, so no files are written.
- `session.mjs`: auto-login, logout, opting out, and the theme/language
  controls on the login page.
- `location.mjs`: OpenStreetMap preview for coordinates and the search fallback
  for plain venue names.
- `audit.mjs`: screenshots plus layout and translation checks. It expects the
  seeded dev data.

## Pinned clock

The suites and the seeded data use November 2026, and the navigation checks
assume "today" is in the week of Mon 2026-11-09 to Sun 2026-11-15. Toolbar
"Next" must reach `/week/2026-11-16`, "Today" must reach `/week/2026-11-09`, and
created events must show up in the current month view.

Every suite pins the browser clock to a fixed time inside that week through
`clock.mjs` (`page.clock.setFixedTime`). The app reads the clock only through
the JS `Date` object, so the pin covers it, while timers keep running in real
time. Use `launchPinned()` (or `pinClock(page)` for your own context) in new
suites instead of `chromium.launch()`, and create test data on fixed November
2026 dates. `node clock-sanity.mjs` checks that the app sees the pin and keeps
it across a reload.

## Running the suites

```sh
# 1. dev stack
systemctl --user start podman.socket
podman compose -f docker/docker-compose.yml up -d
# on a fresh volume: seed the demo calendars and events the suites expect
../scripts/seed-dev.sh
# after an interrupted run: remove test leftovers, keep the seeds
../scripts/clean-dev.sh

# 2. deterministic build and SPA-fallback server
#    (not `trunk serve`, which can serve a half-applied bundle mid-rebuild)
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
node fixed-backend.mjs
node session.mjs
node location.mjs
node links.mjs
node multiday.mjs
node audit.mjs
```

`APP_URL`, `DAV_URL`, `DAV_USER` and `DAV_PASS` override the defaults.

The suites create uniquely named events and delete what they created, so they
can run again against the same server.

**Timezones.** The suites mix Berlin-TZID data (seeds, CalDAV writes) with events
created through the UI. Expectations that compare both have to be computed
in-page from explicit-offset instants (`new Date("2026-11-28T10:00:00+01:00")`),
or they fail on UTC runners. CI runs on UTC and includes smoke, features,
recurrence, visibility, fixed-backend, session, location, links and multiday,
with the seeds. Check timezone-sensitive changes with
`TZ=UTC node features.mjs`.
