# syntax=docker/dockerfile:1
#
# ChefDAV — the calendar frontend only: a static Rust/WASM bundle served by
# Caddy's static file server (no Radicale, no proxy inside the image).
#
# The app talks straight from the browser to *your* CalDAV server, which must
# answer CORS requests (see the README's "Setup" section for a Caddy example).
#
#   docker run -d -p 8080:80 \
#     -e BACKEND_URL=https://dav.example.org \
#     ghcr.io/chefexperte/chefdav:latest
#
# BACKEND_URL (and BACKEND_LOCK) are **runtime** environment variables: on every
# start the entrypoint writes them into /srv/config.json, which the app reads
# before it renders. Omit BACKEND_URL and users type their server at login. The
# image is therefore deployment-agnostic — no rebuild to change servers.

# ---------------------------------------------------------------- build ------
# Rust → WebAssembly with trunk. rust:1-alpine is rustup + musl and much
# smaller than the Debian image; both work (override with --build-arg).
ARG RUST_IMAGE=rust:1-alpine
FROM ${RUST_IMAGE} AS builder

# build-base: C linker (wasm-bindgen-cli is compiled from source by trunk);
# wget: the trunk download; libstdc++: prebuilt tools trunk runs.
RUN apk add --no-cache build-base wget libstdc++ \
 && rustup target add wasm32-unknown-unknown \
 && wget -qO /tmp/trunk.tgz \
      https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-x86_64-unknown-linux-musl.tar.gz \
 && tar -xzf /tmp/trunk.tgz -C /usr/local/bin trunk \
 && rm -f /tmp/trunk.tgz && trunk --version

WORKDIR /src

# Dependency layer: re-runs only when the manifests change, so source edits
# reuse the compiled dependency graph from the layer cache. It must use the
# SAME profile as the real build below (`ci`, from Trunk.release.toml) — a
# different profile would compile every crate again and leave the cache empty.
COPY Cargo.toml Cargo.lock ./
COPY crates/caldav/Cargo.toml crates/caldav/Cargo.toml
RUN mkdir -p src crates/caldav/src \
 && printf '#[cfg(target_arch = "wasm32")] compile_error!("stub");\n' > src/lib.rs \
 && printf 'fn main() {}\n' > src/main.rs \
 && printf '#[cfg(target_arch = "wasm32")] compile_error!("stub");\n' > crates/caldav/src/lib.rs \
 && cargo build --profile ci --target wasm32-unknown-unknown 2>/dev/null || true

# Real sources. Trunk.release.toml (content-hashed file names, so they can be
# cached immutably) replaces the dev Trunk.toml inside the build only.
# BACKEND_URL is deliberately NOT passed in: the image is built without one and
# reads it at runtime instead, so the same image serves any deployment.
COPY . .
COPY Trunk.release.toml Trunk.toml
RUN env -u BACKEND_URL -u CHEFDAV_DAV_URL trunk build --release \
 && test -f dist/index.html \
 && ls dist/chef-dav*_bg.wasm >/dev/null

# ---------------------------------------------------------------- runtime ----
FROM caddy:2-alpine

LABEL org.opencontainers.image.title="ChefDAV" \
      org.opencontainers.image.description="Client-side-only CalDAV calendar frontend (Rust/WASM)" \
      org.opencontainers.image.licenses="MIT"

# Site only: Caddy serves /srv and falls back to index.html for client-side
# routes (/week/2026-11-16 …). No API, no admin listener. The Caddyfile is
# generated at start by the entrypoint (mount path is a runtime setting).
COPY --from=builder /src/dist/ /srv/
COPY --chmod=0755 docker/container-entrypoint.sh /usr/local/bin/chefdav-start

# The bundle is image content; the per-start config.json goes to /srv-runtime.
# Mount a tmpfs there (the shipped compose file does) — it is tiny and holds
# nothing worth persisting.
VOLUME ["/srv-runtime"]

ENV BACKEND_URL="" \
    BACKEND_LOCK=""

EXPOSE 80

# Plain HTTP on :80 (put your own TLS proxy in front) — no 80→443 redirect.
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD wget -q -O /dev/null http://127.0.0.1/healthz || exit 1

ENTRYPOINT ["/usr/local/bin/chefdav-start"]
