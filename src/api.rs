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
use crate::store::{self, CachedEvent, CachedTodo, CalendarCache, OutboxOp};

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
            let resp = r.send().await.map_err(|e| DavError::Transport(e.to_string()))?;
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

/// Verify credentials against the server and discover principal + home.
pub async fn login(server: &str, username: &str, password: &str) -> Result<Session> {
    let server = Url::parse(server.trim().trim_end_matches('/'))
        .map_err(|e| DavError::Transport(format!("invalid server URL: {e}")))?;
    let c = DavClient::new(FetchTransport, server.clone(), username, password);
    c.check().await?;
    let principal = c.current_principal().await?;
    let home = c.calendar_home(&principal).await?;
    Ok(Session {
        server,
        username: username.to_string(),
        password: password.to_string(),
        principal,
        home,
    })
}

pub async fn refresh_calendars(state: &AppState) {
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
            let _ = store::kv_set("cache_calendars", &infos).await;
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

/// Refresh the visible calendars. Uses the per-calendar sync-token for
/// incremental updates; falls back to a full fetch when the token is missing
/// or rejected.
pub async fn refresh_events(state: &AppState) {
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
    state.events.set(all);
    store::cache_save(&caches.into_values().collect::<Vec<_>>()).await;
}

/// Fetch all tasks for visible calendars that support VTODO.
pub async fn refresh_todos(state: &AppState) {
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
            Err(e) => state
                .error
                .set(Some(format!("{} tasks: {e}", cal.display_name))),
        }
    }
    state.todos.set(all);
    store::todos_cache_save(&cached).await;
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
            Ok(()) => {}
            Err(DavError::Transport(_)) => {
                network_down = true;
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
        let old_visible: Vec<(String, bool)> = state
            .calendars
            .get()
            .iter()
            .map(|c| (c.info.href.clone(), c.visible.get()))
            .collect();
        let new: Vec<CalState> = cals
            .into_iter()
            .enumerate()
            .map(|(i, info)| {
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
    state.events.set(events);

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
    state.todos.set(todos);
    state.pending.set(store::outbox_load().await.len());
}
