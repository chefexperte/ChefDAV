#!/bin/sh
# Container entrypoint (docker/Dockerfile).
#
# The app is a static WASM bundle, so it cannot read environment variables by
# itself. This script turns them into `/srv/config.json`, which the app fetches
# once at startup (see src/config.rs). The file is written on **every start**, so
# a container with no writable layer (read-only root) still works.
#
# Environment:
#   BACKEND_URL    CalDAV server the app talks to, e.g. https://dav.example.org
#                  Unset/empty -> users type the server on the login screen.
#   BACKEND_LOCK   true (default when BACKEND_URL is set) -> the login field is
#                  read-only; false -> pre-filled but editable; empty -> defaults.
set -eu

CONFIG=/srv/config.json
url="${BACKEND_URL:-}"
lock="${BACKEND_LOCK:-}"

if [ -z "$lock" ]; then
    if [ -n "$url" ]; then lock=true; else lock=false; fi
fi

# Only `true`/`false` are accepted downstream; anything else would silently
# unlock the deployment.
case "$lock" in
    true|false) ;;
    *)
        echo "chefdav: ERROR — BACKEND_LOCK must be 'true' or 'false' (got '$lock')." >&2
        exit 1
        ;;
esac

# Strip surrounding whitespace and a trailing slash: the CalDAV client appends
# its own paths, and a trailing slash would produce `//`.
url=$(printf '%s' "$url" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' -e 's:/*$::')

# JSON-escape: backslash and double quote are the only characters that matter
# inside a URL, but escaping all control characters keeps the file valid.
json_escape() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

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
