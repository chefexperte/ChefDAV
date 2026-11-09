//! Month view: Monday-first grid, events expanded client-side (recurrence
//! included) and bucketed onto the days they span.

use std::collections::{HashMap, HashSet};

use caldav::recurrence::{expand_series, Occurrence};
use caldav::time::{month_range, month_weeks, today};
use chrono::{Datelike, Duration, NaiveDate};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::browser;
use crate::state::{AppState, DayPopupState};

/// Max number of spanning-bar slots per week row in the month view. Kept at 2
/// so a day cell still has room for at least one chip / the "Show all" entry
/// (max 3 entries per cell in total).
const MAX_BAR_SLOTS: usize = 2;
/// Height of one spanning-bar slot (px); day cells reserve this much space.
const BAR_SLOT_H: usize = 20;

/// One rendered segment of a multi-day event bar, inside one week row.
#[derive(Clone, PartialEq)]
struct BarSeg {
    week: usize,
    slot: usize,
    c0: usize,
    c1: usize,
    idx: usize,
    occ: Occurrence,
    all_day: bool,
    fp: String,
    /// Segment starts at the occurrence's first day (not a continuation).
    true_start: bool,
    /// Segment ends at the occurrence's last day (does not continue).
    true_end: bool,
}

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
        Some(e) => {
            let summary = occ
                .override_idx
                .and_then(|i| e.overrides.get(i))
                .map(|o| o.summary.clone())
                .unwrap_or_else(|| e.event.summary.clone());
            (
                summary,
                e.color.clone(),
                e.href.clone(),
                e.read_only,
                e.event.rrule.is_some(),
            )
        }
        None => (
            String::new(),
            crate::state::ACCENT.to_string(),
            String::new(),
            true,
            false,
        ),
    };
    let time = if all_day {
        crate::i18n::t(state.lang.get(), "all_day_lower").to_string()
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
                    let occurrence = if recurring { occ.recurrence_id } else { None };
                    let display_start = if recurring { Some(occ.start) } else { None };
                    state.open_details(idx, occurrence, display_start, Some((e.client_x() as i32, e.client_y() as i32)));
                }
            }
        >
            <span class="chip-time">{time}</span>
            <span class="chip-title">{summary}</span>
            <Show when=move || read_only fallback=|| ()>
                <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")>"🔒"</span>
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

    let weekday_labels = Memo::new(move |_| {
        (0..7)
            .map(|i| crate::i18n::weekday_short(state.lang.get(), i).to_string())
            .collect::<Vec<String>>()
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
            for occ in expand_series(&ev.event, &ev.overrides, rs, re, &tz) {
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

    // Multi-day (or overnight) occurrences, rendered as bars spanning the day
    // cells. Each occurrence is split at week boundaries; per week row the
    // segments get greedy vertical slots (max `MAX_BAR_SLOTS` — anything that
    // does not fit stays a per-day chip). Also returns the set of
    // (event, occurrence, day) triples covered by a rendered bar, so the day
    // cells do not draw a duplicate chip on those days.
    let bars = Memo::new(move |_| {
        let weeks = weeks.get();
        let d = state.anchor.get();
        let (rs, re) = month_range(d.year(), d.month(), &tz);
        let events = state.events.get();
        let mut spans: Vec<(usize, Occurrence, bool, String, NaiveDate, NaiveDate)> = Vec::new();
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
            for occ in expand_series(&ev.event, &ev.overrides, rs, re, &tz) {
                let start_d = occ.start.with_timezone(&tz).date_naive();
                let end_d = (occ.end - Duration::seconds(1)).with_timezone(&tz).date_naive();
                if end_d > start_d {
                    spans.push((i, occ, all_day, fingerprint.clone(), start_d, end_d));
                }
            }
        }
        let mut out: Vec<BarSeg> = Vec::new();
        let mut covered: HashSet<(usize, chrono::DateTime<chrono_tz::Tz>, NaiveDate)> = HashSet::new();
        for (wi, week) in weeks.iter().enumerate() {
            let w0 = week[0];
            let w6 = week[6];
            let mut segs: Vec<(usize, usize, &usize, &Occurrence, bool, &String, bool, bool)> = Vec::new();
            for (i, occ, all_day, fp, start_d, end_d) in &spans {
                let s = (*start_d).max(w0);
                let e = (*end_d).min(w6);
                if s <= e {
                    segs.push((
                        (s - w0).num_days() as usize,
                        (e - w0).num_days() as usize,
                        i,
                        occ,
                        *all_day,
                        fp,
                        *start_d == s,
                        *end_d == e,
                    ));
                }
            }
            segs.sort_by_key(|(c0, c1, ..)| (*c0, *c1));
            let mut slots: Vec<Vec<(usize, usize)>> = vec![Vec::new(); MAX_BAR_SLOTS];
            for (c0, c1, i, occ, all_day, fp, ts, te) in segs {
                let slot = slots
                    .iter()
                    .position(|s| s.iter().all(|(a, b)| c1 < *a || c0 > *b));
                if let Some(slot) = slot {
                    slots[slot].push((c0, c1));
                    let mut dd = w0 + Duration::days(c0 as i64);
                    let end = w0 + Duration::days(c1 as i64);
                    while dd <= end {
                        covered.insert((*i, occ.start, dd));
                        dd += Duration::days(1);
                    }
                    out.push(BarSeg {
                        week: wi,
                        slot,
                        c0,
                        c1,
                        idx: *i,
                        occ: occ.clone(),
                        all_day,
                        fp: fp.clone(),
                        true_start: ts,
                        true_end: te,
                    });
                }
            }
        }
        (out, covered)
    });

    let weeks_enum = Memo::new(move |_| {
        weeks.get().into_iter().enumerate().collect::<Vec<_>>()
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
        <div class="month-scroll">
        <div class="weekdays">
            <For
                each=move || weekday_labels.get()
                key=|d| d.clone()
                children=|d| view! { <span>{d}</span> }
            />
        </div>
        <div class="month-grid">
            <For
                each=move || weeks_enum.get()
                key=|(_, w)| w[0]
                children=move |(wi, week)| {
                    let week_bars = Memo::new(move |_| {
                        bars.get()
                            .0
                            .iter()
                            .filter(|b| b.week == wi)
                            .cloned()
                            .collect::<Vec<_>>()
                    });
                    let week_days = week.clone();
                    let week_cols = week.clone();
                    view! {
                        <div class="week">
                            <For
                                each=move || week_days.clone()
                                key=|d| *d
                                children=move |day| {
                                    // Must stay reactive: the boundary week row is shared
                                    // between two month grids (same w[0] key), so a captured
                                    // anchor would keep the old month's `out` marks when
                                    // navigating e.g. Dec -> Jan.
                                    let in_month = Memo::new(move |_| {
                                        let a = state.anchor.get();
                                        day.year() == a.year() && day.month() == a.month()
                                    });
                                    let is_today = day == today_d;
                                    let col = week_cols.iter().position(|d| *d == day).unwrap_or(0);
                                    let day_items = Memo::new(move |_| {
                                        by_day.get().get(&day).cloned().unwrap_or_default()
                                    });
                                    // days covered by a spanning bar show the bar, not a chip
                                    let chip_items = Memo::new(move |_| {
                                        let (_, covered) = bars.get();
                                        day_items
                                            .get()
                                            .into_iter()
                                            .filter(|(i, o, _, _)| !covered.contains(&(*i, o.start, day)))
                                            .collect::<Vec<_>>()
                                    });
                                    // vertical space reserved for bars overlapping this day
                                    let bar_pad = Memo::new(move |_| {
                                        bars.get()
                                            .0
                                            .iter()
                                            .filter(|b| b.week == wi && b.c0 <= col && col <= b.c1)
                                            .map(|b| b.slot + 1)
                                            .max()
                                            .unwrap_or(0)
                                            * BAR_SLOT_H
                                    });
                                    // max 3 entries per cell TOTAL (bars + chips + show-all);
                                    // on overflow show one less + a neutral "Show all" entry
                                    let chip_slots = Memo::new(move |_| 3usize.saturating_sub(bar_pad.get() / BAR_SLOT_H));
                                    let shown_items = Memo::new(move |_| {
                                        let all = chip_items.get();
                                        let slots = chip_slots.get();
                                        if all.len() > slots {
                                            all.into_iter().take(slots.saturating_sub(1)).collect::<Vec<_>>()
                                        } else {
                                            all.into_iter().take(slots).collect::<Vec<_>>()
                                        }
                                    });
                                    let more_count = Memo::new(move |_| {
                                        chip_items.get().len().saturating_sub(shown_items.get().len())
                                    });
                                    // no "Show all" when the bars already fill all 3 slots
                                    let show_more = Memo::new(move |_| more_count.get() > 0 && chip_slots.get() >= 1);
                                    view! {
                                        <div
                                            class="day"
                                            class:out=move || !in_month.get()
                                            class:today=is_today
                                            on:click=move |ev| {
                                                state.day_popup.set(Some(DayPopupState {
                                                    date: day,
                                                    x: ev.client_x() as f64,
                                                    y: ev.client_y() as f64,
                                                }))
                                            }
                                            on:dragover=move |ev| ev.prevent_default()
                                            on:drop=move |ev| on_drop(ev, day)
                                        >
                                            <span class="day-num">{day.day()}</span>
                                            <div class="day-events" style=move || format!("--bar-pad:{}px", bar_pad.get())>
                                                <For
                                                    each=move || shown_items.get()
                                                    key=|(i, o, _, fp)| (*i, o.start, fp.clone())
                                                    children=move |(idx, occ, all_day, _)| {
                                                        chip(state, tz, idx, occ, all_day)
                                                    }
                                                />
                                                <Show when=move || show_more.get() fallback=|| ()>
                                                    <button
                                                        class="more-chip"
                                                        on:click=move |ev| {
                                                            ev.stop_propagation();
                                                            state.day_popup.set(Some(DayPopupState {
                                                                date: day,
                                                                x: ev.client_x() as f64,
                                                                y: ev.client_y() as f64,
                                                            }));
                                                        }
                                                    >
                                                        {move || crate::i18n::t(state.lang.get(), "show_all")}
                                                    </button>
                                                </Show>
                                            </div>
                                        </div>
                                    }
                                }
                            />
                            // spanning bars for multi-day events in this week row
                            <For
                                each=move || week_bars.get()
                                // key must include geometry + content: (idx, occ, week, slot)
                                // alone survives a date edit inside the same week row, and
                                // the children read c0/c1/true_start/true_end as plain values
                                key=|b| (b.idx, b.occ.start, b.week, b.slot, b.c0, b.c1, b.true_start, b.true_end, b.fp.clone())
                                children=move |b| {
                                    let ev = state.events.get().get(b.idx).cloned();
                                    let (summary, color, href, read_only, recurring) = match &ev {
                                        Some(e) => {
                                            let summary = b
                                                .occ
                                                .override_idx
                                                .and_then(|i| e.overrides.get(i))
                                                .map(|o| o.summary.clone())
                                                .unwrap_or_else(|| e.event.summary.clone());
                                            (summary, e.color.clone(), e.href.clone(), e.read_only, e.event.rrule.is_some())
                                        }
                                        None => (String::new(), crate::state::ACCENT.into(), String::new(), true, false),
                                    };
                                    let occurrence = if recurring { b.occ.recurrence_id } else { None };
                                    let display_start = if recurring { Some(b.occ.start) } else { None };
                                    let selected = Memo::new({
                                        let href = href.clone();
                                        move |_| state.selected.get().iter().any(|h| h == &href)
                                    });
                                    let time = if !b.all_day && b.true_start {
                                        b.occ.start.with_timezone(&tz).format("%H:%M").to_string()
                                    } else {
                                        String::new()
                                    };
                                    let has_time = !time.is_empty();
                                    let n = b.c1 - b.c0 + 1;
                                    // same inset from the day borders as normal chips
                                    // (8px cell padding) at the true start/end
                                    let left_inset = if b.true_start { 8 } else { 2 };
                                    let right_inset = if b.true_end { 8 } else { 4 };
                                    let left = format!("calc(6px * {} + (100% - 36px) / 7 * {} + {}px)", b.c0, b.c0, left_inset);
                                    let width = format!("calc((100% - 36px) / 7 * {} + 6px * {} - {}px)", n, n.saturating_sub(1), right_inset);
                                    let top = 38 + b.slot * BAR_SLOT_H;
                                    view! {
                                        <div
                                            class="span-bar"
                                            class:cont-left=!b.true_start
                                            class:cont-right=!b.true_end
                                            class:selected=move || selected.get()
                                            title=summary.clone()
                                            style=format!("--chip:{color};left:{left};width:{width};top:{top}px")
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
                                                    state.open_details(b.idx, occurrence, display_start, Some((e.client_x() as i32, e.client_y() as i32)));
                                                }
                                            }
                                        >
                                            <Show when=move || has_time fallback=|| ()>
                                                <span class="chip-time">{time.clone()}</span>
                                            </Show>
                                            <span class="bar-title">{summary.clone()}</span>
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
        </div>
    }
}
