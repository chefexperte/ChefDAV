#!/usr/bin/env bash
# Seed the dev Radicale (127.0.0.1:8080, alice/secret) with the two calendars
# and demo events the e2e suites and screenshots expect. Idempotent: existing
# calendars (405 on MKCALENDAR) and events (PUT overwrites) are kept/updated.
#
# Run after `podman compose -f docker/docker-compose.yml up -d` on a fresh
# volume (e.g. a new machine):   ./scripts/seed-dev.sh
set -euo pipefail

DAV="${DAV_URL:-http://127.0.0.1:8080}"
AUTH="Authorization: Basic $(printf '%s:%s' "${DAV_USER:-alice}" "${DAV_PASS:-secret}" | base64)"

dav() { # dav METHOD PATH [DATA]
  local method="$1" path="$2" data="${3:-}"
  if [ -n "$data" ]; then
    curl -s -o /dev/null -w "%{http_code}" -H "$AUTH" -X "$method" "$DAV$path" \
      -H "Content-Type: application/xml" --data-binary "$data"
  else
    curl -s -o /dev/null -w "%{http_code}" -H "$AUTH" -X "$method" "$DAV$path"
  fi
}

mkcal() { # mkcal HREF DISPLAYNAME COLOR
  local code
  code=$(dav MKCALENDAR "$1" "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<mkcalendar xmlns=\"DAV:\" xmlns:C=\"urn:ietf:params:xml:ns:caldav\" xmlns:A=\"http://apple.com/ns/ical/\">
  <set><prop>
    <displayname>$2</displayname>
    <C:calendar-color>$3</C:calendar-color>
  </prop></set>
</mkcalendar>")
  echo "MKCALENDAR $1 -> $code (405 = already exists)"
}

put() { # put HREF UID ICS_BODY
  local code
  code=$(dav PUT "$1" "$3")
  echo "PUT $1 -> $code"
}

mkcal "/alice/personal/" "Personal" "#2f9e7f"
mkcal "/alice/Arbeit/" "Arbeit" "#c2703d"

ev() { # ev UID SUMMARY START END [EXTRA_LINES]
  local uid="$1" summary="$2" start="$3" end="$4" extra="${5:-}"
  put "/alice/personal/$uid.ics" "$uid" "BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//ChefDAV//seed//EN
BEGIN:VEVENT
UID:$uid
DTSTAMP:20261101T090000Z
DTSTART;TZID=Europe/Berlin:$start
DTEND;TZID=Europe/Berlin:$end
SUMMARY:$summary
$extra
END:VEVENT
END:VCALENDAR"
}

# --- the events e2e/audit.mjs rely on (days the suites assert on: 10, 11,
#     12, 17 must keep these; see e2e/*.mjs) ---
ev "seed-lunch-sam@chef-dav"   "Lunch with Sam"  "20261110T120000" "20261110T130000" "LOCATION:Caf\\u00e9 Central"
ev "seed-standup@chef-dav"     "Standup"         "20261111T090000" "20261111T091500"
ev "seed-design-review@chef-dav" "Design review" "20261112T140000" "20261112T150000"
ev "seed-quick-sync@chef-dav"  "Quick sync"      "20261117T090000" "20261117T091500"
ev "seed-weekly-sync@chef-dav"  "Weekly sync"    "20261109T100000" "20261109T103000" "RRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=8"
ev "seed-dentist@chef-dav"     "Dentist"         "20261125T080000" "20261125T090000"

# --- fun demo data on days the e2e suites do NOT use (1–9, 13, 14, 16, 27, 29, 30) ---
ev "seed-doener-yolo@chef-dav"  "Döner-YOLO-Meeting"        "20261103T120000" "20261103T130000"
ev "seed-kacken@chef-dav"       "Kacken"                    "20261105T080000" "20261105T081500"
ev "seed-ram-altman@chef-dav"   "Meeting with Ram Altman"   "20261106T100000" "20261106T110000"
ev "seed-katerfruehstueck@chef-dav" "Katerfrühstück mit KI"  "20261108T110000" "20261108T130000"
ev "seed-slopping@chef-dav"     "Slopping with the stars"   "20261113T200000" "20261113T230000"
ev "seed-bib-date@chef-dav"     "Bib-Date"                  "20261114T190000" "20261114T210000" "LOCATION:Caf\\u00e9 Kn\\u00f6sel, Heidelberg"
ev "seed-ai-slop@chef-dav"      "Seminar über AI Slop"      "20261116T140000" "20261116T160000"
ev "seed-hausaufgaben@chef-dav" "Hausaufgaben generieren lassen" "20261127T090000" "20261127T100000"

# multi-day all-day event (spanning bar in the month view screenshots)
put "/alice/personal/seed-tacos@chef-dav.ics" "seed-tacos@chef-dav" "BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//ChefDAV//seed//EN
BEGIN:VEVENT
UID:seed-tacos@chef-dav
DTSTAMP:20261101T090000Z
DTSTART;VALUE=DATE:20261130
DTEND;VALUE=DATE:20261203
SUMMARY:TaCoS Conference
END:VEVENT
END:VCALENDAR"

echo "seed done"
