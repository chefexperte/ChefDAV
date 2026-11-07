use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

use crate::browser;
use crate::state::{AppState, Lang, Theme};
use crate::store;
use crate::views::calendar::CalendarView;
use crate::views::login::LoginPage;
use crate::views::shell::Shell;

#[component]
pub fn App() -> impl IntoView {
    let tz = browser::browser_tz();
    let state = AppState::new(tz);
    provide_context(state);

    // Restore persisted preferences once.
    Effect::new(move |_| {
        spawn_local(async move {
            let settings = store::settings_load().await;
            state.theme.set(if settings.theme == "dark" {
                Theme::Dark
            } else {
                Theme::Light
            });
            state.lang.set(if settings.lang == "de" {
                Lang::De
            } else {
                Lang::En
            });
            state.reminders.set(settings.reminders);
        });
    });

    // Reminder loop (no-op until enabled).
    crate::reminders::start(state);

    view! {
        <Router>
            // The session signal — not a route guard — decides whether the app
            // or the login page is shown. This avoids a navigation race where
            // the Shell mounts before the just-set session is visible.
            <Show
                when=move || state.session.get().is_some()
                fallback=|| view! { <LoginPage /> }
            >
                <Routes fallback=|| view! { <p class="p-8">"Nothing here."</p> }>
                    <Route path=path!("/login") view=LoginPage />
                    <ParentRoute path=path!("/") view=Shell>
                        <Route path=path!("") view=CalendarView />
                    </ParentRoute>
                </Routes>
            </Show>
        </Router>
    }
}
