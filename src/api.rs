//! The browser-side wiring: a fetch-based [`HttpTransport`], plus high-level
//! operations that keep the reactive [`AppState`], the IndexedDB cache and the
//! offline outbox in sync with the CalDAV server.

use std::collections::HashMap;
use std::future::Future;

use caldav::client::{resource_url, CalendarInfo, DavClient};
use caldav::error::{DavError, Result};
use caldav::ics::{
    build_ics, build_override_ics, build_todo_ics, parse_cached_event, shift_ics_dates, vevent_block,
    wrap_vevents, DtValue, EventDraft, TodoDraft, VEvent,
};
use caldav::transport::{DavRequest, DavResponse, HttpTransport};
use leptos::prelude::*;
use url::Url;

use crate::browser;
use crate::state::{
    color_for, AppState, CalState, DisplayEvent, SearchHit, Session, TodoItem, ViewMode,
};
use crate::store::{self, CachedEvent, CachedTodo, CalendarCache, OutboxOp, StoredCreds};

/// Fetch the deployment's runtime configuration (`/config.json`).
///
/// The container image writes this file from its environment variables on every
/// start, so one image works for any backend (see `crate::config`). Plain
/// static hosting has no such file — a 404 or any error means "no runtime
/// configuration", never a failure: the app then falls back to its build-time
/// default and an editable server field.
pub async fn fetch_runtime_config() -> String {
    // An absolute URL (reqwest on wasm cannot resolve a relative path, and the
    // builder error would look like "no config"). `index.html` sits at the
    // mount path, which is not known yet — the address bar is the only source
    // of truth here, so a `/ui/month/2026-11` deep link still finds
    // `/ui/config.json`.
    let win = web_sys::window();
    let origin = win
        .as_ref()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default();
    let path = win
        .as_ref()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_default();
    let url = format!("{origin}{}/config.json", runtime_config_dir(&path));
    match reqwest::Client::new().get(&url).send().await {
        Ok(resp) if resp.status().is_success() => resp.text().await.unwrap_or_default(),
        Ok(_) => String::new(), // 404: plain static hosting, no runtime config
        Err(e) => {
            eprintln!("chefdav: no runtime config ({url}): {e}");
            String::new()
        }
    }
}

/// Directory the app is mounted in, from a page path: `/ui/month/2026-11` →
/// `/ui`, `/month/2026-11` → `""`, `/ui/` → `/ui`.
fn runtime_config_dir(pathname: &str) -> String {
    let trimmed = pathname.trim_start_matches('/');
    let mut parts = trimmed.split('/');
    let first = parts.next().unwrap_or("");
    // A known route at the root means the app is not mounted under a prefix.
    if crate::config::detect_base_path(pathname).is_empty() {
        String::new()
    } else {
        format!("/{first}")
    }
}

/// reqwest on wasm32 is backed by `fetch`, which supports arbitrary methods
/// (PROPFIND/REPORT/MKCALENDAR) and custom headers — everything CalDAV needs.
pub struct FetchTransport;

impl HttpTransport for FetchTransport {
    fn request(&self, req: DavRequest) -> impl Future<Output = Result<DavResponse>> {
        async move {
            let method = reqwest::Method::from_bytes(req.method.as_str().as_bytes())
                .map_err(|e| DavError::Transport(e.to_string()))?;
            let mut r = reqwest::Client::new().request(method, req.url.as_str());
            for (k, v) in &req.headers {
                r = r.header(k, v);
            }
            if let Some(b) = req.body {
                r = r.body(b);
            }
            let resp = r.send().await.map_err(|e| {
                // warn, not error: an offline write is an expected failure that
                // the outbox retries, and it must not show up as a console error
                leptos::logging::warn!("chefdav: request to {} failed: {e:?}", req.url);
                DavError::Transport(e.to_string())
            })?;
            let status = resp.status().as_u16();
            let headers = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();
            let body = resp.text().await.map_err(|e| DavError::Transport(e.to_string()))?;
            Ok(DavResponse {
                status,
                headers,
                body,
            })
        }
    }
}

pub fn client(session: &Session) -> DavClient<FetchTransport> {
    DavClient::new(
        FetchTransport,
        session.server.clone(),
        &session.username,
        &session.password,
    )
}

/// Why a CalDAV request never reached the server, in terms a login screen can
/// phrase for someone who is wiring up a self-hosted deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreachable {
    /// No response at all: the browser refused the cross-origin request. For a
    /// CalDAV endpoint this almost always means the CORS proxy is missing or
    /// does not allow the WebDAV methods (PROPFIND/REPORT/...).
    Cors,
    /// Any other network/DNS/TLS failure.
    Network,
    /// The endpoint answered as a whole is unavailable (404 from a proxy
    /// without a backend, 5xx from a stopped server).
    Backend,
}

impl Unreachable {
    /// Build the error a caller should surface for this failure.
    fn into_error(self, lang: crate::state::Lang) -> DavError {
        DavError::Transport(error_text(self, lang).to_string())
    }

    fn i18n_key(self) -> &'static str {
        match self {
            Unreachable::Cors => "server_cors_blocked",
            Unreachable::Network => "server_unreachable",
            Unreachable::Backend => "server_backend_down",
        }
    }
}

/// Translated one-line explanation for a failed connection.
pub fn error_text(reason: Unreachable, lang: crate::state::Lang) -> &'static str {
    crate::i18n::t(lang, reason.i18n_key())
}

/// A failure of the endpoint as a whole: no response at all, or an HTTP
/// 404/5xx answer (a proxy without a backend, a stopped server). A 401/403 is
/// a login problem and is handled on the login page, not as an outage.
pub fn backend_unavailable(e: &DavError) -> bool {
    match e {
        DavError::Transport(_) => true,
        DavError::Http { status, .. } => *status == 404 || *status >= 500,
        _ => false,
    }
}

/// Verify credentials against the server and discover principal + home.
pub async fn login(server: &str, username: &str, password: &str) -> Result<Session> {
    login_classified(server, username, password, crate::state::Lang::En)
        .await
        .map_err(|(_, err)| err)
}

/// [`login`] variant that also reports why it could not reach the server, so
/// the login page can say "this looks like CORS" instead of "unreachable".
pub async fn login_classified(
    server: &str,
    username: &str,
    password: &str,
    lang: crate::state::Lang,
) -> std::result::Result<Session, (Unreachable, DavError)> {
    let server = Url::parse(server.trim().trim_end_matches('/')).map_err(|e| {
        (
            Unreachable::Network,
            DavError::Transport(format!("invalid server URL: {e}")),
        )
    })?;

    // Do not send an unauthenticated request to the server root first: with a
    // `WWW-Authenticate` answer the browser opens its own Basic auth dialog,
    // even though the app supplies the credentials itself. The CalDAV request
    // below classifies transport failures just as well.
    let c = DavClient::new(FetchTransport, server.clone(), username, password);
    c.check().await.map_err(|e| {
        match e {
            // No response at all: almost always a missing CORS proxy, since
            // the CalDAV method and its headers need a preflight.
            DavError::Transport(_) => (
                Unreachable::Cors,
                Unreachable::Cors.into_error(lang),
            ),
            // A proxy without a backend (404) or a stopped server (5xx).
            DavError::Http { status, .. } if status == 404 || status >= 500 => (
                Unreachable::Backend,
                Unreachable::Backend.into_error(lang),
            ),
            // The server answered. A 401/403 is a real login failure and must
            // keep its own message ("check username/password"), not be
            // rewritten as a connection problem.
            other => (Unreachable::Network, other),
        }
    })?;
    // From here on the connection works; any failure is a server/XML answer.
    let plain = |e: DavError| (Unreachable::Network, e);
    let principal = c.current_principal().await.map_err(plain)?;
    let home = c.calendar_home(&principal).await.map_err(plain)?;
    Ok(Session {
        server,
        username: username.to_string(),
        password: password.to_string(),
        principal,
        home,
    })
}

/// Rebuild a [`Session`] from the credentials and cached calendars when the
/// backend cannot be reached at startup. `None` means there is no cache for
/// this server (or it belongs to another server), so the login page is the
/// only sensible place to be.
pub async fn cached_session(creds: &StoredCreds, server: &str) -> Option<Session> {
    let server_url = Url::parse(server.trim().trim_end_matches('/')).ok()?;
    let cals: Vec<CalendarInfo> = store::kv_get("cache_calendars").await.ok().flatten()?;
    if cals.is_empty() {
        return None;
    }
    // A changed BACKEND_URL must not show the old server's cached calendars.
    let origin = server_url.origin();
    let all_from_server = cals.iter().all(|c| {
        Url::parse(&c.href)
            .map(|u| u.origin() == origin)
            .unwrap_or(false)
    });
    if !all_from_server {
        return None;
    }
    let from_server = |raw: Option<String>| -> Option<Url> {
        let url = Url::parse(raw.as_deref()?).ok()?;
        (url.origin() == origin).then_some(url)
    };
    // Older saved logins have no discovery results; the calendar hrefs still
    // reveal the home collection (`/alice/personal/` → `/alice/`).
    let home = from_server(creds.home.clone()).or_else(|| {
        let first = Url::parse(&cals.first()?.href).ok()?;
        let parent = first.join("../").ok()?;
        (parent.origin() == origin).then_some(parent)
    })?;
    let principal = from_server(creds.principal.clone()).unwrap_or_else(|| home.clone());
    Some(Session {
        server: server_url,
        username: creds.username.clone(),
        password: creds.password.clone(),
        principal,
        home,
    })
}

/// Try the saved login again after starting from the cache. On success the
/// real session replaces the placeholder, which lets the shell sync normally.
pub async fn retry_cached_session(state: &AppState) -> bool {
    if !state.cache_only.get_untracked() {
        return false;
    }
    let Ok(Some(creds)) = store::kv_get::<StoredCreds>("creds").await else {
        return false;
    };
    let server = if crate::config::server_is_fixed() {
        crate::config::initial_server()
    } else {
        creds.server.clone()
    };
    match login_classified(
        &server,
        &creds.username,
        &creds.password,
        state.lang.get_untracked(),
    )
    .await
    {
        Ok(session) => {
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
            state.cache_only.set(false);
            state.backend_down.set(false);
            state.login_error.set(None);
            true
        }
        // Keep the cached view; a rejected password must not blank it.
        Err(_) => false,
    }
}

/// The offline cache is browse-only: no write may be queued from it, because
/// the saved credentials were never verified in this tab.
fn ensure_writable(state: &AppState) -> Result<()> {
    if state.cache_only.get_untracked() {
        return Err(DavError::Transport(
            "the server is unreachable; the cached calendar is read-only".into(),
        ));
    }
    Ok(())
}

pub async fn refresh_calendars(state: &AppState) {
    if state.cache_only.get_untracked() {
        return;
    }
    let Some(session) = state.session.get() else {
        return;
    };
    match client(&session).list_calendars(&session.home).await {
        Ok(cals) => {
            let old = state.calendars.get();
            let new: Vec<CalState> = cals
                .into_iter()
                .enumerate()
                .map(|(i, info)| {
                    // Reuse the existing visibility signal for calendars that
                    // are already in the list. Replacing it would leave the
                    // sidebar checkbox bound to the old signal while the
                    // refresh effect tracks the new one, so toggling would
                    // silently do nothing.
                    let visible = old
                        .iter()
                        .find(|c| c.info.href == info.href)
                        .map(|c| c.visible)
                        .unwrap_or_else(|| RwSignal::new(true));
                    let color = color_for(&info.href, &info.color, i);
                    CalState {
                        info,
                        visible,
                        color,
                    }
                })
                .collect();
            let infos: Vec<CalendarInfo> = new.iter().map(|c| c.info.clone()).collect();
            state.calendars.set(new);
            state.calendars_loaded.set(true);
            state.backend_down.set(false);
            let _ = store::kv_set("cache_calendars", &infos).await;
        }
        Err(e) if backend_unavailable(&e) => {
            // The banner in the shell explains what still works.
            state.backend_down.set(true);
            state.error.set(None);
        }
        Err(e) => state
            .error
            .set(Some(format!("Could not load calendars: {e}"))),
    }
}

fn display_events(
    cache: &CalendarCache,
    cal: &CalendarInfo,
    color: &str,
    tz: &chrono_tz::Tz,
) -> Vec<DisplayEvent> {
    let mut out = Vec::new();
    for ce in &cache.events {
        let events = parse_cached_event(&ce.ics, tz);
        let mut by_uid: HashMap<String, (Option<VEvent>, Vec<VEvent>)> = HashMap::new();
        for ev in events {
            let entry = by_uid.entry(ev.uid.clone()).or_default();
            if ev.is_override() {
                entry.1.push(ev);
            } else {
                entry.0 = Some(ev);
            }
        }
        for (_, (master, overrides)) in by_uid {
            let Some(master) = master else { continue };
            out.push(DisplayEvent {
                href: ce.href.clone(),
                etag: ce.etag.clone(),
                event: master,
                overrides,
                calendar_href: cal.href.clone(),
                color: color.to_string(),
                read_only: cal.read_only,
            });
        }
    }
    out
}

/// Rebuild `state.events` from the IndexedDB cache and the current calendar
/// visibility. Offline mode uses this for show/hide and navigation, so the
/// cached data behaves like the live list without touching the server.
async fn restore_cached_events(state: &AppState) {
    let caches = store::cache_load().await;
    let mut events = Vec::new();
    for cache in &caches {
        if let Some(cal) = state
            .calendars
            .get()
            .iter()
            .find(|c| c.info.href == cache.calendar_href)
        {
            if cal.visible.get() {
                events.extend(display_events(cache, &cal.info, &cal.color, &state.tz));
            }
        }
    }
    state.remap_open_modal(Some(&events), None);
    state.events.set(events);
}

/// Same for tasks: the cache always holds everything, visibility decides what
/// the view shows.
async fn restore_cached_todos(state: &AppState) {
    let cached_todos = store::todos_cache_load().await;
    let mut todos = Vec::new();
    for ct in cached_todos {
        if let Some(cal) = state
            .calendars
            .get()
            .iter()
            .find(|c| c.info.href == ct.calendar_href)
        {
            for t in caldav::ics::parse_cached_todo(&ct.ics, &state.tz) {
                todos.push(TodoItem {
                    href: ct.href.clone(),
                    etag: ct.etag.clone(),
                    todo: t,
                    calendar_href: ct.calendar_href.clone(),
                    color: cal.color.clone(),
                    read_only: cal.read_only(),
                });
            }
        }
    }
    state.remap_open_modal(None, Some(&todos));
    state.todos.set(todos);
}

/// Refresh the visible calendars. Uses the per-calendar sync-token for
/// incremental updates; falls back to a full fetch when the token is missing
/// or rejected.
pub async fn refresh_events(state: &AppState) {
    if state.cache_only.get_untracked() {
        restore_cached_events(state).await;
        return;
    }
    let Some(session) = state.session.get() else {
        return;
    };
    let visible: Vec<(CalendarInfo, String)> = state
        .calendars
        .get()
        .iter()
        .filter(|c| c.visible.get() && c.info.supports_events())
        .map(|c| (c.info.clone(), c.color.clone()))
        .collect();
    state.loading.set(true);
    let c = client(&session);
    let mut caches: HashMap<String, CalendarCache> = store::cache_load()
        .await
        .into_iter()
        .map(|c| (c.calendar_href.clone(), c))
        .collect();
    let mut all: Vec<DisplayEvent> = Vec::new();
    let mut any_ok = false;
    for (cal, color) in &visible {
        let Ok(url) = Url::parse(&cal.href) else { continue };
        let mut cache = caches.remove(&cal.href).unwrap_or_else(|| CalendarCache {
            calendar_href: cal.href.clone(),
            ..Default::default()
        });
        let has_cache = !cache.events.is_empty();
        let token = if has_cache {
            cache.sync_token.clone().or_else(|| cal.ctag.clone())
        } else {
            None
        };
        let mut refreshed = false;
        if let Some(tok) = token {
            match c.sync_collection(&url, Some(&tok)).await {
                Ok(sync) => {
                    any_ok = true;
                    cache.events.retain(|e| !sync.deleted.contains(&e.href));
                    if !sync.changed.is_empty() {
                        match c.multiget_events(&url, &sync.changed, &state.tz).await {
                            Ok(fetched) => {
                                let mut by_href: HashMap<String, (Option<String>, Vec<VEvent>)> =
                                    HashMap::new();
                                for de in fetched {
                                    let entry = by_href
                                        .entry(de.href)
                                        .or_insert((de.etag.clone(), Vec::new()));
                                    if entry.0.is_none() {
                                        entry.0 = de.etag;
                                    }
                                    entry.1.push(de.event);
                                }
                                for (href, (etag, events)) in by_href {
                                    cache.events.retain(|e| e.href != href);
                                    let blocks =
                                        events.iter().map(|e| e.raw.clone()).collect::<Vec<_>>();
                                    cache.events.push(CachedEvent {
                                        calendar_href: cal.href.clone(),
                                        href,
                                        etag,
                                        ics: wrap_vevents(&blocks),
                                    });
                                }
                            }
                            Err(e) if backend_unavailable(&e) => {
                                state.backend_down.set(true);
                                state.error.set(None);
                            }
                            Err(e) => state.error.set(Some(format!(
                                "{}: could not fetch changes: {e}",
                                cal.display_name
                            ))),
                        }
                    }
                    if let Some(t) = sync.new_token {
                        cache.sync_token = Some(t);
                    }
                    refreshed = true;
                }
                Err(_) => {
                    // stale/invalid token: fall through to a full fetch
                }
            }
        }
        if !refreshed {
            match c.query_all_events(&url, &state.tz).await {
                Ok(evs) => {
                    any_ok = true;
                    let mut by_href: HashMap<String, (Option<String>, Vec<VEvent>)> = HashMap::new();
                    for e in evs {
                        let entry = by_href
                            .entry(e.href)
                            .or_insert((e.etag.clone(), Vec::new()));
                        if entry.0.is_none() {
                            entry.0 = e.etag;
                        }
                        entry.1.push(e.event);
                    }
                    cache.events = by_href
                        .into_iter()
                        .map(|(href, (etag, events))| {
                            let blocks = events.iter().map(|e| e.raw.clone()).collect::<Vec<_>>();
                            CachedEvent {
                                calendar_href: cal.href.clone(),
                                href,
                                etag,
                                ics: wrap_vevents(&blocks),
                            }
                        })
                        .collect();
                    cache.sync_token = cal.ctag.clone();
                }
                Err(e) if backend_unavailable(&e) => {
                    state.backend_down.set(true);
                    state.error.set(None);
                }
                Err(e) => {
                    state
                        .error
                        .set(Some(format!("{}: {e}", cal.display_name)))
                }
            }
        }
        all.extend(display_events(&cache, cal, color, &state.tz));
        caches.insert(cal.href.clone(), cache);
    }
    state.loading.set(false);
    if any_ok {
        state.backend_down.set(false);
    }
    state.remap_open_modal(Some(&all), None);
    state.events.set(all);
    store::cache_save(&caches.into_values().collect::<Vec<_>>()).await;
}

/// Fetch all tasks for visible calendars that support VTODO.
pub async fn refresh_todos(state: &AppState) {
    if state.cache_only.get_untracked() {
        restore_cached_todos(state).await;
        return;
    }
    let Some(session) = state.session.get() else {
        return;
    };
    let visible: Vec<(CalendarInfo, String)> = state
        .calendars
        .get()
        .iter()
        .filter(|c| c.visible.get() && c.info.supports_todos())
        .map(|c| (c.info.clone(), c.color.clone()))
        .collect();
    if visible.is_empty() {
        state.remap_open_modal(None, Some(&[]));
        state.todos.set(Vec::new());
        return;
    }
    let c = client(&session);
    let mut all = Vec::new();
    let mut cached = Vec::new();
    for (cal, color) in &visible {
        let Ok(url) = Url::parse(&cal.href) else { continue };
        match c.query_todos(&url, &state.tz).await {
            Ok(todos) => {
                for t in todos {
                    cached.push(CachedTodo {
                        calendar_href: cal.href.clone(),
                        href: t.href.clone(),
                        etag: t.etag.clone(),
                        ics: t.todo.raw.clone(),
                    });
                    all.push(TodoItem {
                        href: t.href,
                        etag: t.etag,
                        todo: t.todo,
                        calendar_href: cal.href.clone(),
                        color: color.clone(),
                        read_only: cal.read_only,
                    });
                }
            }
            Err(e) if backend_unavailable(&e) => {
                state.backend_down.set(true);
                state.error.set(None);
            }
            Err(e) => state
                .error
                .set(Some(format!("{} tasks: {e}", cal.display_name))),
        }
    }
    state.remap_open_modal(None, Some(&all));
    state.todos.set(all);
    store::todos_cache_save(&cached).await;
}

/// Fetch a single event by UID from a calendar. Used for resource deep links
/// when the event is not in the loaded set (e.g. its calendar is hidden).
pub async fn fetch_event_by_uid(
    state: &AppState,
    cal: &CalendarInfo,
    uid: &str,
) -> Option<DisplayEvent> {
    if state.cache_only.get_untracked() {
        return None;
    }
    let session = state.session.get()?;
    let url = Url::parse(&cal.href).ok()?;
    let c = client(&session);
    let evs = c.query_all_events(&url, &state.tz).await.ok()?;
    let mut by_href: HashMap<String, (Option<String>, Vec<VEvent>)> = HashMap::new();
    for e in evs {
        if e.event.uid != uid {
            continue;
        }
        let entry = by_href
            .entry(e.href.clone())
            .or_insert((e.etag.clone(), Vec::new()));
        if entry.0.is_none() {
            entry.0 = e.etag;
        }
        entry.1.push(e.event);
    }
    let (href, (etag, events)) = by_href.into_iter().next()?;
    let mut master = None;
    let mut overrides = Vec::new();
    for ev in events {
        if ev.is_override() {
            overrides.push(ev);
        } else {
            master = Some(ev);
        }
    }
    let color = state
        .calendar_for(&cal.href)
        .map(|c| c.color)
        .unwrap_or_else(|| color_for(&cal.href, &cal.color, 0));
    Some(DisplayEvent {
        href,
        etag,
        event: master?,
        overrides,
        calendar_href: cal.href.clone(),
        color,
        read_only: cal.read_only,
    })
}

/// Fetch a single task by UID from a calendar (deep-link fallback).
pub async fn fetch_todo_by_uid(
    state: &AppState,
    cal: &CalendarInfo,
    uid: &str,
) -> Option<TodoItem> {
    if state.cache_only.get_untracked() {
        return None;
    }
    let session = state.session.get()?;
    let url = Url::parse(&cal.href).ok()?;
    let c = client(&session);
    let todos = c.query_todos(&url, &state.tz).await.ok()?;
    let t = todos.into_iter().find(|t| t.todo.uid == uid)?;
    let color = state
        .calendar_for(&cal.href)
        .map(|c| c.color)
        .unwrap_or_else(|| color_for(&cal.href, &cal.color, 0));
    Some(TodoItem {
        href: t.href,
        etag: t.etag,
        todo: t.todo,
        calendar_href: cal.href.clone(),
        color,
        read_only: cal.read_only,
    })
}

/// Client-side search over the cached (full) event set.
pub fn search(state: &AppState, query: &str) {
    let q = query.trim().to_lowercase();
    if q.len() < 2 {
        state.search_results.set(Vec::new());
        return;
    }
    let mut hits = Vec::new();
    for ev in state.events.get() {
        let e = &ev.event;
        if !(e.summary.to_lowercase().contains(&q)
            || e.location.to_lowercase().contains(&q)
            || e.description.to_lowercase().contains(&q))
        {
            continue;
        }
        let (date, time) = match &e.dtstart {
            Some(DtValue::Date(d)) => (*d, "All day".to_string()),
            Some(DtValue::DateTime(t)) => {
                let local = t.with_timezone(&state.tz);
                (local.date_naive(), local.format("%H:%M").to_string())
            }
            None => continue,
        };
        hits.push(SearchHit {
            date,
            time,
            summary: e.summary.clone(),
            location: e.location.clone(),
            color: ev.color.clone(),
            href: ev.href.clone(),
        });
    }
    hits.sort_by_key(|h| (h.date, h.time.clone()));
    hits.truncate(50);
    state.search_results.set(hits);
}

// ---------------------------------------------------------------------------
// Writes (with offline outbox)
// ---------------------------------------------------------------------------

async fn queue(state: &AppState, op: OutboxOp) {
    let mut ops = store::outbox_load().await;
    ops.push(op);
    store::outbox_save(&ops).await;
    state.pending.set(ops.len());
}

fn upsert_local_event(state: &AppState, cal_href: &str, href: &str, ics: &str, etag: Option<String>) {
    let Some(cal) = state.calendar_for(cal_href) else { return };
    let events = parse_cached_event(ics, &state.tz);
    let mut by_uid: HashMap<String, (Option<VEvent>, Vec<VEvent>)> = HashMap::new();
    for ev in events {
        let entry = by_uid.entry(ev.uid.clone()).or_default();
        if ev.is_override() {
            entry.1.push(ev);
        } else {
            entry.0 = Some(ev);
        }
    }
    for (_, (master, overrides)) in by_uid {
        let Some(master) = master else { continue };
        let display = DisplayEvent {
            href: href.to_string(),
            etag: etag.clone(),
            event: master,
            overrides,
            calendar_href: cal_href.to_string(),
            color: cal.color.clone(),
            read_only: cal.read_only(),
        };
        state.events.update(|v| {
            v.retain(|e| e.href != href);
            v.push(display.clone());
        });
    }
}

/// Create or update an event. Returns `true` when the write was queued for
/// later because the network is unavailable.
pub async fn save_event(
    state: &AppState,
    calendar_href: &str,
    href: Option<&str>,
    draft: &EventDraft,
    etag: Option<&str>,
    overrides: &[VEvent],
) -> Result<bool> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let master_ics = build_ics(draft, browser::now_utc());
    let block = vevent_block(&master_ics)
        .ok_or_else(|| DavError::Transport("could not build event".into()))?;
    let mut blocks = vec![block];
    blocks.extend(overrides.iter().map(|o| o.raw.clone()));
    let resource = wrap_vevents(&blocks);
    let cal = Url::parse(calendar_href)?;
    let target = match href {
        Some(h) => Url::parse(h)?,
        None => resource_url(&cal, &draft.uid)?,
    };
    match client(&session).put_resource(&target, &resource, etag).await {
        Ok(_) => Ok(false),
        Err(DavError::Transport(_)) => {
            queue(
                state,
                OutboxOp::PutEvent {
                    calendar_href: calendar_href.to_string(),
                    uid: draft.uid.clone(),
                    ics: resource.clone(),
                    etag: etag.map(str::to_string),
                },
            )
            .await;
            upsert_local_event(
                state,
                calendar_href,
                target.as_str(),
                &resource,
                etag.map(str::to_string),
            );
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

/// Create/replace a `RECURRENCE-ID` exception (single or THISANDFUTURE) in the
/// master's resource. `cancelled` writes `STATUS:CANCELLED` instead.
pub async fn save_override(
    state: &AppState,
    master: &DisplayEvent,
    recurrence_id: chrono::DateTime<chrono_tz::Tz>,
    range: bool,
    draft: &EventDraft,
    cancelled: bool,
) -> Result<()> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let override_ics = build_override_ics(
        &master.event.uid,
        &recurrence_id,
        range,
        draft,
        browser::now_utc(),
        cancelled,
    );
    let block = vevent_block(&override_ics)
        .ok_or_else(|| DavError::Transport("could not build exception".into()))?;
    let mut blocks = vec![master.event.raw.clone()];
    for o in &master.overrides {
        let same_id = o
            .recurrence_id
            .as_ref()
            .and_then(|d| d.to_datetime(&state.tz))
            == Some(recurrence_id);
        // A future override supersedes everything at that id; a single
        // override only replaces another single override (so a cancellation
        // can coexist with a THISANDFUTURE rule starting at the same time).
        let replace = same_id && (range || !o.recurrence_range);
        if !replace {
            blocks.push(o.raw.clone());
        }
    }
    blocks.push(block);
    let resource = wrap_vevents(&blocks);
    let url = Url::parse(&master.href)?;
    client(&session)
        .put_resource(&url, &resource, master.etag.as_deref())
        .await?;
    refresh_events(state).await;
    Ok(())
}

pub async fn delete_event(state: &AppState, href: &str, etag: Option<&str>) -> Result<bool> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let url = Url::parse(href)?;
    match client(&session).delete_event(&url, etag).await {
        Ok(()) => Ok(false),
        Err(DavError::Transport(_)) => {
            queue(
                state,
                OutboxOp::DeleteEvent {
                    href: href.to_string(),
                    etag: etag.map(str::to_string),
                },
            )
            .await;
            state.events.update(|v| v.retain(|e| e.href != href));
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

pub async fn save_todo(
    state: &AppState,
    calendar_href: &str,
    href: Option<&str>,
    draft: &TodoDraft,
    etag: Option<&str>,
) -> Result<bool> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let ics = build_todo_ics(draft, browser::now_utc());
    let cal = Url::parse(calendar_href)?;
    let target = match href {
        Some(h) => Url::parse(h)?,
        None => resource_url(&cal, &draft.uid)?,
    };
    match client(&session).put_resource(&target, &ics, etag).await {
        Ok(_) => Ok(false),
        Err(DavError::Transport(_)) => {
            queue(
                state,
                OutboxOp::PutTodo {
                    calendar_href: calendar_href.to_string(),
                    uid: draft.uid.clone(),
                    ics: ics.clone(),
                    etag: etag.map(str::to_string),
                },
            )
            .await;
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

pub async fn delete_todo(state: &AppState, href: &str, etag: Option<&str>) -> Result<bool> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let url = Url::parse(href)?;
    match client(&session).delete_event(&url, etag).await {
        Ok(()) => Ok(false),
        Err(DavError::Transport(_)) => {
            queue(
                state,
                OutboxOp::DeleteTodo {
                    href: href.to_string(),
                    etag: etag.map(str::to_string),
                },
            )
            .await;
            state.todos.update(|v| v.retain(|t| t.href != href));
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

/// Toggle a task's completion state (rewrites the VTODO with STATUS/COMPLETED).
pub async fn toggle_todo(state: &AppState, index: usize) -> Result<bool> {
    let Some(item) = state.todos.get().get(index).cloned() else {
        return Ok(false);
    };
    let draft = TodoDraft {
        uid: item.todo.uid.clone(),
        summary: item.todo.summary.clone(),
        description: item.todo.description.clone(),
        due: match &item.todo.due {
            Some(DtValue::Date(d)) => Some(*d),
            Some(DtValue::DateTime(t)) => Some(t.with_timezone(&state.tz).date_naive()),
            None => None,
        },
        completed: !item.todo.is_done(),
        priority: item.todo.priority,
    };
    save_todo(
        state,
        &item.calendar_href,
        Some(&item.href),
        &draft,
        item.etag.as_deref(),
    )
    .await
}

/// Create a new calendar collection (MKCALENDAR) in the user's home.
pub async fn create_calendar(state: &AppState, name: &str) -> Result<()> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    client(&session)
        .create_calendar(&session.home, name)
        .await
        .map(|_| ())?;
    refresh_calendars(state).await;
    Ok(())
}

/// Rename a calendar and/or change its color, then refresh the list.
pub async fn update_calendar(
    state: &AppState,
    href: &str,
    name: &str,
    color: Option<&str>,
) -> Result<()> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let url = Url::parse(href)?;
    let c = client(&session);
    if let Some(color) = color {
        c.set_calendar_color(&url, color).await?;
    }
    c.rename_calendar(&url, name).await?;
    refresh_calendars(state).await;
    Ok(())
}

pub async fn rename_calendar(state: &AppState, href: &str, name: &str) -> Result<()> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let url = Url::parse(href)?;
    client(&session).rename_calendar(&url, name).await?;
    refresh_calendars(state).await;
    Ok(())
}

pub async fn delete_calendar(state: &AppState, href: &str) -> Result<()> {
    ensure_writable(state)?;
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let url = Url::parse(href)?;
    client(&session).delete_calendar(&url).await?;
    state.calendars.update(|cals| cals.retain(|c| c.info.href != href));
    state
        .events
        .update(|evs| evs.retain(|e| e.calendar_href != href));
    state.todos.update(|t| t.retain(|x| x.calendar_href != href));
    refresh_calendars(state).await;
    Ok(())
}

/// Delete several events (bulk selection).
pub async fn bulk_delete_events(state: &AppState, hrefs: &[String]) -> Result<usize> {
    let mut deleted = 0;
    for href in hrefs {
        let etag = state
            .events
            .get()
            .iter()
            .find(|e| &e.href == href)
            .and_then(|e| e.etag.clone());
        if delete_event(state, href, etag.as_deref()).await.is_ok() {
            deleted += 1;
        }
    }
    refresh_events(state).await;
    Ok(deleted)
}

/// Move a non-recurring event by `delta_days` (drag & drop).
pub async fn move_event(state: &AppState, href: &str, delta_days: i64) -> Result<bool> {
    ensure_writable(state)?;
    let Some(ev) = state.events.get().iter().find(|e| e.href == href).cloned() else {
        return Err(DavError::Transport("event not found".into()));
    };
    if ev.read_only {
        return Err(DavError::Transport("calendar is read-only".into()));
    }
    if ev.event.rrule.is_some() {
        return Err(DavError::Transport(
            "recurring events cannot be dragged yet".into(),
        ));
    }
    let shifted = shift_ics_dates(&ev.event.raw, delta_days)
        .ok_or_else(|| DavError::Transport("event has no DTSTART".into()))?;
    let ics = wrap_vevents(&[shifted]);
    let session = state
        .session
        .get()
        .ok_or_else(|| DavError::Transport("not logged in".into()))?;
    let url = Url::parse(href)?;
    match client(&session)
        .put_resource(&url, &ics, ev.etag.as_deref())
        .await
    {
        Ok(_) => {
            upsert_local_event(state, &ev.calendar_href, href, &ics, ev.etag.clone());
            Ok(false)
        }
        Err(DavError::Transport(_)) => {
            queue(
                state,
                OutboxOp::PutEvent {
                    calendar_href: ev.calendar_href.clone(),
                    uid: ev.event.uid.clone(),
                    ics: ics.clone(),
                    etag: ev.etag.clone(),
                },
            )
            .await;
            upsert_local_event(state, &ev.calendar_href, href, &ics, ev.etag.clone());
            Ok(true)
        }
        Err(e) => Err(e),
    }
}

fn resolve_resource(calendar_href: &str, uid: &str) -> Result<Url> {
    let cal = Url::parse(calendar_href)?;
    resource_url(&cal, uid)
}

/// Retry outboxed writes. Stops at the first network failure and keeps the
/// remaining operations for the next attempt.
pub async fn flush_outbox(state: &AppState) {
    if state.cache_only.get_untracked() {
        return;
    }
    let ops = store::outbox_load().await;
    if ops.is_empty() {
        state.pending.set(0);
        return;
    }
    let Some(session) = state.session.get() else {
        return;
    };
    let c = client(&session);
    let mut remaining = Vec::new();
    let mut network_down = false;
    for op in ops {
        if network_down {
            remaining.push(op);
            continue;
        }
        let result: Result<()> = match &op {
            OutboxOp::PutEvent {
                calendar_href,
                uid,
                ics,
                etag,
            } => match resolve_resource(calendar_href, uid) {
                Ok(url) => c.put_resource(&url, ics, etag.as_deref()).await.map(|_| ()),
                Err(e) => Err(e),
            },
            OutboxOp::DeleteEvent { href, etag } => match Url::parse(href) {
                Ok(url) => c.delete_event(&url, etag.as_deref()).await,
                Err(e) => Err(DavError::from(e)),
            },
            OutboxOp::PutTodo {
                calendar_href,
                uid,
                ics,
                etag,
            } => match resolve_resource(calendar_href, uid) {
                Ok(url) => c.put_resource(&url, ics, etag.as_deref()).await.map(|_| ()),
                Err(e) => Err(e),
            },
            OutboxOp::DeleteTodo { href, etag } => match Url::parse(href) {
                Ok(url) => c.delete_event(&url, etag.as_deref()).await,
                Err(e) => Err(DavError::from(e)),
            },
        };
        match result {
            Ok(()) => state.backend_down.set(false),
            Err(e) if backend_unavailable(&e) => {
                // Keep the write queued until the backend is back; do not drop
                // it just because a proxy answered 404/5xx in the meantime.
                network_down = true;
                state.backend_down.set(true);
                remaining.push(op);
            }
            Err(e) => {
                // permanent failure (e.g. 412 conflict): drop it and surface it
                state.error.set(Some(format!("Queued change failed: {e}")));
            }
        }
    }
    store::outbox_save(&remaining).await;
    state.pending.set(remaining.len());
    if !network_down {
        refresh_events(state).await;
        if state.view.get() == ViewMode::Tasks {
            refresh_todos(state).await;
        }
    }
}

/// Paint the last cached calendars/events/todos immediately (offline-friendly),
/// before the caller refreshes from the server.
pub async fn restore_cache(state: &AppState) {
    if let Ok(Some(cals)) = store::kv_get::<Vec<CalendarInfo>>("cache_calendars").await {
        let offline = state.cache_only.get_untracked();
        if offline && !cals.is_empty() {
            // The cached list is complete enough for the read-only shell; do
            // not leave the editor thinking no calendar has loaded yet.
            state.calendars_loaded.set(true);
        }
        let old_visible: Vec<(String, bool)> = state
            .calendars
            .get()
            .iter()
            .map(|c| (c.info.href.clone(), c.visible.get()))
            .collect();
        let new: Vec<CalState> = cals
            .into_iter()
            .enumerate()
            .map(|(i, mut info)| {
                if offline {
                    // The server could not confirm its privileges. Treat every
                    // cached calendar as read-only; the retry refreshes this
                    // from the real list.
                    info.read_only = true;
                }
                let visible = old_visible
                    .iter()
                    .find(|(h, _)| *h == info.href)
                    .map(|(_, v)| *v)
                    .unwrap_or(true);
                let color = color_for(&info.href, &info.color, i);
                CalState {
                    info,
                    visible: RwSignal::new(visible),
                    color,
                }
            })
            .collect();
        state.calendars.set(new);
    }
    restore_cached_events(state).await;
    restore_cached_todos(state).await;
    state.pending.set(store::outbox_load().await.len());
}
