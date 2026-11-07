//! Renders the view selected in the toolbar.

use leptos::prelude::*;

use crate::state::{AppState, ViewMode};
use crate::views::agenda::AgendaView;
use crate::views::month::MonthView;
use crate::views::tasks::TasksView;
use crate::views::week::WeekView;

#[component]
pub fn CalendarView() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    view! {
        <Show when=move || state.view.get() == ViewMode::Month fallback=|| ()>
            <MonthView />
        </Show>
        <Show when=move || state.view.get() == ViewMode::Week fallback=|| ()>
            <WeekView />
        </Show>
        <Show when=move || state.view.get() == ViewMode::Day fallback=|| ()>
            <WeekView n_days=1 />
        </Show>
        <Show when=move || state.view.get() == ViewMode::Agenda fallback=|| ()>
            <AgendaView />
        </Show>
        <Show when=move || state.view.get() == ViewMode::Tasks fallback=|| ()>
            <TasksView />
        </Show>
    }
}
