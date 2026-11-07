#!/usr/bin/env bash
# Stable ChefDAV instance for day-to-day use.
#
# - Builds the app from the `stable` git tag in an isolated worktree
#   (.stable-worktree/), so active development cannot disturb it.
# - Starts a separate Radicale + Caddy stack on port 8081 with its own data
#   volume (docker/docker-compose.stable.yml).
# - Serves the built app on http://127.0.0.1:3200.
#
# Usage: ./scripts/stable.sh [tag]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${1:-stable}"
WORKTREE="$ROOT/.stable-worktree"
PORT="${CHEFDAV_STABLE_PORT:-3200}"
DAV_PORT="${CHEFDAV_STABLE_DAV_PORT:-8081}"

cd "$ROOT"

if ! git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "error: git tag '$TAG' not found" >&2
  exit 1
fi

# Container runtime: prefer rootless Podman, fall back to Docker.
if command -v podman >/dev/null 2>&1; then
  systemctl --user start podman.socket >/dev/null 2>&1 || true
  COMPOSE=(podman compose)
else
  COMPOSE=(docker compose)
fi

echo "==> starting stable CalDAV stack on :$DAV_PORT (separate volume)"
(cd "$ROOT/docker" && "${COMPOSE[@]}" -f docker-compose.stable.yml up -d)

echo "==> waiting for the stable CalDAV server"
for _ in $(seq 1 30); do
  code=$(curl -s -o /dev/null -w "%{http_code}" -u alice:secret -X PROPFIND \
    -H "Depth: 0" "http://127.0.0.1:$DAV_PORT/" || true)
  [ "$code" = "207" ] && break
  sleep 2
done

# Give the user a usable calendar out of the box (MKCALENDAR is idempotent
# enough here: an existing collection just returns an error we ignore).
curl -s -o /dev/null -u alice:secret -X MKCALENDAR \
  --data '<?xml version="1.0"?><D:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:set><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname>Personal</D:displayname></D:prop></D:set></D:mkcalendar>' \
  "http://127.0.0.1:$DAV_PORT/alice/personal/" || true

echo "==> preparing worktree at $WORKTREE from tag '$TAG'"
if [ -e "$WORKTREE/.git" ]; then
  git -C "$WORKTREE" fetch --tags origin >/dev/null 2>&1 || true
  git -C "$WORKTREE" checkout --detach "$TAG"
else
  git worktree add --detach "$WORKTREE" "$TAG"
fi

echo "==> building stable bundle (release)"
(
  cd "$WORKTREE"
  CARGO_TARGET_DIR="$ROOT/target-stable" \
    CHEFDAV_DAV_URL="http://127.0.0.1:$DAV_PORT" \
    trunk build --release
)

cat <<EOF

  Stable ChefDAV:  http://127.0.0.1:$PORT
  CalDAV server:   http://127.0.0.1:$DAV_PORT
  Login:           alice / secret

  This instance is built from git tag '$TAG' and uses its own Radicale
  volume, so it is unaffected by development. Press Ctrl+C to stop the
  web server (the containers keep running).

EOF

exec python3 "$ROOT/scripts/serve.py" "$PORT" "$WORKTREE/dist"
