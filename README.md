# ChefDAV

A **client-side-only calendar web app written in Rust, compiled to WebAssembly**,
talking directly to a **CalDAV server (Radicale)**. No app backend: the only
server-side pieces you operate are Radicale itself plus a tiny reverse proxy
that adds CORS headers.

Inspired by [AgenDAV](https://github.com/agendav/agendav) (PHP) — same idea,
modern look, Rust reliability.

```
┌───────────────────────────────┐     fetch: PROPFIND / REPORT / PUT / MKCALENDAR …
│  Browser: Rust → WASM SPA     │     (XML bodies, Basic auth header)
│  (Leptos, this repo)          │ ──▶ requires CORS headers
└───────────────────────────────┘     ▼
                              ┌──────────────┐
                              │ Caddy/nginx  │  adds Access-Control-*, answers OPTIONS
                              └──────────────┘
                                     ▼
                              ┌──────────────┐
                              │   Radicale   │  LDAP auth + file-based rights (ACLs)
                              └──────────────┘
```

## Current status

> **Honest snapshot (2026-11-07).** Everything listed under "Works today" is
> implemented and exercised against a real Radicale. The short "Intentionally
> deferred" list covers items that need a server-side feature or an explicit
> user decision.

### ✅ Works today

- **Login** with Basic auth through the CORS proxy (LDAP is enforced by
  Radicale; the SPA doesn't care which auth backend is used). Credentials are
  saved in IndexedDB (plain for now; the AES-GCM flow is kept in
  `caldav::crypto` for later).
- **Discovery**: `current-user-principal` → `calendar-home-set` → calendars,
  display names, Apple colors and per-calendar privileges.
- **Five views**: Month (grid), Week (7-day timeline), Day (1-day timeline),
  Agenda (30-day list), Tasks.
- **Month view**: Monday-first grid, today highlight, **equal-height cells**
  (max 3 chips + "+N more"), multi-day events on every day they span, drag &
  drop between days (non-recurring events). Clicking a day opens a **popover
  with all of that day's events** (click → details) plus create actions.
- **Week/day timeline**: all-day row and time-slot **popovers** listing the
  events there (click → details) with a prefilled "New event" action;
  overlapping events are capped at two columns with a readable "+N more"
  popup.
- **Day click popup**: choose "New event", "New task" or "Go to day" instead
  of jumping straight into the editor.
- **Event details first**: clicking an event opens a read-only details card
  (time, calendar, location, notes, recurrence); the pencil button opens the
  editor. Read-only calendars show a lock and no edit button.
- **Event CRUD**: create, edit (optimistic concurrency via `If-Match`), delete.
- **Recurrence**: RRULE/RDATE/EXDATE expanded client-side in the event's own
  timezone (DST-correct), an editor for daily/weekly/monthly/yearly rules
  (interval, weekdays, count/until, raw RRULE fallback), and **RECURRENCE-ID
  exceptions**: edit/cancel *this event*, *this and following*, or *all events*.
- **Search** across the cached calendar, **multi-select + bulk delete**.
- **Calendars**: create, rename, delete, show/hide, **change color** (palette
  swatches or a custom picker, stored via PROPPATCH), read-only detection via
  `current-user-privilege-set`, and a **sharing info panel** (calendar URL,
  your privileges, and guidance for Radicale's server-side `[rights]`).
- **Tasks (VTODO)**: create, edit, complete/uncomplete, delete.
- **Incremental sync** via `sync-token` + `calendar-multiget`, with a full
  fetch fallback; **offline outbox** retries queued writes when back online.
- **Local cache** in IndexedDB for instant paint (calendars, events, tasks).
- **Polish**: mint design tokens + dark theme, **full English/German UI**
  (labels, dates, month/weekday names, recurrence descriptions), switch-style
  toggles matching the input height, keyboard shortcuts
  (`n t m w d a k / arrows Esc`), responsive/mobile layout, and a changelog on
  the login screen.
- **Reminders**: page notifications plus a **service worker** that schedules
  upcoming reminders; true delivery while the browser is fully closed would
  require Web Push and a push server.
- **Stable instance** (`./scripts/stable.sh`, ports 3200/8081, separate
  Radicale volume) built from the `stable` git tag.
- **Dev stack**: Radicale + Caddy (CORS) via Docker **or rootless Podman**;
  optional glauth LDAP fixture (verified: LDAP login + e2e).
- **Tests/CI**: 58 native unit tests for the CalDAV/ICS/recurrence/crypto core,
  four committed Playwright suites (`smoke.mjs`, `features.mjs`,
  `recurrence.mjs`, `visibility.mjs`), a screenshot/layout/translation audit
  (`audit.mjs`), and a GitHub Actions workflow (unit tests + WASM build + e2e).

### 🚧 Intentionally deferred

| Item | Why |
| --- | --- |
| Encrypted credentials at rest | **Explicit user request** to drop the passphrase/AES-GCM flow for now. The code remains in `caldav::crypto` and can be wired back in later. |
| True push reminders while the browser is fully closed | A client-only app has no push server; the service worker schedules reminders while the browser keeps it alive. |

**Known limitation:** calendar sharing is managed by Radicale's server-side
`[rights]` file (the app shows the calendar URL, your privileges and guidance);
recurrence edits are applied per occurrence/series as chosen in the scope
dialog. Everything else in the list above is usable day-to-day.

## Roadmap

- [x] **M1 — vertical slice**: login (stored creds), calendar discovery,
      month view, event create/edit/delete over CalDAV, local cache.
- [x] **M2 — usable daily driver**: week/day/agenda views, calendar colors +
      show/hide, create/rename/delete calendars, day popup, time-slot and
      all-day creation, concurrent-event "+N more", details-first event view.
- [x] **M3 — recurrence**: client-side recurrence display plus a simple RRULE
      editor (series-wide).
- [x] **M4 — parity & polish**: search, drag & drop, multi-select, VTODO,
      `sync-token` incremental sync, read-only detection, keyboard shortcuts,
      responsive/mobile layout.
- [x] **Later**: offline write queue, reminders, i18n (main UI), themes,
      stable instance, CI.
- [x] **Sharing info UI**, **service-worker reminders**, **full EN/DE i18n**.
- [x] **Polish round**: equal-sized month cells + "+N more", day and
      time-slot popovers, switch-style toggles, mobile overflow fix,
      localized dates/modals, screenshots + layout/translation audit.
- [ ] **Intentionally deferred (user request)**: encrypted credentials at rest.

## Quickstart (local dev)

Prerequisites: Rust toolchain with the `wasm32-unknown-unknown` target,
[trunk](https://trunkrs.dev) (`cargo install trunk` or a release binary), and a
container runtime.

### Containers: Podman (recommended here) or Docker

Both work. On this machine the Docker socket is root-only, so **rootless
Podman** is the path of least resistance:

```sh
# start the rootless Podman API socket once per login session
systemctl --user start podman.socket

cd docker
podman compose -f docker-compose.yml up -d
```

If your user is in the `docker` group (or you run Docker rootful), the same
stack works with:

```sh
cd docker
docker compose -f docker-compose.yml up -d
```

The stack starts:

| Service | Address | Purpose |
| --- | --- | --- |
| Caddy | `http://127.0.0.1:8080` | CalDAV endpoint with CORS headers |
| Radicale | internal `:5232` | CalDAV server, htpasswd auth |
| glauth (optional) | internal `:389` | LDAP fixture for the LDAP variant |

Dev login: **`alice` / `secret`**.

### App

```sh
# from the repo root
trunk serve
# open http://127.0.0.1:3100
```

Log in with server `http://127.0.0.1:8080`, user `alice`, password `secret`,
and click "Log in".

> If you have no calendar yet, create one once with curl (the UI can't do it
> until M2):
> ```sh
> curl -u alice:secret -X MKCALENDAR \
>   --data '<?xml version="1.0"?><D:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:set><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname>Personal</D:displayname></D:prop></D:set></D:mkcalendar>' \
>   http://127.0.0.1:8080/alice/personal/
> ```

### LDAP variant

```sh
cd docker
podman compose -f docker-compose.yml -f docker-compose.ldap.yml up -d
# logs in via the glauth fixture: alice / secret  (see docker/ldap/)
```

> Status: verified. LDAP login (`alice/secret` → 207, wrong password → 401)
> and the full Playwright e2e suite pass against this stack.
> Note: Radicale 3.9 uses `ldap_uri` / `ldap_reader_dn` / `ldap_secret` and a
> `{0}` placeholder in `ldap_filter` — older `ldap_url`/`ldap_bind_dn` names
> are rejected.

## Stable instance (for day-to-day use)

Development builds and tests change constantly. For a stable copy that is
unaffected by that work:

```sh
./scripts/stable.sh
```

| What | Where |
| --- | --- |
| App | <http://127.0.0.1:3200> |
| CalDAV server | <http://127.0.0.1:8081> |
| Login | `alice` / `secret` |

How it stays stable:

- It builds from the **`stable` git tag** in an isolated worktree
  (`.stable-worktree/`), not from the working tree.
- It runs its **own Radicale + Caddy stack** (`docker/docker-compose.stable.yml`)
  with a separate data volume, on ports 8081/3200. Your calendars there are
  never touched by development or the e2e tests.
- The login screen shows the changelog baked into that build (`CHANGELOG.md`).

After a verified milestone the maintainer moves the `stable` tag forward; re-run
`./scripts/stable.sh` to rebuild and restart the stable app. The containers keep
running when you stop the web server with Ctrl+C.

The stable build defaults the login form to `http://127.0.0.1:8081` (compiled in
via `CHEFDAV_DAV_URL`), creates a starter `Personal` calendar on first run, and
shows its `CHANGELOG.md` section on the login screen. Development and e2e tests
use ports 3100/8080 and a different Radicale volume, so they never touch it.

## Architecture

| Layer | Tech | Notes |
| --- | --- | --- |
| UI | [Leptos](https://leptos.dev) 0.8 (CSR) → WASM | fine-grained signals; hand-rolled month grid |
| CalDAV/ICS core | `crates/caldav` (pure Rust) | XML via roxmltree, own ICS parser, `rrule` for recurrence — **all unit-tested natively** (`cargo test -p caldav`) |
| HTTP | reqwest (wasm → `fetch`) | arbitrary methods/headers for PROPFIND/REPORT/PUT |
| Recurrence | `rrule` + `chrono-tz` | expanded client-side in the event's own TZID → DST-correct |
| Storage | IndexedDB (`idb`) | event/calendar cache for instant paint; credentials stored plain for now (`caldav::crypto` is ready for AES-GCM later) |
| Auth | Basic auth per request | *who may log in* = Radicale `[auth] type = ldap`; *who may write what* = Radicale `[rights]` |
| Build | trunk | `dist/` is a plain static site |

The CalDAV client is generic over a tiny `HttpTransport` trait, so the exact
same protocol code runs against the browser's `fetch` and against mock
transports in tests.

### Project layout

```
ChefDAV/
├── AGENTS.md             # goal, decisions, status, pitfalls (read this first)
├── Cargo.toml            # the WASM app (root package) + workspace
├── index.html  style/  public/  Trunk.toml
├── src/
│   ├── app.rs            # router
│   ├── api.rs            # fetch transport + sync operations
│   ├── browser.rs        # wasm glue: CSPRNG, clock, browser timezone
│   ├── state.rs          # Leptos signals (session, calendars, events, …)
│   ├── store.rs          # IndexedDB (creds + cache + outbox)
│   ├── views/            # login, shell, month
│   └── components/       # event modal
├── crates/caldav/        # pure-Rust, native-testable core
│   └── src/{client,xml,ics,recurrence,time,crypto,transport}.rs
└── docker/               # Radicale + Caddy (CORS) + optional glauth LDAP
```

## Design

Warm paper background, ink text, a single **mint** accent (`#2f9e7f`),
Fraunces for display type, Inter for UI, soft rounded cards. The full palette
(brand, surfaces, calendar colors, dark theme) lives in **`style/tokens.css`**
and is reused everywhere via CSS variables; the Rust fallback palette mirrors
it in `src/state.rs`.

## Production notes

- Serve the built SPA (`trunk build --release`, output in `dist/`) from any
  static host. The release bundle is ~2.7 MB wasm (~853 KB gzipped, mostly the
  timezone database) plus ~10 KB JS and ~3 KB CSS gzipped.
- Pin `Access-Control-Allow-Origin` to your app origin in the Caddyfile
  (the dev one echoes the request origin).
- A single-threaded WASM build needs no COOP/COEP headers (no shared memory).
- If you ever point ChefDAV at a hosted CalDAV you don't control and can't get
  CORS headers, the fallback is a tiny Rust (axum) proxy that adds CORS.
- The native binary (`cargo run --release`) is an optional static file server
  for `dist/`; it is not needed in production.

## Testing

```sh
cargo test -p caldav                            # protocol core (native unit tests)
cargo check --target wasm32-unknown-unknown     # type-check the WASM app
trunk build                                     # full frontend build
# optional production bundle:
trunk build --release
```

## Licensing / credits

See [NOTICE.md](NOTICE.md). ChefDAV itself is MIT OR Apache-2.0. AgenDAV is
AGPL-3.0 — we take inspiration from it, and if any of its code is ever ported
verbatim, this project must be relicensed AGPL-3.0 accordingly.
