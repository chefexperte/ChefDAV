# ChefDAV end-to-end tests

Playwright smoke test that drives the real app against the real dev stack:
login → calendar list → create → edit → delete → stored-credential login →
wrong-password rejection. `features.mjs` covers the newer UI features (popups, search,
multi-select, tasks, recurrence, calendar management, theme/language,
keyboard, offline outbox), and `recurrence.mjs` covers RECURRENCE-ID
exceptions (this event / this and following / cancellation).

## Run

```sh
# 1. dev stack + app
systemctl --user start podman.socket
podman compose -f docker/docker-compose.yml up -d
# a calendar must exist (see README quickstart)
trunk serve   # in the repo root, separate terminal

# 2. test
cd e2e
npm install
npx playwright install chromium   # first time only
npm test
```

Environment overrides: `APP_URL`, `DAV_URL`, `DAV_USER`, `DAV_PASS`.

The test creates uniquely-named events (`E2E <timestamp>`) and deletes what it
created, so it can be re-run against the same server.
