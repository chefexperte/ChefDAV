//! Transport abstraction: the only piece that touches the network.
//!
//! The web app implements this with `fetch` (via reqwest's wasm backend);
//! tests implement it with canned responses. Because WASM futures are not
//! `Send`, the trait uses native async-fn-in-trait (RPITIT) with no `Send`
//! bound, and clients are generic over the transport instead of boxed.

use std::future::Future;

use http::Method;
use url::Url;

use crate::error::Result;

/// Non-standard HTTP methods used by WebDAV/CalDAV.
pub fn dav_method(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).expect("valid HTTP method token")
}

#[derive(Debug, Clone)]
pub struct DavRequest {
    pub method: Method,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

impl DavRequest {
    pub fn new(method: Method, url: Url) -> Self {
        Self {
            method,
            url,
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct DavResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl DavResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn etag(&self) -> Option<&str> {
        self.header("etag")
    }
}

/// One HTTP round trip. Implemented by the browser (fetch) and by mocks.
pub trait HttpTransport {
    fn request(&self, req: DavRequest) -> impl Future<Output = Result<DavResponse>>;
}
