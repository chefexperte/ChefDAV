//! Time-slot popover: events at the clicked time + "New event" prefilled.

use leptos::prelude::*;

use crate::components::day_popup::{clamp_position, occurrence_time};
use crate::day_events::{self, DayItem};
use crate::state::{AppState, ModalState};

#[component]
pub fn SlotPopup() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let popup = state.slot_popup.get().expect("slot popup is open");
    let date = popup.date;
    let minutes = popup.minutes;
    let all_day = popup.all_day;
    let items = Memo::new(move |_| {
        if all_day {
            day_events::events_for_day(&state, date)
                .into_iter()
                .filter(|i| i.all_day)
                .collect::<Vec<DayItem>>()
        } else {
            day_events::events_at_slot(&state, date, minutes)
        }
    });
    let (left, top) = clamp_position(popup.x, popup.y, 340.0, 420.0);

    let close = move |_| state.slot_popup.set(None);
    let new_event = move |_| {
        state.slot_popup.set(None);
        if all_day {
            state.new_event_all_day.set(true);
            state.new_event_time.set(None);
        } else {
            state.new_event_all_day.set(false);
            state.new_event_time
                .set(Some((minutes / 60, minutes % 60)));
        }
        state.modal.set(Some(ModalState::New { date }));
    };

    let title = if all_day {
        format!(
            "{} · {}",
            day_events::popup_day_label(&state, date),
            crate::i18n::t(state.lang.get(), "all_day_lower")
        )
    } else {
        format!(
            "{} · {:02}:{:02}",
            day_events::popup_day_label(&state, date),
            minutes / 60,
            minutes % 60
        )
    };

    view! {
        <div class="popover-backdrop" on:click=close>
            <div
                class="popover"
                style=format!("left:{left}px; top:{top}px")
                on:click=move |e| e.stop_propagation()
                role="dialog"
                aria-modal="true"
            >
                <h3 class="popover-title">{title}</h3>
                <Show
                    when=move || !items.get().is_empty()
                    fallback=move || {
                        view! {
                            <p class="popover-empty">
                                {move || crate::i18n::t(state.lang.get(), "no_events_slot")}
                            </p>
                        }
                    }
                >
                    <ul class="popover-events">
                        <For
                            each=move || items.get()
                            key=day_events::item_key
                            children=move |item| {
                                let idx = item.index;
                                let occ = item.occurrence.clone();
                                let title = day_events::item_title(&state, &item);
                                let color = day_events::item_color(&state, &item);
                                let time = occurrence_time(&state, &item);
                                view! {
                                    <li>
                                        <button
                                            class="popover-event"
                                            style=format!("--chip:{color}")
                                            on:click=move |_| {
                                                state.slot_popup.set(None);
                                                state.modal.set(Some(ModalState::Details {
                                                    index: idx,
                                                    occurrence: occ.recurrence_id,
                                                    display_start: Some(occ.start),
                                                }));
                                            }
                                        >
                                            <span class="popover-time">{time}</span>
                                            <span class="cal-dot" style=format!("background:{color}")></span>
                                            <span class="popover-event-title">{title}</span>
                                        </button>
                                    </li>
                                }
                            }
                        />
                    </ul>
                </Show>
                <div class="popover-actions">
                    <button class="popup-action" on:click=new_event>
                        <span class="popup-icon">"＋"</span>
                        <span>
                            {move || if all_day {
                                crate::i18n::t(state.lang.get(), "new_all_day_event")
                            } else {
                                crate::i18n::t(state.lang.get(), "new_event")
                            }}
                        </span>
                    </button>
                </div>
            </div>
        </div>
    }
}
