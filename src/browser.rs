//! Small wasm-bindgen glue: CSPRNG, UIDs, clock and the browser timezone.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use wasm_bindgen::{JsCast, JsValue};

pub fn window() -> web_sys::Window {
    web_sys::window().expect("running in a browser window")
}

/// Fill `buf` with cryptographically secure random bytes (WebCrypto).
pub fn fill_random(buf: &mut [u8]) {
    window()
        .crypto()
        .expect("WebCrypto available")
        .get_random_values_with_u8_array(buf)
        .expect("getRandomValues failed");
}

pub fn random_hex(nbytes: usize) -> String {
    let mut b = vec![0u8; nbytes];
    fill_random(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A fresh RFC-5545 UID.
pub fn new_uid() -> String {
    format!("{}@chef-dav", random_hex(16))
}

/// Copy text to the clipboard.
///
/// `navigator.clipboard` only exists in a secure context (HTTPS or localhost),
/// so a phone opening the LAN preview over plain HTTP falls back to a hidden
/// textarea and `document.execCommand("copy")`.
pub async fn copy_text(text: &str) -> bool {
    if let Some(win) = web_sys::window() {
        let clipboard = js_sys::Reflect::get(
            win.navigator().as_ref(),
            &JsValue::from_str("clipboard"),
        );
        if let Ok(clipboard) = clipboard {
            if !clipboard.is_undefined() && !clipboard.is_null() {
                let clipboard: &web_sys::Clipboard = clipboard.unchecked_ref();
                if clipboard.write_text(text).await.is_ok() {
                    return true;
                }
            }
        }
        return legacy_copy(text);
    }
    false
}

fn legacy_copy(text: &str) -> bool {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return false;
    };
    let Ok(el) = doc.create_element("textarea") else {
        return false;
    };
    let Ok(area) = el.dyn_into::<web_sys::HtmlTextAreaElement>() else {
        return false;
    };
    area.set_value(text);
    let _ = area.set_attribute("readonly", "");
    let style = area.style();
    let _ = style.set_property("position", "fixed");
    let _ = style.set_property("left", "-9999px");
    let _ = style.set_property("top", "0");
    let Some(body) = doc.body() else {
        return false;
    };
    if body.append_child(&area).is_err() {
        return false;
    }
    area.focus().ok();
    area.select();
    // `Document.execCommand` is not in web-sys (deprecated), so call it via JS.
    let ok = js_sys::Reflect::get(doc.as_ref(), &JsValue::from_str("execCommand"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call1(doc.as_ref(), &JsValue::from_str("copy")).ok())
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let _ = body.remove_child(&area);
    ok
}

/// Resolve after `ms` milliseconds (browser timer).
pub async fn sleep_ms(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
            &resolve,
            ms.min(i32::MAX as u32) as i32,
        );
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Current instant (from the JS clock, which works on wasm).
pub fn now_utc() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(js_sys::Date::now() as i64).expect("valid timestamp")
}

/// The browser's IANA timezone (e.g. "Europe/Berlin") via Intl.
pub fn browser_tz() -> Tz {
    let locales = js_sys::Array::new();
    let options = js_sys::Object::new();
    let dtf = js_sys::Intl::DateTimeFormat::new(&locales, &options);
    // resolvedOptions is a METHOD: Reflect::get(&dtf, "resolvedOptions") only
    // fetched the function object and the "timeZone" lookup on it silently
    // failed, so every build fell back to UTC and TZID events rendered at
    // their UTC times (12:00 Europe/Berlin showed as 11:00). Found by the
    // pinned-clock e2e battery on 2026-11-17.
    let opts = dtf.resolved_options();
    let tz_name = js_sys::Reflect::get(&opts, &JsValue::from_str("timeZone"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_else(|| "UTC".to_string());
    tz_name.parse().unwrap_or(chrono_tz::UTC)
}
