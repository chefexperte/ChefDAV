//! Server configuration.
//!
//! The app is a static bundle, so a value baked in at build time cannot change
//! later. Two sources are supported:
//!
//! 1. **Runtime configuration** — `/config.json` is fetched before the app
//!    renders. This is how the container image works, so the *same* image can
//!    start against any backend, or with no backend at all:
//!
//!    ```json
//!    { "backend_url": "https://dav.example.org", "lock": true }
//!    ```
//!
//!    The image writes this file from the `BACKEND_URL` / `BACKEND_LOCK`
//!    environment variables on every start. A missing file (404) simply means
//!    "no runtime configuration" — e.g. plain static hosting.
//!
//! 2. **Build-time fallback** for self-hosted `dist/` builds: trunk loads a
//!    `.env` file, and `CHEFDAV_DAV_URL` (or `BACKEND_URL`) becomes the server
//!    URL. `CHEFDAV_DAV_URL` pre-fills an editable field; `BACKEND_URL` locks.
//!
//! Precedence: runtime file → build-time value → the local dev stack.

use std::cell::RefCell;

/// Configuration read from `/config.json` at startup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Runtime {
    /// The CalDAV server URL to use; `None` lets the user type one.
    pub backend_url: Option<String>,
    /// Read-only server field (only meaningful when a URL is set).
    pub lock: bool,
}

thread_local! {
    /// Installed once during startup (see `app::App`).
    static RUNTIME: RefCell<Runtime> = const { RefCell::new(Runtime {
        backend_url: None,
        lock: false,
    }) };
}

/// Install the runtime configuration. Called once, before the first render.
pub fn set_runtime(runtime: Runtime) {
    RUNTIME.with(|r| *r.borrow_mut() = runtime);
}

/// The active runtime configuration (empty until `set_runtime` ran).
pub fn runtime() -> Runtime {
    RUNTIME.with(|r| r.borrow().clone())
}

/// Parse a `/config.json` body. Unknown keys are ignored; a malformed document
/// yields the default (no URL, unlocked) rather than breaking the app.
pub fn parse_runtime(json: &str) -> Runtime {
    #[derive(serde::Deserialize)]
    struct Doc {
        #[serde(default)]
        backend_url: Option<String>,
        #[serde(default)]
        lock: Option<bool>,
    }
    let Ok(doc) = serde_json::from_str::<Doc>(json) else {
        return Runtime::default();
    };
    let backend_url = clean(doc.backend_url.as_deref().unwrap_or(""));
    // A configured URL is locked unless the file explicitly says otherwise:
    // the safe default for a deployment.
    let lock = doc.lock.unwrap_or(true) && backend_url.is_some();
    Runtime { backend_url, lock }
}

/// The configuration in effect: runtime file first, then build-time values.
fn effective() -> Runtime {
    let rt = runtime();
    if rt.backend_url.is_some() {
        return rt;
    }
    if let Some(url) = cfg_value(option_env!("BACKEND_URL")) {
        return Runtime {
            backend_url: Some(url),
            lock: true,
        };
    }
    if let Some(url) = cfg_value(option_env!("CHEFDAV_DAV_URL")) {
        return Runtime {
            backend_url: Some(url),
            lock: false,
        };
    }
    Runtime::default()
}

/// The server the login screen starts with (locked URL, editable default, or
/// the local dev stack).
pub fn initial_server() -> String {
    match effective().backend_url {
        Some(url) => url,
        None => "http://127.0.0.1:8080".to_string(),
    }
}

/// When locked, the app never asks for (or accepts) another server.
pub fn is_locked() -> bool {
    effective().lock
}

fn cfg_value(value: Option<&'static str>) -> Option<String> {
    value.and_then(clean)
}

/// Trim a configuration value; blanks count as unset.
pub(crate) fn clean(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty()).then(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_blank_values_are_not_set() {
        assert_eq!(clean(""), None);
        assert_eq!(clean("   "), None);
        assert_eq!(clean(" https://dav.example.org ").as_deref(), Some("https://dav.example.org"));
    }

    #[test]
    fn a_url_in_config_json_locks_by_default() {
        let r = parse_runtime(r#"{"backend_url":"https://dav.example.org"}"#);
        assert_eq!(r.backend_url.as_deref(), Some("https://dav.example.org"));
        assert!(r.lock);
    }

    #[test]
    fn lock_can_be_turned_off_explicitly() {
        let r = parse_runtime(r#"{"backend_url":"https://dav.example.org","lock":false}"#);
        assert!(!r.lock);
        assert_eq!(r.backend_url.as_deref(), Some("https://dav.example.org"));
    }

    #[test]
    fn blank_url_means_unlocked_and_user_entered() {
        for doc in [r#"{"backend_url":""}"#, r#"{"backend_url":"   "}"#] {
            let r = parse_runtime(doc);
            assert_eq!(r.backend_url, None, "{doc}");
            assert!(!r.lock, "{doc}");
        }
    }

    #[test]
    fn malformed_config_is_ignored_not_fatal() {
        assert_eq!(parse_runtime("this is not json"), Runtime::default());
        assert_eq!(parse_runtime(r#"{"backend_url":123}"#), Runtime::default());
    }

    #[test]
    fn runtime_config_drives_the_login_screen() {
        // what the container writes when BACKEND_URL is set …
        set_runtime(parse_runtime(r#"{"backend_url":"https://a.example","lock":true}"#));
        assert_eq!(initial_server(), "https://a.example");
        assert!(is_locked());
        // … and when it is not: an editable field, nothing locked.
        set_runtime(parse_runtime(r#"{"backend_url":""}"#));
        assert!(!is_locked());
    }
}
