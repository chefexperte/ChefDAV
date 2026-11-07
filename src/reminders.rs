//! Browser reminders: notify ~10 minutes before an event starts.
//!
//! Uses the Notification API. Permission is requested from a user gesture
//! (the "Reminders" toggle in the sidebar). Recurring events are skipped for
//! now (their occurrences would need per-instance tracking).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use chrono::Duration;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use crate::browser;
use crate::state::AppState;

/// Register the reminder service worker (called once at startup).
pub fn register_service_worker() {
    if let Some(win) = web_sys::window() {
        let _ = win.navigator().service_worker().register("/sw.js");
    }
}

/// Send upcoming reminders (next 24 h) to the service worker so it can show
/// them even when the page loop is throttled. True push while the browser is
/// closed would require a push server; this is the client-only best effort.
pub fn schedule_in_service_worker(state: AppState) {
    spawn_local(async move {
        let Some(win) = web_sys::window() else { return };
        let container = win.navigator().service_worker();
        let Ok(ready) = container.ready() else { return };
        let Ok(reg_js) = wasm_bindgen_futures::JsFuture::from(ready).await else {
            return;
        };
        let Ok(reg) = reg_js.dyn_into::<web_sys::ServiceWorkerRegistration>() else {
            return;
        };
        let Some(active) = reg.active() else { return };
        let now = browser::now_utc();
        let mut reminders = Vec::new();
        for ev in state.events.get() {
            if ev.event.rrule.is_some() {
                continue;
            }
            let Some(start) = ev
                .event
                .dtstart
                .as_ref()
                .and_then(|d| d.to_datetime(&state.tz))
            else {
                continue;
            };
            let start_utc = start.with_timezone(&chrono::Utc);
            let diff = start_utc - now;
            if diff < Duration::zero() || diff > Duration::hours(24) {
                continue;
            }
            reminders.push(serde_json::json!({
                "when": start_utc.to_rfc3339(),
                "title": ev.event.summary,
                "body": format!("Starts at {}", start.format("%H:%M")),
                "tag": ev.href,
            }));
        }
        let payload = serde_json::json!({
            "type": "schedule-reminders",
            "reminders": reminders,
        });
        if let Ok(js) = serde_wasm_bindgen::to_value(&payload) {
            let _ = active.post_message(&js);
        }
    });
}

/// Ask for notification permission. Returns whether it was granted.
pub async fn request_permission() -> bool {
    let Some(win) = web_sys::window() else { return false };
    let Ok(notification) =
        js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str("Notification"))
    else {
        return false;
    };
    let Some(notification) = notification.dyn_ref::<js_sys::Function>() else {
        return false;
    };
    let promise = match notification.call0(&wasm_bindgen::JsValue::UNDEFINED) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let promise = match promise.dyn_into::<js_sys::Promise>() {
        Ok(p) => p,
        Err(_) => return false,
    };
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(v) => v.as_string().as_deref() == Some("granted"),
        Err(_) => false,
    }
}

/// Start the 60-second reminder loop. Safe to call once.
pub fn start(state: AppState) {
    let notified: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
    let closure = Closure::<dyn FnMut()>::new(move || {
        if !state.reminders.get() {
            return;
        }
        let now = browser::now_utc();
        for ev in state.events.get() {
            if ev.event.rrule.is_some() {
                continue;
            }
            let Some(start) = ev
                .event
                .dtstart
                .as_ref()
                .and_then(|d| d.to_datetime(&state.tz))
            else {
                continue;
            };
            let diff = start.with_timezone(&chrono::Utc) - now;
            if diff >= Duration::zero()
                && diff <= Duration::minutes(10)
                && !notified.borrow().contains(&ev.href)
            {
                // The Notification constructor both creates and shows it.
                let _ = web_sys::Notification::new(&format!(
                    "{} in {} min",
                    ev.event.summary,
                    diff.num_minutes().max(1)
                ));
                notified.borrow_mut().insert(ev.href.clone());
            }
        }
    });
    if let Some(win) = web_sys::window() {
        let _ = win.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            60_000,
        );
    }
    closure.forget();
}
