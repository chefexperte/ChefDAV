//! Compile-time configuration.
//!
//! `CHEFDAV_DAV_URL` lets a build default to a specific CalDAV endpoint — the
//! stable instance uses this to point at its own Radicale (port 8081).

pub const DEFAULT_SERVER: &str = match option_env!("CHEFDAV_DAV_URL") {
    Some(url) => url,
    None => "http://127.0.0.1:8080",
};
