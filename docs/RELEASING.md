# Release checklist

Each tag produces a Docker image (`ghcr.io/chefexperte/chefdav`) and, if you
want one, a GitHub release.

1. **Version.** The login screen shows the newest section of `CHANGELOG.md`, so
   that section has to be the release. Rename `## Unreleased` to
   `## X.Y.Z — YYYY-MM-DD` and add a fresh `## Unreleased` above it.
2. **Checks.** `.github/workflows/ci.yml` has to be green: `cargo test -p caldav`,
   `cargo test -p chef-dav --lib` and the e2e suites.
3. **Image.** `.github/workflows/docker.yml` builds, smoke-tests and pushes on
   every push to the default branch (`:latest`, `:edge`, `:sha-…`). A `v*` tag
   adds `:X.Y.Z` and `:X.Y`.
4. **Tag.** `git tag -a vX.Y.Z -m "ChefDAV X.Y.Z" && git push origin vX.Y.Z`
5. **Release.** Draft a GitHub release from the tag, use the changelog section
   as the body, and link the package page next to the tarballs.

## Changing what goes into the image

The image contains the frontend only: the static WASM app on Caddy's file
server. Your CalDAV server stays outside.

- **Build toolchain:** `ARG RUST_IMAGE` in `docker/Dockerfile`, default
  `rust:1-alpine`.
- **Static server:** `FROM caddy:2-alpine` in the runtime stage.
- **trunk:** the download URL in the builder stage pins a version. Bump it with
  `Trunk.release.toml` when trunk's options change.
- **Site config:** `docker/container-entrypoint.sh` generates the Caddyfile (SPA
  fallback, cache headers, `/healthz`). Keep the hashed-file patterns in sync
  with `filehash = true` in `Trunk.release.toml`.
- **Server URL:** runtime, not build time. The entrypoint writes
  `/srv-runtime/config.json` from `BACKEND_URL` and `BASE_PATH` on every start,
  and the app fetches it before it renders (`src/config.rs`). Do not bake a
  server into the image. These have to stay in sync:
  - the entrypoint's JSON shape and `config::parse_runtime` (and its tests),
  - the tmpfs (`/srv-runtime`) the entrypoint writes to, because the root
    filesystem may be read-only,
  - `/config.json` staying `no-cache` in the generated Caddyfile.
- **Local builds without Docker** still accept build-time values.
  `CHEFDAV_DAV_URL` pre-fills an editable field; `BACKEND_URL` fixes the
  server, so the login screen asks only for credentials.

The workflow builds the image from the tag. Its smoke tests cover the app, the
SPA fallback, cache headers, `/config.json` with and without `BACKEND_URL`, a
`/ui` mount and a read-only root. They have to pass before the push.
