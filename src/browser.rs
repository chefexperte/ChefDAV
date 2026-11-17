//! Small wasm-bindgen glue: CSPRNG, UIDs, clock and the browser timezone.

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use wasm_bindgen::JsValue;

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
