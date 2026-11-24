//! Renders the view selected in the toolbar.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::state::{AppState, ViewMode};
use crate::views::agenda::AgendaView;
use crate::views::month::MonthView;
use crate::views::tasks::TasksView;
use crate::views::week::WeekView;

/// Each view starts at the top of the content scroller. Week/day position
/// themselves at 06:00 slightly later, once their grid has rendered.
fn reset_content_scroll() {
    let Some(doc) = web_sys::window().and_then(|win| win.document()) else {
        return;
    };
    let Some(el) = doc.query_selector(".main-scroll").ok().flatten() else {
        return;
    };
    if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
        el.set_scroll_top(0);
        el.set_scroll_left(0);
    }
}

#[component]
pub fn CalendarView() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let last_view = RwSignal::new(None::<ViewMode>);
    Effect::new(move |_| {
        let view = state.view.get();
        if last_view.get_untracked() != Some(view) {
            last_view.set(Some(view));
            reset_content_scroll();
        }
    });
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
