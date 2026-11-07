//! "New calendar" modal → MKCALENDAR on the server.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::state::AppState;

#[component]
pub fn NewCalendarModal() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let name = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let close = move |_| state.new_calendar.set(false);

    let perform_save = move || {
        let n = name.get().trim().to_string();
        if n.is_empty() || busy.get() {
            return;
        }
        busy.set(true);
        error.set(None);
        let st = state;
        spawn_local(async move {
            match api::create_calendar(&st, &n).await {
                Ok(()) => {
                    st.new_calendar.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("{e}")));
                    busy.set(false);
                }
            }
        });
    };
    let save_click = move |_| perform_save();
    let save_enter = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" {
            perform_save();
        }
    };

    view! {
        <div class="modal-overlay" on:click=close>
            <div class="modal modal-small" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                <h3>{move || crate::i18n::t(state.lang.get(), "new_calendar")}</h3>
                <label class="field">
                    <span>{move || crate::i18n::t(state.lang.get(), "name")}</span>
                    <input
                        type="text"
                        autofocus
                        placeholder=move || crate::i18n::t(state.lang.get(), "calendar_name_placeholder")
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                        on:keydown=save_enter
                    />
                </label>
                <Show when=move || error.get().is_some() fallback=|| ()>
                    <p class="form-error">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="modal-actions">
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>{move || crate::i18n::t(state.lang.get(), "cancel")}</button>
                    <button class="btn btn-primary" disabled=move || busy.get() on:click=save_click>
                        {move || if busy.get() { crate::i18n::t(state.lang.get(), "creating") } else { crate::i18n::t(state.lang.get(), "create") }}
                    </button>
                </div>
            </div>
        </div>
    }
}
