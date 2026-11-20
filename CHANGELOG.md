# Changelog

All notable, user-visible changes to ChefDAV.

## Unreleased

### Changes

- A small static loading screen ("ChefDAV — Loading your calendar…") shows
  immediately, before the WASM bundle has downloaded and compiled. Slow first
  loads no longer look like a blank page.
- On phones the sidebar collapses like an AdwNavigationSplitView: a menu button
  in the toolbar opens it over the content, and a tap on the backdrop (or
  Escape) closes it.
- Phone toolbar: menu, navigation buttons and the title on the first row,
  search on its own row, and the view switcher on the third. The sync status
  has a fixed-width slot on the right of the switcher, so appearing after a
  save does not move anything. On desktop the same status sits left of the
  search field with the same reserved space.
- Day view fits the phone width without horizontal scrolling.

- Opening an event through its resource link (`/event/<calendar>/<uid>`, also
  after a reload or back/forward) shows a full 620px dialog instead of the small
  click popover. Clicking an event still opens the popover at the pointer.
- Drag across the week or day timeline to create an event. A translucent card
  follows the pointer once it actually moves (a click never shows it), and
  releasing opens the editor with the dragged start and end in 15-minute steps.
  Touch devices keep the click behaviour: a tap opens the time-slot popup and
  never starts a drag, so scrolling stays natural. Dragging into another day keeps a staircase
  selection: to the end of the first day, full days in between, and to the drop
  point on the last day. A click without movement still opens the time-slot
  popup.
- Drag across the all-day row to create an all-day event spanning the dragged
  days. The same translucent bar follows the pointer.
- When the CalDAV endpoint answers with a proxy 404 or a 5xx, the app says that
  the calendar backend is not available instead of showing the raw HTTP error,
  and explains that browsing the cache works but creating, editing or deleting
  events does not. Writes made during such an outage stay queued and retry.

### Fixes


- Under a mount path such as `/ui`, the startup splash requested `/logo.svg`
  from the origin root. Radicale answered 401 and the browser opened its own
  Basic auth dialog. The splash now loads the logo under the mount path.
- The shortcuts dialog has proper top padding and a round close button, and
  dialog close buttons sit 16px from the top and right edges.
- Boxed notices in dialogs (create a calendar first, read-only, backend down)
  keep the dialog's 24px inset instead of running into the left and right
  edges.
- Opening the event editor before the calendar list has loaded no longer shows
  a wrong notice or an empty calendar picker; the default calendar is filled in
  when the list arrives.

## 0.3.0 — 2026-11-19

### Fixes

- The browser no longer opens its own Basic auth dialog when the calendar
  loads. An unauthenticated probe request to the CalDAV root caused it, and the
  browser asked for credentials even though the app sends its own. The probe is
  gone; connection problems are still reported as CORS or unreachable from the
  real CalDAV request.

### Adwaita fidelity pass

Refinements after reviewing the Adwaita re-skin, using the values from
libadwaita's stylesheet.

- The all-day switch in the event editor is a proper 44×24 Adwaita switch again
  (grey off, blue on, sliding knob). The generic checkbutton rule had
  out-specified it and left a stray dot.
- Events that cross a month boundary appear in both months. A 30 Nov to 2 Dec
  event used to disappear from December for two reasons: recurring series were
  filtered by start date, and the shared boundary week row kept the previous
  month's bar state. Both are fixed.
- Month-view day cells read as cards. White cells were 1.04:1 against the window
  and had no visible edge; they now carry a card shadow and a hairline. Dark
  mode is unchanged.
- Week and day all-day bars no longer overflow or shrink the day columns; they
  are positioned by grid column.
- The Dark/Light and DE/EN footer buttons are visible. They sit on the sidebar,
  where a 10 % tint disappears, so they use libadwaita's opaque standalone
  button surface with a hairline. The active language and theme read clearly in
  both themes.
- Log out is a red destructive button. The Calendars heading lost its chevron,
  Home/previous/next are raised circular headerbar buttons, and the sidebar
  toggles have consistent widths.
- Year and month switchers: click the year or the month in the month-view title
  to open a 5×3 year grid or the twelve months, keeping the current day where the
  month allows it.
- The details popup has a "New event" button that starts an event in the same
  date and time slot.
- The "+N more" popup lists the events its bar hides. It used to show an event
  that was already visible, one that was never hidden, and events from another
  day. The bar stored indices into that day's occurrence list, and the popup
  looked them up in the global event list. Day and week views now map the hidden
  set correctly, and clicking a row opens that event.
- The "+N more" popup is a plain popover with flat rows instead of a dialog
  containing a card, and it opens at the click.
- Multi-day bars keep the day cell's padding on the right in month view; their
  width calculation ignored the left inset.
- Popovers open where you click at any screen height. The details card, day and
  time-slot popups and "+N more" used to be clamped against a guessed size, so
  anything below the middle of the screen was pushed away from the cursor. They
  are now measured and placed below-right of the pointer, flipped above near the
  bottom, and always kept on screen. The month and year menus open centred under
  their own field.
- The logo and "ChefDAV" wordmark are a link home. One click returns to the
  month view of the current month with nothing open or selected. It is a real
  link, so middle-click opens it in a new tab.
- All icons are real SVGs. The "New event", "New task" and "Go to day" actions,
  the previous/next chevrons, the picker arrows, the search clear button and the
  "new calendar" plus were text characters with different shapes on every
  platform. They now come from one inline icon set.
- Search asks you to keep typing. One character shows "Type at least 2
  characters to search." instead of "No matching events".
- Dialogs follow AdwDialog: padded header, scrollable content and a footer
  action row separated by a hairline.
- The sidebar has a Shortcuts button with a keyboard icon. It opens an
  AdwShortcutsDialog-style list of the keyboard shortcuts, grouped into General,
  Views and Navigation. Escape or a click outside closes it.
- The light/dark toggle shows a sun or moon icon, and the language toggle spells
  out English and German instead of EN and DE.
- The theme and language toggles use the same raised surface as the Shortcuts
  button, in both themes, instead of the darker opaque mix.
- Day cards in the leftmost and rightmost month columns keep their shadow; the
  scroll container no longer clips it.
- The month and year title buttons keep their hover background inside the title
  box, so it is no longer cut off next to the next-arrow button.
- The login page no longer shows the IndexedDB warning or the changelog. The
  changelog lives in `CHANGELOG.md`.
- Creating an event with no calendar says "Please create a calendar first"
  instead of "This calendar is read-only".

### Re-skin to GNOME libadwaita

The black-and-gold Art Deco design from earlier the same day was rejected by the
owner and is replaced by this re-skin.

- The app follows GTK4/libadwaita. Colours, radii (buttons and entries 9 px,
  cards 12 px, dialogs and popovers 15 px) and shadows are the named values from
  libadwaita's stylesheet. Light is the default (`#fafafb` window, white cards,
  `#ebebed` sidebar); dark is Adwaita dark (`#222226` window, `#1d1d20` view,
  white at 8 % for cards).
- Accent is Adwaita blue `#3584e4` with white labels. The calendar palette is the
  eight Adwaita accent hues (blue, red, orange, green, teal, purple, pink,
  slate), each with white chip labels above 3:1.
- Buttons are flat Adwaita raised buttons (10 % ink background, 1 px border),
  and the primary action is the blue suggested action. The view switcher is an
  AdwViewSwitcher-style segmented control. Today is an accent-filled circle as in
  GNOME Calendar. Modals and popovers are dialog-styled cards on a dimmed
  overlay, with no blur. The logo is a flat accent-blue calendar tile.
- Klarissa keeps the wordmark and headings; the body face is Montserrat, self
  hosted under the OFL. Both are bundled in `public/fonts/`, with no CDN. The
  logo is back to the original pastel-mint mark.
- More Adwaita widgets: GTK checkbuttons (6 px radius, accent fill and white
  check), a pill search entry with a magnifier, boxed-list rows for the list and
  tasks views with hairline separators, flat circular icon buttons, and a real
  Adwaita switch.

### Docker image

- ChefDAV ships as a container image, `ghcr.io/chefexperte/chefdav`: the calendar
  frontend (static WASM app) on a small Caddy file server, with an SPA fallback
  for deep links, immutable caching for the hashed bundle and a `/healthz` probe.
- The CalDAV server is chosen at container start. Set `BACKEND_URL` in the
  environment or a `.env` file. With it set, the login screen asks only for
  credentials; with it empty, users enter the server themselves. There is no
  separate lock option.
- Login failures are explained better. When the browser blocks the request
  because the CalDAV server sends no CORS headers, the login page says so and
  points at the fix instead of only "server unreachable".
- The app can be published under a path (`calendar.example.com/ui` with the
  CalDAV server at the site root). Set `BASE_PATH=/ui` and route by `Host()` and
  `PathPrefix()`; it works whether or not the proxy strips the prefix. Deep links
  under `/ui` keep working, and the address bar no longer gets a double slash.
- Fixed: logging in a second time after logging out, or visiting `/login` while
  logged out, stayed on the login page instead of opening the calendar.

## 0.2.0 — 2026-11-17

Second feature batch: cluster layout for overlapping events, self-hosted fonts,
a timezone fix, "+N more" everywhere and clearer month labels.

- "+N more" entries are clearly visible, especially in dark mode: they have their
  own neutral surface and a hairline border instead of a background that vanished
  against the timeline. The month-view overflow chip says "+N more" and counts
  the hidden events, like the week and day bars.
- Overlapping events stack in clusters. Each cluster gets as many columns as its
  busiest moment needs, so a quiet morning keeps its width when the afternoon is
  packed. Chains pack tightly, a lone event keeps full width next to an
  overflowing cluster, and each overflowing cluster gets its own "+N more" bar.
  The bar counts every hidden event (one used to be skipped) and lists exactly
  those events.
- No more third-party CDNs. The fonts are bundled and served from your own
  origin, so nothing loads from Google Fonts or another host. The only network
  peers are your calendar server and OpenStreetMap for the optional map preview.
- Fixed events from other clients showing at the wrong local time. A broken
  `Intl` lookup made the app fall back to UTC, so events with a `TZID` from
  AgenDAV, Thunderbird or the demo seed rendered an hour early in winter and two
  hours early in summer. They now use the browser's timezone.
- The week and day overflow entry is a "+N more" button that spans the time range
  of the hidden events instead of the full column, and only appears on real
  overlap.
- The event details card is a popover anchored at the click; deep links and
  search results open it centred near the top. The Close text button is an × in
  the top-right corner, in the "more events" popup as well.
- Fixed month boundary cells keeping stale out-of-month styling when navigating,
  for example December days showing as in-month after Dec → Jan. The state is
  computed reactively.
- Day view shows up to four overlapping timed events side by side; week view
  keeps two. From five on, three are shown plus a "Show all" bar spanning the
  full column in the freed slot. The all-day row is unchanged.
- Event titles stay pinned at the top of the viewport while you scroll a tall
  event in the week and day timelines.
- Fixed event columns not resizing when a new overlapping event was created in
  day view. The rows stayed stale until a reload.
- Fixed a reloaded `/event/…` deep link opening the wrong event. The details
  modal stored a positional index that shifted when the event list was
  re-synced; it is now remapped by href.
- Fixed the continuation fade of multi-day events when navigating weeks. A
  week-crossing all-day event kept the previous week's fade side and position
  because the bar row was not re-rendered when its geometry changed.
- Deep links for every resource. Opening an event or task puts its URL in the
  address bar (`/event/<calendar>/<uid>`, `/task/<calendar>/<uid>`, with
  `?occ=…` for one occurrence), and the details card has a copy-link button. A
  fresh load of such a link opens the resource, fetching a hidden calendar on
  demand if needed.
- The Agenda view is called "List" in English and "Liste" in German.
- The copy-link button is an icon: a link glyph that turns into a check mark
  once the link is in the clipboard.
- The "Location" label in the details card is a pin icon.
- Fixed a horizontal scrollbar in the edit-event popup.
- Fixed the week-view "+N more" badge rendering too high instead of in its day
  column.
- Day view no longer shows a redundant day header; the toolbar already shows the
  date.
- Multi-day events: all-day and timed events have an end-date picker, so
  overnight events such as 23:00 to 01:00 work. Month view draws them as a bar
  across day cells, and week, day and list views show a striped fade where the
  event continues.
- Event chips and bars use a solid calendar colour (no pale tint with a left
  border) and are a little larger. Month cells show two events plus a "+N more"
  entry styled in a neutral colour; clicking it opens the day popover. The
  week-view "+N more" entry looks the same. The Edit button uses a pencil icon.
- Continuation fades: an event that crosses into another day or week fades out
  with a striped dissolve at the continuing edge. The fade is horizontal at week
  boundaries, where month view draws one spanning bar per week row and the week
  all-day row draws one longer bar. Timed events clipped at the day top or bottom
  fade vertically, in day view too.
- The toolbar's "Today" button is a home icon to the left of prev/next. It jumps
  to the current day.
- Week and day views keep the all-day row at a fixed three-entry height. More
  events collapse into an event-sized "Show all" entry. Each timeline column
  shows two slots; overflow turns the second slot into "Show all".
- The URL reflects the current view and date (`/month/2026-11`,
  `/week/2026-11-09`, `/day/…`, `/list/…`, `/tasks`), so a copied link restores
  that view. Resource links still work and return to the view URL when closed.
  Month cells show up to three events (more becomes two plus "Show all"), with
  more spacing and a matching inset for spanning bars.

## 0.1.1 — 2026-11-08

- The README warns that the project is written entirely by AI, and was rewritten
  as a proper project readme with screenshots, setup, development, limitations
  and licensing.
- Licensing clarified: all original code and assets are MIT (`LICENSE`,
  `license = "MIT"` in both Cargo.tomls). No AgenDAV code was copied.
  Dependencies are permissive crates (see `NOTICE.md`); fonts are OFL and loaded
  at runtime.
- "Stay logged in": the saved session is restored on reload. A checkbox, on by
  default, controls it. Logging out deletes the saved password, and the login
  page keeps the server and username for prefilling.
- The login page has dark/light and EN/DE controls and applies the saved theme
  before rendering.
- OpenStreetMap preview in event details: coordinates show a pinned map, a click
  opens OSM in a new tab, address-like locations are geocoded with Photon, and
  other names get an OSM search link.
- The list view shows start and end times (`12:30–13:30`).
- Week and day views give events shorter than 45 minutes a compact one-line
  layout so the title stays visible.
- The search results panel opens directly under the search box.
- Removed the non-functional reminders bell and service worker.
- New logo: a pastel mint parallelogram with rounded corners and an abstract
  deep-green ring-and-dot mark.

## 0.1.0 — 2026-11-07

- Month, week, day, agenda and tasks views with a shared toolbar.
- Equal-height month cells with up to three chips plus "+N more", a day popover
  listing the day's events, and time-slot and all-day popovers with events and a
  prefilled New event action.
- Clicking an event opens a read-only details card; the pencil button opens the
  editor.
- Week and day views: click a time slot or the all-day row to create an event
  there. Overlapping events collapse into a "+N more" popup.
- Event create, edit and delete, with an RRULE editor (daily, weekly, monthly or
  yearly; interval, weekdays, count or until, and a raw RRULE fallback).
- Recurrence exceptions: edit or cancel this event, this and following, or all
  events (RECURRENCE-ID / THISANDFUTURE).
- Client-side recurrence expansion for RRULE, RDATE and EXDATE, DST-correct.
- Search across the cached calendar, multi-select and bulk delete.
- Drag and drop events between days in month view, for non-recurring events.
- Calendar create, rename, delete and colour editing with a palette and custom
  picker. Read-only calendars are detected through
  `current-user-privilege-set` and shown with a lock. The sharing panel shows the
  calendar URL, your privileges and Radicale `[rights]` guidance.
- VTODO tasks: create, edit, complete, uncomplete and delete.
- Incremental sync through `sync-token` and `calendar-multiget`.
- Offline outbox: writes are queued and retried when the connection returns.
- A reminder service worker scheduled upcoming notifications.
- Fixed calendar visibility toggles after a calendar-list refresh, the "Your
  access" separator, and horizontal overflow on mobile.
- Mint design system with reusable tokens (`style/tokens.css`), a dark theme, a
  full English/German interface, and switch-style toggles.
- Keyboard shortcuts: `n` for a new event, `t` for today, `m/w/d/a/k` for views,
  `/` for search, arrow keys to navigate, `Esc` to close dialogs.
- Abstract logo and a changelog on the login screen.
