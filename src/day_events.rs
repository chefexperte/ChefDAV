//! Shared helpers for the day and time-slot popovers.

use caldav::recurrence::{expand_series, Occurrence};
use caldav::time::agenda_range;
use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use leptos::prelude::*;

use crate::state::AppState;

#[derive(Clone, PartialEq)]
pub struct DayItem {
    pub index: usize,
    pub occurrence: Occurrence,
    pub all_day: bool,
}

/// All occurrences overlapping the given calendar day, sorted by start.
pub fn events_for_day(state: &AppState, day: NaiveDate) -> Vec<DayItem> {
    let (rs, re) = agenda_range(day, 1, &state.tz);
    let mut out = Vec::new();
    for (i, ev) in state.events.get().iter().enumerate() {
        let all_day = ev
            .event
            .dtstart
            .as_ref()
            .map(|d| d.is_date())
            .unwrap_or(false);
        for occ in expand_series(&ev.event, &ev.overrides, rs, re, &state.tz) {
            out.push(DayItem {
                index: i,
                occurrence: occ,
                all_day,
            });
        }
    }
    out.sort_by_key(|x| x.occurrence.start);
    out
}

/// Events overlapping a 30-minute slot starting at `minutes` on `day`.
/// All-day events are included at the top.
pub fn events_at_slot(state: &AppState, day: NaiveDate, minutes: u32) -> Vec<DayItem> {
    let Some(start) = state
        .tz
        .from_local_datetime(
            &day.and_hms_opt(minutes / 60, minutes % 60, 0)
                .expect("valid slot time"),
        )
        .single()
    else {
        return Vec::new();
    };
    let end = start + Duration::minutes(30);
    let mut items: Vec<DayItem> = events_for_day(state, day)
        .into_iter()
        .filter(|item| {
            item.all_day || (item.occurrence.start < end && item.occurrence.end > start)
        })
        .collect();
    items.sort_by_key(|x| (x.all_day, x.occurrence.start));
    items
}

/// The title to show for an occurrence (override title wins).
pub fn item_title(state: &AppState, item: &DayItem) -> String {
    state
        .events
        .get()
        .get(item.index)
        .map(|ev| {
            item.occurrence
                .override_idx
                .and_then(|i| ev.overrides.get(i))
                .map(|o| o.summary.clone())
                .unwrap_or_else(|| ev.event.summary.clone())
        })
        .unwrap_or_default()
}

pub fn item_color(state: &AppState, item: &DayItem) -> String {
    state
        .events
        .get()
        .get(item.index)
        .map(|ev| ev.color.clone())
        .unwrap_or_else(|| crate::state::ACCENT.to_string())
}

/// A stable key for the occurrence (used by `For`).
pub fn item_key(item: &DayItem) -> (usize, chrono::DateTime<chrono_tz::Tz>, Option<usize>) {
    (
        item.index,
        item.occurrence.start,
        item.occurrence.override_idx,
    )
}

/// Format the day label used by popovers.
pub fn popup_day_label(state: &AppState, day: NaiveDate) -> String {
    crate::i18n::day_label(state.lang.get(), day)
}

/// Weekday index helper (0 = Monday).
pub fn weekday_index(day: NaiveDate) -> usize {
    day.weekday().num_days_from_monday() as usize
}
