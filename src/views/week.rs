//! Week/day view: Monday-first 7-day (or 1-day) grid with an all-day row and
//! a 24-hour timeline. Overlapping timed events are packed "Tetris"-style:
//! events that overlap (directly or transitively) form a cluster, a cluster
//! gets as many columns as its maximum simultaneous overlap, and every event
//! in it shares the width `1/columns` — so a busy morning never shrinks a
//! quiet afternoon. Caps: two columns in the week view, four in the
//! (full-width) day view. When a cluster exceeds the cap, one column less is
//! shown and the freed slot becomes a "+N more" bar that spans exactly the
//! hidden events' time range. Event titles stick to the top of the page
//! while scrolling (see .te-title in main.css).

use std::collections::{HashMap, HashSet};

use caldav::recurrence::{expand_series, Occurrence};
use caldav::time::{agenda_range, today, week_days, week_range};
use chrono::{Datelike, Duration, NaiveDate, Timelike};
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;

use crate::browser;
use crate::state::{AppState, ModalState, SlotPopupState, ViewMode};

const HOUR_HEIGHT: f64 = 42.0; // px per hour in the timeline
const MAX_COLUMNS_WEEK: usize = 2;
const MAX_COLUMNS_DAY: usize = 4;

/// A drag in the timed timeline, snapped to 15 minutes. The two endpoints may
/// be on different days (multi-day selection).
#[derive(Clone, PartialEq)]
struct DragSel {
    start_day: NaiveDate,
    start_min: u32,
    end_day: NaiveDate,
    end_min: u32,
}

/// What a timed drag started from, so the pointer can leave the column.
#[derive(Clone)]
struct DragCtx {
    /// Visible columns, left to right: (day, left, right) in client pixels.
    cols: Vec<(NaiveDate, f64, f64)>,
    rect_top: f64,
    start_day: NaiveDate,
    start_min: u32,
    start_x: f64,
    start_y: f64,
    click_x: f64,
    click_y: f64,
}

/// A drag across the all-day row: first and last day, both inclusive.
#[derive(Clone, Copy, PartialEq)]
struct AlldaySel {
    start: NaiveDate,
    end: NaiveDate,
}

/// The all-day row geometry at the start of a drag.
#[derive(Clone)]
struct AlldayCtx {
    row: web_sys::Element,
    days: Vec<NaiveDate>,
    left: f64,
    col_width: f64,
    start_day: NaiveDate,
    start_x: f64,
}

/// Timeline y position -> minutes after midnight, in 15-minute steps.
fn snap_quarter(y: f64) -> u32 {
    let minutes = (y / HOUR_HEIGHT) * 60.0;
    (((minutes / 15.0).round() as i64) * 15).clamp(0, 24 * 60) as u32
}

fn day_at_x(cols: &[(NaiveDate, f64, f64)], x: f64) -> NaiveDate {
    cols.iter()
        .find(|(_, left, right)| x >= *left && x < *right)
        .map(|(d, _, _)| *d)
        .unwrap_or_else(|| {
            if x < cols[0].1 {
                cols[0].0
            } else {
                cols[cols.len() - 1].0
            }
        })
}

/// Normalised endpoints of a timed drag, in reading order.
fn ordered_drag(sel: &DragSel, days: &[NaiveDate]) -> (NaiveDate, u32, NaiveDate, u32) {
    let si = days.iter().position(|d| *d == sel.start_day).unwrap_or(0);
    let ei = days.iter().position(|d| *d == sel.end_day).unwrap_or(0);
    if (ei, sel.end_min) < (si, sel.start_min) {
        (sel.end_day, sel.end_min, sel.start_day, sel.start_min)
    } else {
        (sel.start_day, sel.start_min, sel.end_day, sel.end_min)
    }
}

/// The ghost segment for one day column: (top, height) in pixels, or None.
fn ghost_segment(sel: &DragSel, days: &[NaiveDate], day: NaiveDate) -> Option<(f64, f64)> {
    let (a_day, a_min, b_day, b_min) = ordered_drag(sel, days);
    if day < a_day || day > b_day {
        return None;
    }
    let px = |minutes: u32| minutes as f64 / 60.0 * HOUR_HEIGHT;
    if a_day == b_day {
        let span = b_min.saturating_sub(a_min).max(15);
        return Some((px(a_min), px(span)));
    }
    if day == a_day {
        return Some((px(a_min), px(1440 - a_min)));
    }
    if day == b_day {
        // ends exactly at midnight: nothing to draw on the last day
        return (b_min > 0).then(|| (0.0, px(b_min)));
    }
    Some((0.0, 24.0 * HOUR_HEIGHT))
}

fn minutes_of_day(t: chrono::DateTime<chrono_tz::Tz>, tz: &chrono_tz::Tz) -> f64 {
    let l = t.with_timezone(tz);
    (l.hour() * 60 + l.minute()) as f64
}

/// One "+N more" bar: rendered when an overlap cluster needs more columns
/// than the view allows. It sits in the slot freed by the overflow collapse
/// and spans exactly the time range of the hidden events (clamped to the
/// day), like Google Calendar's "+N more".
#[derive(Clone, PartialEq, Debug)]
struct ClusterBar {
    /// px offsets inside the day column (rounded, for stable `<For>` keys)
    top_px: i64,
    height_px: i64,
    /// number of hidden events for the "+N more" label
    count: usize,
    /// start of the first hidden occurrence (part of the `<For>` key)
    first_start: chrono::DateTime<chrono_tz::Tz>,
    /// hidden occurrences as indices into the **per-day** list that `layout`
    /// was given. The caller must translate them to `state.events` indices
    /// (they used to be fed to the popup raw, which looked them up in the
    /// global event list and showed unrelated events from other days).
    local_indices: Vec<usize>,
    /// the same hidden occurrences as `(global event index, occurrence start)`
    /// — what the "+N more" popup needs to open the right events.
    indices: Vec<(usize, chrono::DateTime<chrono_tz::Tz>)>,
}

/// Finish an overlap cluster: assign `(column, columns, shown)` to each
/// member and, when the cluster exceeds `max_columns`, push a `ClusterBar`
/// covering every hidden member. `members` must be in start order.
#[allow(clippy::too_many_arguments)]
fn flush_cluster(
    members: &[(usize, usize)], // (event index, column)
    cols_used: usize,
    max_columns: usize,
    events: &[Occurrence],
    d: NaiveDate,
    tz: &chrono_tz::Tz,
    result: &mut [(usize, usize, bool)],
    bars: &mut Vec<ClusterBar>,
) {
    if members.is_empty() {
        return;
    }
    if cols_used <= max_columns {
        // fits: every event in the cluster at width 1/cols_used
        for &(i, col) in members {
            result[i] = (col, cols_used, true);
        }
        return;
    }
    // overflow: the cluster still spans all `max_columns` slots — the first
    // max-1 columns stay, the last slot becomes the "+N more" bar, and the
    // bar counts EVERY hidden event (the greedy-global layout silently
    // dropped the event in the collapse slot: 3 shown + "+1 more" for five
    // simultaneous events).
    let mut hidden: Vec<usize> = Vec::new();
    for &(i, col) in members {
        let shown = col + 1 < max_columns;
        result[i] = (col, max_columns, shown);
        if !shown {
            hidden.push(i);
        }
    }
    // the bar spans exactly the hidden events' range, clamped to this day
    let mut s_min = 1440.0f64;
    let mut e_max = 0.0f64;
    for &i in &hidden {
        let occ = &events[i];
        let sd = occ.start.with_timezone(tz).date_naive();
        let ed = (occ.end - Duration::seconds(1)).with_timezone(tz).date_naive();
        let s = if sd < d { 0.0 } else { minutes_of_day(occ.start, tz) };
        let e = if ed > d { 1440.0 } else { minutes_of_day(occ.end, tz) };
        s_min = s_min.min(s);
        e_max = e_max.max(e);
    }
    let top = (s_min / 60.0 * HOUR_HEIGHT).round() as i64;
    let height = (((e_max - s_min).max(20.0)) / 60.0 * HOUR_HEIGHT).round() as i64;
    bars.push(ClusterBar {
        top_px: top,
        height_px: height,
        count: hidden.len(),
        first_start: events[*hidden.first().unwrap()].start,
        local_indices: hidden,
        indices: Vec::new(),
    });
}

/// Cluster-based column layout ("Tetris", Google Calendar / FullCalendar
/// style): events that overlap directly or transitively form a cluster; the
/// cluster gets as many columns as its maximum simultaneous overlap (greedy
/// first-fit achieves exactly that for intervals), and every event in the
/// cluster shares width `1/cols`. Clusters are independent — the old
/// day-global width made a lone 09:00 event render next to a busy 14:00
/// block shrink to the same fraction. When a cluster needs more than
/// `max_columns` slots it is capped: the first `max_columns - 1` columns are
/// shown and the freed slot hosts a "+N more" bar (one bar per overflowing
/// cluster — clusters are disjoint in time, so their bars never collide).
///
/// Returns `(per-event (column, columns, shown), bars)` aligned with `events`.
fn layout(
    events: &[Occurrence],
    max_columns: usize,
    d: NaiveDate,
    tz: &chrono_tz::Tz,
) -> (Vec<(usize, usize, bool)>, Vec<ClusterBar>) {
    let mut order: Vec<usize> = (0..events.len()).collect();
    // start, then longer first (on overflow the longer, more content-rich
    // events stay visible), then input order for stability
    order.sort_by(|&a, &b| {
        events[a]
            .start
            .cmp(&events[b].start)
            .then_with(|| events[b].end.cmp(&events[a].end))
            .then_with(|| a.cmp(&b))
    });

    let mut result = vec![(0usize, 1usize, false); events.len()];
    let mut bars: Vec<ClusterBar> = Vec::new();

    // current cluster state
    let mut members: Vec<(usize, usize)> = Vec::new(); // (event index, column)
    let mut cluster_end: Option<chrono::DateTime<chrono_tz::Tz>> = None;
    let mut cols_used = 0usize;
    let mut active: Vec<(usize, chrono::DateTime<chrono_tz::Tz>)> = Vec::new(); // (column, end)

    for &i in &order {
        let (s, e) = (events[i].start, events[i].end);
        // gap: this event starts at/after everything in the current cluster
        // ends -> the cluster is complete
        if cluster_end.map_or(false, |ce| s >= ce) {
            flush_cluster(&members, cols_used, max_columns, events, d, tz, &mut result, &mut bars);
            members.clear();
            active.clear();
            cols_used = 0;
            cluster_end = None;
        }
        // greedy first-fit column within the cluster
        active.retain(|&(_, end)| end > s);
        let mut col = 0;
        while active.iter().any(|&(c, _)| c == col) {
            col += 1;
        }
        cols_used = cols_used.max(col + 1);
        active.push((col, e));
        members.push((i, col));
        cluster_end = Some(match cluster_end {
            Some(ce) if ce > e => ce,
            _ => e,
        });
    }
    flush_cluster(&members, cols_used, max_columns, events, d, tz, &mut result, &mut bars);
    (result, bars)
}

#[component]
pub fn WeekView(#[prop(default = 7)] n_days: usize) -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;
    let sticky_top: NodeRef<leptos::html::Div> = NodeRef::new();
    let timeline: NodeRef<leptos::html::Div> = NodeRef::new();

    // Start the timeline at 06:00 so the night is not what greets the user.
    // If the visible range contains today and it is still before 06:00, start
    // at midnight instead. This is a one-shot on view load (or reload): clicking
    // an event or stepping through dates must not jump the timeline.
    Effect::new(move |_| {
        let view = state.view.get_untracked();
        if view != ViewMode::Week && view != ViewMode::Day {
            return;
        }
        let anchor = state.anchor.get_untracked();
        let now = browser::now_utc().with_timezone(&tz);
        let today = now.date_naive();
        let contains_today = if n_days == 1 {
            anchor == today
        } else {
            week_days(anchor).contains(&today)
        };
        let start_hour = if contains_today && now.hour() < 6 { 0 } else { 6 };
        let sticky_ref = sticky_top;
        let timeline_ref = timeline;
        spawn_local(async move {
            // Let the grid render and get its final height first. The view may
            // have been left in the meantime; do not touch disposed NodeRefs.
            browser::sleep_ms(16).await;
            if sticky_ref.is_disposed() || timeline_ref.is_disposed() {
                return;
            }
            let (Some(sticky), Some(timeline)) = (sticky_ref.get(), timeline_ref.get()) else {
                return;
            };
            let Some(doc) = web_sys::window().and_then(|win| win.document()) else {
                return;
            };
            let Some(scroller) = doc
                .query_selector(".main-scroll")
                .ok()
                .flatten()
                .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
            else {
                return;
            };
            let target = timeline.get_bounding_client_rect().top()
                - scroller.get_bounding_client_rect().top()
                + scroller.scroll_top() as f64
                - sticky.client_height() as f64
                + start_hour as f64 * HOUR_HEIGHT;
            let target = target.max(0.0);
            scroller.set_scroll_top(target as i32);

            // A browser that laid the new view out late can clamp the first
            // assignment. Retry once with fresh geometry, but only while this
            // WeekView is still mounted, so switching views cannot scroll the
            // next one.
            browser::sleep_ms(80).await;
            if sticky_ref.is_disposed() || timeline_ref.is_disposed() {
                return;
            }
            let retry = (timeline.get_bounding_client_rect().top()
                - scroller.get_bounding_client_rect().top()
                + scroller.scroll_top() as f64
                - sticky.client_height() as f64
                + start_hour as f64 * HOUR_HEIGHT)
                .max(0.0);
            if retry > 5.0 && (scroller.scroll_top() as f64) < retry - 5.0 {
                scroller.set_scroll_top(retry as i32);
            }
        });
    });

    // The day view is full width, so it can afford more side-by-side events
    // than the 7-column week view.
    let max_columns = if n_days == 1 {
        MAX_COLUMNS_DAY
    } else {
        MAX_COLUMNS_WEEK
    };

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

    // Drag on the timeline: a translucent card follows the pointer, and the
    // release opens the editor with that range. A click without movement keeps
    // the old slot popup.
    let drag = RwSignal::new(None::<DragSel>);
    let drag_ctx = RwSignal::new(None::<DragCtx>);
    // A completed drag is followed by a click; do not open the slot popup then.
    let suppress_click = RwSignal::new(false);
    let suppress_allday_click = RwSignal::new(false);
    let finish_drag = move |_ev: web_sys::PointerEvent| {
        drag_ctx.set(None);
        let Some(sel) = drag.get_untracked() else {
            // No movement: the click handler opens the popup.
            return;
        };
        drag.set(None);
        suppress_click.set(true);
        let days = days.get_untracked();
        let (a_day, a_min, b_day, b_min) = ordered_drag(&sel, &days);
        let start = a_min.min(23 * 60 + 45);
        state.new_event_time.set(Some((start / 60, start % 60)));
        state.new_event_all_day.set(false);
        if a_day == b_day {
            let end = b_min.max(start + 15).min(23 * 60 + 59);
            state.new_event_end_time.set(Some((end / 60, end % 60)));
            state.new_event_end_date.set(None);
        } else {
            state.new_event_end_time.set(Some((b_min / 60, b_min % 60)));
            state.new_event_end_date.set(Some(b_day));
        }
        state.modal.set(Some(ModalState::New { date: a_day }));
    };

    // Same idea for the all-day row: drag across cells to create an all-day
    // event whose first and last day are the ones under the pointer.
    let allday_drag = RwSignal::new(None::<AlldaySel>);
    let allday_ctx = RwSignal::new(None::<AlldayCtx>);
    let finish_allday = move |_ev: web_sys::PointerEvent| {
        allday_ctx.set(None);
        let Some(sel) = allday_drag.get_untracked() else {
            // No movement: the cell click opens the all-day popup.
            return;
        };
        allday_drag.set(None);
        let (a, b) = if sel.start <= sel.end {
            (sel.start, sel.end)
        } else {
            (sel.end, sel.start)
        };
        if a == b {
            return;
        }
        suppress_allday_click.set(true);
        state.new_event_all_day.set(true);
        state.new_event_time.set(None);
        state.new_event_end_time.set(None);
        state.new_event_end_date.set(Some(b));
        state.modal.set(Some(ModalState::New { date: a }));
    };
    let cancel_allday = move |_| {
        allday_ctx.set(None);
        allday_drag.set(None);
    };

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
        <div
            class="week-view"
            class:single-day=n_days == 1
            class:dragging=move || drag_ctx.get().is_some() || allday_ctx.get().is_some()
            style=format!("--cols:{n_days}")
        >
            <div class="week-sticky" node_ref=sticky_top>
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
                                    {move || crate::i18n::weekday_short(state.lang.get(), d.weekday().num_days_from_monday() as usize)}
                                </span>
                                <span class="wd-num">{d.day()}</span>
                            </div>
                        }
                    }
                />
            </div>
            </Show>

            <div class="week-allday">
                <div class="week-gutter week-gutter-allday">
                    {move || crate::i18n::t(state.lang.get(), "all_day_gutter")}
                </div>
                <div
                    class="week-allday-cols"
                    on:pointerdown=move |ev: web_sys::PointerEvent| {
                        if ev.pointer_type() != "mouse"
                            || ev.button() != 0
                            || state.cache_only.get_untracked()
                        {
                            return;
                        }
                        let target = ev
                            .target()
                            .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                        let on_control = target
                            .and_then(|t| t.closest(".allday-bar, .allday-chip, .more-chip, button").ok().flatten())
                            .is_some();
                        if on_control {
                            return;
                        }
                        let Some(el) = ev
                            .current_target()
                            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                        else {
                            return;
                        };
                        // No pointer capture yet: a tap must be delivered to the
                        // cell so its click handler can open the popup.
                        let rect = el.get_bounding_client_rect();
                        let ds = days.get_untracked();
                        let n = ds.len().max(1) as f64;
                        let idx = (((ev.client_x() as f64 - rect.left()) / rect.width() * n)
                            .floor() as i64)
                            .clamp(0, ds.len() as i64 - 1) as usize;
                        let day = ds[idx];
                        allday_ctx.set(Some(AlldayCtx {
                            row: el,
                            days: ds,
                            left: rect.left(),
                            col_width: rect.width() / n,
                            start_day: day,
                            start_x: ev.client_x() as f64,
                        }));
                        suppress_allday_click.set(false);
                        allday_drag.set(None);
                    }
                    on:pointermove=move |ev: web_sys::PointerEvent| {
                        let Some(ctx) = allday_ctx.get_untracked() else {
                            return;
                        };
                        // Only a real drag shows the bar; a tap stays a click.
                        if (ev.client_x() as f64 - ctx.start_x).abs() < 4.0 {
                            return;
                        }
                        // From here on this is a drag: keep receiving the pointer
                        // outside the row until it is released.
                        let _ = ctx.row.set_pointer_capture(ev.pointer_id());
                        let idx = (((ev.client_x() as f64 - ctx.left) / ctx.col_width).floor()
                            as i64)
                            .clamp(0, ctx.days.len() as i64 - 1) as usize;
                        let day = ctx.days[idx];
                        allday_drag.set(Some(AlldaySel {
                            start: ctx.start_day,
                            end: day,
                        }));
                    }
                    on:pointerup=move |ev| finish_allday(ev)
                    on:pointercancel=cancel_allday
                >
                <Show when=move || allday_drag.get().is_some() fallback=|| ()>
                    {move || {
                        let sel = allday_drag.get()?;
                        let ds = days.get();
                        let n = ds.len().max(1) as f64;
                        let (a, b) = if sel.start <= sel.end {
                            (sel.start, sel.end)
                        } else {
                            (sel.end, sel.start)
                        };
                        let i = ds.iter().position(|d| *d == a)? as f64;
                        let j = ds.iter().position(|d| *d == b)? as f64;
                        let span = j - i + 1.0;
                        Some(view! {
                            <div
                                class="allday-drag-ghost"
                                style=format!(
                                    "left:calc({:.4}% + 2px);width:calc({:.4}% - 4px)",
                                    i / n * 100.0,
                                    span / n * 100.0
                                )
                            ></div>
                        })
                    }}
                </Show>
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
                                    if suppress_allday_click.get_untracked() {
                                        suppress_allday_click.set(false);
                                        return;
                                    }
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
                                        let tooltip = summary.clone();
                                        view! {
                                            <div
                                                class="allday-chip"
                                                class:cont-left=cl
                                                class:cont-right=cr
                                                class:selected=move || selected.get()
                                                style=format!("--chip:{color}")
                                                title=tooltip
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    if e.ctrl_key() || e.meta_key() {
                                                        toggle_select(&href);
                                                    } else {
                                                        state.open_details(idx, occurrence, display_start, Some((e.client_x() as i32, e.client_y() as i32)));
                                                    }
                                                }
                                            >
                                                {summary}
                                                <Show when=move || read_only fallback=|| ()>
                                                    <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 7V5a3 3 0 0 1 6 0v2" fill="none" stroke="currentColor" stroke-width="1.4" /><rect x="3.4" y="7" width="9.2" height="6.6" rx="1.4" fill="currentColor" /></svg></span>
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
                        // Place the bar by grid column (2 = first day; col 1 is the
                        // time gutter), so the span follows the actual column count —
                        // the old pixel math hardcoded 7 days + a 56px gutter and ran
                        // past the card's right edge in day view.
                        let left = format!("calc((100% - 3px) / var(--cols, 7) * {} + 6px)", b.c0);
                        let width = format!("calc((100% - 3px) / var(--cols, 7) * {} - 4px)", n);
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
                                        state.open_details(b.idx, occurrence, display_start, Some((e.client_x() as i32, e.client_y() as i32)));
                                    }
                                }
                            >
                                <span class="bar-title">{summary.clone()}</span>
                                <Show when=move || read_only fallback=|| ()>
                                    <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 7V5a3 3 0 0 1 6 0v2" fill="none" stroke="currentColor" stroke-width="1.4" /><rect x="3.4" y="7" width="9.2" height="6.6" rx="1.4" fill="currentColor" /></svg></span>
                                </Show>
                            </div>
                        }
                    }
                />
                </div>
            </div>
            </div>

            <div
                class="week-timeline"
                node_ref=timeline
                style=format!("--hour-height:{}px", HOUR_HEIGHT)
            >
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
                        // "Tetris" layout (per overlap cluster, see `layout`):
                        // the shown events plus one "+N more" bar per
                        // overflowing cluster.
                        let laid_out = Memo::new(move |_| {
                            let items = items.get();
                            let occs: Vec<Occurrence> = items.iter().map(|(_, o, _)| o.clone()).collect();
                            let (cols, bars_raw) = layout(&occs, max_columns, d, &tz);
                            let shown = items
                                .iter()
                                .zip(cols.iter())
                                .filter_map(|(item, (col, n_cols, shown))| {
                                    if *shown {
                                        Some((item.0, item.1.clone(), item.2.clone(), *col, *n_cols))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Vec<(usize, Occurrence, String, usize, usize)>>();
                            // Translate the bar's per-day indices into global
                            // event indices + occurrences, so the popup lists
                            // exactly the events this bar hides.
                            let bars = bars_raw
                                .into_iter()
                                .map(|b| {
                                    let indices = b
                                        .local_indices
                                        .iter()
                                        .filter_map(|&li| {
                                            items.get(li).map(|(gi, o, _)| (*gi, o.start))
                                        })
                                        .collect::<Vec<_>>();
                                    ClusterBar { local_indices: Vec::new(), indices, ..b }
                                })
                                .collect::<Vec<_>>();
                            (shown, bars)
                        });
                        let shown = Memo::new(move |_| laid_out.get().0);
                        let bars = Memo::new(move |_| laid_out.get().1);
                        let start_drag = move |ev: web_sys::PointerEvent| {
                            if ev.pointer_type() != "mouse"
                                || ev.button() != 0
                                || state.cache_only.get_untracked()
                            {
                                return;
                            }
                            // Do not start a selection on an event or a "+N more" bar.
                            let target = ev
                                .target()
                                .and_then(|t| t.dyn_into::<web_sys::Element>().ok());
                            let on_control = target
                                .and_then(|t| t.closest(".timed-event, .more-bar").ok().flatten())
                                .is_some();
                            if on_control {
                                return;
                            }
                            let Some(el) = ev
                                .current_target()
                                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                            else {
                                return;
                            };
                            let _ = el.set_pointer_capture(ev.pointer_id());
                            let Some(parent) = el.parent_element() else {
                                return;
                            };
                            let ds = days.get_untracked();
                            let mut cols = Vec::with_capacity(ds.len());
                            let children = parent.children();
                            for (i, cell) in (0..children.length())
                                .filter_map(|i| children.item(i))
                                .filter(|c| c.class_list().contains("day-column"))
                                .enumerate()
                            {
                                if let Some(day) = ds.get(i) {
                                    let r = cell.get_bounding_client_rect();
                                    cols.push((*day, r.left(), r.right()));
                                }
                            }
                            if cols.is_empty() {
                                return;
                            }
                            let rect = el.get_bounding_client_rect();
                            let start = snap_quarter(ev.client_y() as f64 - rect.top());
                            drag_ctx.set(Some(DragCtx {
                                cols,
                                rect_top: rect.top(),
                                start_day: d,
                                start_min: start,
                                start_x: ev.client_x() as f64,
                                start_y: ev.client_y() as f64,
                                click_x: ev.client_x() as f64,
                                click_y: ev.client_y() as f64,
                            }));
                            suppress_click.set(false);
                            drag.set(None);
                        };
                        let move_drag = move |ev: web_sys::PointerEvent| {
                            let Some(ctx) = drag_ctx.get_untracked() else {
                                return;
                            };
                            // Only a real drag shows the ghost; a click stays a click.
                            let dx = (ev.client_x() as f64 - ctx.start_x).abs();
                            let dy = (ev.client_y() as f64 - ctx.start_y).abs();
                            if dx < 4.0 && dy < 4.0 {
                                return;
                            }
                            let end_day = day_at_x(&ctx.cols, ev.client_x() as f64);
                            let end_min = snap_quarter(ev.client_y() as f64 - ctx.rect_top);
                            drag.set(Some(DragSel {
                                start_day: ctx.start_day,
                                start_min: ctx.start_min,
                                end_day,
                                end_min,
                            }));
                        };
                        let cancel_drag = move |_| {
                            drag_ctx.set(None);
                            drag.set(None);
                        };
                        let create_at = move |ev: leptos::ev::MouseEvent| {
                            if suppress_click.get_untracked() {
                                suppress_click.set(false);
                                return;
                            }
                            let Some(el) = ev
                                .current_target()
                                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                            else {
                                return;
                            };
                            let rect = el.get_bounding_client_rect();
                            let y = ev.client_y() as f64 - rect.top();
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
                                on:pointerdown=start_drag
                                on:pointermove=move_drag
                                on:pointerup=move |ev| finish_drag(ev)
                                on:pointercancel=cancel_drag
                                on:click=create_at
                            >
                                <Show
                                    when=move || {
                                        drag.get()
                                            .and_then(|s| ghost_segment(&s, &days.get(), d))
                                            .is_some()
                                    }
                                    fallback=|| ()
                                >
                                    {move || {
                                        let sel = drag.get()?;
                                        let (top, height) = ghost_segment(&sel, &days.get(), d)?;
                                        Some(view! {
                                            <div
                                                class="drag-ghost"
                                                style=format!("top:{top:.1}px;height:{height:.1}px")
                                            ></div>
                                        })
                                    }}
                                </Show>
                                <For
                                    each=move || shown.get()
                                    // key must include the layout (col/cols): the children
                                    // read them as plain values for left/width, and a new
                                    // overlapping event changes them without changing
                                    // (i, occ.start, fp) — stale rows overlapped until reload
                                    key=|(i, o, fp, col, cols)| (*i, o.start, fp.clone(), *col, *cols)
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
                                                class:read-only=read_only
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
                                                        state.open_details(idx, occurrence, display_start, Some((e.client_x() as i32, e.client_y() as i32)));
                                                    }
                                                }
                                            >
                                                <span class="te-time">{time}</span>
                                                <span class="te-title">{summary}</span>
                                                <Show when=move || read_only fallback=|| ()>
                                                    <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 7V5a3 3 0 0 1 6 0v2" fill="none" stroke="currentColor" stroke-width="1.4" /><rect x="3.4" y="7" width="9.2" height="6.6" rx="1.4" fill="currentColor" /></svg></span>
                                                </Show>
                                            </div>
                                        }
                                    }
                                />
                                // One "+N more" bar per overflowing cluster: it occupies
                                // the slot freed by the overflow collapse and spans
                                // exactly the hidden events' range (see flush_cluster).
                                // Clusters are disjoint in time, so their bars never
                                // collide. Left offset: week 1/2 = 50%, day 3/4 = 75%.
                                <For
                                    each=move || bars.get()
                                    // key includes the bar geometry + count (these are
                                    // plain values the children read), the first hidden
                                    // start, AND the hidden events themselves: a refresh
                                    // can produce a bar with identical geometry/count
                                    // whose hidden set differs, and the old closure then
                                    // kept the previous indices (the popup listed events
                                    // from other days).
                                    key=|b| (
                                        b.first_start,
                                        b.top_px,
                                        b.height_px,
                                        b.count,
                                        b.indices.clone(),
                                    )
                                    children=move |b| {
                                        let more_left_pct = (max_columns - 1) as f64 / max_columns as f64 * 100.0;
                                        let indices: Vec<(usize, chrono::DateTime<chrono_tz::Tz>)> = b.indices.clone();
                                        let count = b.count;
                                        view! {
                                            <button
                                                class="more-bar"
                                                style=format!(
                                                    "top:{}px;height:{}px;left:{:.4}%;width:calc({:.4}% - 3px)",
                                                    b.top_px,
                                                    b.height_px,
                                                    more_left_pct,
                                                    100.0 - more_left_pct
                                                )
                                                on:click=move |e| {
                                                    e.stop_propagation();
                                                    // open exactly the events hidden behind this bar
                                                    state.week_more.set(Some((
                                                        indices.clone(),
                                                        e.client_x() as f64,
                                                        e.client_y() as f64,
                                                    )));
                                                }
                                            >
                                                {move || crate::i18n::more_count(state.lang.get(), count)}
                                            </button>
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

#[cfg(test)]
mod layout_tests {
    use super::*;
    use chrono::TimeZone;

    const TZ: chrono_tz::Tz = chrono_tz::Europe::Berlin;
    const DAY: NaiveDate = chrono::NaiveDate::from_ymd_opt(2026, 11, 20).unwrap(); // Friday
    const PX: f64 = HOUR_HEIGHT; // 42 px per hour

    fn occ(start: &str, end: &str) -> Occurrence {
        let dt = |s: &str| {
            let n = chrono::NaiveDateTime::parse_from_str(&format!("2026-11-20T{s}"), "%Y-%m-%dT%H:%M")
                .unwrap();
            TZ.from_local_datetime(&n).single().unwrap()
        };
        Occurrence {
            start: dt(start),
            end: dt(end),
            recurrence_id: None,
            override_idx: None,
        }
    }

    fn lay<'a>(
        evs: impl IntoIterator<Item = &'a str>,
        max_columns: usize,
    ) -> (Vec<Occurrence>, Vec<(usize, usize, bool)>, Vec<ClusterBar>) {
        let events: Vec<Occurrence> = evs.into_iter().map(|s| s.split_once(' ').unwrap()).map(|(a, b)| occ(a, b)).collect();
        let (cols, bars) = layout(&events, max_columns, DAY, &TZ);
        (events, cols, bars)
    }

    fn px(minutes: f64) -> i64 {
        (minutes / 60.0 * PX).round() as i64
    }

    /// A lone event keeps full width even when a busy block exists later in
    /// the day, and each cluster only narrows to its own overlap: the point
    /// of the cluster rewrite (the old day-global width shrank everything).
    /// Staggered chains also pack tighter than their member count — three
    /// 1-hour events shifted by 30 minutes never overlap three at a time,
    /// so they share two columns.
    #[test]
    fn clusters_get_their_own_widths() {
        // pair in the morning, true triple (all simultaneous) in the afternoon
        let (_, cols, bars) = lay(
            ["09:00 11:00", "10:00 12:00", "13:00 14:00", "13:30 15:00", "13:45 14:15"],
            4,
        );
        assert_eq!(cols[0], (0, 2, true), "first pair event: col 0, half width");
        assert_eq!(cols[1], (1, 2, true), "second pair event: col 1, half width");
        assert_eq!(cols[2], (0, 3, true), "triple: col 0, third width");
        assert_eq!(cols[3], (1, 3, true), "triple: col 1");
        assert_eq!(cols[4], (2, 3, true), "triple: col 2");
        assert!(bars.is_empty(), "no overflow, no bar");
    }

    /// A staggered chain (each event overlaps the next, but no moment has
    /// all of them) uses only max-simultaneous columns.
    #[test]
    fn staggered_chain_packs_two_columns() {
        let (_, cols, bars) = lay(["13:00 14:00", "13:30 14:30", "14:00 15:00", "14:30 15:30"], 2);
        assert_eq!(cols[0], (0, 2, true));
        assert_eq!(cols[1], (1, 2, true));
        assert_eq!(cols[2], (0, 2, true), "reuses col 0 after the first one ended");
        assert_eq!(cols[3], (1, 2, true));
        assert!(bars.is_empty(), "a chain that is never 3-deep must not overflow");
    }

    /// A single event touching nothing gets the full column.
    #[test]
    fn singleton_is_full_width() {
        let (_, cols, bars) = lay(["09:00 10:00"], 2);
        assert_eq!(cols[0], (0, 1, true));
        assert!(bars.is_empty());
    }

    /// Events that merely touch (end == next start) do not overlap.
    #[test]
    fn back_to_back_events_are_separate_clusters() {
        let (_, cols, bars) = lay(["09:00 10:00", "10:00 11:00"], 2);
        assert_eq!(cols[0], (0, 1, true), "both must be full width");
        assert_eq!(cols[1], (0, 1, true));
        assert!(bars.is_empty());
    }

    /// Overflow in the day view (cap 4): five simultaneous events show 3 and
    /// the bar counts BOTH hidden ones — the greedy-global layout reported
    /// "+1 more" and silently dropped the event in the collapse slot.
    #[test]
    fn day_overflow_bar_counts_every_hidden_event() {
        let (events, cols, bars) =
            lay(["10:00 12:00", "10:00 12:00", "10:00 12:00", "10:00 12:00", "10:00 12:00"], 4);
        assert_eq!(cols[0], (0, 4, true));
        assert_eq!(cols[1], (1, 4, true));
        assert_eq!(cols[2], (2, 4, true));
        assert_eq!(cols[3], (3, 4, false), "collapse slot hidden");
        assert_eq!(cols[4], (4, 4, false), "beyond the cap hidden");
        assert_eq!(bars.len(), 1);
        let b = &bars[0];
        assert_eq!(b.count, 2, "one per column dropped by the collapse + one over cap");
        assert_eq!(b.top_px, px(600.0));
        assert_eq!(b.height_px, px(120.0));
        assert_eq!(b.first_start, events[3].start);
        assert_eq!(
            b.local_indices,
            vec![3, 4],
            "hidden events stay in start order (per-day indices)"
        );
    }

    /// Overflow in the week view (cap 2): three simultaneous -> one shown +
    /// "+2 more" spanning their range.
    #[test]
    fn week_overflow_shows_one_plus_bar() {
        let (_, cols, bars) = lay(["10:00 11:00", "10:00 11:00", "10:00 11:00"], 2);
        assert_eq!(cols[0], (0, 2, true));
        assert_eq!(cols[1], (1, 2, false));
        assert_eq!(cols[2], (2, 2, false));
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].count, 2);
        assert_eq!(bars[0].top_px, px(600.0));
        assert_eq!(bars[0].height_px, px(60.0));
    }

    /// Overflow only narrows its own cluster: a full-width lone morning event
    /// next to an overflowing afternoon block.
    #[test]
    fn overflow_does_not_shrink_other_clusters() {
        let (_, cols, bars) = lay(["08:00 09:00", "14:00 16:00", "14:00 16:00", "14:00 16:00", "14:00 16:00", "14:00 16:00"], 4);
        assert_eq!(cols[0], (0, 1, true), "lone morning event keeps full width");
        // 5-overlap afternoon cluster: 3 shown + bar
        assert_eq!(cols[1], (0, 4, true));
        assert_eq!(cols[2], (1, 4, true));
        assert_eq!(cols[3], (2, 4, true));
        assert!(!cols[4].2);
        assert!(!cols[5].2);
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].count, 2);
    }

    /// Equal starts: the longer event takes the earlier (visible) column.
    #[test]
    fn longer_events_get_earlier_columns() {
        let (_, cols, bars) = lay(["10:00 11:00", "10:00 11:30", "10:00 11:15", "10:00 11:00", "10:00 11:00"], 4);
        // five simultaneous, sorted longer-first: 11:30 at col 0, 11:15 at col 1…
        assert_eq!(cols[1], (0, 4, true), "longest is in col 0");
        assert_eq!(cols[2], (1, 4, true), "second-longest in col 1");
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].count, 2);
    }

    /// Two separate overflowing blocks produce two bars (time-disjoint).
    #[test]
    fn two_overflowing_clusters_get_two_bars() {
        let (_, cols, bars) = lay(
            ["09:00 09:30", "09:00 09:30", "09:10 09:20", "15:00 15:30", "15:00 15:30", "15:10 15:20"],
            2,
        );
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].top_px, px(540.0), "morning bar at 09:00");
        assert_eq!(bars[1].top_px, px(900.0), "afternoon bar at 15:00");
        assert_eq!(bars[0].count, 2);
        assert_eq!(bars[1].count, 2);
        assert!(cols.iter().any(|c| c.2), "some event is shown");
    }

    /// An overnight continuation (starts the previous day) clamps to the top
    /// of the day for both its layout slot and the bar range.
    #[test]
    fn overnight_continuation_clamps_to_day() {
        let tz = TZ;
        let start = tz
            .from_local_datetime(&chrono::NaiveDateTime::parse_from_str("2026-11-19T23:00", "%Y-%m-%dT%H:%M").unwrap())
            .single()
            .unwrap();
        let end = tz
            .from_local_datetime(&chrono::NaiveDateTime::parse_from_str("2026-11-20T01:00", "%Y-%m-%dT%H:%M").unwrap())
            .single()
            .unwrap();
        let events = vec![
            occ("00:30", "01:00"),
            Occurrence {
                start,
                end,
                recurrence_id: None,
                override_idx: None,
            },
        ];
        let (cols, bars) = layout(&events, 4, DAY, &TZ);
        assert_eq!(cols[0], (1, 2, true), "the 00:30 event is in col 1 (overn is earlier)");
        assert_eq!(cols[1], (0, 2, true), "the overnight continuation takes col 0");
        assert!(bars.is_empty());
    }
}
