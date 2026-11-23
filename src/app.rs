use caldav::error::DavError;
use chrono::{Datelike, NaiveDate};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{ParentRoute, Route, Router, Routes};
use leptos_router::path;

use crate::api;
use crate::browser;
use crate::state::{AppState, Lang, ModalState, Theme, ViewMode};
use crate::store::{self, StoredCreds};
use crate::views::calendar::CalendarView;
use crate::views::login::LoginPage;
use crate::views::shell::Shell;

#[component]
pub fn App() -> impl IntoView {
    let tz = browser::browser_tz();
    let state = AppState::new(tz);
    provide_context(state);

    // Browser tab title: "ChefDAV" until the app knows what is on screen, then
    // the visible view/date/resource. A plain en dash replaces the old dot.
    Effect::new(move |_| {
        let lang = state.lang.get();
        let detail = match state.modal.get() {
            Some(ModalState::Details { index, .. }) | Some(ModalState::Edit { index, .. }) => {
                state
                    .events
                    .get()
                    .get(index)
                    .map(|event| event.event.summary.clone())
            }
            Some(ModalState::EditTodo { index }) => state
                .todos
                .get()
                .get(index)
                .map(|todo| todo.todo.summary.clone()),
            Some(ModalState::New { .. }) => Some(crate::i18n::t(lang, "new_event").to_string()),
            Some(ModalState::NewTodo { .. }) => Some(crate::i18n::t(lang, "new_task").to_string()),
            _ => None,
        };
        let title = page_title(
            lang,
            state.session.get().is_some(),
            state.restoring.get(),
            state.view.get(),
            state.anchor.get(),
            detail,
        );
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(&title);
        }
    });

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
                        Ok(session) => {
                            // Remember the discovery results so a later reload
                            // can open the cache even when the backend is down.
                            let _ = store::kv_set(
                                "creds",
                                &StoredCreds {
                                    server: server.clone(),
                                    username: creds.username.clone(),
                                    password: creds.password.clone(),
                                    principal: Some(session.principal.to_string()),
                                    home: Some(session.home.to_string()),
                                },
                            )
                            .await;
                            state.session.set(Some(session));
                        }
                        Err((_, DavError::Unauthorized)) => {
                            // The saved password no longer works: forget it but
                            // keep server/username for the login form.
                            let _ = store::kv_delete("creds").await;
                            state.login_error.set(Some(
                                crate::i18n::t(lang, "session_expired").to_string(),
                            ));
                        }
                        Err((reason, e)) => {
                            // The backend is unreachable, but the password and
                            // the cache are still here: open the cached calendar
                            // read-only instead of trapping the user on a login
                            // page that cannot work until the server is back.
                            if api::backend_unavailable(&e) {
                                if let Some(session) =
                                    api::cached_session(&creds, &server).await
                                {
                                    state.cache_only.set(true);
                                    state.backend_down.set(true);
                                    state.error.set(None);
                                    state.login_error.set(None);
                                    state.session.set(Some(session));
                                    state.restoring.set(false);
                                    return;
                                }
                            }
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
                            <p class="restoring" role="status" aria-live="polite">
                                <span class="spinner spinner-large" aria-hidden="true"></span>
                                <span>{move || crate::i18n::t(state.lang.get(), "restoring_session")}</span>
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
                                <p class="restoring" role="status" aria-live="polite">
                                    <span class="spinner spinner-large" aria-hidden="true"></span>
                                    <span>{move || crate::i18n::t(state.lang.get(), "restoring_session")}</span>
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

/// Build the browser tab title. Nothing known yet → just "ChefDAV"; otherwise
/// the visible view/date/resource after a plain en dash (not the old "·").
fn page_title(
    lang: Lang,
    has_session: bool,
    restoring: bool,
    view: ViewMode,
    anchor: NaiveDate,
    detail: Option<String>,
) -> String {
    if restoring {
        return "ChefDAV".to_string();
    }
    if !has_session {
        return format!("ChefDAV – {}", crate::i18n::t(lang, "login"));
    }
    if let Some(detail) = detail.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()) {
        return format!("ChefDAV – {detail}");
    }
    let view_title = match view {
        ViewMode::Month => crate::i18n::month_title(lang, anchor.month(), anchor.year()),
        ViewMode::Week => {
            crate::i18n::week_label(lang, &caldav::time::week_days(anchor))
        }
        ViewMode::Day => crate::i18n::day_label(lang, anchor),
        ViewMode::Agenda => crate::i18n::agenda_title(lang, anchor),
        ViewMode::Tasks => crate::i18n::t(lang, "tasks").to_string(),
    };
    format!("ChefDAV – {view_title}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).expect("valid test date")
    }

    #[test]
    fn nothing_known_yet_is_just_the_brand() {
        assert_eq!(
            page_title(Lang::En, false, true, ViewMode::Month, day(), None),
            "ChefDAV"
        );
        assert_eq!(
            page_title(Lang::De, true, true, ViewMode::Week, day(), Some("Zahnarzt".into())),
            "ChefDAV"
        );
    }

    #[test]
    fn the_login_page_gets_a_title() {
        assert_eq!(
            page_title(Lang::En, false, false, ViewMode::Month, day(), None),
            "ChefDAV – Log in"
        );
        assert_eq!(
            page_title(Lang::De, false, false, ViewMode::Month, day(), None),
            "ChefDAV – Anmelden"
        );
    }

    #[test]
    fn views_use_the_visible_date() {
        assert_eq!(
            page_title(Lang::En, true, false, ViewMode::Month, day(), None),
            "ChefDAV – November 2026"
        );
        assert_eq!(
            page_title(Lang::De, true, false, ViewMode::Day, day(), None),
            "ChefDAV – Mo 9. November 2026"
        );
        assert!(page_title(Lang::En, true, false, ViewMode::Week, day(), None)
            .starts_with("ChefDAV – 9–15 November 2026"));
        assert_eq!(
            page_title(Lang::De, true, false, ViewMode::Tasks, day(), None),
            "ChefDAV – Aufgaben"
        );
    }

    #[test]
    fn a_visible_resource_wins_over_the_view() {
        assert_eq!(
            page_title(
                Lang::En,
                true,
                false,
                ViewMode::Month,
                day(),
                Some("Dentist".into())
            ),
            "ChefDAV – Dentist"
        );
        assert_eq!(
            page_title(
                Lang::De,
                true,
                false,
                ViewMode::Month,
                day(),
                Some("   ".into())
            ),
            "ChefDAV – November 2026"
        );
    }

    #[test]
    fn the_old_middle_dot_is_gone() {
        let titles = [
            page_title(Lang::En, false, true, ViewMode::Month, day(), None),
            page_title(Lang::En, false, false, ViewMode::Month, day(), None),
            page_title(Lang::En, true, false, ViewMode::Month, day(), None),
            page_title(Lang::En, true, false, ViewMode::Month, day(), Some("Dentist".into())),
        ];
        assert!(titles.iter().all(|title| !title.contains('·')));
    }
}
