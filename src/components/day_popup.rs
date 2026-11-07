//! Day popover: the day's events + create actions.

use leptos::prelude::*;

use crate::day_events::{self, DayItem};
use crate::state::{AppState, ModalState};

pub fn clamp_position(x: f64, y: f64, width: f64, height: f64) -> (f64, f64) {
    let win = web_sys::window();
    let vw = win
        .as_ref()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(1200.0);
    let vh = win
        .as_ref()
        .and_then(|w| w.inner_height().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0);
    (
        (x + 8.0).min(vw - width - 8.0).max(8.0),
        (y + 8.0).min(vh - height - 8.0).max(8.0),
    )
}

pub fn occurrence_time(state: &AppState, item: &DayItem) -> String {
    if item.all_day {
        crate::i18n::t(state.lang.get(), "all_day").to_string()
    } else {
        item.occurrence
            .start
            .with_timezone(&state.tz)
            .format("%H:%M")
            .to_string()
    }
}

#[component]
pub fn DayPopup() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let popup = state.day_popup.get().expect("day popup is open");
    let date = popup.date;
    let items = Memo::new(move |_| day_events::events_for_day(&state, date));
    let (left, top) = clamp_position(popup.x, popup.y, 340.0, 460.0);

    let close = move |_| state.day_popup.set(None);
    let new_event = move |_| {
        state.day_popup.set(None);
        state.new_event_time.set(None);
        state.new_event_all_day.set(false);
        state.modal.set(Some(ModalState::New { date }));
    };
    let new_task = move |_| {
        state.day_popup.set(None);
        state.modal.set(Some(ModalState::NewTodo { date }));
    };
    let go_day = move |_| {
        state.day_popup.set(None);
        state.anchor.set(date);
        state.view.set(crate::state::ViewMode::Day);
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
                <h3 class="popover-title">{day_events::popup_day_label(&state, date)}</h3>
                <Show
                    when=move || !items.get().is_empty()
                    fallback=move || {
                        view! {
                            <p class="popover-empty">
                                {move || crate::i18n::t(state.lang.get(), "no_events_day")}
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
                                                state.day_popup.set(None);
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
                        <span>{move || crate::i18n::t(state.lang.get(), "new_event")}</span>
                    </button>
                    <button class="popup-action" on:click=new_task>
                        <span class="popup-icon">"☑"</span>
                        <span>{move || crate::i18n::t(state.lang.get(), "new_task")}</span>
                    </button>
                    <button class="popup-action" on:click=go_day>
                        <span class="popup-icon">"→"</span>
                        <span>{move || crate::i18n::t(state.lang.get(), "go_to_day")}</span>
                    </button>
                </div>
            </div>
        </div>
    }
}
