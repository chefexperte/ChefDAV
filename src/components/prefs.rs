//! Theme + language toggles, shared by the sidebar and the login page.

use leptos::prelude::*;
use leptos::task::spawn_local;

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

    view! {
        <div class="prefs-row">
            <button class="btn btn-ghost" on:click=toggle_theme>
                {move || match state.theme.get() {
                    Theme::Light => crate::i18n::t(state.lang.get(), "dark"),
                    Theme::Dark => crate::i18n::t(state.lang.get(), "light"),
                }}
            </button>
            <button class="btn btn-ghost" on:click=toggle_lang>
                {move || state.lang.get().code().to_uppercase()}
            </button>
        </div>
    }
}
