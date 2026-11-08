use caldav::error::DavError;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

use crate::api;
use crate::browser;
use crate::state::{AppState, Lang, Theme};
use crate::store::{self, StoredCreds};
use crate::views::calendar::CalendarView;
use crate::views::login::LoginPage;
use crate::views::shell::Shell;

#[component]
pub fn App() -> impl IntoView {
    let tz = browser::browser_tz();
    let state = AppState::new(tz);
    provide_context(state);

    // Restore persisted preferences once, then try to restore the saved
    // session (auto-login). The shell stays on a small splash until this
    // settles so the login form never flashes for a returning user.
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

            if settings.stay_logged_in {
                if let Ok(Some(creds)) = store::kv_get::<StoredCreds>("creds").await {
                    match api::login(&creds.server, &creds.username, &creds.password).await {
                        Ok(session) => state.session.set(Some(session)),
                        Err(DavError::Unauthorized) => {
                            // The saved password no longer works: forget it but
                            // keep server/username for the login form.
                            let _ = store::kv_delete("creds").await;
                            state.login_error.set(Some(
                                crate::i18n::t(state.lang.get(), "session_expired").to_string(),
                            ));
                        }
                        Err(_) => {
                            state.login_error.set(Some(
                                crate::i18n::t(state.lang.get(), "server_unreachable").to_string(),
                            ));
                        }
                    }
                }
            }
            state.restoring.set(false);
        });
    });

    // Theme: apply to the document root so the login page is themed too.
    Effect::new(move |_| {
        let theme = state.theme.get();
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            if let Some(root) = doc.document_element() {
                let _ = root.set_attribute("data-theme", theme.as_str());
            }
        }
    });

    view! {
        <Router>
            // The session signal — not a route guard — decides whether the app
            // or the login page is shown. This avoids a navigation race where
            // the Shell mounts before the just-set session is visible.
            <Show
                when=move || !state.restoring.get()
                fallback=move || {
                    view! {
                        <div class="login-page">
                            <div class="login-card">
                                <div class="login-brand">
                                    <img src="/logo.svg" width="64" height="64" alt="ChefDAV logo" />
                                    <h1>"ChefDAV"</h1>
                                </div>
                                <p class="restoring">
                                    {move || crate::i18n::t(state.lang.get(), "restoring_session")}
                                </p>
                            </div>
                        </div>
                    }
                }
            >
                <Show
                    when=move || state.session.get().is_some()
                    fallback=|| view! { <LoginPage /> }
                >
                    <Routes fallback=move || view! { <p class="p-8">{move || crate::i18n::t(state.lang.get(), "nothing_here")}</p> }>
                        <Route path=path!("/login") view=LoginPage />
                        <ParentRoute path=path!("/") view=Shell>
                            <Route path=path!("") view=CalendarView />
                        </ParentRoute>
                    </Routes>
                </Show>
            </Show>
        </Router>
    }
}
