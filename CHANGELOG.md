# Changelog

All notable, user-visible changes to ChefDAV. The login screen shows the
latest section of this file (baked into each build).

## Unreleased

- **Deep links for every resource**: opening an event or task pushes its own
  URL (`/event/<calendar>/<uid>`, `/task/<calendar>/<uid>`, plus `?occ=…` for
  one occurrence of a recurring event) into the address bar, and the details
  card has a **copy-link** button. Opening such a link — including on a fresh
  load — opens that resource directly (the calendar is found by name, hidden
  calendars are fetched on demand).
- The **Agenda view is now called "List"** (EN) / **"Liste"** (DE).
- The **Copy link button** in the event details card is now an **icon** (link
  glyph; turns into a check mark once the link is in the clipboard).
- In the event details card the **"Location" text label is now a pin icon**.
- Fixed a **horizontal scrollbar in the edit-event popup** (field rows could
  overflow the modal width).
- Fixed the week view **"+N more" badge** being rendered too high instead of in
  its day column.
- The **day view no longer shows the redundant day header** (weekday
  abbreviation + circled number); the toolbar already shows the date.
- **Multi-day events**: all-day and timed events in the editor now have an
  **end date** picker (overnight events like 23:00 → 01:00 next day work). In
  the month view, multi-day and overnight events render as a **bar spanning
  the day cells**; week/day/all-day rows and the list view respect
  continuation, shown as a **striped fade-out** at the continuing edge.
- **Event styling**: event chips/bars are now **solid calendar color** (no more
  pale tint + left border) and a bit bigger; month cells show **two events +
  a "+N more" entry** that looks like an event but in a neutral color (clicking
  it opens the day popover — no more badge). The week-view "+N more" is
  event-like as well. The **Edit button** uses a proper pencil icon.
- **Continuation fades**: events crossing into another day/week fade out with
  a striped dissolve (smooth, rounded) at the continuing edge — horizontal at
  week boundaries (month view renders multi-day events as one spanning bar per
  week row; the week all-day row as one longer bar), vertical for timed events
  clipped at the day top/bottom (also in day view).
- The toolbar **"Today" button is now a home icon button** left of prev/next
  (jumps to the current day; prev/next unchanged).
- Week/day: the **all-day row is a fixed 3-entry height** (up to 3 events; more
  → an event-sized neutral **"Show all"** entry), and each timeline column
  shows **2 slots — on overflow the second slot is a "Show all" entry**
  instead of the "+N more" badge.
- **The URL now reflects the current view and date** — `/month/2026-11`,
  `/week/2026-11-09`, `/day/…`, `/list/…`, `/tasks` — so a copied link restores
  exactly that view (resource links `/event/…` still work and return to the
  view URL on close). Month cells show up to 3 events (more → 2 + "Show all"),
  with more spacing and the same border inset for spanning bars.

## 0.1.1 — 2026-11-08

- README now warns prominently that the project is **written entirely by AI**.

- README rewritten as a proper project readme (what it is, screenshots, setup,
  dev setup, limitations, licensing).

- License clarified: all original code and assets are **MIT** (`LICENSE`,
  `license = "MIT"` in both Cargo.tomls). No AgenDAV code was ever copied;
  dependencies are permissive crates (see NOTICE.md), fonts are OFL and loaded
  at runtime.

- **Stay logged in**: the saved session is restored automatically on reload;
  a "Stay logged in on this device" checkbox (on by default) controls it.
  Logging out deletes the saved password, and the login page keeps the server
  and username for prefilling.
- The login page now has **dark/light and EN/DE controls** and applies the
  saved theme before rendering.
- **OpenStreetMap preview** in event details: coordinates show a pinned map
  (click opens OSM in a new tab); address-like locations are geocoded with
  Photon, and other names get an OSM search link.
- Agenda view shows **start–end times** (e.g. `12:30–13:30`).
- Week/day: short events (15–45 min) use a **compact one-line layout** so the
  title is no longer clipped.
- The search results panel is now anchored **directly under the search box**.
- Removed the non-functional reminders bell and the reminder service worker.
- New **logo**: pastel mint parallelogram with rounded corners and an abstract deep-green ring-and-dot mark (no rendered text).

## 0.1.0 — 2026-11-07

- Month, week, day, agenda and tasks views with a shared toolbar.
- Equal-height month cells (max three chips + "+N more"), a day popover with
  the day's events, and time-slot/all-day popovers with events + prefilled
  New event.
- Click an event to see a read-only details card first; the pencil button opens
  the editor.
- Week/day: click a time slot or the all-day row to create an event there;
  overlapping events collapse into a readable "+N more" popup.
- Event create/edit/delete with an RRULE editor (daily/weekly/monthly/yearly,
  interval, weekdays, count/until, raw RRULE fallback).
- Recurrence exceptions: edit or cancel **this event**, **this and following**,
  or **all events** (RECURRENCE-ID / THISANDFUTURE).
- Client-side recurrence expansion (RRULE/RDATE/EXDATE), DST-correct.
- Search across the cached calendar, multi-select + bulk delete.
- Drag & drop events between days (month view; non-recurring events).
- Calendar create, rename, delete and **color editing** (palette + custom
  picker); read-only calendars are detected via `current-user-privilege-set`
  and shown with a lock. A sharing panel shows the calendar URL, your
  privileges and Radicale `[rights]` guidance.
- VTODO tasks: create, edit, complete/uncomplete, delete.
- Incremental sync via `sync-token` + `calendar-multiget`.
- Offline outbox: writes are queued and retried when the connection returns.
- Reminder service worker schedules upcoming notifications.
- Fixed calendar visibility toggles after a calendar-list refresh, the
  "Your access" separator, and mobile horizontal overflow.
- Mint design system with reusable tokens (`style/tokens.css`), dark theme,
  full English/German UI (labels, dates, weekdays/months, rule descriptions),
  and switch-style toggles matching the text-input height.
- Keyboard shortcuts: `n` new event, `t` today, `m/w/d/a/k` views, `/` search,
  arrow keys navigate, `Esc` closes dialogs.
- Abstract logo (ring + dot on a pastel parallelogram) and a changelog on the
  login screen.
- Stable instance script (`scripts/stable.sh`) with its own Radicale volume and
  ports, built from the `stable` git tag.
