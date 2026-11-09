//! Week/day view: Monday-first 7-day (or 1-day) grid with an all-day row and
//! a 24-hour timeline. At most two events are shown side by side; the rest are
//! collected behind a "+N more" badge so columns never become unreadable.

use std::collections::{HashMap, HashSet};

use caldav::recurrence::{expand_series, Occurrence};
use caldav::time::{agenda_range, today, week_days, week_range};
use chrono::{Datelike, Duration, NaiveDate, Timelike};
use leptos::prelude::*;

use crate::browser;
use crate::state::{AppState, SlotPopupState};

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
                    // bucket on every day the occurrence spans, so overnight
                    // events also show on the following day (with a ← hint)
                    let start_d = occ.start.with_timezone(&tz).date_naive();
                    let end_d = (occ.end - Duration::seconds(1)).with_timezone(&tz).date_naive();
                    let mut d = start_d;
                    while d <= end_d {
                        timed
                            .entry(d)
                            .or_default()
                            .push((i, occ.clone(), fingerprint.clone()));
                        d += Duration::days(1);
                    }
                }
            }
        }
        for v in timed.values_mut() {
            v.sort_by_key(|(_, o, _)| o.start);
        }
        (all_day, timed)
    });

    /// One spanning bar for a multi-day all-day event in the all-day row.
    #[derive(Clone, PartialEq)]
    struct AlldayBar {
        c0: usize,
        c1: usize,
        slot: usize,
        idx: usize,
        occ: Occurrence,
        fp: String,
        /// continues from the previous week (fade at the left edge)
        cont_left: bool,
        /// continues into the next week (fade at the right edge)
        cont_right: bool,
    }

    /// Multi-day all-day events render as ONE longer bar spanning the all-day
    /// row cells of the visible week; the striped continuation fade is only
    /// applied at week boundaries. Also returns the (event, occurrence, day)
    /// triples covered by a bar so the cells don't draw a duplicate chip.
    let allday_bars = Memo::new(move |_| {
        if n_days != 7 {
            return (Vec::<AlldayBar>::new(), HashSet::new());
        }
        let days = days.get();
        let (w0, w6) = (days[0], days[6]);
        let (rs, re) = week_range(state.anchor.get(), &tz);
        let events = state.events.get();
        let mut spans: Vec<(usize, Occurrence, String, NaiveDate, NaiveDate)> = Vec::new();
        for (i, ev) in events.iter().enumerate() {
            let is_all_day = ev
                .event
                .dtstart
                .as_ref()
                .map(|d| d.is_date())
                .unwrap_or(false);
            if !is_all_day {
                continue;
            }
            let fp = format!(
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
                    spans.push((i, occ, fp.clone(), start_d, end_d));
                }
            }
        }
        let mut segs: Vec<(usize, usize, usize, Occurrence, String, bool, bool)> = Vec::new();
        for (i, occ, fp, start_d, end_d) in &spans {
            let s = (*start_d).max(w0);
            let e = (*end_d).min(w6);
            if s <= e {
                segs.push((
                    (s - w0).num_days() as usize,
                    (e - w0).num_days() as usize,
                    *i,
                    occ.clone(),
                    fp.clone(),
                    *start_d < w0,
                    *end_d > w6,
                ));
            }
        }
        segs.sort_by_key(|(c0, c1, ..)| (*c0, *c1));
        let mut slots: Vec<Vec<(usize, usize)>> = vec![Vec::new(); 2];
        let mut bars = Vec::new();
        let mut covered: HashSet<(usize, chrono::DateTime<chrono_tz::Tz>, NaiveDate)> = HashSet::new();
        for (c0, c1, i, occ, fp, cl, cr) in segs {
            let slot = slots
                .iter()
                .position(|s| s.iter().all(|(a, b)| c1 < *a || c0 > *b));
            if let Some(slot) = slot {
                slots[slot].push((c0, c1));
                let mut dd = w0 + Duration::days(c0 as i64);
                let end = w0 + Duration::days(c1 as i64);
                while dd <= end {
                    covered.insert((i, occ.start, dd));
                    dd += Duration::days(1);
                }
                bars.push(AlldayBar {
                    c0,
                    c1,
                    slot,
                    idx: i,
                    occ,
                    fp,
                    cont_left: cl,
                    cont_right: cr,
                });
            }
        }
        (bars, covered)
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
            // Day view: no day header — the toolbar title already shows the date.
            <Show when=move || n_days == 7 fallback=|| ()>
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
            </Show>

            <div class="week-allday">
                <div class="week-gutter">{move || crate::i18n::t(state.lang.get(), "all_day_lower")}</div>
                <For
                    each=move || days.get()
                    key=|d| *d
                    children=move |d| {
                        let col = days.get().iter().position(|x| *x == d).unwrap_or(0);
                        let items = move || data.get().0.get(&d).cloned().unwrap_or_default();
                        // days covered by a spanning all-day bar show the bar, not a chip
                        let chip_items = Memo::new(move |_| {
                            let (_, covered) = allday_bars.get();
                            items()
                                .into_iter()
                                .filter(|(i, o, _)| !covered.contains(&(*i, o.start, d)))
                                .collect::<Vec<_>>()
                        });
                        let bar_pad = Memo::new(move |_| {
                            allday_bars
                                .get()
                                .0
                                .iter()
                                .filter(|b| b.c0 <= col && col <= b.c1)
                                .map(|b| b.slot + 1)
                                .max()
                                .unwrap_or(0)
                                * 22
                        });
                        // fixed 3-entry row: bars take their slots, chips fill the
                        // rest; on overflow the last slot is a "Show all" entry
                        let shown_allday = Memo::new(move |_| {
                            let slots = 3usize.saturating_sub(bar_pad.get() / 22);
                            chip_items.get().into_iter().take(slots).collect::<Vec<_>>()
                        });
                        let more_allday = Memo::new(move |_| {
                            chip_items.get().len().saturating_sub(shown_allday.get().len())
                        });
                        // no "Show all" when the spanning bars already fill the row
                        let show_more_allday = Memo::new(move |_| more_allday.get() > 0 && bar_pad.get() < 3 * 22);
                        view! {
                            <div
                                class="allday-cell"
                                style=move || format!("--bar-pad:{}px", bar_pad.get())
                                on:click=move |ev| {
                                    state.slot_popup.set(Some(SlotPopupState {
                                        date: d,
                                        minutes: 0,
                                        all_day: true,
                                        x: ev.client_x() as f64,
                                        y: ev.client_y() as f64,
                                    }));
                                }
                            >
                                <For
                                    each=move || shown_allday.get()
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
                                        // day view (single visible day): edge fade when the
                                        // all-day event continues from/into another day
                                        let (cl, cr) = if n_days == 1 {
                                            (
                                                occ.start.with_timezone(&tz).date_naive() < d,
                                                (occ.end - Duration::seconds(1)).with_timezone(&tz).date_naive() > d,
                                            )
                                        } else {
                                            (false, false)
                                        };
                                        let selected = Memo::new({
                                            let href = href.clone();
                                            move |_| state.selected.get().iter().any(|h| h == &href)
                                        });
                                        view! {
                                            <div
                                                class="allday-chip"
                                                class:cont-left=cl
                                                class:cont-right=cr
                                                class:selected=move || selected.get()
                                                style=format!("--chip:{color}")
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    if e.ctrl_key() || e.meta_key() {
                                                        toggle_select(&href);
                                                    } else {
                                                        state.open_details(idx, occurrence, display_start);
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
                                <Show when=move || show_more_allday.get() fallback=|| ()>
                                    <button
                                        class="more-chip"
                                        on:click=move |ev| {
                                            ev.stop_propagation();
                                            state.slot_popup.set(Some(SlotPopupState {
                                                date: d,
                                                minutes: 0,
                                                all_day: true,
                                                x: ev.client_x() as f64,
                                                y: ev.client_y() as f64,
                                            }));
                                        }
                                    >
                                        {move || crate::i18n::t(state.lang.get(), "show_all")}
                                    </button>
                                </Show>
                            </div>
                        }
                    }
                />
                // multi-day all-day events: one longer bar spanning the row cells;
                // striped fade only at the week boundaries (cont-left/cont-right)
                <For
                    each=move || allday_bars.get().0
                    // key must include the segment geometry + fade flags:
                    // navigating weeks keeps (idx, occ.start, slot) identical,
                    // and without c0/c1/cont the row would keep the old
                    // cont-left/cont-right classes and left/width styles
                    key=|b| (b.idx, b.occ.start, b.slot, b.c0, b.c1, b.cont_left, b.cont_right, b.fp.clone())
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
                        let n = b.c1 - b.c0 + 1;
                        let left = format!("calc(62px + {} * ((100% - 98px) / 7 + 6px))", b.c0);
                        let width = format!("calc({} * (100% - 98px) / 7 + {} * 6px - 4px)", n, n.saturating_sub(1));
                        let top = 3 + b.slot * 22;
                        view! {
                            <div
                                class="allday-bar"
                                class:cont-left=b.cont_left
                                class:cont-right=b.cont_right
                                class:selected=move || selected.get()
                                title=summary.clone()
                                style=format!("--chip:{color};left:{left};width:{width};top:{top}px")
                                on:click=move |e| {
                                    e.stop_propagation();
                                    if e.ctrl_key() || e.meta_key() {
                                        toggle_select(&href);
                                    } else {
                                        state.open_details(b.idx, occurrence, display_start);
                                    }
                                }
                            >
                                <span class="bar-title">{summary.clone()}</span>
                                <Show when=move || read_only fallback=|| ()>
                                    <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")>"🔒"</span>
                                </Show>
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
                        // On overflow: show only the first column and put a
                        // "Show all" entry in the second one (no "+N more" badge).
                        let overflow = Memo::new(move |_| !hidden.get().is_empty());
                        let shown = Memo::new(move |_| {
                            let vis = visible.get();
                            if overflow.get() {
                                vis.into_iter().filter(|(_, _, _, col, _)| *col == 0).collect::<Vec<_>>()
                            } else {
                                vis
                            }
                        });
                        let create_at = move |ev: leptos::ev::MouseEvent| {
                            let y = ev.offset_y() as f64;
                            let minutes = ((y / HOUR_HEIGHT) * 60.0 / 30.0).round() as i64 * 30;
                            let minutes = minutes.clamp(0, 23 * 60 + 30) as u32;
                            state.slot_popup.set(Some(SlotPopupState {
                                date: d,
                                minutes,
                                all_day: false,
                                x: ev.client_x() as f64,
                                y: ev.client_y() as f64,
                            }));
                        };
                        view! {
                            <div
                                class="day-column"
                                class:today=d == today_d
                                on:click=create_at
                            >
                                <For
                                    each=move || shown.get()
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
                                        // continuation hints: the occurrence may start before
                                        // this day or end after it — clamp the bar to the day
                                        let occ_start_d = occ.start.with_timezone(&tz).date_naive();
                                        let occ_end_d = (occ.end - Duration::seconds(1)).with_timezone(&tz).date_naive();
                                        let cont_start = occ_start_d < d;
                                        let cont_end = occ_end_d > d;
                                        let start_min = if cont_start { 0.0 } else { minutes_of_day(occ.start, &tz) };
                                        let end_min = if cont_end { 1440.0 } else { minutes_of_day(occ.end, &tz) };
                                        let dur = (end_min - start_min).max(20.0);
                                        let top = start_min / 60.0 * HOUR_HEIGHT;
                                        let height = dur / 60.0 * HOUR_HEIGHT;
                                        let width = 100.0 / cols as f64;
                                        let left = col as f64 * width;
                                        let time = occ.start.with_timezone(&tz).format("%H:%M").to_string();
                                        // Short events cannot fit the usual two lines
                                        // (time above title); switch to a single-line
                                        // layout so the title stays readable.
                                        let compact = height < 34.0;
                                        let tooltip = format!("{time} – {summary}");
                                        let draggable = !read_only && !recurring;
                                        let drag_href = href.clone();
                                        let selected = Memo::new({
                                            let href = href.clone();
                                            move |_| state.selected.get().iter().any(|h| h == &href)
                                        });
                                        view! {
                                            <div
                                                class="timed-event"
                                                class:compact=compact
                                                class:cont-top=cont_start
                                                class:cont-bottom=cont_end
                                                class:selected=move || selected.get()
                                                title=tooltip
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
                                                        state.open_details(idx, occurrence, display_start);
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
                                        // Anchor the badge right below the last visible
                                        // event (bottom of the rendered stack), not at
                                        // the top of the day column.
                                        let more_top = Memo::new(move |_| {
                                            shown.get().iter().map(|(_, occ, _, _, _)| {
                                                let s = minutes_of_day(occ.start, &tz);
                                                let e = minutes_of_day(occ.end, &tz);
                                                let dur = (e - s).max(20.0);
                                                (s + dur) / 60.0 * HOUR_HEIGHT
                                            }).fold(4.0, f64::max).round()
                                        });
                                        view! {
                                            <button
                                                class="more-bar"
                                                style=move || format!("top:{}px", more_top.get())
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    // "show all": every timed event of this day
                                                    let indices = laid_out.get().iter().map(|(i, _, _, _, _)| *i).collect::<Vec<_>>();
                                                    state.week_more.set(Some(indices));
                                                }
                                            >
                                                {move || crate::i18n::t(state.lang.get(), "show_all")}
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
