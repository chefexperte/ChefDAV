//! Popover listing the events hidden behind a "+N more" badge in the week and
//! day views.
//!
//! It is a plain AdwPopover: one surface (`--popover-bg` + popover shadow),
//! with the event rows as flat list items — no card nested inside the card.

use leptos::prelude::*;

use crate::components::popover_pos;
use crate::state::AppState;

#[component]
pub fn WeekMoreModal() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let (indices, px, py) = state.week_more.get().expect("week more is open");
    let tz = state.tz;
    let close = move |_| state.week_more.set(None);
    let panel: NodeRef<leptos::html::Div> = NodeRef::new();
    crate::components::popover_pos::use_place(
        panel,
        crate::components::popover_pos::Anchor::Cursor(px, py),
    );

    // `indices` are (global event index, occurrence start) pairs, resolved by
    // the view from the day's own occurrence list.
    let items = indices
        .into_iter()
        .filter_map(|(i, occ_start)| {
            state.events.get().get(i).cloned().map(|e| (i, occ_start, e))
        })
        .collect::<Vec<_>>();

    view! {
        <div class="popover-backdrop" on:click=close>
            <div
                class="popover more-popover"
                node_ref=panel
                on:click=move |e| e.stop_propagation()
                role="dialog"
                aria-modal="true"
            >
                <h3 class="popover-title">
                    {move || crate::i18n::t(state.lang.get(), "more_events")}
                </h3>
                <ul class="more-list agenda-list popover-events">
                    <For
                        each=move || items.clone()
                        key=|(i, occ, e)| (*i, *occ, e.href.clone())
                        children=move |(idx, occ_start, ev)| {
                            let dtstart = ev.event.dtstart.clone();
                            let summary = ev.event.summary.clone();
                            let tooltip = summary.clone();
                            let color = ev.color.clone();
                            view! {
                                <li>
                                    <button
                                        class="popover-event"
                                        style=format!("--chip:{color}")
                                        title=tooltip
                                        on:click=move |e| {
                                            state.week_more.set(None);
                                            state.open_details(
                                                idx,
                                                Some(occ_start),
                                                Some(occ_start),
                                                Some((e.client_x() as i32, e.client_y() as i32)),
                                            );
                                        }
                                    >
                                        <span class="popover-time">
                                            {move || match &dtstart {
                                                Some(caldav::ics::DtValue::Date(_)) => {
                                                    crate::i18n::t(state.lang.get(), "all_day").to_string()
                                                }
                                                Some(caldav::ics::DtValue::DateTime(t)) => {
                                                    t.with_timezone(&tz).format("%H:%M").to_string()
                                                }
                                                None => String::new(),
                                            }}
                                        </span>
                                        <span class="cal-dot" style=format!("background:{color}")></span>
                                        <span class="popover-event-title">{summary}</span>
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
            </div>
        </div>
    }
}
