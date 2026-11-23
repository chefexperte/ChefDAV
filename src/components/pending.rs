//! Pending offline writes: inspect the outbox and discard individual changes.

use caldav::ics::{parse_cached_event, parse_cached_todo, DtValue};
use chrono::NaiveDate;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::state::AppState;
use crate::store::{self, OutboxOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PendingKind {
    Save,
    Delete,
}

#[derive(Clone, Debug)]
struct PendingRow {
    index: usize,
    op: OutboxOp,
    kind: PendingKind,
    is_todo: bool,
    summary: String,
    date: Option<NaiveDate>,
    time: Option<String>,
    all_day: bool,
    calendar: String,
    color: String,
}

fn date_parts(
    value: Option<&DtValue>,
    tz: &chrono_tz::Tz,
) -> (Option<NaiveDate>, Option<String>, bool) {
    match value {
        Some(DtValue::Date(date)) => (Some(*date), None, true),
        Some(DtValue::DateTime(start)) => {
            let local = start.with_timezone(tz);
            (
                Some(local.date_naive()),
                Some(local.format("%H:%M").to_string()),
                false,
            )
        }
        None => (None, None, false),
    }
}

fn calendar_meta(state: &AppState, href: &str) -> (String, String) {
    state
        .calendars
        .get_untracked()
        .iter()
        .find(|cal| cal.info.href == href)
        .map(|cal| (cal.info.display_name.clone(), cal.color.clone()))
        .unwrap_or_else(|| {
            (
                crate::i18n::t(state.lang.get_untracked(), "calendar").to_string(),
                crate::state::ACCENT.to_string(),
            )
        })
}

async fn load_rows(state: AppState) -> Vec<PendingRow> {
    let ops = store::outbox_load().await;
    let event_caches = store::cache_load().await;
    let todo_cache = store::todos_cache_load().await;
    let mut rows = Vec::new();

    for (index, op) in ops.iter().enumerate() {
        let (kind, is_todo, summary, date, time, all_day, calendar_href) = match op {
            OutboxOp::PutEvent {
                calendar_href, ics, ..
            } => {
                let mut summary = String::new();
                let mut date = None;
                let mut time = None;
                let mut all_day = false;
                if let Some(event) = parse_cached_event(ics, &state.tz)
                    .into_iter()
                    .find(|event| !event.is_override())
                {
                    summary = event.summary.clone();
                    (date, time, all_day) = date_parts(event.dtstart.as_ref(), &state.tz);
                }
                (
                    PendingKind::Save,
                    false,
                    summary,
                    date,
                    time,
                    all_day,
                    calendar_href.clone(),
                )
            }
            OutboxOp::DeleteEvent { href, .. } => {
                let mut summary = String::new();
                let mut date = None;
                let mut time = None;
                let mut all_day = false;
                let mut calendar_href = String::new();
                if let Some(cached) = event_caches
                    .iter()
                    .flat_map(|cache| cache.events.iter())
                    .find(|event| event.href == *href)
                {
                    calendar_href = cached.calendar_href.clone();
                    if let Some(event) = parse_cached_event(&cached.ics, &state.tz)
                        .into_iter()
                        .find(|event| !event.is_override())
                    {
                        summary = event.summary.clone();
                        (date, time, all_day) = date_parts(event.dtstart.as_ref(), &state.tz);
                    }
                }
                (
                    PendingKind::Delete,
                    false,
                    summary,
                    date,
                    time,
                    all_day,
                    calendar_href,
                )
            }
            OutboxOp::PutTodo {
                calendar_href, ics, ..
            } => {
                let mut summary = String::new();
                let mut date = None;
                let mut time = None;
                let mut all_day = false;
                if let Some(todo) = parse_cached_todo(ics, &state.tz).into_iter().next() {
                    summary = todo.summary.clone();
                    (date, time, all_day) = date_parts(todo.due.as_ref(), &state.tz);
                }
                (
                    PendingKind::Save,
                    true,
                    summary,
                    date,
                    time,
                    all_day,
                    calendar_href.clone(),
                )
            }
            OutboxOp::DeleteTodo { href, .. } => {
                let mut summary = String::new();
                let mut date = None;
                let mut time = None;
                let mut all_day = false;
                let mut calendar_href = String::new();
                if let Some(cached) = todo_cache.iter().find(|todo| todo.href == *href) {
                    calendar_href = cached.calendar_href.clone();
                    if let Some(todo) = parse_cached_todo(&cached.ics, &state.tz)
                        .into_iter()
                        .next()
                    {
                        summary = todo.summary.clone();
                        (date, time, all_day) = date_parts(todo.due.as_ref(), &state.tz);
                    }
                }
                (
                    PendingKind::Delete,
                    true,
                    summary,
                    date,
                    time,
                    all_day,
                    calendar_href,
                )
            }
        };
        let (calendar, color) = calendar_meta(&state, &calendar_href);
        rows.push(PendingRow {
            index,
            op: op.clone(),
            kind,
            is_todo,
            summary,
            date,
            time,
            all_day,
            calendar,
            color,
        });
    }
    rows
}

#[component]
pub fn PendingModal() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let rows = RwSignal::new(Vec::<PendingRow>::new());
    let loading = RwSignal::new(true);
    let discarding = RwSignal::new(false);

    let close = move |_| state.pending_open.set(false);

    Effect::new(move |_| {
        let _ = state.pending.get();
        loading.set(true);
        spawn_local(async move {
            let loaded = load_rows(state).await;
            rows.set(loaded);
            loading.set(false);
        });
    });

    let discard = move |op: OutboxOp| {
        if discarding.get_untracked() {
            return;
        }
        discarding.set(true);
        let st = state;
        spawn_local(async move {
            api::discard_outbox_op(&st, &op).await;
            discarding.set(false);
            if st.pending.get_untracked() == 0 {
                st.pending_open.set(false);
            }
        });
    };

    view! {
        <div class="modal-overlay" on:click=close>
            <div
                class="modal modal-small pending-modal"
                on:click=move |event| event.stop_propagation()
                role="dialog"
                aria-modal="true"
            >
                <h3>{move || crate::i18n::t(state.lang.get(), "pending_changes")}</h3>
                <p class="pending-hint">
                    {move || crate::i18n::t(state.lang.get(), "pending_hint")}
                </p>
                <Show
                    when=move || !loading.get()
                    fallback=move || {
                        view! {
                            <div class="pending-loading">
                                <span class="spinner" aria-hidden="true"></span>
                            </div>
                        }
                    }
                >
                    <ul class="pending-list">
                        <For
                            each=move || rows.get()
                            key=|row| {
                                (
                                    row.index,
                                    row.summary.clone(),
                                    row.date,
                                    row.time.clone(),
                                    row.is_todo,
                                    row.kind,
                                )
                            }
                            children=move |row| {
                                let op = row.op.clone();
                                let summary = row.summary.clone();
                                let has_summary = !summary.trim().is_empty();
                                let fallback_key = if row.is_todo { "pending_task" } else { "pending_event" };
                                let summary_text = Memo::new(move |_| {
                                    if has_summary {
                                        summary.clone()
                                    } else {
                                        crate::i18n::t(state.lang.get(), fallback_key).to_string()
                                    }
                                });
                                let kind = row.kind;
                                let calendar = row.calendar.clone();
                                let color = row.color.clone();
                                let date = row.date;
                                let time = row.time.clone();
                                let all_day = row.all_day;
                                let when = Memo::new(move |_| match date {
                                    Some(date) => {
                                        let label = crate::i18n::day_label(state.lang.get(), date);
                                        if all_day {
                                            format!("{label} – {}", crate::i18n::t(state.lang.get(), "all_day"))
                                        } else if let Some(time) = &time {
                                            format!("{label} – {time}")
                                        } else {
                                            label
                                        }
                                    }
                                    None => String::new(),
                                });
                                let discard = discard;
                                view! {
                                    <li class="pending-item">
                                        <span class="cal-dot" style=format!("background:{color}")></span>
                                        <div class="pending-main">
                                            <span class="pending-summary">
                                                {move || summary_text.get()}
                                            </span>
                                            <span class="pending-meta">
                                                <span class="pending-action">
                                                    {move || crate::i18n::t(
                                                        state.lang.get(),
                                                        match kind {
                                                            PendingKind::Save => "pending_save",
                                                            PendingKind::Delete => "pending_delete",
                                                        },
                                                    )}
                                                </span>
                                                <span>{calendar.clone()}</span>
                                                <Show when=move || date.is_some() fallback=|| ()>
                                                    <span>{move || when.get()}</span>
                                                </Show>
                                            </span>
                                        </div>
                                        <button
                                            class="btn btn-icon btn-ghost pending-discard"
                                            title=move || crate::i18n::t(state.lang.get(), "pending_discard")
                                            aria-label=move || crate::i18n::t(state.lang.get(), "pending_discard")
                                            disabled=move || discarding.get()
                                            on:click=move |_| discard(op.clone())
                                        >
                                            <svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
                                                <path d="M3 4h10M6 4V2.8h4V4M4.5 4l.5 9h6l.5-9M6.5 7v4M9.5 7v4" />
                                            </svg>
                                        </button>
                                    </li>
                                }
                            }
                        />
                    </ul>
                </Show>
                <div class="modal-actions">
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>
                        {move || crate::i18n::t(state.lang.get(), "close")}
                    </button>
                </div>
            </div>
        </div>
    }
}
