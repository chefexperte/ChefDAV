#!/usr/bin/env bash
# Remove all e2e test leftovers from the dev Radicale: every .ics whose UID is
# not a `seed-*` UID, plus any calendar other than personal/Arbeit.
# Idempotent; safe to run anytime.   ./scripts/clean-dev.sh
set -euo pipefail

DAV="${DAV_URL:-http://127.0.0.1:8080}"
USER_NAME="${DAV_USER:-alice}"
PASS="${DAV_PASS:-secret}"
AUTH="Authorization: Basic $(printf '%s:%s' "$USER_NAME" "$PASS" | base64)"
KEEP_CALS="personal Arbeit"

# list calendar collections
cals=$(curl -s -H "$AUTH" -X PROPFIND -H "Depth: 1" "$DAV/$USER_NAME/" \
  | grep -o '<href>[^<]*/</href>' | sed -e 's/<[^>]*>//g' -e "s|^/$USER_NAME/||" -e 's|/$||' \
  | grep -v "^$USER_NAME$" || true)

for cal in $cals; do
  case " $KEEP_CALS " in
    *" $cal "*) ;;
    *)
      echo "DELETE calendar /alice/$cal/"
      curl -s -o /dev/null -w "  -> %{http_code}\n" -H "$AUTH" -X DELETE "$DAV/$USER_NAME/$cal/"
      continue
      ;;
  esac
  hrefs=$(curl -s -H "$AUTH" -X PROPFIND -H "Depth: 1" "$DAV/$USER_NAME/$cal/" \
    | grep -o '<href>[^<]*\.ics</href>' | sed 's/<[^>]*>//g' || true)
  for h in $hrefs; do
    ics=$(curl -s -H "$AUTH" "$DAV$h")
    if ! printf '%s' "$ics" | grep -q '^UID:seed-'; then
      echo "DELETE $h"
      curl -s -o /dev/null -w "  -> %{http_code}\n" -H "$AUTH" -X DELETE "$DAV$h"
    fi
  done
done
echo "clean done"
