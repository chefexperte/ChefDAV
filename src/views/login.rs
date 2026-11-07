//! Login page.
//!
//! Credentials are stored in plain IndexedDB for now (the passphrase/AES-GCM
//! flow is kept in `caldav::crypto` for later, but not wired up). LDAP is
//! enforced server-side by Radicale; the SPA only sends Basic auth.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use crate::api;
use crate::state::AppState;
use crate::store::{self, StoredCreds};

#[component]
pub fn LoginPage() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let navigate = use_navigate();

    let server = RwSignal::new("http://127.0.0.1:8080".to_string());
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Prefill the last used server/username/password (plain IndexedDB).
    spawn_local(async move {
        if let Ok(Some(creds)) = store::kv_get::<StoredCreds>("creds").await {
            server.set(creds.server);
            username.set(creds.username);
            password.set(creds.password);
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
        busy.set(true);
        error.set(None);
        spawn_local(async move {
            match api::login(&server_s, &user, &pass).await {
                Ok(session) => {
                    // Store the credentials so the next visit is one click.
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
                    <p class="tagline">"Your CalDAV calendar, served fresh — 100% in your browser."</p>
                </div>
                <form on:submit=submit>
                    <label class="field">
                        <span>"Calendar server"</span>
                        <input type="url" required placeholder="http://127.0.0.1:8080"
                            prop:value=move || server.get()
                            on:input=move |ev| server.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>"Username"</span>
                        <input type="text" required autocomplete="username"
                            prop:value=move || username.get()
                            on:input=move |ev| username.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>"Password"</span>
                        <input type="password" required autocomplete="current-password"
                            prop:value=move || password.get()
                            on:input=move |ev| password.set(event_target_value(&ev)) />
                    </label>
                    <Show when=move || error.get().is_some() fallback=|| ()>
                        <p class="form-error">{move || error.get().unwrap_or_default()}</p>
                    </Show>
                    <button class="btn btn-primary btn-block" type="submit" disabled=move || busy.get()>
                        {move || if busy.get() { "Connecting…" } else { "Log in" }}
                    </button>
                </form>
                <p class="login-hint">
                    "Credentials are saved in this browser's IndexedDB (unencrypted for now).
                    The server only sees Basic auth via your CORS-enabled proxy; who may log in
                    is decided by Radicale (LDAP)."
                </p>
            </div>
        </div>
    }
}
