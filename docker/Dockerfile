# syntax=docker/dockerfile:1
#
# ChefDAV — the calendar frontend only: a static Rust/WASM bundle served by
# Caddy's static file server (no Radicale, no proxy inside the image).
#
# The app talks straight from the browser to *your* CalDAV server, which must
# answer CORS requests (see the README's "Setup" section for a Caddy example).
#
#   docker run -d -p 8080:80 ghcr.io/chefexperte/chefdav:latest
#
# BACKEND_URL (the CalDAV server to lock the app to) is a build argument: a
# static app cannot read environment variables at runtime. See .env.example for
# the local build/serve workflow, and .github/workflows/docker.yml for the
# published image (repository variable BACKEND_URL).

# ---------------------------------------------------------------- build ------
# Rust → WebAssembly with trunk. rust:1-alpine is rustup + musl and much
# smaller than the Debian image; both work (override with --build-arg).
ARG RUST_IMAGE=rust:1-alpine
FROM ${RUST_IMAGE} AS builder

# The CalDAV server to lock the app to (see src/config.rs). Baked into the WASM
# bundle when the image is built. Empty = users type their server on the login
# screen.
ARG BACKEND_URL=""
ENV BACKEND_URL=${BACKEND_URL}

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
# reuse the compiled dependency graph from the layer cache.
COPY Cargo.toml Cargo.lock ./
COPY crates/caldav/Cargo.toml crates/caldav/Cargo.toml
RUN mkdir -p src crates/caldav/src \
 && printf '' > src/lib.rs \
 && printf '' > src/main.rs \
 && printf '#[cfg(target_arch = "wasm32")] compile_error!("deps-layer stub");\n' \
      > crates/caldav/src/lib.rs \
 && cargo build --release --target wasm32-unknown-unknown || true

# Real sources. Trunk.release.toml (content-hashed file names, so they can be
# cached immutably) replaces the dev Trunk.toml inside the build only.
COPY . .
COPY Trunk.release.toml Trunk.toml
RUN trunk build --release \
 && test -f dist/index.html \
 && ls dist/chef-dav*_bg.wasm >/dev/null

# ---------------------------------------------------------------- runtime ----
FROM caddy:2-alpine

# Which CalDAV server this image was built for (informational; the real value
# is inside the WASM bundle).
ARG BACKEND_URL=""
LABEL org.opencontainers.image.title="ChefDAV" \
      org.opencontainers.image.description="Client-side-only CalDAV calendar frontend (Rust/WASM)" \
      org.opencontainers.image.licenses="MIT" \
      io.chefdav.backend-url="${BACKEND_URL}"

# Site only: Caddy serves /srv and falls back to index.html for client-side
# routes (/week/2026-11-16 …). No API, no admin listener.
COPY docker/frontend/Caddyfile /etc/caddy/Caddyfile
COPY --from=builder /src/dist/ /srv/

EXPOSE 80

# Plain HTTP on :80 (put your own TLS proxy in front) — no 80→443 redirect.
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD wget -q -O /dev/null http://127.0.0.1/healthz || exit 1

CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
