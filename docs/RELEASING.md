# Release checklist

ChefDAV ships two artifacts from one tag: the **Docker image**
(`ghcr.io/chefexperte/chefdav`) and, optionally, a GitHub release.

1. **Version** — the login screen shows the changelog's latest section, so the
   top section of `CHANGELOG.md` must be the release: rename `## Unreleased`
   to `## X.Y.Z — YYYY-MM-DD` (and start a fresh `## Unreleased` above it).
2. **Dry runs** — the checks in `.github/workflows/ci.yml` must be green
   (`cargo test -p caldav`, `cargo test -p chef-dav --lib`, the e2e suites).
3. **Image** — `.github/workflows/docker.yml` builds, smoke-tests and pushes on
   every push to the default branch (`:latest`, `:edge`, `:sha-…`). A `v*` tag
   adds `:X.Y.Z` and `:X.Y`.
4. **Tag** — `git tag -a vX.Y.Z -m "ChefDAV X.Y.Z" && git push origin vX.Y.Z`.
5. **Release** — draft a GitHub release for the tag with the changelog section
   as its body; link the image (packages page) next to the tarballs.
6. **Stable instance** (this machine) — `./scripts/stable.sh` keeps using the
   `stable` git tag: `git tag -f -a stable -m "Stable checkpoint"` and re-run it.

## Bumping the image contents

The image is **frontend only** (the static WASM app on Caddy's file server);
your CalDAV server is deliberately not part of it.

- **Build toolchain**: `ARG RUST_IMAGE` in `docker/Dockerfile` (default
  `rust:1-alpine`).
- **Static server**: `FROM caddy:2-alpine` (runtime stage).
- **trunk**: the download URL in the builder stage pins a version; bump it
  together with `Trunk.release.toml` if trunk's options change.
- **Site config**: `docker/frontend/Caddyfile` (SPA fallback, cache headers,
  `/healthz`). Keep the hashed-file patterns in sync with `filehash = true` in
  `Trunk.release.toml`.
- **Server URL**: **runtime**, not build time. `docker/container-entrypoint.sh`
  writes `/srv/config.json` from the `BACKEND_URL` / `BACKEND_LOCK` environment
  variables on every start; the app fetches that file before it renders
  (`src/config.rs`). The published image is therefore deployment-agnostic — do
  **not** bake a server in. Keep in sync:
  - the entrypoint's JSON shape ↔ `config::parse_runtime` (and its tests),
  - `/srv` ownership in the Dockerfile (the entrypoint rewrites the file even
    with a read-only root filesystem),
  - `/config.json` staying `no-cache` in `docker/frontend/Caddyfile`.
- **Local builds without Docker** still support build-time values:
  `CHEFDAV_DAV_URL` (pre-filled, editable) and `BACKEND_URL` (locked).

The staging steps for the image are the same as for any other change: the
workflow builds it from the tag and its smoke tests (app served, SPA fallback,
cache headers, `/config.json` for each `BACKEND_URL` variant, read-only root)
must pass before the push.
