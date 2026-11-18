//! Title menu: clicking the year or the month in the toolbar title opens a
//! picker (AdwMenuPanel-style popover anchored under the title).
//!
//! * year  → a 5×3 grid of years with the shown year in the middle row;
//! * month → the twelve months of the shown year.
//!
//! Both keep the anchor day (clamped, so 31 March → 30 April is safe) and
//! close again; the view itself is not changed, so switching the month while
//! in the week view simply moves to that month's week.

use chrono::{Datelike, NaiveDate};
use leptos::prelude::*;

use crate::components::day_popup::clamp_position;
use crate::state::AppState;

/// Years per block in the year picker (5 columns × 3 rows).
const YEARS_PER_BLOCK: i32 = 15;
const COLS: usize = 5;

/// `year`-`month` with the same day as `anchor`, clamped to the month's length
/// (31 Jan → 28/29 Feb).
fn clamp_day(anchor: NaiveDate, year: i32, month: u32) -> NaiveDate {
    let mut day = anchor.day();
    loop {
        if let Some(d) = NaiveDate::from_ymd_opt(year, month, day) {
            return d;
        }
        if day <= 1 {
            return NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(anchor);
        }
        day -= 1;
    }
}

/// The month picker is bare buttons; the year picker wraps them in the
/// popover surface (same component, two shapes).
fn picker_body(state: AppState, year_mode: bool, anchor: NaiveDate, lang: crate::state::Lang) -> impl IntoView {
    let block_start = anchor.year() - 7;
    let pick = move |date: NaiveDate| {
        state.anchor.set(date);
        state.jump_open.set(None);
    };
    let cells = if year_mode {
        (0..YEARS_PER_BLOCK)
            .map(|i| block_start + i)
            .map(|y| {
                let is_cur = y == anchor.year();
                let date = clamp_day(anchor, y, anchor.month());
                view! {
                    <button class="jump-cell" class:active=is_cur on:click=move |_| pick(date)>
                        {y.to_string()}
                    </button>
                }
            })
            .collect_view()
            .into_any()
    } else {
        (1..=12u32)
            .map(|m| {
                let is_cur = m == anchor.month();
                let date = clamp_day(anchor, anchor.year(), m);
                let name = crate::i18n::month_short(lang, m);
                view! {
                    <button class="jump-cell" class:active=is_cur on:click=move |_| pick(date)>
                        {name}
                    </button>
                }
            })
            .collect_view()
            .into_any()
    };
    view! { <div class="jump-grid" style=format!("--cols:{}", COLS)>{cells}</div> }
}

#[component]
pub fn YearMenu() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let anchor = state.anchor.get();
    let lang = state.lang.get();
    let (left, top) = clamp_position(250.0, 150.0, 248.0, 250.0);
    let close = move |_| state.jump_open.set(None);
    let block_start = anchor.year() - 7;
    let step = move |delta: i32| {
        let a = state.anchor.get();
        state
            .anchor
            .set(clamp_day(a, a.year() + delta * YEARS_PER_BLOCK, a.month()));
    };
    view! {
        <div class="popover-backdrop" on:click=close>
            <div
                class="popover jump-popover"
                style=format!("left:{left}px;top:{top}px")
                on:click=move |e| e.stop_propagation()
                role="dialog"
                aria-modal="true"
                aria-label=move || crate::i18n::t(state.lang.get(), "jump_year")
            >
                <div class="jump-header">
                    <button class="btn btn-icon btn-ghost" aria-label=move || crate::i18n::t(state.lang.get(), "previous") on:click=move |_| step(-1)>"‹"</button>
                    <span class="jump-title">{format!("{} – {}", block_start, block_start + YEARS_PER_BLOCK - 1)}</span>
                    <button class="btn btn-icon btn-ghost" aria-label=move || crate::i18n::t(state.lang.get(), "next") on:click=move |_| step(1)>"›"</button>
                </div>
                {picker_body(state, true, anchor, lang)}
            </div>
        </div>
    }
}

#[component]
pub fn MonthMenu() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let anchor = state.anchor.get();
    let lang = state.lang.get();
    let (left, top) = clamp_position(250.0, 150.0, 268.0, 250.0);
    let close = move |_| state.jump_open.set(None);
    let step = move |delta: i32| {
        let a = state.anchor.get();
        state.anchor.set(clamp_day(a, a.year() + delta, a.month()));
    };
    view! {
        <div class="popover-backdrop" on:click=close>
            <div
                class="popover jump-popover"
                style=format!("left:{left}px;top:{top}px")
                on:click=move |e| e.stop_propagation()
                role="dialog"
                aria-modal="true"
                aria-label=move || crate::i18n::t(state.lang.get(), "jump_month")
            >
                <div class="jump-header">
                    <button class="btn btn-icon btn-ghost" aria-label=move || crate::i18n::t(state.lang.get(), "previous") on:click=move |_| step(-1)>"‹"</button>
                    <span class="jump-title">{anchor.year().to_string()}</span>
                    <button class="btn btn-icon btn-ghost" aria-label=move || crate::i18n::t(state.lang.get(), "next") on:click=move |_| step(1)>"›"</button>
                </div>
                {picker_body(state, false, anchor, lang)}
            </div>
        </div>
    }
}
