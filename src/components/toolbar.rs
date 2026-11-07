//! Shared toolbar: navigation, adaptive title, search, view switcher and
//! bulk-selection actions.

use caldav::time::{day_label, month_name, week_days, week_label};
use chrono::{Datelike, Duration, NaiveDate};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::browser;
use crate::state::{AppState, ModalState, ViewMode};

fn shift_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let total = date.year() * 12 + (date.month() as i32 - 1) + delta;
    let y = total.div_euclid(12);
    let m = (total.rem_euclid(12) + 1) as u32;
    NaiveDate::from_ymd_opt(y, m, 1).unwrap_or(date)
}

#[component]
pub fn Toolbar() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();

    let prev = move |_| {
        let d = state.anchor.get();
        state.anchor.set(match state.view.get() {
            ViewMode::Month => shift_month(d, -1),
            ViewMode::Week | ViewMode::Agenda => d - Duration::days(7),
            ViewMode::Day => d - Duration::days(1),
            ViewMode::Tasks => d,
        });
    };
    let next = move |_| {
        let d = state.anchor.get();
        state.anchor.set(match state.view.get() {
            ViewMode::Month => shift_month(d, 1),
            ViewMode::Week | ViewMode::Agenda => d + Duration::days(7),
            ViewMode::Day => d + Duration::days(1),
            ViewMode::Tasks => d,
        });
    };
    let go_today = move |_| {
        state
            .anchor
            .set(caldav::time::today(&state.tz, browser::now_utc()));
    };

    let title = Memo::new(move |_| match state.view.get() {
        ViewMode::Month => {
            let d = state.anchor.get();
            format!("{} {}", month_name(d.month()), d.year())
        }
        ViewMode::Week => week_label(&week_days(state.anchor.get())),
        ViewMode::Day => day_label(state.anchor.get()),
        ViewMode::Agenda => {
            let d = state.anchor.get();
            format!("From {} {}", d.day(), month_name(d.month()))
        }
        ViewMode::Tasks => "Tasks".to_string(),
    });

    let on_search = move |ev| {
        let q = event_target_value(&ev);
        state.search_query.set(q.clone());
        state.search_open.set(!q.trim().is_empty());
        api::search(&state, &q);
    };
    let clear_search = move |_| {
        state.search_query.set(String::new());
        state.search_open.set(false);
        state.search_results.set(Vec::new());
    };

    let selected = Memo::new(move |_| state.selected.get());
    let clear_selection = move |_| state.selected.set(Vec::new());
    let delete_selected = move |_| {
        let hrefs = state.selected.get();
        if hrefs.is_empty() {
            return;
        }
        let st = state;
        spawn_local(async move {
            let _ = api::bulk_delete_events(&st, &hrefs).await;
            st.selected.set(Vec::new());
        });
    };

    view! {
        <div class="toolbar-wrap">
            <div class="toolbar">
                <div class="toolbar-left">
                    <button class="btn btn-icon" on:click=prev title="Previous">"‹"</button>
                    <button class="btn btn-ghost" on:click=go_today>"Today"</button>
                    <button class="btn btn-icon" on:click=next title="Next">"›"</button>
                </div>
                <h2 class="month-title">{title}</h2>
                <div class="toolbar-right">
                    <Show when=move || (state.pending.get() > 0) fallback=|| ()>
                        <span class="pending-badge" title="Changes waiting to sync">
                            {move || format!("{} pending", state.pending.get())}
                        </span>
                    </Show>
                    <Show when=move || state.loading.get() fallback=|| ()>
                        <span class="loading">"syncing…"</span>
                    </Show>
                    <div class="search-box">
                        <input
                            type="search"
                            placeholder="Search events…"
                            prop:value=move || state.search_query.get()
                            on:input=on_search
                        />
                        <Show when=move || !state.search_query.get().is_empty() fallback=|| ()>
                            <button class="search-clear" on:click=clear_search title="Clear">"×"</button>
                        </Show>
                    </div>
                    <div class="segmented" role="tablist">
                        <For
                            each=|| [ViewMode::Month, ViewMode::Week, ViewMode::Day, ViewMode::Agenda, ViewMode::Tasks].to_vec()
                            key=|v| *v
                            children=move |v| {
                                view! {
                                    <button
                                        class="segmented-btn"
                                        class:active=move || state.view.get() == v
                                        on:click=move |_| state.view.set(v)
                                    >
                                        {v.label()}
                                    </button>
                                }
                            }
                        />
                    </div>
                </div>
            </div>

            <Show when=move || state.search_open.get() fallback=|| ()>
                <div class="search-panel">
                    <Show
                        when=move || !state.search_results.get().is_empty()
                        fallback=move || {
                            view! { <p class="search-empty">"No matching events."</p> }
                        }
                    >
                        <For
                            each=move || state.search_results.get()
                            key=|h| (h.href.clone(), h.date)
                            children=move |hit| {
                                let date = hit.date;
                                let href = hit.href.clone();
                                let time = hit.time.clone();
                                let summary = hit.summary.clone();
                                let location = hit.location.clone();
                                let has_location = !location.is_empty();
                                let location_text = location.clone();
                                let color = hit.color.clone();
                                let date_label = day_label(date);
                                view! {
                                    <button
                                        class="search-hit"
                                        style=format!("--chip:{color}")
                                        on:click=move |_| {
                                            state.anchor.set(date);
                                            state.view.set(ViewMode::Day);
                                            state.search_open.set(false);
                                            let idx = state.events.get().iter().position(|e| e.href == href);
                                            if let Some(idx) = idx {
                                                state.modal.set(Some(ModalState::Details { index: idx }));
                                            }
                                        }
                                    >
                                        <span class="agenda-dot"></span>
                                        <span class="search-summary">{summary}</span>
                                        <span class="search-when">{date_label}" · "{time}</span>
                                        <Show when=move || has_location fallback=|| ()>
                                            <span class="search-loc">{location_text.clone()}</span>
                                        </Show>
                                    </button>
                                }
                            }
                        />
                    </Show>
                </div>
            </Show>

            <Show when=move || !selected.get().is_empty() fallback=|| ()>
                <div class="selection-bar">
                    <span class="selection-count">
                        {move || format!("{} selected", state.selected.get().len())}
                    </span>
                    <button class="btn btn-danger" on:click=delete_selected>"Delete selected"</button>
                    <button class="btn btn-ghost" on:click=clear_selection>"Clear"</button>
                </div>
            </Show>
        </div>
    }
}
