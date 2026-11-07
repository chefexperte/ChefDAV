//! Popup listing the events hidden behind a "+N more" badge in the week view.

use leptos::prelude::*;

use crate::state::{AppState, ModalState};

#[component]
pub fn WeekMoreModal() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let indices = state.week_more.get().expect("week more is open");
    let tz = state.tz;
    let close = move |_| state.week_more.set(None);

    let items = indices
        .into_iter()
        .filter_map(|i| state.events.get().get(i).cloned().map(|e| (i, e)))
        .collect::<Vec<_>>();

    view! {
        <div class="modal-overlay" on:click=close>
            <div class="modal modal-small" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                <h3>"More events"</h3>
                <ul class="more-list">
                    <For
                        each=move || items.clone()
                        key=|(i, e)| (*i, e.href.clone())
                        children=move |(idx, ev)| {
                            let time = match &ev.event.dtstart {
                                Some(caldav::ics::DtValue::Date(_)) => "All day".to_string(),
                                Some(caldav::ics::DtValue::DateTime(t)) => {
                                    t.with_timezone(&tz).format("%H:%M").to_string()
                                }
                                None => String::new(),
                            };
                            let summary = ev.event.summary.clone();
                            let color = ev.color.clone();
                            view! {
                                <li>
                                    <button
                                        class="agenda-item"
                                        style=format!("--chip:{color}")
                                        on:click=move |_| {
                                            state.week_more.set(None);
                                            state.modal.set(Some(ModalState::Details { index: idx }));
                                        }
                                    >
                                        <span class="agenda-time">{time}</span>
                                        <span class="agenda-dot"></span>
                                        <span class="agenda-title">{summary}</span>
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
                <div class="modal-actions">
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>"Close"</button>
                </div>
            </div>
        </div>
    }
}
