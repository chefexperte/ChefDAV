//! Compile-time configuration.
//!
//! The app is a static bundle with no backend, so anything that points it at a
//! server has to be baked in when it is built. Two variables are read from the
//! build environment (trunk loads a `.env` file automatically — see
//! `.env.example`):
//!
//! - `BACKEND_URL` — the CalDAV server URL, **locked**: when set, the login
//!   screen shows it read-only and it cannot be changed in the app. This is the
//!   variable a deployment image should set.
//! - `CHEFDAV_DAV_URL` — an older name for the same thing that only supplies
//!   the *default* (the field stays editable). The stable instance uses it.
//!
//! Values are trimmed, and an empty value counts as "not set".

/// `BACKEND_URL`: the locked server URL, or `None`.
pub fn backend_url() -> Option<&'static str> {
    non_empty(option_env!("BACKEND_URL"))
}

/// `CHEFDAV_DAV_URL`: a pre-filled but still editable server URL.
pub fn default_server() -> Option<&'static str> {
    match backend_url() {
        Some(url) => Some(url),
        None => non_empty(option_env!("CHEFDAV_DAV_URL")),
    }
}

/// The server the login screen starts with: the locked backend when set,
/// otherwise the editable default, otherwise the dev stack.
pub fn initial_server() -> &'static str {
    default_server().unwrap_or("http://127.0.0.1:8080")
}

/// When a backend is locked in, the app never asks for (or accepts) another.
pub fn is_locked() -> bool {
    backend_url().is_some()
}

fn non_empty(value: Option<&'static str>) -> Option<&'static str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::non_empty;

    #[test]
    fn empty_or_blank_values_are_not_set() {
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some("")), None);
        assert_eq!(non_empty(Some("   ")), None);
        assert_eq!(non_empty(Some(" https://dav.example.org ")), Some("https://dav.example.org"));
    }
}
