//! Week/day view: Monday-first 7-day (or 1-day) grid with an all-day row and
//! a 24-hour timeline. At most two events are shown side by side; the rest are
//! collected behind a "+N more" badge so columns never become unreadable.

use std::collections::HashMap;

use caldav::recurrence::{expand_series, Occurrence};
use caldav::time::{agenda_range, today, week_days, week_range};
use chrono::{Datelike, Duration, NaiveDate, Timelike};
use leptos::prelude::*;

use crate::browser;
use crate::state::{AppState, ModalState};

const HOUR_HEIGHT: f64 = 42.0; // px per hour in the timeline
const MAX_COLUMNS: usize = 2;

fn minutes_of_day(t: chrono::DateTime<chrono_tz::Tz>, tz: &chrono_tz::Tz) -> f64 {
    let l = t.with_timezone(tz);
    (l.hour() * 60 + l.minute()) as f64
}

/// Assign overlapping events to columns (greedy interval partitioning).
/// Returns `(column, column_count, event_index)`; events with `column >=
/// MAX_COLUMNS` are hidden and shown behind a "+N more" badge.
fn layout(events: &[Occurrence]) -> Vec<(usize, usize, usize)> {
    let mut order: Vec<usize> = (0..events.len()).collect();
    order.sort_by_key(|&i| events[i].start);
    let mut result = vec![(0usize, 1usize, 0usize); events.len()];
    let mut active: Vec<(usize, chrono::DateTime<chrono_tz::Tz>)> = Vec::new();
    let mut max_col = 0;
    for &i in &order {
        let start = events[i].start;
        active.retain(|(_, end)| *end > start);
        let used: Vec<usize> = active.iter().map(|(c, _)| *c).collect();
        let mut col = 0;
        while used.contains(&col) {
            col += 1;
        }
        max_col = max_col.max(col);
        active.push((col, events[i].end));
        result[i] = (col, 1, i);
    }
    let cols = (max_col + 1).min(MAX_COLUMNS);
    for r in &mut result {
        r.1 = cols;
    }
    result
}

#[component]
pub fn WeekView(#[prop(default = 7)] n_days: usize) -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;

    let days = Memo::new(move |_| {
        let anchor = state.anchor.get();
        if n_days == 7 {
            week_days(anchor)
        } else {
            vec![anchor]
        }
    });
    let today_d = today(&tz, browser::now_utc());
    let hours: Vec<u32> = (0..24).collect();

    let data = Memo::new(move |_| {
        let anchor = state.anchor.get();
        let (rs, re) = if n_days == 7 {
            week_range(anchor, &tz)
        } else {
            agenda_range(anchor, 1, &tz)
        };
        let events = state.events.get();
        let mut all_day: HashMap<NaiveDate, Vec<(usize, Occurrence, String)>> = HashMap::new();
        let mut timed: HashMap<NaiveDate, Vec<(usize, Occurrence, String)>> = HashMap::new();
        for (i, ev) in events.iter().enumerate() {
            let is_all_day = ev
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
            for occ in expand_series(&ev.event, &ev.overrides, rs, re, &tz) {
                if is_all_day {
                    let start_d = occ.start.with_timezone(&tz).date_naive();
                    let end_d = (occ.end - Duration::seconds(1)).with_timezone(&tz).date_naive();
                    let mut d = start_d;
                    while d <= end_d {
                        all_day
                            .entry(d)
                            .or_default()
                            .push((i, occ.clone(), fingerprint.clone()));
                        d += Duration::days(1);
                    }
                } else {
                    let d = occ.start.with_timezone(&tz).date_naive();
                    timed
                        .entry(d)
                        .or_default()
                        .push((i, occ.clone(), fingerprint.clone()));
                }
            }
        }
        for v in timed.values_mut() {
            v.sort_by_key(|(_, o, _)| o.start);
        }
        (all_day, timed)
    });

    let toggle_select = move |href: &str| {
        state.selected.update(|sel| {
            if let Some(pos) = sel.iter().position(|h| h == href) {
                sel.remove(pos);
            } else {
                sel.push(href.to_string());
            }
        });
    };

    view! {
        <div class="week-view" style=format!("--cols:{n_days}")>
            <div class="week-head">
                <div class="week-gutter"></div>
                <For
                    each=move || days.get()
                    key=|d| *d
                    children=move |d| {
                        let is_today = d == today_d;
                        view! {
                            <div class="week-day-head" class:today=is_today>
                                <span class="wd-name">
                                    {crate::i18n::weekday_short(state.lang.get(), d.weekday().num_days_from_monday() as usize)}
                                </span>
                                <span class="wd-num">{d.day()}</span>
                            </div>
                        }
                    }
                />
            </div>

            <div class="week-allday">
                <div class="week-gutter">{move || crate::i18n::t(state.lang.get(), "all_day_lower")}</div>
                <For
                    each=move || days.get()
                    key=|d| *d
                    children=move |d| {
                        let items = move || data.get().0.get(&d).cloned().unwrap_or_default();
                        view! {
                            <div
                                class="allday-cell"
                                on:click=move |_| {
                                    state.new_event_all_day.set(true);
                                    state.new_event_time.set(None);
                                    state.modal.set(Some(ModalState::New { date: d }));
                                }
                            >
                                <For
                                    each=items
                                    key=|(i, o, fp)| (*i, o.start, fp.clone())
                                    children=move |(idx, occ, _)| {
                                        let ev = state.events.get().get(idx).cloned();
                                        let (summary, color, href, read_only, recurring) = match &ev {
                                            Some(e) => {
                                                let summary = occ
                                                    .override_idx
                                                    .and_then(|i| e.overrides.get(i))
                                                    .map(|o| o.summary.clone())
                                                    .unwrap_or_else(|| e.event.summary.clone());
                                                (summary, e.color.clone(), e.href.clone(), e.read_only, e.event.rrule.is_some())
                                            }
                                            None => (String::new(), crate::state::ACCENT.into(), String::new(), true, false),
                                        };
                                        let occurrence = if recurring { occ.recurrence_id } else { None };
                                        let display_start = if recurring { Some(occ.start) } else { None };
                                        let selected = Memo::new({
                                            let href = href.clone();
                                            move |_| state.selected.get().iter().any(|h| h == &href)
                                        });
                                        view! {
                                            <div
                                                class="allday-chip"
                                                class:selected=move || selected.get()
                                                style=format!("--chip:{color}")
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    if e.ctrl_key() || e.meta_key() {
                                                        toggle_select(&href);
                                                    } else {
                                                        state.modal.set(Some(ModalState::Details { index: idx, occurrence, display_start }));
                                                    }
                                                }
                                            >
                                                {summary}
                                                <Show when=move || read_only fallback=|| ()>
                                                    <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")>"🔒"</span>
                                                </Show>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        }
                    }
                />
            </div>

            <div class="week-timeline" style=format!("--hour-height:{}px", HOUR_HEIGHT)>
                <div class="time-gutter">
                    <For
                        each=move || hours.clone()
                        key=|h| *h
                        children=|h| {
                            view! { <div class="hour-label">{format!("{h:02}:00")}</div> }
                        }
                    />
                </div>
                <For
                    each=move || days.get()
                    key=|d| *d
                    children=move |d| {
                        let items = Memo::new(move |_| {
                            data.get().1.get(&d).cloned().unwrap_or_default()
                        });
                        let laid_out = Memo::new(move |_| {
                            let items = items.get();
                            let occs: Vec<Occurrence> = items.iter().map(|(_, o, _)| o.clone()).collect();
                            let layout = layout(&occs);
                            items
                                .into_iter()
                                .zip(layout)
                                .map(|((i, o, fp), (col, cols, _))| (i, o, fp, col, cols))
                                .collect::<Vec<_>>()
                        });
                        let visible = Memo::new(move |_| {
                            laid_out.get().into_iter().filter(|(_, _, _, col, _)| *col < MAX_COLUMNS).collect::<Vec<_>>()
                        });
                        let hidden = Memo::new(move |_| {
                            laid_out.get().into_iter().filter(|(_, _, _, col, _)| *col >= MAX_COLUMNS).collect::<Vec<_>>()
                        });
                        let create_at = move |ev: leptos::ev::MouseEvent| {
                            let y = ev.offset_y() as f64;
                            let minutes = ((y / HOUR_HEIGHT) * 60.0 / 30.0).round() as i64 * 30;
                            let minutes = minutes.clamp(0, 23 * 60 + 30);
                            state.new_event_all_day.set(false);
                            state.new_event_time.set(Some(((minutes / 60) as u32, (minutes % 60) as u32)));
                            state.modal.set(Some(ModalState::New { date: d }));
                        };
                        view! {
                            <div
                                class="day-column"
                                class:today=d == today_d
                                on:click=create_at
                            >
                                <For
                                    each=move || visible.get()
                                    key=|(i, o, fp, _, _)| (*i, o.start, fp.clone())
                                    children=move |(idx, occ, _, col, cols)| {
                                        let ev = state.events.get().get(idx).cloned();
                                        let (summary, color, href, read_only, recurring) = match &ev {
                                            Some(e) => {
                                                let summary = occ
                                                    .override_idx
                                                    .and_then(|i| e.overrides.get(i))
                                                    .map(|o| o.summary.clone())
                                                    .unwrap_or_else(|| e.event.summary.clone());
                                                (summary, e.color.clone(), e.href.clone(), e.read_only, e.event.rrule.is_some())
                                            }
                                            None => (String::new(), crate::state::ACCENT.into(), String::new(), true, false),
                                        };
                                        let occurrence = if recurring { occ.recurrence_id } else { None };
                                        let display_start = if recurring { Some(occ.start) } else { None };
                                        let start_min = minutes_of_day(occ.start, &tz);
                                        let end_min = minutes_of_day(occ.end, &tz);
                                        let dur = (end_min - start_min).max(20.0);
                                        let top = start_min / 60.0 * HOUR_HEIGHT;
                                        let height = dur / 60.0 * HOUR_HEIGHT;
                                        let width = 100.0 / cols as f64;
                                        let left = col as f64 * width;
                                        let time = occ.start.with_timezone(&tz).format("%H:%M").to_string();
                                        let draggable = !read_only && !recurring;
                                        let drag_href = href.clone();
                                        let selected = Memo::new({
                                            let href = href.clone();
                                            move |_| state.selected.get().iter().any(|h| h == &href)
                                        });
                                        view! {
                                            <div
                                                class="timed-event"
                                                class:selected=move || selected.get()
                                                draggable=draggable
                                                style=format!(
                                                    "--chip:{color};top:{top}px;height:{height}px;left:{left}%;width:calc({width}% - 3px)"
                                                )
                                                on:dragstart=move |ev| {
                                                    if let Some(dt) = ev.data_transfer() {
                                                        let _ = dt.set_data("text/plain", &drag_href);
                                                    }
                                                }
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    if e.ctrl_key() || e.meta_key() {
                                                        toggle_select(&href);
                                                    } else {
                                                        state.modal.set(Some(ModalState::Details { index: idx, occurrence, display_start }));
                                                    }
                                                }
                                            >
                                                <span class="te-time">{time}</span>
                                                <span class="te-title">{summary}</span>
                                                <Show when=move || read_only fallback=|| ()>
                                                    <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")>"🔒"</span>
                                                </Show>
                                            </div>
                                        }
                                    }
                                />
                                <Show when=move || !hidden.get().is_empty() fallback=|| ()>
                                    {
                                        let hidden = hidden.clone();
                                        view! {
                                            <button
                                                class="more-badge"
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    let indices = hidden.get().iter().map(|(i, _, _, _, _)| *i).collect::<Vec<_>>();
                                                    state.week_more.set(Some(indices));
                                                }
                                            >
                                                {move || format!("+{} {}", hidden.get().len(), crate::i18n::t(state.lang.get(), "more"))}
                                            </button>
                                        }
                                    }
                                </Show>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}
