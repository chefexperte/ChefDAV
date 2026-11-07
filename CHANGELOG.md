# Changelog

All notable, user-visible changes to ChefDAV. The login screen shows the
latest section of this file (baked into each build).

## 0.1.0 — 2026-11-07

- Month, week, day, agenda and tasks views with a shared toolbar.
- Click an event to see a read-only details card first; the pencil button opens
  the editor.
- Week/day: click a time slot or the all-day row to create an event there;
  overlapping events collapse into a readable "+N more" popup.
- Event create/edit/delete with an RRULE editor (daily/weekly/monthly/yearly,
  interval, weekdays, count/until, raw RRULE fallback).
- Client-side recurrence expansion (RRULE/RDATE/EXDATE), DST-correct.
- Search across the cached calendar, multi-select + bulk delete.
- Drag & drop events between days (month view; non-recurring events).
- Calendar create, rename, delete and **color editing** (palette + custom
  picker); read-only calendars are detected via `current-user-privilege-set`
  and shown with a lock.
- VTODO tasks: create, edit, complete/uncomplete, delete.
- Incremental sync via `sync-token` + `calendar-multiget`.
- Offline outbox: writes are queued and retried when the connection returns.
- Mint design system with reusable tokens (`style/tokens.css`), dark theme,
  English/German UI.
- Keyboard shortcuts: `n` new event, `t` today, `m/w/d/a/k` views, `/` search,
  arrow keys navigate, `Esc` closes dialogs.
- "CD." logo (Fraunces outlines, mint dot) and a changelog on the login screen.
- Stable instance script (`scripts/stable.sh`) with its own Radicale volume and
  ports, built from the `stable` git tag.
