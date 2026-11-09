//! Deep links for individual resources.
//!
//! Every event and task has its own URL that also lives in the address bar:
//!
//! - `/event/<calendar>/<uid>` — the event details card (`<calendar>` is the
//!   calendar's display name, `<uid>` the event's `UID`; both percent-encoded).
//! - `/event/<calendar>/<uid>?occ=<rfc3339>` — one occurrence of a recurring
//!   event (`occ` is the original `RECURRENCE-ID` in UTC).
//! - `/task/<calendar>/<uid>` — the task editor.
//!
//! Opening a resource navigates to its URL; opening the URL (fresh load,
//! back/forward, pasted link) opens the resource. The router keeps path params
//! percent-encoded, so `escape`/`unescape` are applied explicitly here.

use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat};
use chrono_tz::Tz;
use leptos::prelude::*;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::state::{AppState, CalState, ViewMode};

/// Encode set: everything except RFC 3986 unreserved characters
/// (`A-Z a-z 0-9 - _ . ~`), matching what `encodeURIComponent` leaves alone
/// for typical UIDs and calendar names.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// Percent-encode a path segment / query value. The router keeps path params
/// percent-encoded, so encoding/decoding is done explicitly here.
pub fn escape(s: &str) -> String {
    utf8_percent_encode(s, SEGMENT).to_string()
}

pub fn unescape(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().into_owned()
}

/// URL path (+ query) for an event, optionally one occurrence of a series.
pub fn event_path(cal_name: &str, uid: &str, occurrence: Option<DateTime<Tz>>) -> String {
    let mut p = format!("/event/{}/{}", escape(cal_name), escape(uid));
    if let Some(occ) = occurrence {
        p.push_str(&format!(
            "?occ={}",
            escape(&occ.to_rfc3339_opts(SecondsFormat::Secs, true))
        ));
    }
    p
}

/// URL path for a task.
pub fn task_path(cal_name: &str, uid: &str) -> String {
    format!("/task/{}/{}", escape(cal_name), escape(uid))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResourceKind {
    Event,
    Task,
}

/// Parse a raw (still percent-encoded) path like `/event/<cal>/<uid>`.
pub fn parse_path(path: &str) -> Option<(ResourceKind, String, String)> {
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    match segs.as_slice() {
        ["event", cal, uid] => Some((ResourceKind::Event, unescape(cal), unescape(uid))),
        ["task", cal, uid] => Some((ResourceKind::Task, unescape(cal), unescape(uid))),
        _ => None,
    }
}

/// URL for a calendar view + anchor date, e.g. `/month/2026-11`,
/// `/week/2026-11-09`. Always includes the date so the link is shareable.
pub fn view_path(view: ViewMode, anchor: NaiveDate) -> String {
    match view {
        ViewMode::Month => format!("/month/{}-{:02}", anchor.year(), anchor.month()),
        ViewMode::Week => format!("/week/{}", anchor.format("%Y-%m-%d")),
        ViewMode::Day => format!("/day/{}", anchor.format("%Y-%m-%d")),
        ViewMode::Agenda => format!("/list/{}", anchor.format("%Y-%m-%d")),
        ViewMode::Tasks => "/tasks".to_string(),
    }
}

/// Parse a view path (`/month/2026-11`, `/week/…`, `/day/…`, `/list/…`,
/// `/tasks`, or the bare forms without a date). Returns the view and the
/// anchor date if one was given.
pub fn parse_view_path(path: &str) -> Option<(ViewMode, Option<NaiveDate>)> {
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    let date = |s: &str, fmt: &str| NaiveDate::parse_from_str(s, fmt).ok();
    let month = |s: &str| {
        let mut it = s.split('-');
        match (it.next(), it.next(), it.next()) {
            (Some(y), Some(m), None) => match (y.parse::<i32>().ok(), m.parse::<u32>().ok()) {
                (Some(y), Some(m)) => NaiveDate::from_ymd_opt(y, m, 1),
                _ => None,
            },
            _ => None,
        }
    };
    match segs.as_slice() {
        ["month"] => Some((ViewMode::Month, None)),
        ["month", d] => month(d).map(|d| (ViewMode::Month, Some(d))),
        ["week"] => Some((ViewMode::Week, None)),
        ["week", d] => date(d, "%Y-%m-%d").map(|d| (ViewMode::Week, Some(d))),
        ["day"] => Some((ViewMode::Day, None)),
        ["day", d] => date(d, "%Y-%m-%d").map(|d| (ViewMode::Day, Some(d))),
        ["list"] => Some((ViewMode::Agenda, None)),
        ["list", d] => date(d, "%Y-%m-%d").map(|d| (ViewMode::Agenda, Some(d))),
        ["tasks"] => Some((ViewMode::Tasks, None)),
        _ => None,
    }
}

/// Extract and decode the `occ` query parameter from a raw search string.
pub fn occ_from_search(search: &str, tz: &Tz) -> Option<DateTime<Tz>> {
    let raw = search
        .trim_start_matches('?')
        .split('&')
        .find_map(|kv| kv.strip_prefix("occ="))?;
    DateTime::parse_from_rfc3339(&unescape(raw))
        .ok()
        .map(|d| d.with_timezone(tz))
}

/// The last path segment of a collection href (e.g. `/alice/personal/` →
/// `personal`). Used as a fallback when matching the calendar in a link, so
/// links survive a display-name rename as long as the collection name is
/// unchanged.
fn href_tail(href: &str) -> &str {
    href.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// Find a calendar by display name, falling back to the collection name.
pub fn find_calendar(state: &AppState, cal_param: &str) -> Option<CalState> {
    let cals = state.calendars.get();
    cals.iter()
        .find(|c| c.info.display_name == cal_param)
        .or_else(|| cals.iter().find(|c| href_tail(&c.info.href) == cal_param))
        .cloned()
}

/// Resolve an event link against the loaded events. Returns the index into
/// `state.events` plus the modal's occurrence/display-start pair.
pub fn resolve_event(
    state: &AppState,
    cal_param: &str,
    uid: &str,
    occ: Option<DateTime<Tz>>,
) -> Option<(usize, Option<DateTime<Tz>>, Option<DateTime<Tz>>)> {
    let cal = find_calendar(state, cal_param)?;
    let events = state.events.get();
    let index = events
        .iter()
        .position(|e| e.calendar_href == cal.info.href && e.event.uid == uid)?;
    let ev = &events[index];
    let occurrence = occ.map(|o| o.with_timezone(&state.tz));
    // For a recurring occurrence, find the real displayed start by expanding
    // the series around the RECURRENCE-ID (a THISANDFUTURE override may have
    // shifted it).
    let display_start = occurrence.and_then(|occ| {
        let rs = occ - chrono::Duration::days(8);
        let re = occ + chrono::Duration::days(8);
        caldav::recurrence::expand_series(&ev.event, &ev.overrides, rs, re, &state.tz)
            .into_iter()
            .find(|o| o.recurrence_id == Some(occ))
            .map(|o| o.start)
    });
    Some((index, occurrence, display_start))
}

/// Resolve a task link against the loaded todos. Returns the index into
/// `state.todos`.
pub fn resolve_todo(state: &AppState, cal_param: &str, uid: &str) -> Option<usize> {
    let cal = find_calendar(state, cal_param)?;
    state
        .todos
        .get()
        .iter()
        .position(|t| t.calendar_href == cal.info.href && t.todo.uid == uid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn escape_roundtrip_special_chars() {
        for s in [
            "Personal",
            "Meine Kalender",
            "a/b@c.d",
            "100% sûr",
            "uid-with-!~*'()",
            "タスク",
        ] {
            let enc = escape(s);
            assert!(!enc.contains('/'), "slash must be encoded: {s}");
            assert_eq!(unescape(&enc), s);
        }
    }

    #[test]
    fn parse_event_path() {
        let cal = escape("Meine Kalender");
        let uid = escape("abc@x.de");
        let (kind, cal_p, id) = parse_path(&format!("/event/{cal}/{uid}")).unwrap();
        assert_eq!(kind, ResourceKind::Event);
        assert_eq!(cal_p, "Meine Kalender");
        assert_eq!(id, "abc@x.de");
    }

    #[test]
    fn parse_task_path() {
        let (kind, cal_p, id) = parse_path("/task/Personal/uid-123").unwrap();
        assert_eq!(kind, ResourceKind::Task);
        assert_eq!(cal_p, "Personal");
        assert_eq!(id, "uid-123");
    }

    #[test]
    fn parse_rejects_other_paths() {
        assert!(parse_path("/").is_none());
        assert!(parse_path("/event").is_none());
        assert!(parse_path("/event/only-cal").is_none());
        assert!(parse_path("/event/a/b/c").is_none());
        assert!(parse_path("/login").is_none());
    }

    #[test]
    fn event_path_includes_occ_query() {
        let tz: Tz = chrono_tz::Europe::Berlin;
        let occ = tz
            .with_ymd_and_hms(2026, 11, 10, 9, 0, 0)
            .unwrap();
        let p = event_path("Personal", "uid-1", Some(occ));
        assert!(p.starts_with("/event/Personal/uid-1?occ="), "{p}");
        // the occ value must decode back to the same instant (RFC3339, UTC)
        let q = p.split('?').nth(1).unwrap();
        let parsed = occ_from_search(q, &tz).unwrap();
        assert_eq!(parsed, occ);
    }

    #[test]
    fn occ_from_search_decodes_percent() {
        let tz: Tz = chrono_tz::UTC;
        let occ = occ_from_search("?occ=2026-11-10T09%3A00%3A00%2B01%3A00", &tz).unwrap();
        assert_eq!(occ.to_rfc3339_opts(SecondsFormat::Secs, true), "2026-11-10T08:00:00Z");
        assert!(occ_from_search("?foo=bar", &tz).is_none());
        assert!(occ_from_search("", &tz).is_none());
    }

    #[test]
    fn view_path_roundtrip() {
        use chrono::NaiveDate;
        let d = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        for (view, expect) in [
            (ViewMode::Month, "/month/2026-11"),
            (ViewMode::Week, "/week/2026-11-09"),
            (ViewMode::Day, "/day/2026-11-09"),
            (ViewMode::Agenda, "/list/2026-11-09"),
            (ViewMode::Tasks, "/tasks"),
        ] {
            let p = view_path(view, d);
            assert_eq!(p, expect);
            let (v2, date2) = parse_view_path(&p).unwrap();
            assert_eq!(v2, view);
            if view == ViewMode::Month {
                assert_eq!(date2, Some(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()));
            } else if view != ViewMode::Tasks {
                assert_eq!(date2, Some(d));
            }
        }
        // bare forms (no date) and junk
        assert_eq!(parse_view_path("/month").unwrap().0, ViewMode::Month);
        assert!(parse_view_path("/month/2026-13").is_none());
        assert!(parse_view_path("/event/a/b").is_none());
        assert!(parse_view_path("/").is_none());
    }
}
