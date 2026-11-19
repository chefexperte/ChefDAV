//! App shell: sidebar with calendar list + logout, shared toolbar, routed
//! view, global modals, keyboard shortcuts and online/offline handling.

use std::rc::Rc;

use chrono::Datelike;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::Outlet;
use leptos_router::hooks::{use_location, use_navigate};

use crate::api;
use crate::components::calendar_action::CalendarActionModal;
use crate::components::day_popup::DayPopup;
use crate::components::event_details::EventDetails;
use crate::components::event_modal::EventModal;
use crate::components::jump::{MonthMenu, YearMenu};
use crate::components::new_calendar::NewCalendarModal;
use crate::components::prefs::PrefsRow;
use crate::components::shortcuts::{ShortcutsButton, ShortcutsDialog};
use crate::components::slot_popup::SlotPopup;
use crate::components::todo_modal::TodoModal;
use crate::components::toolbar::Toolbar;
use crate::components::week_more::WeekMoreModal;
use crate::links;
use crate::state::{AppState, CalendarAction, ModalState, ViewMode};
use crate::store;

/// Strip the mount path from a browser pathname, so it compares against the
/// router's routes: `/ui/month/2026-11` → `/month/2026-11`, `/ui/` → `/`.
/// At the root (`base_path == ""`) it only guarantees the leading slash.
fn route_path(pathname: &str) -> String {
    let base = crate::config::runtime().base_path;
    if !base.is_empty() {
        if pathname == base {
            return "/".to_string();
        }
        if let Some(rest) = pathname.strip_prefix(&format!("{base}/")) {
            return format!("/{}", rest.trim_start_matches('/'));
        }
    }
    if pathname.starts_with('/') {
        pathname.to_string()
    } else {
        format!("/{pathname}")
    }
}

#[component]
pub fn Shell() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let navigate = use_navigate();
    let go_home_nav = navigate.clone();

    // Give AppState access to the router so any view can push a resource URL
    // into the address bar (see `AppState::open_details` / `open_todo`).
    {
        let nav = navigate.clone();
        state
            .navigator
            .set(Some(Rc::new(move |path, opts| nav(path, opts))));
    }

    // Deep links: keep the address bar and the resource modal in sync.
    // - a resource URL that is not open yet (pasted link, fresh load,
    //   back/forward) is resolved and opened;
    // - leaving the resource URL (back button) closes the modal again.
    {
        let location = use_location();
        Effect::new(move |_| {
            let path = route_path(&location.pathname.get());
            let search = location.search.get();
            let link_active = state.link_active.get();
            // Track data loads so resolution re-runs once events/todos arrive.
            let loading = state.loading.get();
            let _ = state.events.get();
            let _ = state.todos.get();
            let _ = state.calendars.get();

            if let Some((kind, cal_p, id)) = links::parse_path(&path) {
                if link_active {
                    // Already synced; closing the modal moves the URL (below).
                    return;
                }
                match kind {
                    links::ResourceKind::Event => {
                        let occ = links::occ_from_search(&search, &state.tz);
                        if let Some((index, occurrence, display_start)) =
                            links::resolve_event(&state, &cal_p, &id, occ)
                        {
                            state.modal.set(Some(ModalState::Details {
                                index,
                                occurrence,
                                display_start,
                                anchor: None,
                            }));
                            state.link_active.set(true);
                        } else if state.session.get().is_some()
                            && !state.calendars.get().is_empty()
                            && !loading
                            && state.link_fetching.get() != path
                        {
                            // Not in the loaded set (hidden calendar or not yet
                            // synced): fetch the resource directly, re-resolve.
                            state.link_fetching.set(path.clone());
                            let st = state;
                            let cal_p = cal_p.clone();
                            let id = id.clone();
                            spawn_local(async move {
                                if let Some(cal) = links::find_calendar(&st, &cal_p) {
                                    if let Some(ev) =
                                        api::fetch_event_by_uid(&st, &cal.info, &id).await
                                    {
                                        st.events.update(|v| {
                                            if !v.iter().any(|e| e.href == ev.href) {
                                                v.push(ev);
                                            }
                                        });
                                    }
                                }
                                let occ = links::occ_from_search(&search, &st.tz);
                                if let Some((index, occurrence, display_start)) =
                                    links::resolve_event(&st, &cal_p, &id, occ)
                                {
                                    st.modal.set(Some(ModalState::Details {
                                        index,
                                        occurrence,
                                        display_start,
                                        anchor: None,
                                    }));
                                    st.link_active.set(true);
                                } else {
                                    st.error.set(Some(
                                        crate::i18n::t(st.lang.get(), "event_not_found")
                                            .to_string(),
                                    ));
                                    let base = st.base_url.get();
                                    st.navigate_to(if base.is_empty() { "/" } else { &base }, true);
                                }
                            });
                        }
                    }
                    links::ResourceKind::Task => {
                        if let Some(index) = links::resolve_todo(&state, &cal_p, &id) {
                            state.modal.set(Some(ModalState::EditTodo { index }));
                            state.link_active.set(true);
                        } else if state.session.get().is_some()
                            && !state.calendars.get().is_empty()
                            && !loading
                            && state.link_fetching.get() != path
                        {
                            state.link_fetching.set(path.clone());
                            let st = state;
                            let cal_p = cal_p.clone();
                            let id = id.clone();
                            spawn_local(async move {
                                if let Some(cal) = links::find_calendar(&st, &cal_p) {
                                    if let Some(todo) =
                                        api::fetch_todo_by_uid(&st, &cal.info, &id).await
                                    {
                                        st.todos.update(|v| {
                                            if !v.iter().any(|t| t.href == todo.href) {
                                                v.push(todo);
                                            }
                                        });
                                    }
                                }
                                if let Some(index) = links::resolve_todo(&st, &cal_p, &id) {
                                    st.modal.set(Some(ModalState::EditTodo { index }));
                                    st.link_active.set(true);
                                } else {
                                    st.error.set(Some(
                                        crate::i18n::t(st.lang.get(), "task_not_found")
                                            .to_string(),
                                    ));
                                    let base = st.base_url.get();
                                    st.navigate_to(if base.is_empty() { "/" } else { &base }, true);
                                }
                            });
                        }
                    }
                }
            } else {
                state.leave_resource_url();
            }
        });

        // A linked modal was closed from the UI while its URL is still in the
        // address bar: return the address bar to the calendar view.
        Effect::new(move |_| {
            let _ = state.modal.get();
            let path = route_path(&location.pathname.get());
            if links::parse_path(&path).is_some() {
                state.close_linked_modal_url();
            }
        });

        // View + date in the URL (/month/2026-11, /week/2026-11-09, …):
        // URL -> state, so a shared link restores the view. Only react to actual
        // path changes — otherwise this would stomp the user's own view clicks.
            // The router matches routes relative to the mount path ("/ui"), but
        // `location.pathname` is the full browser path; strip the prefix so
        // "/ui/month/2026-11" and "/month/2026-11" compare equal.
        let last_view_path = RwSignal::new(String::new());
        Effect::new(move |_| {
            let path = route_path(&location.pathname.get());
            if path == last_view_path.get() {
                return;
            }
            last_view_path.set(path.clone());
            if links::parse_path(&path).is_some() {
                return; // resource link — handled above
            }
            if let Some((view, date)) = links::parse_view_path(&path) {
                if state.view.get() != view {
                    state.view.set(view);
                }
                let anchor =
                    date.unwrap_or_else(|| caldav::time::today(&state.tz, crate::browser::now_utc()));
                let cur = state.anchor.get();
                // for the month view any day in the month is the same view
                let same = if view == ViewMode::Month {
                    cur.year() == anchor.year() && cur.month() == anchor.month()
                } else {
                    cur == anchor
                };
                if !same {
                    state.anchor.set(anchor);
                }
            } else if path == "/" {
                let t = caldav::time::today(&state.tz, crate::browser::now_utc());
                if state.view.get() != ViewMode::Month {
                    state.view.set(ViewMode::Month);
                }
                if state.anchor.get() != t {
                    state.anchor.set(t);
                }
            }
        });

        // state -> URL (replace, so arrow keys don't spam the history).
        Effect::new(move |_| {
            let view = state.view.get();
            let anchor = state.anchor.get();
            let url = links::view_path(view, anchor);
            state.base_url.set(url.clone());
            let path = route_path(&location.pathname.get());
            if links::parse_path(&path).is_some() {
                return; // a resource link is shown; don't override it
            }
            if path != url {
                state.navigate_to(&url, true);
            }
        });
    }

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
                            st.new_event_end_time.set(None);
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
                            st.jump_open.set(None);
                            st.shortcuts_open.set(false);
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
                // The brand is a link home: month view, current month, nothing
                // selected. A real <a> keeps middle-click / open-in-new-tab.
                <a
                    class="brand"
                    href="/"
                    title=move || crate::i18n::t(state.lang.get(), "go_home")
                    aria-label=move || crate::i18n::t(state.lang.get(), "go_home")
                    on:click=move |ev| {
                        // Keep it an in-app navigation for plain left clicks
                        // (modified clicks still open a new tab / window).
                        if ev.ctrl_key() || ev.meta_key() || ev.shift_key() || ev.button() != 0 {
                            return;
                        }
                        ev.prevent_default();
                        state.modal.set(None);
                        state.day_popup.set(None);
                        state.slot_popup.set(None);
                        state.week_more.set(None);
                        state.selected.set(Vec::new());
                        state.search_query.set(String::new());
                        state.search_open.set(false);
                        state.jump_open.set(None);
                        state.link_active.set(false);
                        // Set the target state as well as the URL: navigating to
                        // "/" alone only rewrites the path, and the router's
                        // location effect does not re-apply "month view, today"
                        // when the path it already saw was a resource link.
                        let today = caldav::time::today(&state.tz, crate::browser::now_utc());
                        state.view.set(crate::state::ViewMode::Month);
                        state.anchor.set(today);
                        // "/" resolves against the router base ("/ui"), while
                        // "" would resolve relative to the current path.
                        go_home_nav("/", Default::default());
                    }
                >
                    <img src=move || crate::config::asset_url("logo.svg") width="34" height="34" alt="ChefDAV logo" />
                    <span class="brand-name">"ChefDAV"</span>
                </a>
                <button
                    class="btn btn-primary btn-block new-event"
                    on:click=move |_| {
                        let date = caldav::time::today(&state.tz, crate::browser::now_utc());
                        state.new_event_time.set(None);
                        state.new_event_end_time.set(None);
                        state.new_event_all_day.set(false);
                        state.modal.set(Some(ModalState::New { date }));
                    }
                >
                    <svg class="btn-add-icon" viewBox="0 0 16 16" aria-hidden="true">
                        <path d="M8 3v10M3 8h10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
                    </svg>
                    {move || crate::i18n::t(state.lang.get(), "new_event")}
                </button>
                <div class="sidebar-heading-row">
                    <span class="sidebar-heading-label">
                        <h3 class="sidebar-heading">{move || crate::i18n::t(state.lang.get(), "calendars")}</h3>
                    </span>
                    <button
                        class="btn-icon-plain"
                        title=move || crate::i18n::t(state.lang.get(), "new_calendar")
                        on:click=move |_| state.new_calendar.set(true)
                    >
                        <crate::components::icons::IconPlus />
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
                                            <span class="lock" title=move || crate::i18n::t(state.lang.get(), "lock")><svg viewBox="0 0 16 16" aria-hidden="true"><path d="M5 7V5a3 3 0 0 1 6 0v2" fill="none" stroke="currentColor" stroke-width="1.4" /><rect x="3.4" y="7" width="9.2" height="6.6" rx="1.4" fill="currentColor" /></svg></span>
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
                                        // flat "open menu" icon (three dots)
                                        <svg viewBox="0 0 16 16" aria-hidden="true">
                                            <circle cx="3" cy="8" r="1.35" fill="currentColor" />
                                            <circle cx="8" cy="8" r="1.35" fill="currentColor" />
                                            <circle cx="13" cy="8" r="1.35" fill="currentColor" />
                                        </svg>
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
                    <ShortcutsButton />
                    <PrefsRow />
                    <button class="btn btn-danger btn-block" on:click=logout>
                        {move || crate::i18n::t(state.lang.get(), "log_out")}
                    </button>
                </div>
            </aside>
            <main class="main">
                <Toolbar />
                <Outlet />
            </main>
        </div>
        <Show when=move || state.shortcuts_open.get() fallback=|| ()>
            <ShortcutsDialog />
        </Show>
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
        <Show when=move || state.jump_open.get().map(|(k, _)| k) == Some("year") fallback=|| ()>
            <YearMenu />
        </Show>
        <Show when=move || state.jump_open.get().map(|(k, _)| k) == Some("month") fallback=|| ()>
            <MonthMenu />
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::route_path;
    use crate::config::{set_runtime, Runtime};

    fn with_base(base: &str) {
        set_runtime(Runtime {
            backend_url: None,
            server_fixed: false,
            base_path: base.to_string(),
        });
    }

    #[test]
    fn at_the_root_the_path_is_unchanged() {
        with_base("");
        assert_eq!(route_path("/month/2026-11"), "/month/2026-11");
        assert_eq!(route_path("/"), "/");
        assert_eq!(route_path("/event/cal/uid"), "/event/cal/uid");
    }

    #[test]
    fn a_mount_path_is_stripped() {
        with_base("/ui");
        assert_eq!(route_path("/ui/month/2026-11"), "/month/2026-11");
        assert_eq!(route_path("/ui/week/2026-11-16"), "/week/2026-11-16");
        assert_eq!(route_path("/ui/event/cal/uid"), "/event/cal/uid");
        // the mount point itself is the app root, with or without a slash
        assert_eq!(route_path("/ui"), "/");
        assert_eq!(route_path("/ui/"), "/");
    }

    #[test]
    fn a_deeper_mount_path_works_too() {
        with_base("/calendar/ui");
        assert_eq!(route_path("/calendar/ui/month/2026-11"), "/month/2026-11");
    }
}
