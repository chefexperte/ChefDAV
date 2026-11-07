# NOTICE — licenses and code provenance

## ChefDAV itself

All code in this repository (unless noted otherwise) is original work for this
project and is licensed **MIT OR Apache-2.0** (see the `license` fields in the
`Cargo.toml` files).

## Inspiration (no code copied)

- **[AgenDAV](https://github.com/agendav/agendav)** — AGPL-3.0-only.
  ChefDAV is architecturally inspired by it (a CalDAV web frontend), but no
  AgenDAV source code has been copied. **Important:** if any AgenDAV code is
  ever ported into this repository, this project becomes a derivative work and
  must be distributed under **AGPL-3.0** — update the `license` fields and this
  notice at that point.
- **[Runway](https://github.com/liesen/runway)** (MIT) — prior art for a
  Rust/WASM CalDAV frontend paired with a backend.

## Third-party crates (via Cargo)

Borrowed code comes from crates.io dependencies, used under their licenses:

| Crate | License | Used for |
| --- | --- | --- |
| leptos, leptos_router | MIT | UI framework (WASM) |
| reqwest, http, url | MIT OR Apache-2.0 | HTTP client (fetch backend on wasm) |
| roxmltree | MIT | WebDAV multistatus XML parsing |
| rrule | MIT | RRULE expansion (DST-correct with chrono-tz) |
| chrono, chrono-tz | MIT OR Apache-2.0 | date/time + timezones |
| icalendar-free hand-rolled ICS parser | — | `crates/caldav/src/ics.rs` is original |
| idb, wasm-bindgen, web-sys, js-sys, serde-wasm-bindgen | MIT OR Apache-2.0 | IndexedDB + wasm-bindgen glue |
| aes-gcm, pbkdf2, sha2, base64 | MIT OR Apache-2.0 | credential encryption at rest |
| thiserror, serde, serde_json, console_error_panic_hook, futures | MIT OR Apache-2.0 | utilities |

Full license texts ship with the crates in `~/.cargo/registry` and are also
linked from each crate's crates.io page. `cargo license` / `cargo deny` can be
used to audit this list (`cargo install cargo-deny`, then `cargo deny check`).

## Infrastructure images (dev only)

- `radicale/radicale` — AGPL-3.0 (server, not part of the app)
- `caddy:2-alpine` — Apache-2.0
- `glauth/glauth` — MIT (LDAP fixture, dev only)
