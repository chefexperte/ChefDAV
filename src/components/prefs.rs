//! Theme + language toggles, shared by the sidebar and the login page.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::icons::{IconMoon, IconSun};
use crate::state::{AppState, Lang, Theme};
use crate::store;

#[component]
pub fn PrefsRow() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();

    let toggle_theme = move |_| {
        let next = match state.theme.get() {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        state.theme.set(next);
        let st = state;
        spawn_local(async move {
            let mut s = store::settings_load().await;
            s.theme = st.theme.get().as_str().to_string();
            store::settings_save(&s).await;
        });
    };

    let toggle_lang = move |_| {
        let next = match state.lang.get() {
            Lang::En => Lang::De,
            Lang::De => Lang::En,
        };
        state.lang.set(next);
        let st = state;
        spawn_local(async move {
            let mut s = store::settings_load().await;
            s.lang = st.lang.get().code().to_string();
            store::settings_save(&s).await;
        });
    };

    // The label names the action, so the icon shows the theme you switch to.
    let theme_label = move || match state.theme.get() {
        Theme::Light => crate::i18n::t(state.lang.get(), "dark"),
        Theme::Dark => crate::i18n::t(state.lang.get(), "light"),
    };

    view! {
        <div class="prefs-row linked">
            <button class="btn" on:click=toggle_theme title=theme_label aria-label=theme_label>
                {move || match state.theme.get() {
                    Theme::Light => view! { <IconMoon /> }.into_any(),
                    Theme::Dark => view! { <IconSun /> }.into_any(),
                }}
                <span>{theme_label}</span>
            </button>
            <button
                class="btn"
                on:click=toggle_lang
                title=move || crate::i18n::t(state.lang.get(), "language")
                aria-label=move || crate::i18n::t(state.lang.get(), "language")
            >
                {move || match state.lang.get() {
                    Lang::En => crate::i18n::t(state.lang.get(), "english"),
                    Lang::De => crate::i18n::t(state.lang.get(), "german"),
                }}
            </button>
        </div>
    }
}
