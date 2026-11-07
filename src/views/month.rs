//! Month view: Monday-first grid, events expanded client-side (recurrence
//! included) and bucketed onto the days they span.

use std::collections::HashMap;

use caldav::recurrence::{expand_event, Occurrence};
use caldav::time::{month_range, month_weeks, today, WEEKDAYS_MON_FIRST};
use chrono::{Datelike, Duration, NaiveDate};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::browser;
use crate::state::{AppState, ModalState};

/// One event chip inside a day cell.
fn chip(
    state: AppState,
    tz: chrono_tz::Tz,
    idx: usize,
    occ: Occurrence,
    all_day: bool,
) -> impl IntoView {
    let ev = state.events.get().get(idx).cloned();
    let (summary, color, href, read_only, recurring) = match &ev {
        Some(e) => (
            e.event.summary.clone(),
            e.color.clone(),
            e.href.clone(),
            e.read_only,
            e.event.rrule.is_some(),
        ),
        None => (String::new(), crate::state::ACCENT.to_string(), String::new(), true, false),
    };
    let time = if all_day {
        "all day".to_string()
    } else {
        occ.start.with_timezone(&tz).format("%H:%M").to_string()
    };
    let draggable = !read_only && !recurring;
    let drag_href = href.clone();
    let selected = Memo::new({
        let href = href.clone();
        move |_| state.selected.get().iter().any(|h| h == &href)
    });
    view! {
        <div
            class="chip"
            class:selected=move || selected.get()
            style=format!("--chip:{color}")
            draggable=draggable
            on:dragstart=move |ev| {
                if let Some(dt) = ev.data_transfer() {
                    let _ = dt.set_data("text/plain", &drag_href);
                }
            }
            on:click=move |e| {
                e.stop_propagation();
                if e.ctrl_key() || e.meta_key() {
                    state.selected.update(|sel| {
                        if let Some(pos) = sel.iter().position(|h| h == &href) {
                            sel.remove(pos);
                        } else {
                            sel.push(href.clone());
                        }
                    });
                } else {
                    state.modal.set(Some(ModalState::Edit { index: idx }));
                }
            }
        >
            <span class="chip-time">{time}</span>
            <span class="chip-title">{summary}</span>
            <Show when=move || read_only fallback=|| ()>
                <span class="lock" title="Read-only">"🔒"</span>
            </Show>
        </div>
    }
}

#[component]
pub fn MonthView() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;
    let today_d = today(&tz, browser::now_utc());

    let weeks = Memo::new(move |_| {
        let d = state.anchor.get();
        month_weeks(d.year(), d.month())
    });

    let by_day = Memo::new(move |_| {
        let d = state.anchor.get();
        let (rs, re) = month_range(d.year(), d.month(), &tz);
        let events = state.events.get();
        let mut map: HashMap<NaiveDate, Vec<(usize, Occurrence, bool, String)>> = HashMap::new();
        for (i, ev) in events.iter().enumerate() {
            let all_day = ev
                .event
                .dtstart
                .as_ref()
                .map(|d| d.is_date())
                .unwrap_or(false);
            let fingerprint = format!(
                "{}|{}|{}|{}|{}",
                ev.href,
                ev.etag.as_deref().unwrap_or(""),
                ev.event.summary,
                ev.event.location,
                ev.event.description
            );
            for occ in expand_event(&ev.event, rs, re, &tz) {
                let start_d = occ.start.with_timezone(&tz).date_naive();
                let end_d = (occ.end - Duration::seconds(1)).with_timezone(&tz).date_naive();
                let mut d = start_d;
                while d <= end_d {
                    map.entry(d)
                        .or_default()
                        .push((i, occ.clone(), all_day, fingerprint.clone()));
                    d += Duration::days(1);
                }
            }
        }
        for v in map.values_mut() {
            v.sort_by_key(|(_, o, _, _)| o.start);
        }
        map
    });

    let on_drop = move |ev: leptos::ev::DragEvent, day: NaiveDate| {
        ev.prevent_default();
        let Some(dt) = ev.data_transfer() else { return };
        let Ok(href) = dt.get_data("text/plain") else { return };
        if href.is_empty() {
            return;
        }
        let Some(ev) = state.events.get().iter().find(|e| e.href == href).cloned() else {
            return;
        };
        let Some(start) = ev.event.dtstart.as_ref().and_then(|d| d.to_datetime(&tz)) else {
            return;
        };
        let delta = day.signed_duration_since(start.with_timezone(&tz).date_naive()).num_days();
        if delta == 0 {
            return;
        }
        let st = state;
        spawn_local(async move {
            match api::move_event(&st, &href, delta).await {
                Ok(_) => api::refresh_events(&st).await,
                Err(e) => st.error.set(Some(format!("{e}"))),
            }
        });
    };

    view! {
        <Show when=move || state.error.get().is_some() fallback=|| ()>
            <p class="banner-error">{move || state.error.get().unwrap_or_default()}</p>
        </Show>
        <div class="weekdays">
            <For
                each=|| WEEKDAYS_MON_FIRST.to_vec()
                key=|d| *d
                children=|d| view! { <span>{d}</span> }
            />
        </div>
        <div class="month-grid">
            <For
                each=move || weeks.get()
                key=|w| w[0]
                children=move |week| {
                    view! {
                        <div class="week">
                            <For
                                each=move || week.clone()
                                key=|d| *d
                                children=move |day| {
                                    let anchor = state.anchor.get();
                                    let in_month = day.year() == anchor.year() && day.month() == anchor.month();
                                    let is_today = day == today_d;
                                    let day_events = move || {
                                        by_day.get().get(&day).cloned().unwrap_or_default()
                                    };
                                    view! {
                                        <div
                                            class="day"
                                            class:out=!in_month
                                            class:today=is_today
                                            on:click=move |_| state.day_popup.set(Some(day))
                                            on:dragover=move |ev| ev.prevent_default()
                                            on:drop=move |ev| on_drop(ev, day)
                                        >
                                            <span class="day-num">{day.day()}</span>
                                            <div class="day-events">
                                                <For
                                                    each=day_events
                                                    key=|(i, o, _, fp)| (*i, o.start, fp.clone())
                                                    children=move |(idx, occ, all_day, _)| {
                                                        chip(state, tz, idx, occ, all_day)
                                                    }
                                                />
                                            </div>
                                        </div>
                                    }
                                }
                            />
                        </div>
                    }
                }
            />
        </div>
    }
}
