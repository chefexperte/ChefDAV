//! Shared toolbar: navigation, adaptive title, search, view switcher and
//! bulk-selection actions.

use caldav::time::week_days;
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
            crate::i18n::month_title(state.lang.get(), d.month(), d.year())
        }
        ViewMode::Week => crate::i18n::week_label(state.lang.get(), &week_days(state.anchor.get())),
        ViewMode::Day => crate::i18n::day_label(state.lang.get(), state.anchor.get()),
        ViewMode::Agenda => {
            let d = state.anchor.get();
            crate::i18n::agenda_title(state.lang.get(), d)
        }
        ViewMode::Tasks => crate::i18n::t(state.lang.get(), "tasks").to_string(),
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

    // Anchor the results panel under the search input instead of the
    // toolbar's right edge (the view switcher sits between them).
    let search_input = NodeRef::<leptos::html::Input>::new();
    let search_panel = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        if !state.search_open.get() {
            return;
        }
        let (Some(input), Some(panel)) = (search_input.get(), search_panel.get()) else {
            return;
        };
        let Some(wrap) = panel.parent_element() else {
            return;
        };
        let ir = input.get_bounding_client_rect();
        let wr = wrap.get_bounding_client_rect();
        let vw = web_sys::window()
            .and_then(|w| w.inner_width().ok())
            .and_then(|v| v.as_f64())
            .unwrap_or(1280.0);
        let width = 460.0_f64.min(vw - 24.0);
        let viewport_left = (ir.right() - width)
            .max(12.0)
            .min((vw - width - 12.0).max(12.0));
        let left = viewport_left - wr.left();
        let top = ir.bottom() - wr.top() + 8.0;
        let style = web_sys::HtmlElement::style(&panel);
        let _ = style.set_property("left", &format!("{left}px"));
        let _ = style.set_property("top", &format!("{top}px"));
        let _ = style.set_property("width", &format!("{width}px"));
        let _ = style.set_property("right", "auto");
    });

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
                    <button class="btn btn-icon" on:click=prev title=move || crate::i18n::t(state.lang.get(), "previous")>"‹"</button>
                    <button class="btn btn-ghost" on:click=go_today>{move || crate::i18n::t(state.lang.get(), "today")}</button>
                    <button class="btn btn-icon" on:click=next title=move || crate::i18n::t(state.lang.get(), "next")>"›"</button>
                </div>
                <h2 class="month-title">{title}</h2>
                <div class="toolbar-right">
                    <Show when=move || (state.pending.get() > 0) fallback=|| ()>
                        <span class="pending-badge" title="Changes waiting to sync">
                            {move || format!("{} {}", state.pending.get(), crate::i18n::t(state.lang.get(), "pending"))}
                        </span>
                    </Show>
                    <Show when=move || state.loading.get() fallback=|| ()>
                        <span class="loading">{move || crate::i18n::t(state.lang.get(), "syncing")}</span>
                    </Show>
                    <div class="search-box">
                        <input
                            node_ref=search_input
                            type="search"
                            placeholder=move || crate::i18n::t(state.lang.get(), "search_placeholder")
                            prop:value=move || state.search_query.get()
                            on:input=on_search
                        />
                        <Show when=move || !state.search_query.get().is_empty() fallback=|| ()>
                            <button class="search-clear" on:click=clear_search title=move || crate::i18n::t(state.lang.get(), "clear")>"×"</button>
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
                                        {move || crate::i18n::t(state.lang.get(), v.label_key())}
                                    </button>
                                }
                            }
                        />
                    </div>
                </div>
            </div>

            <Show when=move || state.search_open.get() fallback=|| ()>
                <div class="search-panel" node_ref=search_panel>
                    <Show
                        when=move || !state.search_results.get().is_empty()
                        fallback=move || {
                            view! { <p class="search-empty">{move || crate::i18n::t(state.lang.get(), "no_matching_events")}</p> }
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
                                let date_label = crate::i18n::day_label(state.lang.get(), date);
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
                                                state.modal.set(Some(ModalState::Details {
                                                    index: idx,
                                                    occurrence: None,
                                                    display_start: None,
                                                }));
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
                        {move || format!("{} {}", state.selected.get().len(), crate::i18n::t(state.lang.get(), "selected"))}
                    </span>
                    <button class="btn btn-danger" on:click=delete_selected>{move || crate::i18n::t(state.lang.get(), "delete_selected")}</button>
                    <button class="btn btn-ghost" on:click=clear_selection>{move || crate::i18n::t(state.lang.get(), "clear")}</button>
                </div>
            </Show>
        </div>
    }
}
