//! The keyboard shortcuts button and its AdwShortcutsDialog-style panel.
//!
//! The dialog is mounted by the Shell next to the other modals, not inside the
//! sidebar: a fixed overlay inside a grid item paints below the main content.

use leptos::prelude::*;

use crate::components::icons::{IconClose, IconKeyboard};
use crate::state::AppState;

/// One shortcut: a label on the left, keycaps on the right.
fn shortcut_row(label: &'static str, keys: &'static [&'static str]) -> impl IntoView {
    view! {
        <div class="shortcut-row">
            <span class="shortcut-label">{label}</span>
            <span class="shortcut-keys">
                {keys.iter().map(|key| view! { <kbd>{*key}</kbd> }).collect_view()}
            </span>
        </div>
    }
}

/// The sidebar button that opens the shortcut list.
#[component]
pub fn ShortcutsButton() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();

    view! {
        <button
            class="btn btn-block shortcuts-open"
            on:click=move |_| state.shortcuts_open.set(true)
        >
            <IconKeyboard />
            <span>{move || crate::i18n::t(state.lang.get(), "shortcuts")}</span>
        </button>
    }
}

/// The shortcut list itself, in the shape of an AdwShortcutsDialog.
#[component]
pub fn ShortcutsDialog() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let t = move |key: &'static str| crate::i18n::t(state.lang.get(), key);

    view! {
        <div class="modal-overlay" on:click=move |_| state.shortcuts_open.set(false)>
            <div
                class="modal shortcuts-dialog"
                role="dialog"
                aria-modal="true"
                aria-label=move || t("shortcuts")
                on:click=move |ev| ev.stop_propagation()
            >
                <h3>{move || t("shortcuts")}</h3>
                <button
                    class="modal-close"
                    title=move || t("close")
                    aria-label=move || t("close")
                    on:click=move |_| state.shortcuts_open.set(false)
                >
                    <IconClose />
                </button>

                <div class="shortcut-section">
                    <h4>{move || t("shortcuts_general")}</h4>
                    {shortcut_row(t("shortcut_new_event"), &["n"])}
                    {shortcut_row(t("shortcut_today"), &["t"])}
                    {shortcut_row(t("shortcut_search"), &["/"])}
                    {shortcut_row(t("shortcut_close"), &["Esc"])}
                </div>

                <div class="shortcut-section">
                    <h4>{move || t("shortcuts_views")}</h4>
                    {shortcut_row(t("shortcut_month"), &["m"])}
                    {shortcut_row(t("shortcut_week"), &["w"])}
                    {shortcut_row(t("shortcut_day"), &["d"])}
                    {shortcut_row(t("shortcut_list"), &["a"])}
                    {shortcut_row(t("shortcut_tasks"), &["k"])}
                </div>

                <div class="shortcut-section">
                    <h4>{move || t("shortcuts_navigation")}</h4>
                    {shortcut_row(t("shortcut_prev_period"), &["←"])}
                    {shortcut_row(t("shortcut_next_period"), &["→"])}
                </div>

                <p class="shortcuts-hint">{move || t("shortcuts_hint")}</p>
            </div>
        </div>
    }
}
