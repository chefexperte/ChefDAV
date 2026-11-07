//! Rename / delete a calendar collection.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::state::{AppState, CalendarAction};

#[component]
pub fn CalendarActionModal() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let action = state.calendar_action.get().expect("calendar action is open");
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let (href, name, is_delete) = match &action {
        CalendarAction::Rename { href, name } => (href.clone(), name.clone(), false),
        CalendarAction::Delete { href, name } => (href.clone(), name.clone(), true),
    };
    let name_input = RwSignal::new(name.clone());
    let href_sig = RwSignal::new(href.clone());
    let close = move |_| state.calendar_action.set(None);

    let rename = move |_| {
        if busy.get() {
            return;
        }
        let n = name_input.get().trim().to_string();
        if n.is_empty() {
            error.set(Some("Name must not be empty.".into()));
            return;
        }
        busy.set(true);
        let st = state;
        let href = href_sig.get();
        spawn_local(async move {
            match api::rename_calendar(&st, &href, &n).await {
                Ok(()) => st.calendar_action.set(None),
                Err(e) => {
                    error.set(Some(format!("{e}")));
                    busy.set(false);
                }
            }
        });
    };

    let delete = move |_| {
        if busy.get() {
            return;
        }
        busy.set(true);
        let st = state;
        let href = href_sig.get();
        spawn_local(async move {
            match api::delete_calendar(&st, &href).await {
                Ok(()) => {
                    st.calendar_action.set(None);
                    api::refresh_events(&st).await;
                }
                Err(e) => {
                    error.set(Some(format!("{e}")));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <div class="modal-overlay" on:click=close>
            <div class="modal modal-small" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                <h3>{if is_delete { "Delete calendar" } else { "Rename calendar" }}</h3>
                <Show
                    when=move || is_delete
                    fallback=move || {
                        view! {
                            <label class="field">
                                <span>"Name"</span>
                                <input type="text" autofocus
                                    prop:value=move || name_input.get()
                                    on:input=move |ev| name_input.set(event_target_value(&ev)) />
                            </label>
                        }
                    }
                >
                    <p class="confirm-text">
                        "Delete " <strong>{name.clone()}</strong> " and all of its events and tasks? This cannot be undone."
                    </p>
                </Show>
                <Show when=move || error.get().is_some() fallback=|| ()>
                    <p class="form-error">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="modal-actions">
                    <Show when=move || is_delete fallback=|| ()>
                        <button class="btn btn-danger" disabled=move || busy.get() on:click=delete>
                            {move || if busy.get() { "Deleting…" } else { "Delete calendar" }}
                        </button>
                    </Show>
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>"Cancel"</button>
                    <Show when=move || !is_delete fallback=|| ()>
                        <button class="btn btn-primary" disabled=move || busy.get() on:click=rename>
                            {move || if busy.get() { "Saving…" } else { "Save" }}
                        </button>
                    </Show>
                </div>
            </div>
        </div>
    }
}
