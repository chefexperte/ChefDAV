//! App shell: sidebar with calendar list + logout, shared toolbar, routed
//! view, global modals, keyboard shortcuts and online/offline handling.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::Outlet;
use leptos_router::hooks::use_navigate;

use crate::api;
use crate::components::calendar_action::CalendarActionModal;
use crate::components::day_popup::DayPopup;
use crate::components::event_details::EventDetails;
use crate::components::event_modal::EventModal;
use crate::components::new_calendar::NewCalendarModal;
use crate::components::prefs::PrefsRow;
use crate::components::slot_popup::SlotPopup;
use crate::components::todo_modal::TodoModal;
use crate::components::toolbar::Toolbar;
use crate::components::week_more::WeekMoreModal;
use crate::state::{AppState, CalendarAction, ModalState};
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
                            st.slot_popup.set(None);
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
        let st = state;
        let nav = navigate.clone();
        spawn_local(async move {
            // Forgetting the password is what makes "Log out" stick across a
            // reload; server/username stay for prefilling the form. Wait for
            // the delete so the login page cannot prefill a stale password.
            let _ = store::kv_delete("creds").await;
            st.session.set(None);
            st.calendars.set(Vec::new());
            st.events.set(Vec::new());
            st.todos.set(Vec::new());
            st.login_error.set(None);
            st.restoring.set(false);
            nav("/login", Default::default());
        });
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
                    {move || format!("+ {}", crate::i18n::t(state.lang.get(), "new_event"))}
                </button>
                <div class="sidebar-heading-row">
                    <h3 class="sidebar-heading">{move || crate::i18n::t(state.lang.get(), "calendars")}</h3>
                    <button
                        class="btn-icon-plain"
                        title=move || crate::i18n::t(state.lang.get(), "new_calendar")
                        on:click=move |_| state.new_calendar.set(true)
                    >
                        "+"
                    </button>
                </div>
                <ul class="cal-list">
                    <For
                        each=move || state.calendars.get()
                        // include the display fingerprint so a rename, color or
                        // privilege change recreates the row; the `visible`
                        // signal itself is preserved by refresh_calendars
                        key=|c| {
                            (
                                c.info.href.clone(),
                                c.info.display_name.clone(),
                                c.color.clone(),
                                c.read_only(),
                                c.info.privileges.join(","),
                            )
                        }
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
                                            <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")>"🔒"</span>
                                        </Show>
                                    </label>
                                    <button
                                        class="cal-menu-btn"
                                        title=move || crate::i18n::t(state.lang.get(), "rename_or_delete")
                                        on:click=move |_| {
                                            let found = state
                                                .calendars
                                                .get()
                                                .iter()
                                                .find(|c| c.info.href == href)
                                                .map(|c| (c.info.display_name.clone(), c.color.clone()));
                                            let (name, color) = found.unwrap_or_default();
                                            state.calendar_action.set(Some(CalendarAction::Rename {
                                                href: href.clone(),
                                                name,
                                                color,
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
                            {move || format!("{} {}", state.pending.get(), crate::i18n::t(state.lang.get(), "changes_pending"))}
                        </span>
                    </Show>
                    <PrefsRow />
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
                Some(ModalState::Details { .. }) => view! { <EventDetails /> }.into_any(),
                _ => view! { <EventModal /> }.into_any(),
            }}
        </Show>
        <Show when=move || state.day_popup.get().is_some() fallback=|| ()>
            <DayPopup />
        </Show>
        <Show when=move || state.slot_popup.get().is_some() fallback=|| ()>
            <SlotPopup />
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
