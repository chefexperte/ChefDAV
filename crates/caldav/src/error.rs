use thiserror::Error;

#[derive(Debug, Error)]
pub enum DavError {
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("XML parse error: {0}")]
    Xml(String),
    #[error("ICS parse error: {0}")]
    Ics(String),
    #[error("not found on server: {0}")]
    NotFound(String),
    #[error("unauthorized (401/403) — check username/password")]
    Unauthorized,
    #[error("conflict (412): the event changed on the server, please reload")]
    Conflict,
    #[error("recurrence error: {0}")]
    Recurrence(String),
}

pub type Result<T> = std::result::Result<T, DavError>;
