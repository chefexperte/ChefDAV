#!/bin/sh
# Container entrypoint (docker/Dockerfile).
#
# The app is a static WASM bundle, so it cannot read environment variables by
# itself. This script turns them into /srv/config.json, which the app fetches
# once at startup (see src/config.rs), and then starts Caddy.
#
# /srv/config.json lives on a tmpfs mount of its own (/srv-runtime), so writing
# it works even with a read-only root filesystem; Caddy reads it from there.
#
# Environment:
#   BACKEND_URL    CalDAV server the app talks to, e.g. https://dav.example.org
#                  Unset/empty → users type the server on the login screen.
#   BACKEND_LOCK   true (default when BACKEND_URL is set) → read-only server
#                  field; false → pre-filled but editable.
set -eu

RUNTIME_DIR=/srv-runtime
CONFIG="$RUNTIME_DIR/config.json"
url="${BACKEND_URL:-}"
lock="${BACKEND_LOCK:-}"

if [ -z "$lock" ]; then
    if [ -n "$url" ]; then lock=true; else lock=false; fi
fi

# Only true/false are meaningful; anything else would silently unlock a
# deployment, so refuse to start instead.
case "$lock" in
    true|false) ;;
    *)
        echo "chefdav: ERROR — BACKEND_LOCK must be 'true' or 'false' (got '$lock')." >&2
        exit 1
        ;;
esac

# Trim whitespace and trailing slashes: the CalDAV client appends its own paths,
# and a trailing slash would produce `//`.
url=$(printf '%s' "$url" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' -e 's:/*$::')

# Escape the two characters that can break a JSON string (URLs cannot contain
# raw control characters, so this is sufficient).
json_escape() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

mkdir -p "$RUNTIME_DIR"
cat > "$CONFIG" <<EOF
{
  "backend_url": "$(json_escape "$url")",
  "lock": $lock
}
EOF

if [ -n "$url" ]; then
    echo "chefdav: backend $url (lock: $lock)"
else
    echo "chefdav: no BACKEND_URL — users choose their calendar server at login"
fi

exec caddy run --config /etc/caddy/Caddyfile --adapter caddyfile
