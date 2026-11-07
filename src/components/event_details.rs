//! Read-only event details. The editor opens only from the pencil button.

use caldav::ics::{DtValue, VEvent};
use caldav::recurrence::describe_rule;
use chrono::Duration;
use leptos::prelude::*;

use crate::state::{AppState, Lang, ModalState};

fn format_range(
    lang: Lang,
    start: chrono::DateTime<chrono_tz::Tz>,
    end: chrono::DateTime<chrono_tz::Tz>,
    all_day: bool,
    tz: &chrono_tz::Tz,
) -> String {
    let start = start.with_timezone(tz);
    let end = end.with_timezone(tz);
    if all_day {
        let start_d = start.date_naive();
        let end_d = (end - Duration::days(1)).date_naive();
        if end_d > start_d {
            format!(
                "{} – {}",
                crate::i18n::day_label(lang, start_d),
                crate::i18n::day_label(lang, end_d)
            )
        } else {
            format!(
                "{} · {}",
                crate::i18n::day_label(lang, start_d),
                crate::i18n::t(lang, "all_day_lower")
            )
        }
    } else if end.date_naive() == start.date_naive() {
        format!(
            "{} · {}–{}",
            crate::i18n::day_label(lang, start.date_naive()),
            start.format("%H:%M"),
            end.format("%H:%M")
        )
    } else {
        format!(
            "{} {} – {} {}",
            crate::i18n::day_label(lang, start.date_naive()),
            start.format("%H:%M"),
            crate::i18n::day_label(lang, end.date_naive()),
            end.format("%H:%M")
        )
    }
}

fn duration_days(ev: &VEvent) -> Option<i64> {
    match (&ev.dtstart, &ev.dtend) {
        (Some(DtValue::Date(a)), Some(DtValue::Date(b))) => Some(b.signed_duration_since(*a).num_days()),
        _ => None,
    }
}

#[component]
pub fn EventDetails() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;
    let modal = state.modal.get().expect("details modal is open");
    let (index, occurrence, display_start) = match modal {
        ModalState::Details {
            index,
            occurrence,
            display_start,
        } => (index, occurrence, display_start),
        _ => (0, None, None),
    };
    let event = move || state.events.get().get(index).cloned();

    view! {
        <div class="modal-overlay" on:click=move |_| state.modal.set(None)>
            <div class="modal details-modal" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                {move || {
                    event().map(|ev| {
                        let override_event = occurrence.and_then(|occ| {
                            ev.overrides
                                .iter()
                                .find(|o| {
                                    o.recurrence_id
                                        .as_ref()
                                        .and_then(|d| d.to_datetime(&tz))
                                        == Some(occ)
                                })
                                .cloned()
                        });
                        let shown = override_event.as_ref().unwrap_or(&ev.event);
                        let summary = shown.summary.clone();
                        let color = ev.color.clone();
                        let all_day = shown
                            .dtstart
                            .as_ref()
                            .map(|d| d.is_date())
                            .unwrap_or(false)
                            || ev
                                .event
                                .dtstart
                                .as_ref()
                                .map(|d| d.is_date())
                                .unwrap_or(false);
                        let start = display_start
                            .or_else(|| shown.dtstart.as_ref().and_then(|d| d.to_datetime(&tz)))
                            .or_else(|| {
                                ev.event
                                    .dtstart
                                    .as_ref()
                                    .and_then(|d| d.to_datetime(&tz))
                            });
                        let dur = caldav::recurrence::event_duration(shown)
                            .or_else(|| caldav::recurrence::event_duration(&ev.event))
                            .unwrap_or_default();
                        let when = start
                            .map(|s| format_range(state.lang.get(), s, s + dur, all_day, &tz))
                            .unwrap_or_else(|| {
                                crate::i18n::t(state.lang.get(), "no_date").to_string()
                            });
                        let location = shown.location.clone();
                        let description = shown.description.clone();
                        let calendar = state
                            .calendar_for(&ev.calendar_href)
                            .map(|c| c.info.display_name)
                            .unwrap_or_else(|| crate::i18n::t(state.lang.get(), "calendar").to_string());
                        let read_only = ev.read_only;
                        let is_exception = override_event.is_some();
                        let rule = ev.event.rrule.clone();
                        let has_rule = rule.is_some();
                        let rule_text = rule
                            .as_deref()
                            .map(|r| crate::i18n::describe_rule(state.lang.get(), r))
                            .unwrap_or_default();
                        let days = duration_days(&ev.event);
                        let has_location = !location.is_empty();
                        let has_description = !description.is_empty();
                        let is_multi_day = days.map(|d| d > 1).unwrap_or(false);
                        view! {
                            <div class="details-head">
                                <span class="cal-dot" style=format!("background:{color}")></span>
                                <h3>{summary}</h3>
                            </div>
                            <p class="details-when">{when}</p>
                            <p class="details-meta">
                                <span class="details-cal">{calendar}</span>
                                <Show when=move || is_multi_day fallback=|| ()>
                                    <span class="details-days">{move || format!("{} {}", days.unwrap_or(0), crate::i18n::t(state.lang.get(), "days"))}</span>
                                </Show>
                                <Show when=move || is_exception fallback=|| ()>
                                    <span class="details-days">{move || crate::i18n::t(state.lang.get(), "changed_occurrence")}</span>
                                </Show>
                                <Show when=move || read_only fallback=|| ()>
                                    <span class="details-readonly">{move || crate::i18n::t(state.lang.get(), "read_only_short")}</span>
                                </Show>
                            </p>
                            <Show when=move || has_location fallback=|| ()>
                                <p class="details-row"><span class="details-label">{move || crate::i18n::t(state.lang.get(), "location")}</span>{location.clone()}</p>
                            </Show>
                            <Show when=move || has_description fallback=|| ()>
                                <p class="details-row details-notes"><span class="details-label">{move || crate::i18n::t(state.lang.get(), "notes")}</span>{description.clone()}</p>
                            </Show>
                            <Show when=move || has_rule fallback=|| ()>
                                <p class="details-row">
                                    <span class="details-label">{move || crate::i18n::t(state.lang.get(), "repeats")}</span>
                                    {rule_text.clone()}
                                </p>
                            </Show>
                            <div class="modal-actions">
                                <span class="spacer"></span>
                                <button class="btn btn-ghost" on:click=move |_| state.modal.set(None)>
                                    {move || crate::i18n::t(state.lang.get(), "close")}
                                </button>
                                <Show when=move || !read_only fallback=|| ()>
                                    <button
                                        class="btn btn-primary"
                                        on:click=move |_| state.modal.set(Some(ModalState::Edit {
                                            index,
                                            occurrence,
                                            display_start,
                                        }))
                                    >
                                        {move || format!("✎ {}", crate::i18n::t(state.lang.get(), "edit_event"))}
                                    </button>
                                </Show>
                            </div>
                        }
                    })
                }}
            </div>
        </div>
    }
}
