# NOTICE — licenses and code provenance

## ChefDAV

All code and assets in this repository are original work for this project unless
noted otherwise. They are MIT licensed. See `LICENSE` and the `license = "MIT"`
fields in the `Cargo.toml` files.

## Inspiration

- [AgenDAV](https://github.com/agendav/agendav) (AGPL-3.0-only) is the closest
  relative: a CalDAV web frontend with the same purpose. No AgenDAV source code,
  assets or text were copied.
- [Runway](https://github.com/liesen/runway) (MIT) is prior art for a Rust/WASM
  CalDAV frontend. No code was copied.

## Fonts

The app loads nothing from third-party CDNs (a project rule, also recorded in
AGENTS.md). Both fonts ship in this repository and are served from the app's own
origin:

- `public/fonts/montserrat-*.woff2` is **Montserrat** under the SIL Open Font
  License 1.1 (body and UI face, 400/500/600/700 plus italic), © 2011 The
  Montserrat Project Authors (Julieta Ulanovsky et al.,
  https://github.com/googlefonts/montserrat). The OFL permits redistribution and
  bundling. The woff2 subsets were downloaded from Google Fonts once; at runtime
  the app only requests them from its own origin.
- `public/fonts/Klarissa-Regular.ttf` is **Klarissa** (accent and display face),
  © 2003 Dieter Steffmann / Typographer Mediengestaltung. It is bundled under the
  1001Fonts Free For Commercial Use (FFC) license, and the EULA shipped with the
  font is committed as `public/fonts/1001fonts-klarissa-eula.txt`. Font sites
  disagree about the status of this 2003 freeware release; the project ships it
  on the basis of the 1001Fonts FFC grant, as decided by the project owner on
  2026-11-18.

The logo (`public/logo.svg`, `public/favicon.svg`) is an original SVG drawing: a
pastel mint parallelogram with a ring-and-dot mark, no font outlines.

## Design reference

The interface follows GTK4/libadwaita. The colour values, corner radii and
shadows in `style/tokens.css` come from libadwaita's public stylesheet
(`src/stylesheet/_colors.scss`, `_palette.scss`, `_common.scss`). libadwaita is
LGPL-2.1-only. No code was copied; only design values such as colour constants
and corner radii are used.
Reference: https://gitlab.gnome.org/GNOME/libadwaita

## Third-party crates

Code from crates.io dependencies is used under each crate's own license. All of
them are permissive and none is copyleft. The full license texts ship with the
crates in the cargo registry and are linked from each crate's crates.io page.
`cargo deny check` can audit this (`cargo install cargo-deny`).

License groups from `cargo metadata`, with ChefDAV's own crates excluded:

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

No GPL, AGPL or LGPL crate is in the dependency tree. If one is ever added,
ChefDAV's MIT license stays valid, but that crate's distribution terms apply and
belong in this file.

## Infrastructure images (dev only)

- `tomsquest/docker-radicale` — AGPL-3.0 (the server)
- `caddy:2-alpine` — Apache-2.0
- `glauth/glauth` — MIT (LDAP fixture)
