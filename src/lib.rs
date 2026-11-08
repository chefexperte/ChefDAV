//! ChefDAV — client-side-only CalDAV frontend (Rust → WASM).
//!
//! All protocol logic lives in the `caldav` crate (native-testable); this
//! crate is the browser shell: fetch transport, IndexedDB cache, encryption,
//! and the Leptos UI.

pub mod api;
pub mod app;
pub mod browser;
pub mod components;
pub mod config;
pub mod day_events;
pub mod i18n;
pub mod links;
pub mod location;
pub mod state;
pub mod store;
pub mod views;
