//! Small popup shown when a day cell is clicked: choose what to create.

use leptos::prelude::*;

use crate::state::{AppState, ModalState};

#[component]
pub fn DayPopup() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let date = state.day_popup.get().expect("day popup is open");
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
        <div class="modal-overlay" on:click=close>
            <div class="modal modal-tiny" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                <h3 class="popup-title">{caldav::time::day_label(date)}</h3>
                <div class="popup-actions">
                    <button class="popup-action" on:click=new_event>
                        <span class="popup-icon">"＋"</span>
                        <span>"New event"</span>
                    </button>
                    <button class="popup-action" on:click=new_task>
                        <span class="popup-icon">"☑"</span>
                        <span>"New task"</span>
                    </button>
                    <button class="popup-action" on:click=go_day>
                        <span class="popup-icon">"→"</span>
                        <span>"Go to day"</span>
                    </button>
                </div>
            </div>
        </div>
    }
}
