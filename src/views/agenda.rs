//! Agenda view: a chronological list of occurrences for the next 30 days,
//! grouped by day.

use std::collections::BTreeMap;

use caldav::recurrence::expand_series;
use caldav::time::{agenda_range, today};
use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use leptos::prelude::*;

use crate::browser;
use crate::state::AppState;

#[component]
pub fn AgendaView() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;
    let today_d = today(&tz, browser::now_utc());

    let groups = Memo::new(move |_| {
        let anchor = state.anchor.get();
        let (rs, re) = agenda_range(anchor, 31, &tz);
        let events = state.events.get();
        let mut map: BTreeMap<
            NaiveDate,
            Vec<(
                usize,
                String,
                String,
                String,
                bool,
                Option<chrono::DateTime<chrono_tz::Tz>>,
                Option<chrono::DateTime<chrono_tz::Tz>>,
            )>,
        > = BTreeMap::new();
        for (i, ev) in events.iter().enumerate() {
            let is_all_day = ev
                .event
                .dtstart
                .as_ref()
                .map(|d| d.is_date())
                .unwrap_or(false);
            for occ in expand_series(&ev.event, &ev.overrides, rs, re, &tz) {
                let start_d = occ.start.with_timezone(&tz).date_naive();
                let end_d = (occ.end - Duration::seconds(1)).with_timezone(&tz).date_naive();
                let summary0 = occ
                    .override_idx
                    .and_then(|idx| ev.overrides.get(idx))
                    .map(|o| o.summary.clone())
                    .unwrap_or_else(|| ev.event.summary.clone());
                // list the occurrence on every day it spans, with ← / →
                // continuation hints on the title
                let mut dd = start_d;
                while dd <= end_d {
                    let cont_start = dd > start_d;
                    let cont_end = dd < end_d;
                    let time = if is_all_day {
                        crate::i18n::t(state.lang.get(), "all_day").to_string()
                    } else {
                        // clamp the shown range to this day
                        let day0 = tz
                            .from_local_datetime(&dd.and_hms_opt(0, 0, 0).unwrap())
                            .earliest()
                            .unwrap_or(occ.start);
                        let day1 = tz
                            .from_local_datetime(&(dd + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
                            .earliest()
                            .unwrap_or(occ.end);
                        let s = if cont_start { day0 } else { occ.start }.with_timezone(&tz);
                        let e = if cont_end { day1 } else { occ.end }.with_timezone(&tz);
                        format!("{}–{}", s.format("%H:%M"), e.format("%H:%M"))
                    };
                    let mut summary = summary0.clone();
                    if cont_start {
                        summary = format!("← {summary}");
                    }
                    if cont_end {
                        summary = format!("{summary} →");
                    }
                    map.entry(dd).or_default().push((
                        i,
                        time,
                        summary,
                        ev.color.clone(),
                        is_all_day,
                        occ.recurrence_id,
                        Some(occ.start),
                    ));
                    dd += Duration::days(1);
                }
            }
        }
        for v in map.values_mut() {
            v.sort_by_key(|(_, _, _, _, _, occ, ds)| (*occ, *ds));
        }
        map.into_iter().collect::<Vec<_>>()
    });

    let day_label = move |d: NaiveDate| {
        let wd = crate::i18n::weekday_short(state.lang.get(), d.weekday().num_days_from_monday() as usize);
        format!("{wd} {} {}", d.day(), crate::i18n::month_name(state.lang.get(), d.month()))
    };

    view! {
        <div class="agenda">
            <Show
                when=move || !groups.get().is_empty()
                fallback=move || {
                    view! {
                        <div class="empty-state">
                            <p class="empty-title">{move || crate::i18n::t(state.lang.get(), "no_events")}</p>
                            <p class="empty-hint">{move || crate::i18n::t(state.lang.get(), "no_events_next")}</p>
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
                                        <span class="today-pill">{move || crate::i18n::t(state.lang.get(), "today_pill")}</span>
                                    </Show>
                                </h3>
                                <ul class="agenda-list">
                                    <For
                                        each=move || items.clone()
                                        key=|(i, t, s, ..)| (*i, t.clone(), s.clone())
                                        children=move |(idx, time, summary, color, all_day, occurrence, display_start)| {
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
                                                                state.open_details(idx, occurrence, display_start)
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
                                                            <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")>"🔒"</span>
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
