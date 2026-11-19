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
//!    { "backend_url": "https://dav.example.org", "base_path": "/ui" }
//!    ```
//!
//!    The image writes this file from the `BACKEND_URL` / `BASE_PATH`
//!    environment variables on every start. A missing file (404) simply means
//!    "no runtime configuration" — e.g. plain static hosting.
//!
//! 2. **Build-time fallback** for self-hosted `dist/` builds: trunk loads a
//!    `.env` file, and `CHEFDAV_DAV_URL` (or `BACKEND_URL`) becomes the server
//!    URL. `CHEFDAV_DAV_URL` pre-fills an editable field; `BACKEND_URL` is a
//!    deployment setting, so the login screen shows no server field at all.
//!
//! Precedence: runtime file → build-time value → the local dev stack.

use std::cell::RefCell;

/// Configuration read from `/config.json` at startup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Runtime {
    /// The CalDAV server URL to use; `None` lets the user type one.
    pub backend_url: Option<String>,
    /// The URL comes from the deployment (`BACKEND_URL` / `config.json`), so
    /// the login screen does not offer a server field. A build-time
    /// `CHEFDAV_DAV_URL` is only a prefill and leaves the field editable.
    pub server_fixed: bool,
    /// URL path the app is mounted under, e.g. `/ui` when a reverse proxy maps
    /// `calendar.example.org/ui/*` here. Empty means "served at the root".
    /// The asset URLs are relative, so this only tells the router where its
    /// routes start (`/ui/month/2026-11` → `month`).
    pub base_path: String,
}

thread_local! {
    /// Installed once during startup (see `app::App`).
    static RUNTIME: RefCell<Runtime> = const { RefCell::new(Runtime {
        backend_url: None,
        server_fixed: false,
        base_path: String::new(),
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
/// yields the default (no URL, user picks the server) rather than breaking the
/// app.
pub fn parse_runtime(json: &str) -> Runtime {
    #[derive(serde::Deserialize)]
    struct Doc {
        #[serde(default)]
        backend_url: Option<String>,
        #[serde(default)]
        base_path: Option<String>,
    }
    let Ok(doc) = serde_json::from_str::<Doc>(json) else {
        return Runtime::default();
    };
    let backend_url = clean(doc.backend_url.as_deref().unwrap_or(""));
    Runtime {
        // A URL from the deployment is used as given: the login screen does
        // not offer a server field at all.
        server_fixed: backend_url.is_some(),
        backend_url,
        base_path: normalize_base_path(doc.base_path.as_deref().unwrap_or("")),
    }
}

/// Turn anything a user might write (`ui`, `/ui`, `ui/`, `//ui/`) into `/ui`,
/// and "" / "/" into "" (served at the root).
pub fn normalize_base_path(raw: &str) -> String {
    let t = raw.trim().trim_matches('/');
    if t.is_empty() {
        String::new()
    } else {
        format!("/{}", t.trim_end_matches('/'))
    }
}

/// Guess the mount path from the address bar, for a deployment that forgot to
/// set `BASE_PATH`: `/ui/month/2026-11` → `/ui`, `/ui/` → `/ui`, `/` → "".
///
/// Only used for the *first* load; after that the app owns the URL. Anything
/// that is not a known route is treated as the mount path, which is what an
/// operator means by "the app lives under /ui".
pub fn detect_base_path(pathname: &str) -> String {
    const ROUTES: [&str; 9] = [
        "month", "week", "day", "list", "tasks", "event", "task", "login", "",
    ];
    let trimmed = pathname.trim_start_matches('/');
    let first = trimmed.split('/').next().unwrap_or("");
    if ROUTES.contains(&first) {
        String::new()
    } else {
        normalize_base_path(first)
    }
}

/// The router base: `/ui` for a `/ui` mount, "" at the root.
///
/// **No trailing slash**: Leptos joins it as `base + normalize(path)` without
/// trimming, so `/ui/` would produce `/ui//month/2026-11`. It strips the prefix
/// before matching routes.
pub fn router_base() -> String {
    runtime().base_path
}

/// Asset URL for the startup splash, before `/config.json` has been read:
/// the mount path is inferred from the address bar. A root-absolute
/// `/logo.svg` on a `/ui` mount would otherwise hit the CalDAV server at the
/// origin root, which answers 401 and makes the browser open its Basic auth
/// dialog.
pub fn splash_asset_url(file: &str) -> String {
    let base = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .map(|path| detect_base_path(&path))
        .unwrap_or_default();
    format!("{base}/{}", file.trim_start_matches('/'))
}

/// URL for an asset that lives next to `index.html` (`logo.svg`, `fonts/…`).
///
/// Anchored at the mount path, not at the current route: a relative `logo.svg`
/// on `/ui/month/2026-11` would resolve to `/ui/month/logo.svg`. When the mount
/// path is unknown (""), a root-absolute URL is right — the app is at the root.
pub fn asset_url(file: &str) -> String {
    format!("{}/{}", runtime().base_path, file.trim_start_matches('/'))
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
            server_fixed: true,
            base_path: rt.base_path,
        };
    }
    if let Some(url) = cfg_value(option_env!("CHEFDAV_DAV_URL")) {
        return Runtime {
            backend_url: Some(url),
            server_fixed: false,
            base_path: rt.base_path,
        };
    }
    // Nothing configured at all (e.g. the container started without
    // BACKEND_URL): an empty field, so the user is asked rather than being
    // pointed at a localhost that only exists on a developer machine.
    Runtime {
        base_path: rt.base_path,
        ..Runtime::default()
    }
}

/// The server the login screen starts with, or `""` to leave the field empty.
///
/// Empty is the right answer for a deployment that was not given a backend:
/// the user types their own. (Developers of the dev stack get the local
/// endpoint from the placeholder text and the e2e suites fill the field.)
pub fn initial_server() -> String {
    effective().backend_url.unwrap_or_default()
}

/// The server is part of the deployment: the login screen shows no server
/// field, and the user only enters a username and password.
pub fn server_is_fixed() -> bool {
    effective().server_fixed
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
    fn a_url_in_config_json_is_a_fixed_deployment_setting() {
        let r = parse_runtime(r#"{"backend_url":"https://dav.example.org"}"#);
        assert_eq!(r.backend_url.as_deref(), Some("https://dav.example.org"));
        assert!(r.server_fixed);
    }

    #[test]
    fn a_legacy_lock_key_is_ignored() {
        // config.json used to carry `"lock"`; an old file must not make the
        // server editable again — a configured URL always hides the field.
        let r = parse_runtime(r#"{"backend_url":"https://dav.example.org","lock":false}"#);
        assert_eq!(r.backend_url.as_deref(), Some("https://dav.example.org"));
        assert!(r.server_fixed);
    }

    #[test]
    fn blank_url_means_the_user_enters_it() {
        for doc in [r#"{"backend_url":""}"#, r#"{"backend_url":"   "}"#] {
            let r = parse_runtime(doc);
            assert_eq!(r.backend_url, None, "{doc}");
            assert!(!r.server_fixed, "{doc}");
        }
    }

    #[test]
    fn malformed_config_is_ignored_not_fatal() {
        assert_eq!(parse_runtime("this is not json"), Runtime::default());
        assert_eq!(parse_runtime(r#"{"backend_url":123}"#), Runtime::default());
    }

    #[test]
    fn base_paths_are_normalised() {
        assert_eq!(normalize_base_path(""), "");
        assert_eq!(normalize_base_path("/"), "");
        assert_eq!(normalize_base_path("ui"), "/ui");
        assert_eq!(normalize_base_path("/ui"), "/ui");
        assert_eq!(normalize_base_path("ui/"), "/ui");
        assert_eq!(normalize_base_path("//ui//"), "/ui");
        assert_eq!(normalize_base_path("/calendar/ui"), "/calendar/ui");
    }

    #[test]
    fn base_path_is_read_from_config_json() {
        let r = parse_runtime(r#"{"backend_url":"https://a.example","base_path":"/ui"}"#);
        assert_eq!(r.base_path, "/ui");
    }

    #[test]
    fn mount_path_is_detected_from_the_first_segment() {
        assert_eq!(detect_base_path("/"), "");
        assert_eq!(detect_base_path("/month/2026-11"), "");
        assert_eq!(detect_base_path("/week/2026-11-16"), "");
        assert_eq!(detect_base_path("/ui"), "/ui");
        assert_eq!(detect_base_path("/ui/"), "/ui");
        assert_eq!(detect_base_path("/ui/month/2026-11"), "/ui");
        // one mount level only; a deeper prefix must be configured explicitly
        assert_eq!(detect_base_path("/calendar/ui/week"), "/calendar");
    }

    #[test]
    fn runtime_config_drives_the_login_screen() {
        // what the container writes when BACKEND_URL is set: the server is
        // fixed and the field is not offered …
        set_runtime(parse_runtime(r#"{"backend_url":"https://a.example"}"#));
        assert_eq!(initial_server(), "https://a.example");
        assert!(server_is_fixed());
        // … and when it is not: the user types the server themselves.
        set_runtime(parse_runtime(r#"{"backend_url":""}"#));
        assert_eq!(initial_server(), "");
        assert!(!server_is_fixed());
    }
}
