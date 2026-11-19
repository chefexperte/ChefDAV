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

    // The mount path is only known after `/config.json` was loaded, and the
    // router needs it at construction time. So the whole app (router included)
    // is created once this is set — before that, only the splash renders.
    let router_base = RwSignal::new(None::<String>);

    // Load runtime configuration, then restore preferences and try the saved
    // session (auto-login). The splash stays up until all of this settles, so
    // the login form never flashes — and never briefly shows a server field
    // that a locked deployment (`/config.json`) would make read-only.
    Effect::new(move |_| {
        spawn_local(async move {
            let mut runtime =
                crate::config::parse_runtime(&crate::api::fetch_runtime_config().await);
            if runtime.base_path.is_empty() {
                // Not configured: infer it from the address bar, so an app
                // mounted under /ui works even if the operator set nothing.
                if let Some(path) = web_sys::window()
                    .and_then(|w| w.location().pathname().ok())
                {
                    runtime.base_path = crate::config::detect_base_path(&path);
                }
            }
            crate::config::set_runtime(runtime);
            router_base.set(Some(crate::config::router_base()));

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
                    let lang = state.lang.get_untracked();
                    // A deployment that sets BACKEND_URL is authoritative: the
                    // saved session is restored against it, never against a
                    // server the user once typed.
                    let server = if crate::config::server_is_fixed() {
                        crate::config::initial_server()
                    } else {
                        creds.server.clone()
                    };
                    match api::login_classified(&server, &creds.username, &creds.password, lang).await {
                        Ok(session) => state.session.set(Some(session)),
                        Err((_, DavError::Unauthorized)) => {
                            // The saved password no longer works: forget it but
                            // keep server/username for the login form.
                            let _ = store::kv_delete("creds").await;
                            state.login_error.set(Some(
                                crate::i18n::t(lang, "session_expired").to_string(),
                            ));
                        }
                        Err((reason, _)) => {
                            state.login_error.set(Some(api::error_text(reason, lang).to_string()));
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
        <Show
            when=move || router_base.get().is_some()
            fallback=move || {
                let logo = crate::config::splash_asset_url("logo.svg");
                view! {
                    <div class="login-page">
                        <div class="login-card">
                            <div class="login-brand">
                                <img src=logo width="64" height="64" alt="ChefDAV logo" />
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
        // `base` is the mount path ("/ui/" or ""): the router strips it before
        // matching, so /ui/month/2026-11 routes like /month/2026-11.
        <Router base=router_base.get_untracked().unwrap_or_default()>
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
                                    <img src=move || crate::config::asset_url("logo.svg") width="64" height="64" alt="ChefDAV logo" />
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
                            // View + date in the URL (shareable): /month/2026-11, …
                            <Route path=path!("month") view=CalendarView />
                            <Route path=path!("month/:date") view=CalendarView />
                            <Route path=path!("week") view=CalendarView />
                            <Route path=path!("week/:date") view=CalendarView />
                            <Route path=path!("day") view=CalendarView />
                            <Route path=path!("day/:date") view=CalendarView />
                            <Route path=path!("list") view=CalendarView />
                            <Route path=path!("list/:date") view=CalendarView />
                            <Route path=path!("tasks") view=CalendarView />
                            // Resource deep links render the calendar view
                            // underneath; the Shell opens the resource modal
                            // from the URL.
                            <Route path=path!("event/:cal/:uid") view=CalendarView />
                            <Route path=path!("task/:cal/:uid") view=CalendarView />
                        </ParentRoute>
                    </Routes>
                </Show>
            </Show>
        </Router>
        </Show>
    }
}
