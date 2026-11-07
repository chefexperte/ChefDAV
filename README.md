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

> **Honest snapshot (2026-11-07).** The core vertical slice works against a
> real Radicale. Everything listed under "Not yet" is planned, not broken.

### ✅ Works today

- **Login** with Basic auth through the CORS proxy (LDAP is enforced by
  Radicale; the SPA doesn't care which auth backend is used).
- **Credential storage**: server/username/password are saved in IndexedDB
  (plain for now; the AES-GCM passphrase flow is kept in `caldav::crypto` for later).
- **Discovery**: `current-user-principal` → `calendar-home-set` → calendar list,
  including display names and Apple calendar colors.
- **Four views** switchable from the toolbar: **Month** (grid), **Week**
  (7-day timeline), **Day** (1-day timeline), **Agenda** (30-day list).
- **Month view**: Monday-first grid, today highlight, per-day event chips,
  multi-day events on every day they span.
- **Week/day timeline**: all-day row, 24-hour grid, overlapping events split
  into columns.
- **Create calendars** from the sidebar (`MKCALENDAR`).
- **Recurring events**: RRULE/RDATE/EXDATE expanded **client-side** in the
  event's own timezone (DST-correct).
- **Event CRUD**: create, edit (optimistic concurrency via `If-Match`), delete.
- **Local cache**: calendars + events cached in IndexedDB, painted before the
  network refresh; calendar visibility toggles.
- **Dev stack**: Radicale + Caddy (CORS) via Docker **or rootless Podman**;
  optional glauth LDAP fixture (verified: LDAP login + full e2e suite).
- **Tests**: 29 native unit tests for the CalDAV/ICS/recurrence/crypto core +
  a committed Playwright smoke test (`e2e/`) covering the full CRUD flow.

### 🚧 Not yet implemented

| Feature | Milestone |
| --- | --- |
| Credentials stored unencrypted for now (passphrase flow removed) | M2 follow-up |
| Week/day view: click a time slot to create an event at that time | M2 follow-up |
| Week/day view: click the all-day row to create an all-day event | M2 follow-up |
| Day click opens a small "New event / New task" popup instead of the editor | M2 follow-up |
| Week view: readable layout for 3+ concurrent events (cap columns / "+N more") | M2 follow-up |
| Rename / delete calendars from the UI (creation works) | M2 follow-up |
| Search, drag & drop, multi-select | M4 |
| VTODO (tasks) | M4 |
| Recurrence **editor** (expansion already works) | M3 |
| Read-only calendar detection via `current-user-privilege-set` (forbidden writes currently just return 403) | M4 |
| Incremental sync via `sync-token` (currently re-fetches the visible range) | M4 |
| Offline write queue (cache is read-through only) | later |
| CI (the e2e test is committed and runnable, just not automated) | M1 follow-up |

**Known limitation right now:** no search, no drag & drop, no task support, and
recurring events can be displayed but not edited as a series. Daily planning
works; AgenDAV parity is not complete yet.

## Roadmap

- [x] **M1 — vertical slice**: login (stored creds), calendar discovery,
      month view, event create/edit/delete over CalDAV, local cache.
- [x] **M2 — usable daily driver**: week/day/agenda views, calendar colors +
      show/hide, create calendars, empty states.
- [ ] **M2 follow-ups**: rename/delete calendars; click a week/day time slot to
      create an event there; day click opens a small "New event / New task"
      popup; readable layout for concurrent events in the week view.
- [ ] **M3 — recurrence**: full client-side recurrence display (mostly done)
      plus a recurrence editor (RRULE builder, this/this-and-future/all).
- [ ] **M4 — parity & polish**: search, drag & drop, multi-select, VTODO,
      `sync-token` incremental sync, read-only detection, keyboard shortcuts,
      responsive/mobile layout.
- [ ] **Later**: offline write queue, reminders/notifications, i18n, themes.

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
