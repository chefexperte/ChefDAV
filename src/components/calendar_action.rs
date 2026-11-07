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

    let (href, name, color, is_delete) = match &action {
        CalendarAction::Rename { href, name, color } => {
            (href.clone(), name.clone(), color.clone(), false)
        }
        CalendarAction::Delete { href, name } => {
            (href.clone(), name.clone(), String::new(), true)
        }
    };
    let name_input = RwSignal::new(name.clone());
    let color_sig = RwSignal::new(color.clone());
    let original_color = RwSignal::new(color.clone());
    let href_sig = RwSignal::new(href.clone());
    let confirm_delete = RwSignal::new(is_delete);
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
        let color = color_sig.get();
        let color_opt = if color != original_color.get() {
            Some(color.clone())
        } else {
            None
        };
        spawn_local(async move {
            match api::update_calendar(&st, &href, &n, color_opt.as_deref()).await {
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
                <h3>{move || if confirm_delete.get() { "Delete calendar" } else { "Rename calendar" }}</h3>
                <Show
                    when=move || confirm_delete.get()
                    fallback=move || {
                        view! {
                            <label class="field">
                                <span>"Name"</span>
                                <input type="text" autofocus
                                    prop:value=move || name_input.get()
                                    on:input=move |ev| name_input.set(event_target_value(&ev)) />
                            </label>
                            <div class="field">
                                <span>"Color"</span>
                                <div class="color-swatches">
                                    <For
                                        each=|| crate::state::PALETTE.to_vec()
                                        key=|c| c.clone()
                                        children=move |c| {
                                            let value = c.clone();
                                            let active = value.clone();
                                            view! {
                                                <button
                                                    type="button"
                                                    class="color-swatch"
                                                    class:active=move || color_sig.get() == active
                                                    style=format!("background:{value}")
                                                    data-color=value.clone()
                                                    on:click=move |_| color_sig.set(value.to_string())
                                                />
                                            }
                                        }
                                    />
                                    <input
                                        type="color"
                                        title="Custom color"
                                        prop:value=move || color_sig.get()
                                        on:input=move |ev| color_sig.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>
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
                    <Show
                        when=move || confirm_delete.get()
                        fallback=move || {
                            view! {
                                <button
                                    class="btn btn-danger"
                                    on:click=move |_| confirm_delete.set(true)
                                >
                                    "Delete calendar"
                                </button>
                            }
                        }
                    >
                        <button class="btn btn-danger" disabled=move || busy.get() on:click=delete>
                            {move || if busy.get() { "Deleting…" } else { "Delete calendar" }}
                        </button>
                    </Show>
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>"Cancel"</button>
                    <Show when=move || !confirm_delete.get() fallback=|| ()>
                        <button class="btn btn-primary" disabled=move || busy.get() on:click=rename>
                            {move || if busy.get() { "Saving…" } else { "Save" }}
                        </button>
                    </Show>
                </div>
            </div>
        </div>
    }
}
