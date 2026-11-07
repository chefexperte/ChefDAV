//! WebDAV XML: building request bodies (PROPFIND / REPORT / MKCALENDAR) and
//! parsing `multistatus` responses. Parsing uses roxmltree (parse-only, so we
//! extract everything we need while walking the tree).

use chrono::{DateTime, Utc};

use crate::error::{DavError, Result};

pub const NS_DAV: &str = "DAV:";
pub const NS_CALDAV: &str = "urn:ietf:params:xml:ns:caldav";
pub const NS_APPLE_ICAL: &str = "http://apple.com/ns/ical/";

const XML_HEADER: &str = r#"<?xml version="1.0" encoding="utf-8"?>"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop {
    pub ns: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct MultistatusResponse {
    /// `href` of the resource (path or absolute URL, as sent by the server).
    pub href: String,
    pub props: Vec<Prop>,
    pub is_collection: bool,
    pub is_calendar: bool,
    /// Privileges granted on this resource (from `current-user-privilege-set`),
    /// e.g. `["read", "write"]`.
    pub privileges: Vec<String>,
    /// Supported component types (VEVENT, VTODO, …).
    pub components: Vec<String>,
    /// Response-level status code (used by sync-collection to report 404s).
    pub status: Option<u16>,
}

impl MultistatusResponse {
    /// First property with the given namespace + local name.
    pub fn prop(&self, ns: &str, name: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|p| p.ns == ns && p.name == name)
            .map(|p| p.value.as_str())
    }

    /// True when the server says this resource may be written.
    pub fn can_write(&self) -> bool {
        self.privileges.iter().any(|p| p == "write" || p == "write-content")
    }
}

/// Parse "HTTP/1.1 404 Not Found" → 404.
fn parse_status_code(s: &str) -> Option<u16> {
    s.split_whitespace().nth(1)?.parse().ok()
}

/// All text inside a node (roxmltree has no `text_content`, so we collect).
fn text_of(node: roxmltree::Node) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}

pub fn parse_multistatus(body: &str) -> Result<Vec<MultistatusResponse>> {
    let doc = roxmltree::Document::parse(body).map_err(|e| DavError::Xml(e.to_string()))?;
    let mut out = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        if node.tag_name().name() != "response" || node.tag_name().namespace() != Some(NS_DAV) {
            continue;
        }
        let mut href = String::new();
        let mut props = Vec::new();
        let mut is_collection = false;
        let mut is_calendar = false;
        let mut privileges = Vec::new();
        let mut components = Vec::new();
        let mut status = None;
        for child in node.children().filter(|n| n.is_element()) {
            if child.tag_name().namespace() == Some(NS_DAV) && child.tag_name().name() == "href" {
                href = child.text().unwrap_or("").trim().to_string();
            } else if child.tag_name().namespace() == Some(NS_DAV)
                && child.tag_name().name() == "status"
            {
                status = parse_status_code(child.text().unwrap_or(""));
            } else if child.tag_name().namespace() == Some(NS_DAV)
                && child.tag_name().name() == "propstat"
            {
                for p in child.children().filter(|n| n.is_element()) {
                    if p.tag_name().namespace() == Some(NS_DAV) && p.tag_name().name() == "prop" {
                        for pr in p.children().filter(|n| n.is_element()) {
                            let ns = pr.tag_name().namespace().unwrap_or("").to_string();
                            let name = pr.tag_name().name().to_string();
                            if name == "resourcetype" {
                                for r in pr.descendants().filter(|n| n.is_element()) {
                                    if r.tag_name().name() == "collection" {
                                        is_collection = true;
                                    }
                                    if r.tag_name().name() == "calendar"
                                        && r.tag_name().namespace() == Some(NS_CALDAV)
                                    {
                                        is_calendar = true;
                                    }
                                }
                            }
                            if name == "current-user-privilege-set" {
                                // privileges are element names, not text
                                for p in pr.descendants().filter(|n| n.is_element()) {
                                    if p.parent().map(|par| par.tag_name().name()) == Some("privilege")
                                    {
                                        privileges.push(p.tag_name().name().to_string());
                                    }
                                }
                            }
                            if name == "supported-calendar-component-set" {
                                for c in pr.descendants().filter(|n| n.is_element()) {
                                    if c.tag_name().name() == "comp" {
                                        if let Some(n) = c.attribute("name") {
                                            components.push(n.to_string());
                                        }
                                    }
                                }
                            }
                            props.push(Prop {
                                value: text_of(pr).trim().to_string(),
                                ns,
                                name,
                            });
                        }
                    }
                }
            }
        }
        out.push(MultistatusResponse {
            href,
            props,
            is_collection,
            is_calendar,
            privileges,
            components,
            status,
        });
    }
    Ok(out)
}

/// Extract the top-level `<sync-token>` value from a multistatus body.
pub fn parse_sync_token(body: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(body).ok()?;
    doc.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "sync-token")
        .and_then(|n| n.text())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// PROPFIND on the server root asking for the current user principal.
pub fn propfind_root_body() -> String {
    format!(
        r#"{XML_HEADER}
<D:propfind xmlns:D="DAV:"><D:prop><D:current-user-principal/><D:resourcetype/><D:displayname/></D:prop></D:propfind>"#
    )
}

/// PROPFIND on a principal asking for its calendar-home-set.
pub fn propfind_principal_body() -> String {
    format!(
        r#"{XML_HEADER}
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:resourcetype/><C:calendar-home-set/><D:displayname/></D:prop></D:propfind>"#
    )
}

/// PROPFIND depth 1 on a calendar home listing the calendar collections.
pub fn propfind_calendars_body() -> String {
    format!(
        r#"{XML_HEADER}
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/"><D:prop>
<D:resourcetype/><D:displayname/><D:getetag/><D:sync-token/><D:getcontenttype/>
<D:current-user-privilege-set/>
<C:supported-calendar-component-set/><A:calendar-color/>
</D:prop></D:propfind>"#
    )
}

/// REPORT calendar-query: all VEVENTs overlapping [start, end).
pub fn calendar_query_body(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    format!(
        r#"{XML_HEADER}
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop>
<C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT">
<C:time-range start="{s}" end="{e}"/>
</C:comp-filter></C:comp-filter></C:filter></C:calendar-query>"#,
        s = start.format("%Y%m%dT%H%M%SZ"),
        e = end.format("%Y%m%dT%H%M%SZ"),
    )
}

/// REPORT calendar-query for every VEVENT in the collection (no time range).
/// Used for the initial full cache; later refreshes are incremental.
pub fn calendar_query_all_body() -> String {
    format!(
        r#"{XML_HEADER}
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop>
<C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT"/></C:comp-filter></C:filter></C:calendar-query>"#
    )
}

/// REPORT calendar-query for all VTODOs (no time-range filter: tasks may have
/// no due date at all).
pub fn calendar_query_todos_body() -> String {
    format!(
        r#"{XML_HEADER}
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop>
<C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VTODO"/></C:comp-filter></C:filter></C:calendar-query>"#
    )
}

/// PROPPATCH setting the Apple calendar color of a collection.
pub fn proppatch_calendar_color_body(color: &str) -> String {
    format!(
        r#"{XML_HEADER}
<D:propertyupdate xmlns:D="DAV:" xmlns:A="http://apple.com/ns/ical/"><D:set><D:prop><A:calendar-color>{c}</A:calendar-color></D:prop></D:set></D:propertyupdate>"#,
        c = xml_escape(color)
    )
}

/// PROPPATCH renaming a collection (displayname).
pub fn proppatch_displayname_body(display_name: &str) -> String {
    format!(
        r#"{XML_HEADER}
<D:propertyupdate xmlns:D="DAV:"><D:set><D:prop><D:displayname>{n}</D:displayname></D:prop></D:set></D:propertyupdate>"#,
        n = xml_escape(display_name)
    )
}

/// REPORT sync-collection: changes since `token` (or everything when `None`).
pub fn sync_collection_body(token: Option<&str>) -> String {
    let token = match token {
        Some(t) => format!("<D:sync-token>{}</D:sync-token>", xml_escape(t)),
        None => "<D:sync-token/>".to_string(),
    };
    format!(
        r#"{XML_HEADER}
<D:sync-collection xmlns:D="DAV:">
{token}
<D:sync-level>1</D:sync-level>
<D:prop><D:getetag/></D:prop>
</D:sync-collection>"#
    )
}

/// REPORT calendar-multiget: fetch several resources by href in one request.
pub fn calendar_multiget_body(hrefs: &[String]) -> String {
    let mut body = format!(
        r#"{XML_HEADER}
<C:calendar-multiget xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:prop><D:getetag/><C:calendar-data/></D:prop>"#
    );
    for h in hrefs {
        body.push_str(&format!("<D:href>{}</D:href>", xml_escape(h)));
    }
    body.push_str("</C:calendar-multiget>");
    body
}

/// MKCALENDAR body creating a new calendar collection with a display name.
pub fn mkcalendar_body(display_name: &str) -> String {
    format!(
        r#"{XML_HEADER}
<D:mkcalendar xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:set><D:prop>
<D:resourcetype><D:collection/><C:calendar/></D:resourcetype><D:displayname>{n}</D:displayname>
</D:prop></D:set></D:mkcalendar>"#,
        n = xml_escape(display_name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:A="http://apple.com/ns/ical/">
  <D:response>
    <D:href>/alice/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/></D:resourcetype>
      <D:displayname>alice</D:displayname>
    </D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
  <D:response>
    <D:href>/alice/personal/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
      <D:displayname>Personal</D:displayname>
      <D:getetag>"abc123"</D:getetag>
      <A:calendar-color>#e4572e</A:calendar-color>
    </D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
</D:multistatus>"#;

    #[test]
    fn parses_multistatus() {
        let ms = parse_multistatus(SAMPLE).unwrap();
        assert_eq!(ms.len(), 2);
        assert_eq!(ms[0].href, "/alice/");
        assert!(ms[0].is_collection);
        assert!(!ms[0].is_calendar);
        assert_eq!(ms[1].href, "/alice/personal/");
        assert!(ms[1].is_calendar);
        assert_eq!(ms[1].prop(NS_DAV, "displayname"), Some("Personal"));
        assert_eq!(ms[1].prop(NS_DAV, "getetag"), Some("\"abc123\""));
        assert_eq!(ms[1].prop(NS_APPLE_ICAL, "calendar-color"), Some("#e4572e"));
    }

    #[test]
    fn parses_privileges_and_status() {
        let body = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/alice/ro/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/></D:resourcetype>
      <D:current-user-privilege-set>
        <D:privilege><D:read/></D:privilege>
      </D:current-user-privilege-set>
    </D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
  <D:response>
    <D:href>/alice/rw/</D:href>
    <D:propstat><D:prop>
      <D:resourcetype><D:collection/></D:resourcetype>
      <D:current-user-privilege-set>
        <D:privilege><D:read/></D:privilege>
        <D:privilege><D:write/></D:privilege>
        <D:privilege><D:write-content/></D:privilege>
      </D:current-user-privilege-set>
    </D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat>
  </D:response>
  <D:response>
    <D:href>/alice/gone.ics</D:href>
    <D:status>HTTP/1.1 404 Not Found</D:status>
  </D:response>
</D:multistatus>"#;
        let ms = parse_multistatus(body).unwrap();
        assert_eq!(ms[0].privileges, vec!["read"]);
        assert!(!ms[0].can_write());
        assert!(ms[1].can_write());
        assert_eq!(ms[2].status, Some(404));
    }

    #[test]
    fn parses_supported_components() {
        let body = r#"<?xml version="1.0"?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav"><D:response>
<D:href>/a/</D:href><D:propstat><D:prop>
<D:resourcetype><D:collection/><C:calendar/></D:resourcetype>
<C:supported-calendar-component-set><C:comp name="VEVENT"/><C:comp name="VTODO"/></C:supported-calendar-component-set>
</D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#;
        let ms = parse_multistatus(body).unwrap();
        assert_eq!(ms[0].components, vec!["VEVENT", "VTODO"]);
    }

    #[test]
    fn parses_sync_token() {
        let body = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>/a/</D:href></D:response><D:sync-token>http://radicale.org/ns/sync/1234</D:sync-token></D:multistatus>"#;
        assert_eq!(
            parse_sync_token(body).as_deref(),
            Some("http://radicale.org/ns/sync/1234")
        );
        assert_eq!(parse_sync_token("<x/>"), None);
    }

    #[test]
    fn sync_and_multiget_bodies() {
        let body = sync_collection_body(Some("http://x/sync/1"));
        assert!(body.contains("<D:sync-token>http://x/sync/1</D:sync-token>"));
        assert!(body.contains("sync-level"));
        let body = calendar_multiget_body(&["/a/1.ics".into(), "/a/2.ics".into()]);
        assert!(body.contains("<D:href>/a/1.ics</D:href>"));
        assert!(body.contains("<D:href>/a/2.ics</D:href>"));
        assert!(body.contains("calendar-multiget"));
    }

    #[test]
    fn proppatch_escapes_name() {
        let body = proppatch_displayname_body("A & B");
        assert!(body.contains("<D:displayname>A &amp; B</D:displayname>"));
    }

    #[test]
    fn query_body_contains_time_range() {
        let start = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-12-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let body = calendar_query_body(start, end);
        assert!(body.contains(r#"start="20261101T000000Z""#));
        assert!(body.contains(r#"end="20261201T000000Z""#));
        assert!(body.contains("calendar-query"));
    }

    #[test]
    fn mkcalendar_escapes_name() {
        let body = mkcalendar_body("Work & <Stuff>");
        assert!(body.contains("Work &amp; &lt;Stuff&gt;"));
    }
}
