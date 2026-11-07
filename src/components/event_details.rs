//! Read-only event details. The editor opens only from the pencil button.

use caldav::ics::{DtValue, VEvent};
use caldav::recurrence::describe_rule;
use caldav::time::day_label;
use chrono::{Duration, NaiveDate};
use leptos::prelude::*;

use crate::state::{AppState, ModalState};

fn format_when(ev: &VEvent, tz: &chrono_tz::Tz) -> String {
    match &ev.dtstart {
        Some(DtValue::Date(d)) => {
            let end = match &ev.dtend {
                Some(DtValue::Date(e)) => Some(*e),
                _ => None,
            };
            match end {
                Some(e) if e > *d + Duration::days(1) => {
                    format!("{} – {}", day_label(*d), day_label(e - Duration::days(1)))
                }
                _ => format!("{} · all day", day_label(*d)),
            }
        }
        Some(DtValue::DateTime(t)) => {
            let start = t.with_timezone(tz);
            let end = ev
                .dtend
                .as_ref()
                .and_then(|x| x.to_datetime(tz))
                .map(|x| x.with_timezone(tz));
            let end_txt = end
                .map(|e| {
                    if e.date_naive() == start.date_naive() {
                        e.format("%H:%M").to_string()
                    } else {
                        format!("{} {}", day_label(e.date_naive()), e.format("%H:%M"))
                    }
                })
                .unwrap_or_default();
            if end_txt.is_empty() {
                format!("{} · {}", day_label(start.date_naive()), start.format("%H:%M"))
            } else {
                format!(
                    "{} · {}–{}",
                    day_label(start.date_naive()),
                    start.format("%H:%M"),
                    end_txt
                )
            }
        }
        None => "No date".to_string(),
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
    let index = match modal {
        ModalState::Details { index } => index,
        _ => 0,
    };
    let event = move || state.events.get().get(index).cloned();

    view! {
        <div class="modal-overlay" on:click=move |_| state.modal.set(None)>
            <div class="modal details-modal" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                {move || {
                    event().map(|ev| {
                        let summary = ev.event.summary.clone();
                        let color = ev.color.clone();
                        let when = format_when(&ev.event, &tz);
                        let location = ev.event.location.clone();
                        let description = ev.event.description.clone();
                        let calendar = state
                            .calendar_for(&ev.calendar_href)
                            .map(|c| c.info.display_name)
                            .unwrap_or_else(|| "Calendar".to_string());
                        let read_only = ev.read_only;
                        let rule = ev.event.rrule.clone();
                        let has_rule = rule.is_some();
                        let rule_text = rule
                            .as_deref()
                            .map(describe_rule)
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
                                    <span class="details-days">{format!("{} days", days.unwrap_or(0))}</span>
                                </Show>
                                <Show when=move || read_only fallback=|| ()>
                                    <span class="details-readonly">"read-only"</span>
                                </Show>
                            </p>
                            <Show when=move || has_location fallback=|| ()>
                                <p class="details-row"><span class="details-label">"Location"</span>{location.clone()}</p>
                            </Show>
                            <Show when=move || has_description fallback=|| ()>
                                <p class="details-row details-notes"><span class="details-label">"Notes"</span>{description.clone()}</p>
                            </Show>
                            <Show when=move || has_rule fallback=|| ()>
                                <p class="details-row">
                                    <span class="details-label">"Repeats"</span>
                                    {rule_text.clone()}
                                </p>
                            </Show>
                            <div class="modal-actions">
                                <span class="spacer"></span>
                                <button class="btn btn-ghost" on:click=move |_| state.modal.set(None)>
                                    "Close"
                                </button>
                                <Show when=move || !read_only fallback=|| ()>
                                    <button
                                        class="btn btn-primary"
                                        on:click=move |_| state.modal.set(Some(ModalState::Edit { index }))
                                    >
                                        "✎ Edit"
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
