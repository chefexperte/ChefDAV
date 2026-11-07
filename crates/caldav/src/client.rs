//! High-level CalDAV client. Generic over [`HttpTransport`], so the exact same
//! code runs against a real server (browser fetch) and against mocks in tests.

use base64::Engine;
use chrono::{DateTime, Utc};
use http::Method;
use url::Url;

use crate::error::{DavError, Result};
use crate::ics::{parse_calendar, VEvent};
use crate::transport::{dav_method, DavRequest, DavResponse, HttpTransport};
use crate::xml;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CalendarInfo {
    /// Absolute URL of the calendar collection.
    pub href: String,
    pub display_name: String,
    pub color: Option<String>,
    /// sync-token or collection ETag, used for incremental sync.
    pub ctag: Option<String>,
    /// The server says the current user may not write this collection.
    #[serde(default)]
    pub read_only: bool,
    /// Component types the collection accepts (VEVENT, VTODO, …).
    #[serde(default)]
    pub components: Vec<String>,
}

impl CalendarInfo {
    pub fn supports_events(&self) -> bool {
        self.components.is_empty() || self.components.iter().any(|c| c == "VEVENT")
    }
    pub fn supports_todos(&self) -> bool {
        self.components.is_empty() || self.components.iter().any(|c| c == "VTODO")
    }
}

#[derive(Debug, Clone)]
pub struct DavEvent {
    pub href: String,
    pub etag: Option<String>,
    pub event: VEvent,
}

#[derive(Debug, Clone)]
pub struct DavTodo {
    pub href: String,
    pub etag: Option<String>,
    pub todo: crate::ics::VTodo,
}

/// Result of a `sync-collection` REPORT.
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    /// Resources created or changed since the token.
    pub changed: Vec<String>,
    /// Resources deleted since the token.
    pub deleted: Vec<String>,
    /// Token to use for the next sync.
    pub new_token: Option<String>,
}

pub struct DavClient<T: HttpTransport> {
    transport: T,
    server: Url,
    auth: String,
}

/// Absolute URL of the resource for an event/task UID in a calendar.
pub fn resource_url(calendar: &Url, uid: &str) -> Result<Url> {
    Ok(calendar.join(&format!("{}.ics", encode_segment(uid)))?)
}

/// Percent-encode a single path segment (for MKCALENDAR collection names).
fn encode_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl<T: HttpTransport> DavClient<T> {
    pub fn new(transport: T, server: Url, username: &str, password: &str) -> Self {
        let token =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        Self {
            transport,
            server,
            auth: format!("Basic {token}"),
        }
    }

    pub fn server(&self) -> &Url {
        &self.server
    }

    /// Resolve a server-sent href (absolute path or URL) against the server root.
    fn url(&self, href: &str) -> Result<Url> {
        Ok(self.server.join(href)?)
    }

    fn request(
        &self,
        method: Method,
        url: Url,
        depth: Option<u8>,
        body: Option<String>,
    ) -> DavRequest {
        self.request_typed(method, url, depth, body, "application/xml; charset=utf-8")
    }

    /// Like [`Self::request`] but with an explicit Content-Type. Exactly one
    /// Content-Type header is ever sent (duplicates get comma-joined by the
    /// browser and rejected by Radicale).
    fn request_typed(
        &self,
        method: Method,
        url: Url,
        depth: Option<u8>,
        body: Option<String>,
        content_type: &str,
    ) -> DavRequest {
        let mut req = DavRequest::new(method, url).header("Authorization", &self.auth);
        if let Some(d) = depth {
            req = req.header("Depth", &d.to_string());
        }
        if let Some(b) = body {
            req = req.header("Content-Type", content_type).body(b);
        }
        req
    }

    async fn send(&self, req: DavRequest) -> Result<DavResponse> {
        let resp = self.transport.request(req).await?;
        match resp.status {
            401 | 403 => Err(DavError::Unauthorized),
            412 => Err(DavError::Conflict),
            s if (200..300).contains(&s) => Ok(resp),
            s => Err(DavError::Http {
                status: s,
                body: resp.body,
            }),
        }
    }

    /// Cheap liveness + credentials check: PROPFIND depth 0 on the root.
    pub async fn check(&self) -> Result<()> {
        let req = self.request(
            dav_method("PROPFIND"),
            self.server.clone(),
            Some(0),
            Some(xml::propfind_root_body()),
        );
        self.send(req).await.map(|_| ())
    }

    /// Discover the current-user-principal URL.
    pub async fn current_principal(&self) -> Result<Url> {
        let req = self.request(
            dav_method("PROPFIND"),
            self.server.clone(),
            Some(0),
            Some(xml::propfind_root_body()),
        );
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let href = ms
            .iter()
            .find_map(|r| r.prop(xml::NS_DAV, "current-user-principal"))
            .ok_or_else(|| DavError::NotFound("current-user-principal".into()))?;
        Ok(self.url(href)?)
    }

    /// Discover the calendar-home-set of a principal.
    pub async fn calendar_home(&self, principal: &Url) -> Result<Url> {
        let req = self.request(
            dav_method("PROPFIND"),
            principal.clone(),
            Some(0),
            Some(xml::propfind_principal_body()),
        );
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let href = ms
            .iter()
            .find_map(|r| r.prop(xml::NS_CALDAV, "calendar-home-set"))
            .ok_or_else(|| DavError::NotFound("calendar-home-set".into()))?;
        Ok(self.url(href)?)
    }

    /// List all calendar collections in the given home.
    pub async fn list_calendars(&self, home: &Url) -> Result<Vec<CalendarInfo>> {
        let req = self.request(
            dav_method("PROPFIND"),
            home.clone(),
            Some(1),
            Some(xml::propfind_calendars_body()),
        );
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let mut out = Vec::new();
        for r in &ms {
            if !r.is_calendar {
                continue;
            }
            out.push(CalendarInfo {
                href: self.url(&r.href)?.to_string(),
                display_name: r
                    .prop(xml::NS_DAV, "displayname")
                    .unwrap_or("Calendar")
                    .to_string(),
                color: r.prop(xml::NS_APPLE_ICAL, "calendar-color").map(str::to_string),
                ctag: r
                    .prop(xml::NS_DAV, "sync-token")
                    .or_else(|| r.prop(xml::NS_DAV, "getetag"))
                    .map(str::to_string),
                read_only: !r.can_write(),
                components: r.components.clone(),
            });
        }
        Ok(out)
    }

    /// REPORT calendar-query: all VEVENTs overlapping [start, end).
    pub async fn query_events(
        &self,
        calendar: &Url,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        default_tz: &chrono_tz::Tz,
    ) -> Result<Vec<DavEvent>> {
        let body = xml::calendar_query_body(start, end);
        let req = self.request(dav_method("REPORT"), calendar.clone(), Some(0), Some(body));
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let mut out = Vec::new();
        for r in &ms {
            let Some(data) = r.prop(xml::NS_CALDAV, "calendar-data") else {
                continue;
            };
            // One malformed resource must not blank the whole calendar: skip it
            // and keep the rest.
            let Ok(events) = parse_calendar(data, default_tz) else {
                continue;
            };
            for event in events {
                out.push(DavEvent {
                    href: self.url(&r.href)?.to_string(),
                    etag: r.prop(xml::NS_DAV, "getetag").map(str::to_string),
                    event,
                });
            }
        }
        Ok(out)
    }

    /// Create or update an event. `etag` enables optimistic concurrency via
    /// If-Match (412 → [`DavError::Conflict`]). Returns the new ETag.
    pub async fn put_event(
        &self,
        calendar: &Url,
        uid: &str,
        ics: &str,
        etag: Option<&str>,
    ) -> Result<String> {
        let url = resource_url(calendar, uid)?;
        self.put_resource(&url, ics, etag).await
    }

    /// PUT an event/task body to an exact resource URL (used when the
    /// server-side filename differs from the UID).
    pub async fn put_resource(&self, url: &Url, ics: &str, etag: Option<&str>) -> Result<String> {
        let mut req = self.request_typed(
            Method::PUT,
            url.clone(),
            None,
            Some(ics.to_string()),
            "text/calendar; charset=utf-8",
        );
        if let Some(etag) = etag {
            req = req.header("If-Match", etag);
        }
        let resp = self.send(req).await?;
        Ok(resp.etag().unwrap_or("").to_string())
    }

    /// Delete an event resource.
    pub async fn delete_event(&self, href: &Url, etag: Option<&str>) -> Result<()> {
        let mut req = self.request(Method::DELETE, href.clone(), None, None);
        if let Some(e) = etag {
            req = req.header("If-Match", e);
        }
        self.send(req).await.map(|_| ())
    }

    /// Rename a calendar collection (PROPPATCH displayname).
    pub async fn rename_calendar(&self, calendar: &Url, name: &str) -> Result<()> {
        let req = self.request(
            dav_method("PROPPATCH"),
            calendar.clone(),
            None,
            Some(xml::proppatch_displayname_body(name)),
        );
        self.send(req).await.map(|_| ())
    }

    /// Set the Apple calendar color of a collection (PROPPATCH).
    pub async fn set_calendar_color(&self, calendar: &Url, color: &str) -> Result<()> {
        let req = self.request(
            dav_method("PROPPATCH"),
            calendar.clone(),
            None,
            Some(xml::proppatch_calendar_color_body(color)),
        );
        self.send(req).await.map(|_| ())
    }

    /// Delete a whole calendar collection.
    pub async fn delete_calendar(&self, calendar: &Url) -> Result<()> {
        let req = self.request(Method::DELETE, calendar.clone(), None, None);
        self.send(req).await.map(|_| ())
    }

    /// REPORT calendar-query for every VEVENT in a collection (no time range).
    pub async fn query_all_events(
        &self,
        calendar: &Url,
        default_tz: &chrono_tz::Tz,
    ) -> Result<Vec<DavEvent>> {
        let req = self.request(
            dav_method("REPORT"),
            calendar.clone(),
            Some(0),
            Some(xml::calendar_query_all_body()),
        );
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let mut out = Vec::new();
        for r in &ms {
            let Some(data) = r.prop(xml::NS_CALDAV, "calendar-data") else {
                continue;
            };
            let Ok(events) = parse_calendar(data, default_tz) else {
                continue;
            };
            for event in events {
                out.push(DavEvent {
                    href: self.url(&r.href)?.to_string(),
                    etag: r.prop(xml::NS_DAV, "getetag").map(str::to_string),
                    event,
                });
            }
        }
        Ok(out)
    }

    /// REPORT calendar-query for all VTODOs in a collection.
    pub async fn query_todos(
        &self,
        calendar: &Url,
        default_tz: &chrono_tz::Tz,
    ) -> Result<Vec<DavTodo>> {
        let req = self.request(
            dav_method("REPORT"),
            calendar.clone(),
            Some(0),
            Some(xml::calendar_query_todos_body()),
        );
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let mut out = Vec::new();
        for r in &ms {
            let Some(data) = r.prop(xml::NS_CALDAV, "calendar-data") else {
                continue;
            };
            let Ok(todos) = crate::ics::parse_todos(data, default_tz) else {
                continue;
            };
            for todo in todos {
                out.push(DavTodo {
                    href: self.url(&r.href)?.to_string(),
                    etag: r.prop(xml::NS_DAV, "getetag").map(str::to_string),
                    todo,
                });
            }
        }
        Ok(out)
    }

    /// Create or update a task resource.
    pub async fn put_todo(
        &self,
        calendar: &Url,
        uid: &str,
        ics: &str,
        etag: Option<&str>,
    ) -> Result<String> {
        let url = resource_url(calendar, uid)?;
        let mut req = self.request_typed(
            Method::PUT,
            url,
            None,
            Some(ics.to_string()),
            "text/calendar; charset=utf-8",
        );
        if let Some(etag) = etag {
            req = req.header("If-Match", etag);
        }
        let resp = self.send(req).await?;
        Ok(resp.etag().unwrap_or("").to_string())
    }

    /// REPORT sync-collection: list changed/deleted resources since `token`.
    pub async fn sync_collection(&self, calendar: &Url, token: Option<&str>) -> Result<SyncResult> {
        let req = self.request(
            dav_method("REPORT"),
            calendar.clone(),
            Some(0),
            Some(xml::sync_collection_body(token)),
        );
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let mut result = SyncResult {
            new_token: xml::parse_sync_token(&resp.body),
            ..Default::default()
        };
        for r in &ms {
            // A response-level 404 means the resource was deleted.
            if r.status == Some(404) {
                result.deleted.push(self.url(&r.href)?.to_string());
            } else if r.prop(xml::NS_DAV, "getetag").is_some() {
                result.changed.push(self.url(&r.href)?.to_string());
            }
        }
        Ok(result)
    }

    /// REPORT calendar-multiget: fetch several event resources in one request.
    pub async fn multiget_events(
        &self,
        calendar: &Url,
        hrefs: &[String],
        default_tz: &chrono_tz::Tz,
    ) -> Result<Vec<DavEvent>> {
        if hrefs.is_empty() {
            return Ok(Vec::new());
        }
        let req = self.request(
            dav_method("REPORT"),
            calendar.clone(),
            Some(0),
            Some(xml::calendar_multiget_body(hrefs)),
        );
        let resp = self.send(req).await?;
        let ms = xml::parse_multistatus(&resp.body)?;
        let mut out = Vec::new();
        for r in &ms {
            let Some(data) = r.prop(xml::NS_CALDAV, "calendar-data") else {
                continue;
            };
            let Ok(events) = parse_calendar(data, default_tz) else {
                continue;
            };
            for event in events {
                out.push(DavEvent {
                    href: self.url(&r.href)?.to_string(),
                    etag: r.prop(xml::NS_DAV, "getetag").map(str::to_string),
                    event,
                });
            }
        }
        Ok(out)
    }

    /// Create a new calendar collection (MKCALENDAR) inside `home`.
    pub async fn create_calendar(&self, home: &Url, name: &str) -> Result<Url> {
        let url = home.join(&format!("{}/", encode_segment(name)))?;
        let req = self.request(
            dav_method("MKCALENDAR"),
            url.clone(),
            None,
            Some(xml::mkcalendar_body(name)),
        );
        self.send(req).await?;
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::HttpTransport;
    use futures::executor::block_on;

    struct Route {
        method: &'static str,
        path: &'static str,
        depth: Option<&'static str>,
        body_contains: Option<&'static str>,
        status: u16,
        headers: Vec<(&'static str, &'static str)>,
        body: String,
    }

    fn route(
        method: &'static str,
        path: &'static str,
        depth: Option<&'static str>,
        body_contains: Option<&'static str>,
        status: u16,
        body: &str,
    ) -> Route {
        Route {
            method,
            path,
            depth,
            body_contains,
            status,
            headers: vec![],
            body: body.to_string(),
        }
    }

    struct Mock {
        routes: Vec<Route>,
        last_request: std::cell::RefCell<Option<DavRequest>>,
    }

    impl Mock {
        fn new() -> Self {
            let calendars = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
            <D:response><D:href>/alice/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
            <D:response><D:href>/alice/personal/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname>Personal</D:displayname><D:getetag>"ctag1"</D:getetag><D:sync-token>http://radicale.org/ns/sync/1</D:sync-token><A:calendar-color>#e4572e</A:calendar-color><D:current-user-privilege-set><D:privilege><D:read/></D:privilege><D:privilege><D:write/></D:privilege></D:current-user-privilege-set><C:supported-calendar-component-set><C:comp name="VEVENT"/></C:supported-calendar-component-set></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
            <D:response><D:href>/alice/readonly/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname>Shared</D:displayname><D:current-user-privilege-set><D:privilege><D:read/></D:privilege></D:current-user-privilege-set><C:supported-calendar-component-set><C:comp name="VEVENT"/></C:supported-calendar-component-set></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
            <D:response><D:href>/alice/tasks/</D:href><D:propstat><D:prop><D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname>Tasks</D:displayname><D:current-user-privilege-set><D:privilege><D:read/></D:privilege><D:privilege><D:write/></D:privilege></D:current-user-privilege-set><C:supported-calendar-component-set><C:comp name="VTODO"/></C:supported-calendar-component-set></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
            </D:multistatus>"#;
            let events = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
            <D:response><D:href>/alice/personal/evt1.ics</D:href><D:propstat><D:prop><D:getetag>"e1"</D:getetag><C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:evt1@x
DTSTART;TZID=Europe/Berlin:20261110T100000
DTEND;TZID=Europe/Berlin:20261110T110000
SUMMARY:Standup
END:VEVENT
END:VCALENDAR</C:calendar-data></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
            </D:multistatus>"#;
            let sync = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:">
            <D:response><D:href>/alice/personal/evt1.ics</D:href><D:propstat><D:prop><D:getetag>"e2"</D:getetag></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
            <D:response><D:href>/alice/personal/evt2.ics</D:href><D:status>HTTP/1.1 404 Not Found</D:status></D:response>
            <D:sync-token>http://radicale.org/ns/sync/2</D:sync-token>
            </D:multistatus>"#;
            let todos = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
            <D:response><D:href>/alice/tasks/todo1.ics</D:href><D:propstat><D:prop><D:getetag>"t1"</D:getetag><C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VTODO
UID:todo1@x
SUMMARY:Buy milk
DUE;VALUE=DATE:20261109
STATUS:NEEDS-ACTION
END:VTODO
END:VCALENDAR</C:calendar-data></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>
            </D:multistatus>"#;

            let mut routes = vec![
                route("PROPFIND", "/", Some("0"), None, 207, r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>/</D:href><D:propstat><D:prop><D:current-user-principal><D:href>/alice/</D:href></D:current-user-principal></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#),
                route("PROPFIND", "/alice/", Some("0"), None, 207, r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:response><D:href>/alice/</D:href><D:propstat><D:prop><C:calendar-home-set><D:href>/alice/</D:href></C:calendar-home-set></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#),
                route("PROPFIND", "/alice/", Some("1"), None, 207, calendars),
                route("REPORT", "/alice/personal/", Some("0"), Some("time-range"), 207, events),
                route("REPORT", "/alice/personal/", Some("0"), Some("calendar-query"), 207, events),
                route("REPORT", "/alice/personal/", Some("0"), Some("sync-collection"), 207, sync),
                route("REPORT", "/alice/personal/", Some("0"), Some("calendar-multiget"), 207, events),
                route("REPORT", "/alice/tasks/", Some("0"), Some("VTODO"), 207, todos),
                route("PUT", "/alice/personal/new1%40chef-dav.ics", None, None, 201, ""),
                route("PUT", "/alice/tasks/todo1%40chef-dav.ics", None, None, 201, ""),
                route("DELETE", "/alice/personal/evt1.ics", None, None, 204, ""),
                route("PROPPATCH", "/alice/personal/", None, None, 207, ""),
                route("DELETE", "/alice/personal/", None, None, 204, ""),
                route("PROPFIND", "/unauthorized/", Some("1"), None, 401, ""),
            ];
            routes[8].headers = vec![("ETag", "\"new-etag\"")];
            routes[9].headers = vec![("ETag", "\"todo-etag\"")];
            Mock {
                routes,
                last_request: std::cell::RefCell::new(None),
            }
        }

        fn last(&self) -> DavRequest {
            self.last_request.borrow().clone().expect("a request was made")
        }
    }

    impl HttpTransport for Mock {
        fn request(&self, req: DavRequest) -> impl std::future::Future<Output = Result<DavResponse>> {
            let method = req.method.as_str().to_string();
            let path = req.url.path().to_string();
            let depth = req
                .headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("depth"))
                .map(|(_, v)| v.clone());
            let body = req.body.clone().unwrap_or_default();
            *self.last_request.borrow_mut() = Some(req.clone());
            let found = self
                .routes
                .iter()
                .find(|r| {
                    r.method == method
                        && r.path == path
                        && r.depth.map(str::to_string) == depth
                        && r.body_contains
                            .map(|needle| body.contains(needle))
                            .unwrap_or(true)
                })
                .map(|r| {
                    (
                        r.status,
                        r.headers
                            .iter()
                            .map(|(k, v)| (k.to_string(), v.to_string()))
                            .collect::<Vec<_>>(),
                        r.body.clone(),
                    )
                });
            async move {
                match found {
                    Some((status, headers, body)) => Ok(DavResponse {
                        status,
                        headers,
                        body,
                    }),
                    None => Ok(DavResponse {
                        status: 404,
                        headers: vec![],
                        body: "not found".into(),
                    }),
                }
            }
        }
    }

    fn client() -> DavClient<Mock> {
        DavClient::new(
            Mock::new(),
            Url::parse("http://cal.test/").unwrap(),
            "alice",
            "secret",
        )
    }

    #[test]
    fn discovery_flow() {
        let c = client();
        block_on(async {
            c.check().await.unwrap();
            let principal = c.current_principal().await.unwrap();
            assert_eq!(principal.as_str(), "http://cal.test/alice/");
            let home = c.calendar_home(&principal).await.unwrap();
            assert_eq!(home.as_str(), "http://cal.test/alice/");
        });
    }

    #[test]
    fn lists_only_calendar_collections() {
        let c = client();
        block_on(async {
            let home = Url::parse("http://cal.test/alice/").unwrap();
            let cals = c.list_calendars(&home).await.unwrap();
            assert_eq!(cals.len(), 3);
            assert_eq!(cals[0].display_name, "Personal");
            assert_eq!(cals[0].href, "http://cal.test/alice/personal/");
            assert_eq!(cals[0].color.as_deref(), Some("#e4572e"));
            assert_eq!(cals[0].ctag.as_deref(), Some("http://radicale.org/ns/sync/1"));
            assert!(!cals[0].read_only);
            assert!(cals[0].supports_events());
            assert!(cals[1].read_only, "Shared must be read-only");
            assert!(cals[2].supports_todos());
        });
    }

    #[test]
    fn sync_collection_reports_changes_and_deletions() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            let r = c
                .sync_collection(&cal, Some("http://radicale.org/ns/sync/1"))
                .await
                .unwrap();
            assert_eq!(r.changed, vec!["http://cal.test/alice/personal/evt1.ics"]);
            assert_eq!(r.deleted, vec!["http://cal.test/alice/personal/evt2.ics"]);
            assert_eq!(r.new_token.as_deref(), Some("http://radicale.org/ns/sync/2"));
        });
    }

    #[test]
    fn query_all_events_works() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            let evs = c.query_all_events(&cal, &chrono_tz::Europe::Berlin).await.unwrap();
            assert_eq!(evs.len(), 1);
            assert_eq!(evs[0].event.summary, "Standup");
        });
    }

    #[test]
    fn multiget_fetches_events() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            let evs = c
                .multiget_events(
                    &cal,
                    &["/alice/personal/evt1.ics".to_string()],
                    &chrono_tz::Europe::Berlin,
                )
                .await
                .unwrap();
            assert_eq!(evs.len(), 1);
            assert_eq!(evs[0].event.summary, "Standup");
        });
    }

    #[test]
    fn queries_todos() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/tasks/").unwrap();
            let todos = c.query_todos(&cal, &chrono_tz::Europe::Berlin).await.unwrap();
            assert_eq!(todos.len(), 1);
            assert_eq!(todos[0].todo.summary, "Buy milk");
            assert_eq!(todos[0].etag.as_deref(), Some("\"t1\""));
        });
    }

    #[test]
    fn put_todo_returns_etag() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/tasks/").unwrap();
            let etag = c
                .put_todo(&cal, "todo1@chef-dav", "BEGIN:VCALENDAR\nEND:VCALENDAR", None)
                .await
                .unwrap();
            assert_eq!(etag, "\"todo-etag\"");
        });
    }

    #[test]
    fn sets_calendar_color() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            c.set_calendar_color(&cal, "#d1495b").await.unwrap();
            let body = c.transport.last().body.unwrap();
            assert!(body.contains("calendar-color"));
            assert!(body.contains("#d1495b"));
        });
    }

    #[test]
    fn rename_and_delete_calendar() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            c.rename_calendar(&cal, "Renamed").await.unwrap();
            assert!(c.transport.last().body.unwrap().contains("Renamed"));
            c.delete_calendar(&cal).await.unwrap();
        });
    }

    #[test]
    fn queries_and_parses_events() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            let start = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z").unwrap().with_timezone(&Utc);
            let end = DateTime::parse_from_rfc3339("2026-12-01T00:00:00Z").unwrap().with_timezone(&Utc);
            let evs = c.query_events(&cal, start, end, &chrono_tz::Europe::Berlin).await.unwrap();
            assert_eq!(evs.len(), 1);
            assert_eq!(evs[0].event.summary, "Standup");
            assert_eq!(evs[0].etag.as_deref(), Some("\"e1\""));
            assert_eq!(evs[0].href, "http://cal.test/alice/personal/evt1.ics");
        });
    }

    #[test]
    fn put_returns_etag_and_delete_works() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            let etag = c
                .put_event(&cal, "new1@chef-dav", "BEGIN:VCALENDAR\nEND:VCALENDAR", None)
                .await
                .unwrap();
            assert_eq!(etag, "\"new-etag\"");
            let href = Url::parse("http://cal.test/alice/personal/evt1.ics").unwrap();
            c.delete_event(&href, Some("\"e1\"")).await.unwrap();
        });
    }

    /// Regression: a PUT must send exactly one Content-Type header. Two of them
    /// get comma-joined by fetch and Radicale rejects with
    /// "unknown encoding: utf-8, text/calendar".
    #[test]
    fn put_sends_exactly_one_content_type() {
        let c = client();
        block_on(async {
            let cal = Url::parse("http://cal.test/alice/personal/").unwrap();
            c.put_event(&cal, "new1@chef-dav", "BEGIN:VCALENDAR\nEND:VCALENDAR", None)
                .await
                .unwrap();
            let req = c.transport.last();
            let content_types: Vec<&str> = req
                .headers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.as_str())
                .collect();
            assert_eq!(content_types, vec!["text/calendar; charset=utf-8"]);
        });
    }

    #[test]
    fn unauthorized_maps_to_error() {
        let c = client();
        block_on(async {
            let url = Url::parse("http://cal.test/unauthorized/").unwrap();
            let err = c.list_calendars(&url).await.unwrap_err();
            assert!(matches!(err, DavError::Unauthorized));
        });
    }
}
