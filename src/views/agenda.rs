//! Agenda view: a chronological list of occurrences for the next 30 days,
//! grouped by day.

use std::collections::BTreeMap;

use caldav::recurrence::expand_event;
use caldav::time::{agenda_range, month_name, today, WEEKDAYS_MON_FIRST};
use chrono::{Datelike, NaiveDate};
use leptos::prelude::*;

use crate::browser;
use crate::state::{AppState, ModalState};

#[component]
pub fn AgendaView() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;
    let today_d = today(&tz, browser::now_utc());

    let groups = Memo::new(move |_| {
        let anchor = state.anchor.get();
        let (rs, re) = agenda_range(anchor, 31, &tz);
        let events = state.events.get();
        let mut map: BTreeMap<NaiveDate, Vec<(usize, String, String, String, bool)>> = BTreeMap::new();
        for (i, ev) in events.iter().enumerate() {
            let is_all_day = ev
                .event
                .dtstart
                .as_ref()
                .map(|d| d.is_date())
                .unwrap_or(false);
            for occ in expand_event(&ev.event, rs, re, &tz) {
                let d = occ.start.with_timezone(&tz).date_naive();
                let time = if is_all_day {
                    "All day".to_string()
                } else {
                    occ.start.with_timezone(&tz).format("%H:%M").to_string()
                };
                map.entry(d).or_default().push((
                    i,
                    time,
                    ev.event.summary.clone(),
                    ev.color.clone(),
                    is_all_day,
                ));
            }
        }
        for v in map.values_mut() {
            v.sort_by_key(|(_, t, ..)| t.clone());
        }
        map.into_iter().collect::<Vec<_>>()
    });

    let day_label = move |d: NaiveDate| {
        let wd = WEEKDAYS_MON_FIRST[d.weekday().num_days_from_monday() as usize];
        format!("{wd} {} {}", d.day(), month_name(d.month()))
    };

    view! {
        <div class="agenda">
            <Show
                when=move || !groups.get().is_empty()
                fallback=|| {
                    view! {
                        <div class="empty-state">
                            <p class="empty-title">"Nothing on the horizon"</p>
                            <p class="empty-hint">"No events in the next 30 days."</p>
                        </div>
                    }
                }
            >
                <For
                    each=move || groups.get()
                    key=|(d, _)| *d
                    children=move |(d, items)| {
                        let is_today = d == today_d;
                        view! {
                            <section class="agenda-day" class:today=is_today>
                                <h3 class="agenda-date">
                                    {day_label(d)}
                                    <Show when=move || is_today fallback=|| ()>
                                        <span class="today-pill">"today"</span>
                                    </Show>
                                </h3>
                                <ul class="agenda-list">
                                    <For
                                        each=move || items.clone()
                                        key=|(i, t, s, _, _)| (*i, t.clone(), s.clone())
                                        children=move |(idx, time, summary, color, all_day)| {
                                            let ev = state.events.get().get(idx).cloned();
                                            let location = ev
                                                .as_ref()
                                                .map(|e| e.event.location.clone())
                                                .unwrap_or_default();
                                            let has_location = !location.is_empty();
                                            let location_text = location.clone();
                                            let href = ev.as_ref().map(|e| e.href.clone()).unwrap_or_default();
                                            let read_only = ev.as_ref().map(|e| e.read_only).unwrap_or(false);
                                            let selected = Memo::new({
                                                let href = href.clone();
                                                move |_| state.selected.get().iter().any(|h| h == &href)
                                            });
                                            view! {
                                                <li>
                                                    <button
                                                        class="agenda-item"
                                                        class:selected=move || selected.get()
                                                        style=format!("--chip:{color}")
                                                        on:click=move |e| {
                                                            if e.ctrl_key() || e.meta_key() {
                                                                state.selected.update(|sel| {
                                                                    if let Some(pos) = sel.iter().position(|h| h == &href) {
                                                                        sel.remove(pos);
                                                                    } else {
                                                                        sel.push(href.clone());
                                                                    }
                                                                });
                                                            } else {
                                                                state.modal.set(Some(ModalState::Details { index: idx }))
                                                            }
                                                        }
                                                    >
                                                        <span class="agenda-time" class:all-day=all_day>
                                                            {time}
                                                        </span>
                                                        <span class="agenda-dot"></span>
                                                        <span class="agenda-title">{summary}</span>
                                                        <Show when=move || has_location fallback=|| ()>
                                                            <span class="agenda-loc">{location_text.clone()}</span>
                                                        </Show>
                                                        <Show when=move || read_only fallback=|| ()>
                                                            <span class="lock">"🔒"</span>
                                                        </Show>
                                                    </button>
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
