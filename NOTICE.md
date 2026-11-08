# NOTICE — licenses and code provenance

## ChefDAV itself (original work)

All code and assets in this repository (unless noted otherwise) are original
work for this project and are licensed **MIT** — see `LICENSE` and the
`license = "MIT"` fields in the `Cargo.toml` files.

## Inspiration (no code copied)

- **[AgenDAV](https://github.com/agendav/agendav)** — AGPL-3.0-only.
  ChefDAV is architecturally inspired by it (a CalDAV web frontend), but no
  AgenDAV source code, assets or text have been copied — only the general idea.
  **Important:** if any AgenDAV code is ever ported into this repository, this
  project becomes a derivative work and must be distributed under
  **AGPL-3.0** — update `LICENSE`, the `license` fields and this notice at
  that point.
- **[Runway](https://github.com/liesen/runway)** (MIT) — prior art for a
  Rust/WASM CalDAV frontend; no code copied.

## Fonts (loaded at runtime from Google Fonts, not shipped)

- **Fraunces** — SIL Open Font License 1.1 (display type)
- **Inter** — SIL Open Font License 1.1 (UI)

OFL-licensed fonts are loaded via `<link>` from fonts.googleapis.com; the font
files themselves are not part of this repository. The logo is an original
abstract SVG (no font outlines).

## Third-party crates (via Cargo)

Borrowed code comes from crates.io dependencies, used under their own licenses
(all permissive; none copyleft). Full texts ship with the crates in the cargo
registry and are linked from each crate's crates.io page. `cargo deny check`
can audit this (`cargo install cargo-deny`).

License groups from `cargo metadata` (ChefDAV's own crates excluded):

- **MIT**: leptos, leptos_router, tachys, reactive_graph, reqwest, hyper, tokio,
  tower, axum, bytes, serde, serde_json, syn, quote, proc-macro2, roxmltree,
  rrule, wasm-bindgen, web-sys, js-sys, idb, gloo-net, phf, slotmap, tracing,
  … (largest group)
- **MIT OR Apache-2.0**: chrono, chrono-tz, url, http, base64, aes-gcm, pbkdf2,
  sha2, hmac, thiserror, futures-*, getrandom, hashbrown, itoa, memchr,
  regex, wasm-bindgen-futures, web-sys, serde-wasm-bindgen, idna, …
- **Apache-2.0**: sync_wrapper, indexmap, equivalent, pin-project(-lite),
  event-listener, rustc-hash, uuid, …
- **Apache-2.0 OR MIT**: async-lock, autocfg, cmov, const-oid, ghash, polyval,
  …
- **Apache-2.0 OR BSL-1.0**: ryu
- **Apache-2.0 WITH LLVM-exception**: wasi
- **Unlicense OR MIT**: aho-corasick, memchr, winapi-util
- **Unlicense/MIT**: same-file, walkdir
- **BSD-3-Clause (with MIT)**: matchit
- **CC0-1.0**: base16
- **Zlib**: const_format, konst, slotmap
- **Unicode-3.0**: unicode-ident, icu_* (via idna)
- **MIT AND BSD-3-Clause**: matchit (also listed above)

No GPL/AGPL/LGPL crate is in the dependency tree. If one is ever added, this
project's MIT license stays valid (the crate keeps its own license), but the
distribution terms of that crate must be followed — record it here.

## Infrastructure images (dev only, not part of the app)

- Radicale image (`tomsquest/docker-radicale`) — AGPL-3.0 (server)
- `caddy:2-alpine` — Apache-2.0
- `glauth/glauth` — MIT (LDAP fixture, dev only)
