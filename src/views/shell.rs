//! App shell: sidebar with calendar list + logout, shared toolbar, routed
//! view, global modals, keyboard shortcuts and online/offline handling.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_navigate;

use crate::api;
use crate::components::calendar_action::CalendarActionModal;
use crate::components::day_popup::DayPopup;
use crate::components::event_modal::EventModal;
use crate::components::new_calendar::NewCalendarModal;
use crate::components::todo_modal::TodoModal;
use crate::components::toolbar::Toolbar;
use crate::components::week_more::WeekMoreModal;
use crate::state::{AppState, CalendarAction, Lang, ModalState, Theme};
use crate::store;

#[component]
pub fn Shell() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let navigate = use_navigate();

    // When a session appears: paint the cache instantly, flush offline writes,
    // then sync with the server.
    Effect::new(move |_| {
        if state.session.get().is_some() {
            let st = state;
            spawn_local(async move {
                api::restore_cache(&st).await;
                api::refresh_calendars(&st).await;
                api::flush_outbox(&st).await;
                api::refresh_events(&st).await;
                api::refresh_todos(&st).await;
            });
        }
    });

    // Reload events whenever the anchor/view or a calendar's visibility changes.
    Effect::new(move |_| {
        let _vis: Vec<bool> = state.calendars.get().iter().map(|c| c.visible.get()).collect();
        let _ = state.anchor.get();
        let _ = state.view.get();
        if state.session.get().is_some() {
            let st = state;
            spawn_local(async move {
                api::refresh_events(&st).await;
                if st.view.get() == crate::state::ViewMode::Tasks {
                    api::refresh_todos(&st).await;
                }
            });
        }
    });

    // Theme: apply to the document root.
    Effect::new(move |_| {
        let theme = state.theme.get();
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Some(root) = doc.document_element() {
                let _ = root.set_attribute("data-theme", theme.as_str());
            }
        }
    });

    // Online/offline: flush the outbox when connectivity returns.
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        let st = state;
        if let Some(win) = web_sys::window() {
            let online = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                st.online.set(true);
                spawn_local(async move { api::flush_outbox(&st).await });
            });
            let offline = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                st.online.set(false);
            });
            let _ = win.add_event_listener_with_callback("online", online.as_ref().unchecked_ref());
            let _ = win.add_event_listener_with_callback("offline", offline.as_ref().unchecked_ref());
            online.forget();
            offline.forget();
        }
    }

    // Keyboard shortcuts.
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;
        let st = state;
        if let Some(win) = web_sys::window() {
            let handler = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |ev: web_sys::KeyboardEvent| {
                    let target = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok());
                    let typing = target
                        .map(|t| {
                            let tag = t.tag_name().to_lowercase();
                            tag == "input" || tag == "textarea" || tag == "select"
                        })
                        .unwrap_or(false);
                    if typing {
                        return;
                    }
                    match ev.key().as_str() {
                        "n" => {
                            ev.prevent_default();
                            let date = caldav::time::today(&st.tz, crate::browser::now_utc());
                            st.new_event_time.set(None);
                            st.new_event_all_day.set(false);
                            st.modal.set(Some(ModalState::New { date }));
                        }
                        "t" => {
                            ev.prevent_default();
                            st.anchor
                                .set(caldav::time::today(&st.tz, crate::browser::now_utc()));
                        }
                        "m" => st.view.set(crate::state::ViewMode::Month),
                        "w" => st.view.set(crate::state::ViewMode::Week),
                        "d" => st.view.set(crate::state::ViewMode::Day),
                        "a" => st.view.set(crate::state::ViewMode::Agenda),
                        "k" => st.view.set(crate::state::ViewMode::Tasks),
                        "/" => {
                            ev.prevent_default();
                            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                if let Some(el) = doc.query_selector(".search-box input").ok().flatten() {
                                    if let Some(el) = el.dyn_ref::<web_sys::HtmlElement>() {
                                        let _ = el.focus();
                                    }
                                }
                            }
                        }
                        "ArrowLeft" => {
                            ev.prevent_default();
                            let d = st.anchor.get();
                            st.anchor.set(d - chrono::Duration::days(1));
                        }
                        "ArrowRight" => {
                            ev.prevent_default();
                            let d = st.anchor.get();
                            st.anchor.set(d + chrono::Duration::days(1));
                        }
                        "Escape" => {
                            st.modal.set(None);
                            st.day_popup.set(None);
                            st.calendar_action.set(None);
                            st.new_calendar.set(false);
                            st.week_more.set(None);
                            st.search_open.set(false);
                        }
                        _ => {}
                    }
                },
            );
            let _ = win.add_event_listener_with_callback("keydown", handler.as_ref().unchecked_ref());
            handler.forget();
        }
    }

    let logout = move |_| {
        state.session.set(None);
        state.calendars.set(Vec::new());
        state.events.set(Vec::new());
        state.todos.set(Vec::new());
        navigate("/login", Default::default());
    };

    let toggle_theme = move |_| {
        let next = match state.theme.get() {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        state.theme.set(next);
        let st = state;
        spawn_local(async move {
            let mut s = store::settings_load().await;
            s.theme = st.theme.get().as_str().to_string();
            store::settings_save(&s).await;
        });
    };
    let toggle_lang = move |_| {
        let next = match state.lang.get() {
            Lang::En => Lang::De,
            Lang::De => Lang::En,
        };
        state.lang.set(next);
        let st = state;
        spawn_local(async move {
            let mut s = store::settings_load().await;
            s.lang = st.lang.get().code().to_string();
            store::settings_save(&s).await;
        });
    };
    let toggle_reminders = move |_| {
        let st = state;
        if state.reminders.get() {
            state.reminders.set(false);
            spawn_local(async move {
                let mut s = store::settings_load().await;
                s.reminders = false;
                store::settings_save(&s).await;
            });
        } else {
            spawn_local(async move {
                if crate::reminders::request_permission().await {
                    st.reminders.set(true);
                    let mut s = store::settings_load().await;
                    s.reminders = true;
                    store::settings_save(&s).await;
                } else {
                    st.error
                        .set(Some("Notifications are blocked by the browser.".into()));
                }
            });
        }
    };

    view! {
        <div class="app-shell">
            <aside class="sidebar">
                <div class="brand">
                    <img src="/logo.svg" width="34" height="34" alt="ChefDAV logo" />
                    <span class="brand-name">"ChefDAV"</span>
                </div>
                <button
                    class="btn btn-primary btn-block new-event"
                    on:click=move |_| {
                        let date = caldav::time::today(&state.tz, crate::browser::now_utc());
                        state.new_event_time.set(None);
                        state.new_event_all_day.set(false);
                        state.modal.set(Some(ModalState::New { date }));
                    }
                >
                    {move || crate::i18n::t(state.lang.get(), "new_event")}
                </button>
                <div class="sidebar-heading-row">
                    <h3 class="sidebar-heading">{move || crate::i18n::t(state.lang.get(), "calendars")}</h3>
                    <button
                        class="btn-icon-plain"
                        title="New calendar"
                        on:click=move |_| state.new_calendar.set(true)
                    >
                        "+"
                    </button>
                </div>
                <ul class="cal-list">
                    <For
                        each=move || state.calendars.get()
                        key=|c| c.info.href.clone()
                        children=move |c| {
                            let name = c.info.display_name.clone();
                            let color = c.color.clone();
                            let href = c.info.href.clone();
                            let read_only = c.read_only();
                            view! {
                                <li class="cal-item">
                                    <label>
                                        <input
                                            type="checkbox"
                                            prop:checked=move || c.visible.get()
                                            on:change=move |ev| c.visible.set(event_target_checked(&ev))
                                        />
                                        <span class="cal-dot" style=format!("background: {color}")></span>
                                        <span class="cal-name">{name}</span>
                                        <Show when=move || read_only fallback=|| ()>
                                            <span class="lock" title="Read-only">"🔒"</span>
                                        </Show>
                                    </label>
                                    <button
                                        class="cal-menu-btn"
                                        title="Rename or delete calendar"
                                        on:click=move |_| {
                                            let name = state
                                                .calendars
                                                .get()
                                                .iter()
                                                .find(|c| c.info.href == href)
                                                .map(|c| c.info.display_name.clone())
                                                .unwrap_or_default();
                                            state.calendar_action.set(Some(CalendarAction::Rename {
                                                href: href.clone(),
                                                name,
                                            }));
                                        }
                                    >
                                        "⋯"
                                    </button>
                                </li>
                            }
                        }
                    />
                </ul>
                <div class="sidebar-footer">
                    <Show
                        when=move || state.session.get().is_some()
                        fallback=|| view! { <span></span> }
                    >
                        <span class="session-info">
                            {move || {
                                let s = state.session.get().unwrap();
                                format!("{} · {}", s.username, s.server.host_str().unwrap_or(""))
                            }}
                        </span>
                    </Show>
                    <Show when=move || (state.pending.get() > 0) fallback=|| ()>
                        <span class="pending-badge">
                            {move || format!("{} change(s) pending sync", state.pending.get())}
                        </span>
                    </Show>
                    <div class="prefs-row">
                        <button class="btn btn-ghost" on:click=toggle_theme>
                            {move || match state.theme.get() {
                                Theme::Light => "Dark",
                                Theme::Dark => "Light",
                            }}
                        </button>
                        <button class="btn btn-ghost" on:click=toggle_lang>
                            {move || state.lang.get().code().to_uppercase()}
                        </button>
                        <button
                            class="btn btn-ghost"
                            class:active-pref=move || state.reminders.get()
                            title="Toggle browser reminders"
                            on:click=toggle_reminders
                        >
                            {move || if state.reminders.get() { "🔔" } else { "🔕" }}
                        </button>
                    </div>
                    <button class="btn btn-ghost btn-block" on:click=logout>
                        {move || crate::i18n::t(state.lang.get(), "log_out")}
                    </button>
                </div>
            </aside>
            <main class="main">
                <Toolbar />
                <Outlet />
            </main>
        </div>
        <Show when=move || state.modal.get().is_some() fallback=|| ()>
            {move || match state.modal.get() {
                Some(ModalState::NewTodo { .. } | ModalState::EditTodo { .. }) => view! { <TodoModal /> }.into_any(),
                _ => view! { <EventModal /> }.into_any(),
            }}
        </Show>
        <Show when=move || state.day_popup.get().is_some() fallback=|| ()>
            <DayPopup />
        </Show>
        <Show when=move || state.new_calendar.get() fallback=|| ()>
            <NewCalendarModal />
        </Show>
        <Show when=move || state.calendar_action.get().is_some() fallback=|| ()>
            <CalendarActionModal />
        </Show>
        <Show when=move || state.week_more.get().is_some() fallback=|| ()>
            <WeekMoreModal />
        </Show>
    }
}
