//! Create / edit / delete a VTODO (task).

use caldav::ics::{DtValue, TodoDraft};
use chrono::NaiveDate;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::browser;
use crate::state::{AppState, ModalState};

#[component]
pub fn TodoModal() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;

    let modal = state.modal.get().expect("modal is open");
    let existing = match modal {
        ModalState::EditTodo { index } => state.todos.get().get(index).cloned(),
        _ => None,
    };
    let is_edit = existing.is_some();
    let default_date = match modal {
        ModalState::NewTodo { date } => Some(date),
        _ => None,
    };

    let (summary0, desc0, due0, completed0, priority0, cal0, uid0) = match &existing {
        Some(t) => {
            let due = match &t.todo.due {
                Some(DtValue::Date(d)) => Some(*d),
                Some(DtValue::DateTime(dt)) => Some(dt.with_timezone(&tz).date_naive()),
                None => None,
            };
            (
                t.todo.summary.clone(),
                t.todo.description.clone(),
                due,
                t.todo.is_done(),
                t.todo.priority.unwrap_or(0),
                t.calendar_href.clone(),
                t.todo.uid.clone(),
            )
        }
        None => (
            String::new(),
            String::new(),
            default_date,
            false,
            0,
            state
                .first_writable_todo_calendar()
                .map(|c| c.info.href)
                .unwrap_or_default(),
            browser::new_uid(),
        ),
    };

    let summary = RwSignal::new(summary0);
    let description = RwSignal::new(desc0);
    let due = RwSignal::new(due0.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default());
    let completed = RwSignal::new(completed0);
    let priority = RwSignal::new(priority0);
    let cal_href = RwSignal::new(cal0);
    let uid = RwSignal::new(uid0);
    let existing = RwSignal::new(existing);
    let saving = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let read_only = Memo::new(move |_| {
        existing.get().map(|t| t.read_only).unwrap_or_else(|| {
            state
                .calendars
                .get()
                .iter()
                .find(|c| c.info.href == cal_href.get())
                .map(|c| c.read_only())
                .unwrap_or(true)
        })
    });

    let close = move |_| state.modal.set(None);

    let save = move |_| {
        if saving.get() || read_only.get() {
            return;
        }
        error.set(None);
        let due_date = if due.get().trim().is_empty() {
            None
        } else {
            match NaiveDate::parse_from_str(&due.get(), "%Y-%m-%d") {
                Ok(d) => Some(d),
                Err(e) => {
                    error.set(Some(format!("Bad due date: {e}")));
                    return;
                }
            }
        };
        let draft = TodoDraft {
            uid: uid.get(),
            summary: summary.get().trim().to_string(),
            description: description.get(),
            due: due_date,
            completed: completed.get(),
            priority: if priority.get() > 0 {
                Some(priority.get())
            } else {
                None
            },
        };
        saving.set(true);
        let st = state;
        let cal = cal_href.get();
        let href = existing.get().map(|t| t.href);
        let etag = existing.get().and_then(|t| t.etag);
        spawn_local(async move {
            match api::save_todo(&st, &cal, href.as_deref(), &draft, etag.as_deref()).await {
                Ok(queued) => {
                    if !queued {
                        api::refresh_todos(&st).await;
                    }
                    st.modal.set(None);
                }
                Err(e) => {
                    error.set(Some(format!("{e}")));
                    saving.set(false);
                }
            }
        });
    };

    let delete = move |_| {
        let Some(t) = existing.get() else { return };
        let st = state;
        spawn_local(async move {
            match api::delete_todo(&st, &t.href, t.etag.as_deref()).await {
                Ok(queued) => {
                    if !queued {
                        api::refresh_todos(&st).await;
                    }
                    st.modal.set(None);
                }
                Err(e) => error.set(Some(format!("{e}"))),
            }
        });
    };

    view! {
        <div class="modal-overlay" on:click=close>
            <div class="modal modal-small" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                <h3>{move || if is_edit { crate::i18n::t(state.lang.get(), "edit_task") } else { crate::i18n::t(state.lang.get(), "new_task") }}</h3>
                <Show when=move || read_only.get() fallback=|| ()>
                    <p class="banner-info">{move || crate::i18n::t(state.lang.get(), "read_only_calendar")}</p>
                </Show>
                <fieldset class="modal-fields" disabled=move || read_only.get()>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "task")}</span>
                        <input type="text" autofocus placeholder=move || crate::i18n::t(state.lang.get(), "what_needs_doing")
                            prop:value=move || summary.get()
                            on:input=move |ev| summary.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "due_date")}</span>
                        <input type="date"
                            prop:value=move || due.get()
                            on:input=move |ev| due.set(event_target_value(&ev)) />
                    </label>
                    <div class="field-row">
                        <label class="field field-time">
                            <span>{move || crate::i18n::t(state.lang.get(), "priority")}</span>
                            <input type="number" min="0" max="9"
                                prop:value=move || priority.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                                        priority.set(v.min(9));
                                    }
                                } />
                        </label>
                        <label class="field field-check">
                            <input type="checkbox"
                                prop:checked=move || completed.get()
                                on:change=move |ev| completed.set(event_target_checked(&ev)) />
                            <span>{move || crate::i18n::t(state.lang.get(), "done")}</span>
                        </label>
                    </div>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "calendar")}</span>
                        <select
                            prop:value=move || cal_href.get()
                            on:change=move |ev| cal_href.set(event_target_value(&ev))
                        >
                            <For
                                each=move || {
                                    state.calendars.get()
                                        .iter()
                                        .filter(|c| c.info.supports_todos() && (!c.read_only() || existing.get().map(|t| t.calendar_href == c.info.href).unwrap_or(false)))
                                        .map(|c| (c.info.href.clone(), c.info.display_name.clone()))
                                        .collect::<Vec<_>>()
                                }
                                key=|c| c.clone()
                                children=move |(href, name)| {
                                    view! { <option value=href>{name}</option> }
                                }
                            />
                        </select>
                    </label>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "notes")}</span>
                        <textarea rows="2" placeholder=move || crate::i18n::t(state.lang.get(), "optional")
                            prop:value=move || description.get()
                            on:input=move |ev| description.set(event_target_value(&ev)) />
                    </label>
                </fieldset>
                <Show when=move || error.get().is_some() fallback=|| ()>
                    <p class="form-error">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="modal-actions">
                    <Show when=move || is_edit && !read_only.get() fallback=|| ()>
                        <button class="btn btn-danger" on:click=delete>{move || crate::i18n::t(state.lang.get(), "delete")}</button>
                    </Show>
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>{move || crate::i18n::t(state.lang.get(), "cancel")}</button>
                    <Show when=move || !read_only.get() fallback=|| ()>
                        <button class="btn btn-primary" disabled=move || saving.get() on:click=save>
                            {move || if saving.get() { crate::i18n::t(state.lang.get(), "saving") } else { crate::i18n::t(state.lang.get(), "save") }}
                        </button>
                    </Show>
                </div>
            </div>
        </div>
    }
}
