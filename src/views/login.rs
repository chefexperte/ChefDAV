//! Login page.
//!
//! Credentials are stored in plain IndexedDB for now (the passphrase/AES-GCM
//! flow is kept in `caldav::crypto` for later, but not wired up). LDAP is
//! enforced server-side by Radicale; the SPA only sends Basic auth.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use crate::api;
use crate::components::prefs::PrefsRow;
use crate::state::AppState;
use crate::store::{self, LastLogin, StoredCreds};

const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

/// Latest changelog section: (title, bullet items).
fn latest_changelog() -> (String, Vec<String>) {
    let mut title = String::new();
    let mut items = Vec::new();
    let mut in_section = false;
    for line in CHANGELOG.lines() {
        let line = line.trim_end();
        if line.starts_with("## ") {
            if in_section {
                break;
            }
            in_section = true;
            title = line.trim_start_matches("## ").to_string();
            continue;
        }
        if in_section {
            if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
                items.push(item.to_string());
            }
        }
    }
    (title, items)
}

#[component]
pub fn LoginPage() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let navigate = use_navigate();

    let server = RwSignal::new(crate::config::DEFAULT_SERVER.to_string());
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let stay = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let (changelog_title, changelog_items) = latest_changelog();

    // Prefill the last used server/username (and the password when it was
    // saved). The checkbox mirrors the persisted "stay logged in" choice.
    spawn_local(async move {
        let settings = store::settings_load().await;
        stay.set(settings.stay_logged_in);
        if let Ok(Some(last)) = store::kv_get::<LastLogin>("last_login").await {
            server.set(last.server);
            username.set(last.username);
        }
        if settings.stay_logged_in {
            if let Ok(Some(creds)) = store::kv_get::<StoredCreds>("creds").await {
                server.set(creds.server);
                username.set(creds.username);
                password.set(creds.password);
            }
        }
    });

    // If the login route is visited while already logged in, go to the app.
    Effect::new(move |_| {
        if state.session.get().is_some() {
            navigate("/", Default::default());
        }
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if busy.get() {
            return;
        }
        let server_s = server.get();
        let user = username.get();
        let pass = password.get();
        let remember = stay.get();
        busy.set(true);
        error.set(None);
        state.login_error.set(None);
        spawn_local(async move {
            match api::login(&server_s, &user, &pass).await {
                Ok(session) => {
                    // Keep the non-secret part for prefilling after logout.
                    let _ = store::kv_set(
                        "last_login",
                        &LastLogin {
                            server: server_s.clone(),
                            username: user.clone(),
                        },
                    )
                    .await;
                    let mut settings = store::settings_load().await;
                    settings.stay_logged_in = remember;
                    store::settings_save(&settings).await;
                    if remember {
                        // NOTE: plain IndexedDB — not encrypted (temporary).
                        let _ = store::kv_set(
                            "creds",
                            &StoredCreds {
                                server: server_s.clone(),
                                username: user.clone(),
                                password: pass.clone(),
                            },
                        )
                        .await;
                    } else {
                        let _ = store::kv_delete("creds").await;
                    }
                    state.session.set(Some(session));
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <div class="login-page">
            <div class="login-card">
                <div class="login-brand">
                    <img src="/logo.svg" width="64" height="64" alt="ChefDAV logo" />
                    <h1>"ChefDAV"</h1>
                    <p class="tagline">{move || crate::i18n::t(state.lang.get(), "tagline")}</p>
                </div>
                <form on:submit=submit>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "server")}</span>
                        <input type="url" required placeholder="http://127.0.0.1:8080"
                            prop:value=move || server.get()
                            on:input=move |ev| server.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "username")}</span>
                        <input type="text" required autocomplete="username"
                            prop:value=move || username.get()
                            on:input=move |ev| username.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "password")}</span>
                        <input type="password" required autocomplete="current-password"
                            prop:value=move || password.get()
                            on:input=move |ev| password.set(event_target_value(&ev)) />
                    </label>
                    <label class="check-row">
                        <input type="checkbox"
                            prop:checked=move || stay.get()
                            on:change=move |ev| stay.set(event_target_checked(&ev)) />
                        <span>{move || crate::i18n::t(state.lang.get(), "stay_logged_in")}</span>
                    </label>
                    <Show
                        when=move || error.get().is_some() || state.login_error.get().is_some()
                        fallback=|| ()
                    >
                        <p class="form-error">
                            {move || error.get().or_else(|| state.login_error.get()).unwrap_or_default()}
                        </p>
                    </Show>
                    <button class="btn btn-primary btn-block" type="submit" disabled=move || busy.get()>
                        {move || if busy.get() { crate::i18n::t(state.lang.get(), "connecting") } else { crate::i18n::t(state.lang.get(), "login") }}
                    </button>
                </form>
                <p class="login-hint">
                    {move || crate::i18n::t(state.lang.get(), "login_hint")}
                </p>
                <details class="changelog">
                    <summary>
                        {move || crate::i18n::t(state.lang.get(), "whats_new")} 
                        <span class="changelog-version">{changelog_title}</span>
                    </summary>
                    <ul>
                        <For
                            each=move || changelog_items.clone()
                            key=|item| item.clone()
                            children=|item| view! { <li>{item}</li> }
                        />
                    </ul>
                </details>
                <div class="login-prefs">
                    <PrefsRow />
                </div>
            </div>
        </div>
    }
}
