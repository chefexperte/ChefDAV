//! Shared toolbar: navigation, adaptive title, search, view switcher and
//! bulk-selection actions.

use caldav::time::week_days;
use chrono::{Datelike, Duration, NaiveDate};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use leptos::task::spawn_local;

use crate::api;
use crate::browser;
use crate::state::{AppState, ViewMode};

/// Pending-write badge and the "syncing" text. Rendered in a fixed slot so
/// their appearance does not shift the search bar or the view switcher.
#[component]
fn SyncStatus() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    view! {
        <Show when=move || (state.pending.get() > 0) fallback=|| ()>
            <span class="pending-badge" title="Changes waiting to sync">
                {move || format!("{} {}", state.pending.get(), crate::i18n::t(state.lang.get(), "pending"))}
            </span>
        </Show>
        <Show when=move || state.loading.get() fallback=|| ()>
            <span class="loading">{move || crate::i18n::t(state.lang.get(), "syncing")}</span>
        </Show>
    }
}

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

    // The month view's title is split so the month and the year are each
    // clickable (month picker / year picker). Every other view keeps one title.
    let month_part = Memo::new(move |_| {
        crate::i18n::month_name(state.lang.get(), state.anchor.get().month()).to_string()
    });
    let year_part = Memo::new(move |_| state.anchor.get().year().to_string());
    // The picker opens centred under the field that was clicked, so pass the
    // clicked button's own rect (it used to be a hardcoded position).
    let open_menu = move |kind: &'static str, ev: leptos::ev::MouseEvent| {
        let cur = state.jump_open.get();
        if cur.map(|(k, _)| k) == Some(kind) {
            state.jump_open.set(None);
            return;
        }
        let target = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
        let rect = target.map(|el| {
            let r = el.get_bounding_client_rect();
            (r.left(), r.right(), r.bottom())
        });
        if let Some(rect) = rect {
            state.jump_open.set(Some((kind, rect)));
        }
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

    // Search runs from 2 characters (see `api::search`); a shorter query must
    // not claim "no matching events" — the panel says "keep typing" instead.
    let min_chars = 2;
    let query = Memo::new(move |_| state.search_query.get().trim().to_string());
    let too_short = Memo::new(move |_| {
        let n = query.get().chars().count();
        n > 0 && n < min_chars
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
        if state.cache_only.get() {
            return;
        }
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
                <button
                    class="btn btn-icon sidebar-toggle"
                    on:click=move |_| state.sidebar_open.set(true)
                    title=move || crate::i18n::t(state.lang.get(), "menu")
                    aria-label=move || crate::i18n::t(state.lang.get(), "menu")
                >
                    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" aria-hidden="true">
                        <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
                    </svg>
                </button>
                <div class="toolbar-left">
                    <button
                        class="btn btn-icon"
                        on:click=go_today
                        title=move || crate::i18n::t(state.lang.get(), "today")
                        aria-label=move || crate::i18n::t(state.lang.get(), "today")
                    >
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path>
                            <polyline points="9 22 9 12 15 12 15 22"></polyline>
                        </svg>
                    </button>
                    <button class="btn btn-icon" on:click=prev title=move || crate::i18n::t(state.lang.get(), "previous") aria-label=move || crate::i18n::t(state.lang.get(), "previous")>
                        <crate::components::icons::IconChevronLeft />
                    </button>
                    <button class="btn btn-icon" on:click=next title=move || crate::i18n::t(state.lang.get(), "next") aria-label=move || crate::i18n::t(state.lang.get(), "next")>
                        <crate::components::icons::IconChevronRight />
                    </button>
                </div>
                <Show
                    when=move || state.view.get() == ViewMode::Month
                    fallback=move || view! { <h2 class="month-title">{title}</h2> }
                >
                    <h2 class="month-title month-title-split">
                        <button
                            class="title-part"
                            on:click=move |ev| open_menu("month", ev)
                            title=move || crate::i18n::t(state.lang.get(), "jump_month")
                        >{month_part}</button>
                        <button
                            class="title-part"
                            on:click=move |ev| open_menu("year", ev)
                            title=move || crate::i18n::t(state.lang.get(), "jump_year")
                        >{year_part}</button>
                    </h2>
                </Show>
                <div class="toolbar-search">
                    <div class="toolbar-status toolbar-status-desktop">
                        <SyncStatus />
                    </div>
                    <div class="search-box">
                        <input
                            node_ref=search_input
                            type="search"
                            placeholder=move || crate::i18n::t(state.lang.get(), "search_placeholder")
                            prop:value=move || state.search_query.get()
                            on:input=on_search
                        />
                        <Show when=move || !state.search_query.get().is_empty() fallback=|| ()>
                            <button class="search-clear" on:click=clear_search title=move || crate::i18n::t(state.lang.get(), "clear")>
                                <crate::components::icons::IconClose />
                            </button>
                        </Show>
                    </div>
                </div>
                <div class="toolbar-right">
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
                    <div class="toolbar-status toolbar-status-mobile">
                        <SyncStatus />
                    </div>
                </div>
            </div>

            <Show when=move || state.search_open.get() fallback=|| ()>
                <div class="search-panel" node_ref=search_panel>
                    <Show when=move || too_short.get() fallback=move || ()>
                        <p class="search-empty search-hint">
                            {move || crate::i18n::t(state.lang.get(), "search_min_chars")}
                        </p>
                    </Show>
                    <Show when=move || !too_short.get() fallback=|| ()>
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
                                        on:click=move |e| {
                                            state.anchor.set(date);
                                            state.view.set(ViewMode::Day);
                                            state.search_open.set(false);
                                            let idx = state.events.get().iter().position(|e| e.href == href);
                                            if let Some(idx) = idx {
                                                state.open_details(idx, None, None, Some((e.client_x() as i32, e.client_y() as i32)));
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
                    </Show>
                </div>
            </Show>

            <Show when=move || !selected.get().is_empty() fallback=|| ()>
                <div class="selection-bar">
                    <span class="selection-count">
                        {move || format!("{} {}", state.selected.get().len(), crate::i18n::t(state.lang.get(), "selected"))}
                    </span>
                    <button
                        class="btn btn-danger"
                        disabled=move || state.cache_only.get()
                        on:click=delete_selected
                    >
                        {move || crate::i18n::t(state.lang.get(), "delete_selected")}
                    </button>
                    <button class="btn btn-ghost" on:click=clear_selection>{move || crate::i18n::t(state.lang.get(), "clear")}</button>
                </div>
            </Show>
        </div>
    }
}
