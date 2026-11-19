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
    parse_latest_changelog(CHANGELOG)
}

fn parse_latest_changelog(text: &str) -> (String, Vec<String>) {
    let mut title = String::new();
    let mut items = Vec::new();
    let mut current: Option<String> = None;
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with("## ") {
            if in_section {
                break;
            }
            in_section = true;
            title = line.trim_start_matches("## ").to_string();
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            if let Some(prev) = current.take() {
                items.push(prev);
            }
            current = Some(item.trim().to_string());
        } else if line.starts_with('#') {
            // subheadings (### …) are not list items; close the current one
            if let Some(prev) = current.take() {
                items.push(prev);
            }
        } else if line.trim().is_empty() {
            if let Some(prev) = current.take() {
                items.push(prev);
            }
        } else if let Some(cur) = current.as_mut() {
            // wrapped continuation of the current bullet
            cur.push(' ');
            cur.push_str(line.trim());
        }
    }
    if let Some(prev) = current {
        items.push(prev);
    }
    (title, items)
}

#[cfg(test)]
mod tests {
    use super::parse_latest_changelog;

    #[test]
    fn latest_section_joins_wrapped_bullets_and_skips_subheadings() {
        let text = "# Changelog\n\n## Unreleased\n\n### Redesign\n\n- First bullet\n  continues here.\n\n- Second *bullet*\n\n## 0.1.0 — yesterday\n\n- Old item\n";
        let (title, items) = parse_latest_changelog(text);
        assert_eq!(title, "Unreleased");
        assert_eq!(
            items,
            vec!["First bullet continues here.", "Second *bullet*"]
        );
    }
}

#[component]
pub fn LoginPage() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let navigate = use_navigate();

    // A deployment with a locked backend (BACKEND_URL / config.json) never
    // takes a server from the user or from previously stored logins.
    //
    // This page can render before `/config.json` has been fetched (App fetches
    // it in an effect, and the login page is also reachable directly), so the
    // configuration is re-read whenever it arrives: `config_ready` flips once,
    // and the effect below applies it.
    let config_ready = RwSignal::new(false);
    let locked = RwSignal::new(crate::config::is_locked());
    let server = RwSignal::new(crate::config::initial_server());
    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let stay = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    // Extra context for a failed connection (e.g. "this looks like CORS").
    let hint = RwSignal::new(None::<String>);
    let (changelog_title, changelog_items) = latest_changelog();

    // Prefill the last used server/username (and the password when it was
    // saved). The checkbox mirrors the persisted "stay logged in" choice.
    // With a locked backend the server stays as configured — only the
    // username/password are restored.
    // The last server this browser logged in to (None on a first visit). Read
    // together with the rest of the preferences.
    let last_server = RwSignal::new(None::<String>);

    spawn_local(async move {
        let settings = store::settings_load().await;
        stay.set(settings.stay_logged_in);
        if let Ok(Some(last)) = store::kv_get::<LastLogin>("last_login").await {
            last_server.set(Some(last.server));
            username.set(last.username);
        }
        if settings.stay_logged_in {
            if let Ok(Some(creds)) = store::kv_get::<StoredCreds>("creds").await {
                username.set(creds.username);
                password.set(creds.password);
            }
        }
        config_ready.set(true);
    });

    // Settle the server field against the runtime configuration, which may
    // arrive after this page rendered. A locked deployment always wins over a
    // stored login; an unlocked one prefers the server this browser used last.
    Effect::new(move |_| {
        let is_locked = crate::config::is_locked();
        locked.set(is_locked);
        if is_locked {
            server.set(crate::config::initial_server());
        } else if config_ready.get() {
            server.set(match last_server.get() {
                Some(saved) => saved,
                None => crate::config::initial_server(),
            });
        }
    });

    // If the login route is visited while already logged in, go to the app.
    Effect::new(move |_| {
        if state.session.get().is_some() {
            // "" is the route root; with a mount path ("/ui/") a leading "/"
            // would produce "/ui//".
            navigate("", Default::default());
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
        hint.set(None);
        state.login_error.set(None);
        spawn_local(async move {
            match api::login_classified(&server_s, &user, &pass, state.lang.get_untracked()).await {
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
                Err((reason, e)) => {
                    error.set(Some(e.to_string()));
                    if reason == api::Unreachable::Cors {
                        hint.set(Some(
                            crate::i18n::t(state.lang.get_untracked(), "server_cors_hint")
                                .to_string(),
                        ));
                    }
                    busy.set(false);
                }
            }
        });
    };

    view! {
        <div class="login-page">
            <div class="login-card">
                <div class="login-brand">
                    <img src=move || crate::config::asset_url("logo.svg") width="64" height="64" alt="ChefDAV logo" />
                    <h1>"ChefDAV"</h1>
                    <p class="tagline">{move || crate::i18n::t(state.lang.get(), "tagline")}</p>
                </div>
                <form on:submit=submit>
                    <label class="field has-icon">
                        <span>{move || crate::i18n::t(state.lang.get(), "server")}</span>
                        <span class="field-icon" aria-hidden="true">
                            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3">
                                <circle cx="8" cy="8" r="6" />
                                <path d="M2 8h12M8 2c1.8 1.8 2.7 3.8 2.7 6S9.8 12.2 8 14c-1.8-1.8-2.7-3.8-2.7-6S6.2 3.8 8 2z" />
                            </svg>
                        </span>
                        <input type="url" required placeholder="http://127.0.0.1:8080"
                            // Locked deployment (BACKEND_URL): read-only. The
                            // attribute (not just the property) so it is
                            // reflected in the DOM for a11y and tests; the
                            // value still submits.
                            readonly=move || locked.get().then_some("")
                            class:locked=move || locked.get()
                            title=move || {
                                if locked.get() {
                                    crate::i18n::t(state.lang.get(), "server_locked").to_string()
                                } else {
                                    String::new()
                                }
                            }
                            prop:value=move || server.get()
                            on:input=move |ev| {
                                if !locked.get_untracked() {
                                    server.set(event_target_value(&ev));
                                }
                            } />
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
                    <Show when=move || hint.get().is_some() fallback=|| ()>
                        <p class="form-hint">{move || hint.get().unwrap_or_default()}</p>
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
