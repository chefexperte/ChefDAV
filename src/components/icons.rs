//! Inline SVG icons.
//!
//! Every icon in the UI is a real SVG (never a text character or emoji): text
//! glyphs render differently on every platform, ignore `currentColor` and
//! `stroke-width`, and some fonts draw `＋`/`→` at wildly different weights.
//! These components keep the markup in one place so the set stays consistent.

use leptos::prelude::*;

/// A stroke icon at the standard UI weight (1.8–2 px on a 16-unit grid).
fn stroke(paths: &'static str) -> impl IntoView {
    view! {
        <svg
            class="icon"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.9"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d=paths />
        </svg>
    }
}

/// Chevron pointing left — "previous" in the toolbar and the pickers.
#[component]
pub fn IconChevronLeft() -> impl IntoView {
    stroke("M9.8 3.2 5 8l4.8 4.8")
}

/// Chevron pointing right — "next".
#[component]
pub fn IconChevronRight() -> impl IntoView {
    stroke("M6.2 3.2 11 8l-4.8 4.8")
}

/// Plus — create actions ("New event", the sidebar button).
#[component]
pub fn IconPlus() -> impl IntoView {
    stroke("M8 3.2v9.6M3.2 8h9.6")
}

/// Checkmark in a rounded box — "New task".
#[component]
pub fn IconTaskAdd() -> impl IntoView {
    view! {
        <svg
            class="icon"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.9"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d="M2.9 8.4 5.6 11l3.1-3.6" />
            <path d="M9.4 11h4.8M11.8 8.6v4.8" />
        </svg>
    }
}

/// Arrow pointing right — "go to day".
#[component]
pub fn IconArrowRight() -> impl IntoView {
    stroke("M3 8h9.4M8.6 4.2 12.4 8l-3.8 3.8")
}

/// Times/cross — clearing the search entry.
#[component]
pub fn IconClose() -> impl IntoView {
    stroke("M4 4l8 8M12 4l-8 8")
}

/// Sun — switch a dark theme back to light.
#[component]
pub fn IconSun() -> impl IntoView {
    view! {
        <svg
            class="icon"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.9"
            stroke-linecap="round"
            aria-hidden="true"
        >
            <circle cx="8" cy="8" r="3.1" />
            <path d="M8 1.4v1.6M8 13v1.6M1.4 8h1.6M13 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1" />
        </svg>
    }
}

/// Crescent moon — switch a light theme to dark.
#[component]
pub fn IconMoon() -> impl IntoView {
    stroke("M13 9.9A5.5 5.5 0 0 1 6.1 3a5.5 5.5 0 1 0 6.9 6.9Z")
}

/// Keyboard — opens the shortcuts dialog.
#[component]
pub fn IconKeyboard() -> impl IntoView {
    view! {
        <svg
            class="icon"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <rect x="1.7" y="3.6" width="12.6" height="8.8" rx="1.6" />
            <path d="M4.2 6.3h.01M6.4 6.3h.01M8.6 6.3h.01M10.8 6.3h.01M4.2 8.3h.01M6.4 8.3h.01M8.6 8.3h.01M10.8 8.3h.01M5 10.4h6" />
        </svg>
    }
}
