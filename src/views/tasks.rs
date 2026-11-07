//! Tasks (VTODO) view: grouped by due date, with completion toggles.

use std::collections::BTreeMap;

use caldav::ics::DtValue;
use caldav::time::month_name;
use chrono::{Datelike, NaiveDate};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::state::{AppState, ModalState, TodoItem};

fn due_date(t: &TodoItem, tz: &chrono_tz::Tz) -> Option<NaiveDate> {
    match &t.todo.due {
        Some(DtValue::Date(d)) => Some(*d),
        Some(DtValue::DateTime(dt)) => Some(dt.with_timezone(tz).date_naive()),
        None => None,
    }
}

#[component]
pub fn TasksView() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;

    let groups = Memo::new(move |_| {
        let mut map: BTreeMap<Option<NaiveDate>, Vec<(usize, TodoItem)>> = BTreeMap::new();
        for (i, t) in state.todos.get().iter().enumerate() {
            map.entry(due_date(t, &tz)).or_default().push((i, t.clone()));
        }
        map.into_iter().collect::<Vec<_>>()
    });

    let toggle = move |index: usize| {
        let st = state;
        spawn_local(async move {
            let _ = api::toggle_todo(&st, index).await;
            api::refresh_todos(&st).await;
        });
    };

    let label = |d: Option<NaiveDate>| match d {
        Some(d) => format!(
            "{} {} {}",
            caldav::time::WEEKDAYS_MON_FIRST[d.weekday().num_days_from_monday() as usize],
            d.day(),
            month_name(d.month())
        ),
        None => "No due date".to_string(),
    };

    view! {
        <div class="tasks-view">
            <div class="tasks-head">
                <h2 class="tasks-title">"Tasks"</h2>
                <button
                    class="btn btn-primary"
                    on:click=move |_| {
                        let date = caldav::time::today(&state.tz, crate::browser::now_utc());
                        state.modal.set(Some(ModalState::NewTodo { date }));
                    }
                >
                    "+ New task"
                </button>
            </div>
            <Show
                when=move || !groups.get().is_empty()
                fallback=|| {
                    view! {
                        <div class="empty-state">
                            <p class="empty-title">"All clear"</p>
                            <p class="empty-hint">"No tasks yet. Add one to get started."</p>
                        </div>
                    }
                }
            >
                <For
                    each=move || groups.get()
                    key=|(d, _)| *d
                    children=move |(d, _items)| {
                        // Read the group from the memo instead of capturing the
                        // initial `items` snapshot: the outer key (due date)
                        // stays the same when a task is added, so a captured
                        // Vec would never see the new task.
                        let items = Memo::new(move |_| {
                            groups
                                .get()
                                .into_iter()
                                .find(|(gd, _)| *gd == d)
                                .map(|(_, v)| v)
                                .unwrap_or_default()
                        });
                        view! {
                            <section class="task-group">
                                <h3 class="agenda-date">{label(d)}</h3>
                                <ul class="task-list">
                                    <For
                                        each=move || items.get()
                                        key=|(i, t)| (*i, t.etag.clone())
                                        children=move |(idx, t)| {
                                            let done = t.todo.is_done();
                                            let summary = t.todo.summary.clone();
                                            let color = t.color.clone();
                                            let priority = t.todo.priority;
                                            view! {
                                                <li class="task-item" class:done=done>
                                                    <input
                                                        type="checkbox"
                                                        prop:checked=done
                                                        disabled=t.read_only
                                                        on:change=move |_| toggle(idx)
                                                    />
                                                    <span class="cal-dot" style=format!("background:{color}")></span>
                                                    <button
                                                        class="task-title"
                                                        on:click=move |_| {
                                                            state.modal.set(Some(ModalState::EditTodo { index: idx }))
                                                        }
                                                    >
                                                        {summary}
                                                    </button>
                                                    <Show when=move || (priority.unwrap_or(0) > 0) fallback=|| ()>
                                                        <span class="task-prio">{format!("P{}", priority.unwrap_or(0))}</span>
                                                    </Show>
                                                    <Show when=move || t.read_only fallback=|| ()>
                                                        <span class="lock" title="Read-only">"🔒"</span>
                                                    </Show>
                                                </li>
                                            }
                                        }
                                    />
                                </ul>
                            </section>
                        }
                    }
                />
            </Show>
        </div>
    }
}
