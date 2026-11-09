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
node audit.mjs
```

Environment overrides: `APP_URL`, `DAV_URL`, `DAV_USER`, `DAV_PASS`.

The suites create uniquely-named events and delete what they created, so they
can be re-run against the same server.
