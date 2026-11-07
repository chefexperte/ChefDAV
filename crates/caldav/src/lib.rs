//! Pure-Rust CalDAV + iCalendar core for ChefDAV.
//!
//! Everything in this crate is platform-independent and unit-testable on a
//! native target: WebDAV XML, ICS parsing/serialization, recurrence expansion,
//! calendar math, credential encryption and the high-level CalDAV client.
//! The web layer only has to provide an [`transport::HttpTransport`]
//! implementation (fetch/reqwest in the browser, a mock in tests).

pub mod client;
pub mod crypto;
pub mod error;
pub mod ics;
pub mod recurrence;
pub mod time;
pub mod transport;
pub mod xml;

pub use error::{DavError, Result};
