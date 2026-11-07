//! Minimal, fully controlled iCalendar (ICS / RFC 5545) parsing and
//! serialization for VEVENTs. We deliberately hand-roll this instead of
//! pulling in a big crate: the subset we need is small, and every line of it
//! is unit-tested below.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::error::{DavError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcsParam {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct IcsProperty {
    pub name: String,
    pub params: Vec<IcsParam>,
    pub value: String,
}

impl IcsProperty {
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.value.as_str())
    }
}

/// A DTSTART/DTEND value: either a date (all-day) or a zoned date-time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DtValue {
    Date(NaiveDate),
    DateTime(DateTime<Tz>),
}

impl DtValue {
    pub fn is_date(&self) -> bool {
        matches!(self, DtValue::Date(_))
    }

    /// Interpret as a zoned date-time (dates become midnight in `tz`).
    pub fn to_datetime(&self, tz: &Tz) -> Option<DateTime<Tz>> {
        match self {
            DtValue::DateTime(t) => Some(*t),
            DtValue::Date(d) => tz
                .from_local_datetime(&d.and_hms_opt(0, 0, 0)?)
                .single(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VEvent {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub dtstart: Option<DtValue>,
    pub dtend: Option<DtValue>,
    pub duration: Option<Duration>,
    pub rrule: Option<String>,
    pub rdate: Vec<String>,
    pub exdate: Vec<String>,
    pub status: Option<String>,
    pub created: Option<String>,
    pub last_modified: Option<String>,
    /// The original VEVENT block, for round-tripping into the local cache.
    pub raw: String,
}

/// Split ICS text into unfolded logical lines (RFC 5545 §3.1).
pub fn unfold(ics: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in ics.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if raw.is_empty() {
            continue;
        }
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            lines.last_mut().unwrap().push_str(&raw[1..]);
        } else {
            lines.push(raw.to_string());
        }
    }
    lines
}

fn split_unquoted(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut in_quotes = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            _ if c == sep && !in_quotes => {
                out.push(&s[start..i]);
                start = i + sep.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

pub fn unescape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            _ => out.push(c),
        }
    }
    out
}

/// Parse one content line into name / params / value.
pub fn parse_property(line: &str) -> Option<IcsProperty> {
    let bytes = line.as_bytes();
    let mut in_quotes = false;
    let mut colon = None;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => in_quotes = !in_quotes,
            b':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let head = &line[..colon];
    let value = unescape_text(&line[colon + 1..]);
    let mut parts = split_unquoted(head, ';');
    let name = parts.remove(0).to_uppercase();
    let mut params = Vec::new();
    for p in parts {
        if let Some(eq) = p.find('=') {
            params.push(IcsParam {
                name: p[..eq].to_uppercase(),
                value: p[eq + 1..].trim_matches('"').to_string(),
            });
        }
    }
    Some(IcsProperty {
        name,
        params,
        value,
    })
}

/// Parse a DTSTART/DTEND/DTSTAMP-style value with its parameters.
pub fn parse_dt(value: &str, params: &[IcsParam], default_tz: &Tz) -> Result<DtValue> {
    let v = value.trim();
    let naive = |s: &str| {
        NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S")
            .map_err(|e| DavError::Ics(format!("bad date-time {s:?}: {e}")))
    };
    // A trailing Z always means UTC, even if a (non-standard) TZID is present:
    // Radicale stores TZID=UTC:...Z and we must still read it back.
    if let Some(stripped) = v.strip_suffix('Z') {
        let n = naive(stripped)?;
        return Ok(DtValue::DateTime(chrono_tz::UTC.from_utc_datetime(&n)));
    }
    if let Some(tzid) = params.iter().find(|p| p.name == "TZID").map(|p| p.value.as_str()) {
        let tz: Tz = tzid
            .parse()
            .map_err(|_| DavError::Ics(format!("unknown TZID {tzid:?}")))?;
        let n = naive(v)?;
        return Ok(DtValue::DateTime(
            tz.from_local_datetime(&n)
                .single()
                .ok_or_else(|| DavError::Ics(format!("ambiguous or nonexistent local time {v:?}")))?,
        ));
    }
    if v.contains('T') {
        let n = naive(v)?;
        return Ok(DtValue::DateTime(
            default_tz
                .from_local_datetime(&n)
                .single()
                .ok_or_else(|| DavError::Ics(format!("ambiguous local time {v:?}")))?,
        ));
    }
    let d = NaiveDate::parse_from_str(v, "%Y%m%d")
        .map_err(|e| DavError::Ics(format!("bad date {v:?}: {e}")))?;
    Ok(DtValue::Date(d))
}

/// Parse a subset of ISO-8601 durations used in DURATION (P1DT2H30M...).
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    let body = s
        .strip_prefix('P')
        .ok_or_else(|| DavError::Ics(format!("bad duration {s:?}")))?;
    let (days_part, time_part) = match body.split_once('T') {
        Some((d, t)) => (d, Some(t)),
        None => (body, None),
    };
    let mut dur = Duration::zero();
    let mut num = String::new();
    let flush = |num: &mut String, unit: char, dur: &mut Duration| -> Result<()> {
        if num.is_empty() {
            return Ok(());
        }
        let n: i64 = num
            .parse()
            .map_err(|_| DavError::Ics(format!("bad duration number in {s:?}")))?;
        match unit {
            'D' => *dur += Duration::days(n),
            'H' => *dur += Duration::hours(n),
            'M' => *dur += Duration::minutes(n),
            'S' => *dur += Duration::seconds(n),
            'W' => *dur += Duration::weeks(n),
            _ => return Err(DavError::Ics(format!("bad duration unit {unit:?}"))),
        }
        num.clear();
        Ok(())
    };
    for part in [Some(days_part), time_part].into_iter().flatten() {
        for c in part.chars() {
            if c.is_ascii_digit() {
                num.push(c);
            } else {
                flush(&mut num, c.to_ascii_uppercase(), &mut dur)?;
            }
        }
    }
    Ok(dur)
}

fn build_vevent(props: Vec<IcsProperty>, raw: String, default_tz: &Tz) -> Result<VEvent> {
    let get = |name: &str| props.iter().find(|p| p.name == name);
    let dt = |name: &str| -> Result<Option<DtValue>> {
        match get(name) {
            Some(p) => Ok(Some(parse_dt(&p.value, &p.params, default_tz)?)),
            None => Ok(None),
        }
    };
    Ok(VEvent {
        uid: get("UID").map(|p| p.value.clone()).unwrap_or_default(),
        summary: get("SUMMARY").map(|p| p.value.clone()).unwrap_or_default(),
        description: get("DESCRIPTION").map(|p| p.value.clone()).unwrap_or_default(),
        location: get("LOCATION").map(|p| p.value.clone()).unwrap_or_default(),
        dtstart: dt("DTSTART")?,
        dtend: dt("DTEND")?,
        duration: get("DURATION").map(|p| parse_duration(&p.value)).transpose()?,
        rrule: get("RRULE").map(|p| p.value.clone()),
        rdate: props
            .iter()
            .filter(|p| p.name == "RDATE")
            .map(|p| p.value.clone())
            .collect(),
        exdate: props
            .iter()
            .filter(|p| p.name == "EXDATE")
            .map(|p| p.value.clone())
            .collect(),
        status: get("STATUS").map(|p| p.value.clone()),
        created: get("CREATED").map(|p| p.value.clone()),
        last_modified: get("LAST-MODIFIED").map(|p| p.value.clone()),
        raw,
    })
}

/// Parse all VEVENTs contained in a VCALENDAR string.
pub fn parse_calendar(ics: &str, default_tz: &Tz) -> Result<Vec<VEvent>> {
    let mut events = Vec::new();
    let mut cur: Option<(Vec<IcsProperty>, String)> = None;
    for line in unfold(ics) {
        let upper = line.to_uppercase();
        if upper == "BEGIN:VEVENT" {
            cur = Some((Vec::new(), "BEGIN:VEVENT\n".to_string()));
        } else if upper == "END:VEVENT" {
            if let Some((props, mut raw)) = cur.take() {
                raw.push_str("END:VEVENT\n");
                events.push(build_vevent(props, raw, default_tz)?);
            }
        } else if let Some((props, raw)) = cur.as_mut() {
            if let Some(p) = parse_property(&line) {
                raw.push_str(&line);
                raw.push('\n');
                props.push(p);
            }
        }
    }
    Ok(events)
}

/// Parse a cached VEVENT. New caches store a complete `BEGIN:VEVENT … END:VEVENT`
/// block; older caches stored only the body, so fall back to wrapping it.
pub fn parse_cached_event(ics: &str, default_tz: &Tz) -> Vec<VEvent> {
    let mut events = parse_calendar(ics, default_tz).unwrap_or_default();
    if events.is_empty() {
        let wrapped = format!("BEGIN:VCALENDAR\nBEGIN:VEVENT\n{ics}\nEND:VEVENT\nEND:VCALENDAR\n");
        events = parse_calendar(&wrapped, default_tz).unwrap_or_default();
    }
    events
}

fn build_vtodo(props: Vec<IcsProperty>, raw: String, default_tz: &Tz) -> Result<VTodo> {
    let get = |name: &str| props.iter().find(|p| p.name == name);
    let due = match get("DUE") {
        Some(p) => Some(parse_dt(&p.value, &p.params, default_tz)?),
        None => None,
    };
    Ok(VTodo {
        uid: get("UID").map(|p| p.value.clone()).unwrap_or_default(),
        summary: get("SUMMARY").map(|p| p.value.clone()).unwrap_or_default(),
        description: get("DESCRIPTION").map(|p| p.value.clone()).unwrap_or_default(),
        due,
        status: get("STATUS").map(|p| p.value.clone()),
        completed: get("COMPLETED").map(|p| p.value.clone()),
        priority: get("PRIORITY").and_then(|p| p.value.trim().parse().ok()),
        raw,
    })
}

/// Parse all VTODOs contained in a VCALENDAR string.
pub fn parse_todos(ics: &str, default_tz: &Tz) -> Result<Vec<VTodo>> {
    let mut todos = Vec::new();
    let mut cur: Option<(Vec<IcsProperty>, String)> = None;
    for line in unfold(ics) {
        let upper = line.to_uppercase();
        if upper == "BEGIN:VTODO" {
            cur = Some((Vec::new(), "BEGIN:VTODO\n".to_string()));
        } else if upper == "END:VTODO" {
            if let Some((props, mut raw)) = cur.take() {
                raw.push_str("END:VTODO\n");
                todos.push(build_vtodo(props, raw, default_tz)?);
            }
        } else if let Some((props, raw)) = cur.as_mut() {
            if let Some(p) = parse_property(&line) {
                raw.push_str(&line);
                raw.push('\n');
                props.push(p);
            }
        }
    }
    Ok(todos)
}

/// Parse a cached VTODO (same wrapper fallback as [`parse_cached_event`]).
pub fn parse_cached_todo(ics: &str, default_tz: &Tz) -> Vec<VTodo> {
    let mut todos = parse_todos(ics, default_tz).unwrap_or_default();
    if todos.is_empty() {
        let wrapped = format!("BEGIN:VCALENDAR\nBEGIN:VTODO\n{ics}\nEND:VTODO\nEND:VCALENDAR\n");
        todos = parse_todos(&wrapped, default_tz).unwrap_or_default();
    }
    todos
}

/// A new or edited event, ready to be serialized with [`build_ics`].
#[derive(Debug, Clone)]
pub struct EventDraft {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub location: String,
    pub all_day: bool,
    pub start: DateTime<Tz>,
    pub end: DateTime<Tz>,
    /// Optional RRULE (e.g. `FREQ=WEEKLY;BYDAY=MO,WE`). `None` = single event.
    pub rrule: Option<String>,
    /// Recurrence exception dates (kept verbatim when editing a series).
    pub exdate: Vec<String>,
    /// Extra recurrence dates (kept verbatim when editing a series).
    pub rdate: Vec<String>,
}

/// A VTODO (task) as parsed from the server.
#[derive(Debug, Clone, PartialEq)]
pub struct VTodo {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub due: Option<DtValue>,
    pub status: Option<String>,
    pub completed: Option<String>,
    pub priority: Option<u8>,
    pub raw: String,
}

impl VTodo {
    pub fn is_done(&self) -> bool {
        self.status
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("COMPLETED") || s.eq_ignore_ascii_case("CANCELLED"))
            .unwrap_or(false)
    }
}

/// A new or edited task, ready to be serialized with [`build_todo_ics`].
#[derive(Debug, Clone)]
pub struct TodoDraft {
    pub uid: String,
    pub summary: String,
    pub description: String,
    pub due: Option<NaiveDate>,
    pub completed: bool,
    pub priority: Option<u8>,
}

fn dt_line(name: &str, dt: &DateTime<Tz>, all_day: bool) -> String {
    if all_day {
        format!("{name};VALUE=DATE:{}", dt.format("%Y%m%d"))
    } else if dt.timezone() == chrono_tz::UTC {
        // RFC 5545: UTC times are written with a trailing Z and no TZID.
        // (Radicale rewrites TZID=UTC into the invalid "TZID=UTC:...Z", so
        // never emit that form in the first place.)
        format!("{name}:{}Z", dt.format("%Y%m%dT%H%M%S"))
    } else {
        format!(
            "{name};TZID={}:{}",
            dt.timezone(),
            dt.format("%Y%m%dT%H%M%S")
        )
    }
}

/// Fold a content line at 75 octets (RFC 5545 §3.1), char-boundary safe.
pub fn fold_line(line: &str) -> String {
    let mut out = String::new();
    let mut cur = String::new();
    for c in line.chars() {
        if cur.len() + c.len_utf8() > 74 {
            out.push_str(&cur);
            out.push_str("\r\n ");
            cur.clear();
        }
        cur.push(c);
    }
    out.push_str(&cur);
    out
}

/// Serialize an [`EventDraft`] into a complete VCALENDAR string.
pub fn build_ics(ev: &EventDraft, now: DateTime<Utc>) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//ChefDAV//ChefDAV//EN".to_string(),
        "CALSCALE:GREGORIAN".to_string(),
        "BEGIN:VEVENT".to_string(),
        format!("UID:{}", ev.uid),
        format!("DTSTAMP:{}", now.format("%Y%m%dT%H%M%SZ")),
        dt_line("DTSTART", &ev.start, ev.all_day),
        dt_line("DTEND", &ev.end, ev.all_day),
        format!("SUMMARY:{}", escape_text(&ev.summary)),
        format!("DESCRIPTION:{}", escape_text(&ev.description)),
        format!("LOCATION:{}", escape_text(&ev.location)),
    ];
    if let Some(rule) = ev.rrule.as_deref().filter(|r| !r.trim().is_empty()) {
        lines.push(format!("RRULE:{}", rule.trim()));
    }
    for x in &ev.exdate {
        lines.push(format!("EXDATE:{}", x));
    }
    for r in &ev.rdate {
        lines.push(format!("RDATE:{}", r));
    }
    lines.push("END:VEVENT".to_string());
    lines.push("END:VCALENDAR".to_string());
    let mut out = String::new();
    for l in &lines {
        out.push_str(&fold_line(l));
        out.push_str("\r\n");
    }
    out
}

/// Serialize a [`TodoDraft`] into a complete VCALENDAR string.
pub fn build_todo_ics(todo: &TodoDraft, now: DateTime<Utc>) -> String {
    let mut lines = vec![
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".to_string(),
        "PRODID:-//ChefDAV//ChefDAV//EN".to_string(),
        "BEGIN:VTODO".to_string(),
        format!("UID:{}", todo.uid),
        format!("DTSTAMP:{}", now.format("%Y%m%dT%H%M%SZ")),
        format!("SUMMARY:{}", escape_text(&todo.summary)),
        format!("DESCRIPTION:{}", escape_text(&todo.description)),
        "STATUS:NEEDS-ACTION".to_string(),
    ];
    if let Some(due) = todo.due {
        lines.push(format!("DUE;VALUE=DATE:{}", due.format("%Y%m%d")));
    }
    if let Some(p) = todo.priority {
        lines.push(format!("PRIORITY:{}", p.min(9)));
    }
    if todo.completed {
        // replace the default status and add the completion timestamp
        let status = lines
            .iter_mut()
            .find(|l| l.starts_with("STATUS:"))
            .expect("status line exists");
        *status = "STATUS:COMPLETED".to_string();
        lines.push(format!("COMPLETED:{}", now.format("%Y%m%dT%H%M%SZ")));
    }
    lines.push("END:VTODO".to_string());
    lines.push("END:VCALENDAR".to_string());
    let mut out = String::new();
    for l in &lines {
        out.push_str(&fold_line(l));
        out.push_str("\r\n");
    }
    out
}

/// Shift the first DTSTART/DTEND pair in an ICS by `delta_days`, preserving
/// every other property (description, alarms, categories, …). Used by drag &
/// drop. Returns `None` if no DTSTART was found.
pub fn shift_ics_dates(ics: &str, delta_days: i64) -> Option<String> {
    let mut out = String::new();
    let mut seen_start = false;
    let mut seen_end = false;
    for line in unfold(ics) {
        let upper = line.to_uppercase();
        let is_start = upper.starts_with("DTSTART");
        let is_end = upper.starts_with("DTEND");
        if (is_start && !seen_start) || (is_end && !seen_end) {
            if let Some(p) = parse_property(&line) {
                if let Some(shifted) = shift_dt_value(&p, delta_days) {
                    out.push_str(&fold_line(&shifted));
                    out.push_str("\r\n");
                    if is_start {
                        seen_start = true;
                    } else {
                        seen_end = true;
                    }
                    continue;
                }
            }
        }
        out.push_str(&fold_line(&line));
        out.push_str("\r\n");
    }
    if seen_start {
        Some(out)
    } else {
        None
    }
}

fn shift_dt_value(p: &IcsProperty, delta_days: i64) -> Option<String> {
    let v = p.value.trim();
    let params = if let Some(tzid) = p.param("TZID") {
        format!(";TZID={tzid}")
    } else if p.param("VALUE").map(|x| x.eq_ignore_ascii_case("DATE")).unwrap_or(false) {
        ";VALUE=DATE".to_string()
    } else {
        String::new()
    };
    if let Some(stripped) = v.strip_suffix('Z') {
        let n = NaiveDateTime::parse_from_str(stripped, "%Y%m%dT%H%M%S").ok()?;
        let shifted = n + Duration::days(delta_days);
        return Some(format!("{}{}:{}Z", p.name, params, shifted.format("%Y%m%dT%H%M%S")));
    }
    if v.contains('T') {
        let n = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
        let shifted = n + Duration::days(delta_days);
        return Some(format!("{}{}:{}", p.name, params, shifted.format("%Y%m%dT%H%M%S")));
    }
    let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
    let shifted = d + Duration::days(delta_days);
    Some(format!("{};VALUE=DATE:{}", p.name, shifted.format("%Y%m%d")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Berlin;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
BEGIN:VEVENT\r\n\
UID:abc-123@chef-dav\r\n\
DTSTART;TZID=Europe/Berlin:20261107T100000\r\n\
DTEND;TZID=Europe/Berlin:20261107T110000\r\n\
SUMMARY:Team sync\\, week\r\n ly\r\n\
DESCRIPTION:Line one\\nline two\r\n\
LOCATION:Room 4\r\n\
RRULE:FREQ=WEEKLY;COUNT=8\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn parses_vevent_with_folded_and_escaped_lines() {
        let events = parse_calendar(SAMPLE, &Tz::UTC).unwrap();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.uid, "abc-123@chef-dav");
        assert_eq!(ev.summary, "Team sync, weekly");
        assert_eq!(ev.description, "Line one\nline two");
        assert_eq!(ev.location, "Room 4");
        assert_eq!(ev.rrule.as_deref(), Some("FREQ=WEEKLY;COUNT=8"));
        match &ev.dtstart {
            Some(DtValue::DateTime(t)) => {
                assert_eq!(t.with_timezone(&Berlin).format("%Y-%m-%d %H:%M").to_string(), "2026-11-07 10:00");
            }
            other => panic!("expected zoned dtstart, got {other:?}"),
        }
    }

    #[test]
    fn parses_utc_and_floating_and_date() {
        let tz = &Tz::UTC;
        let p = parse_property("DTSTART:20261107T100000Z").unwrap();
        assert!(matches!(parse_dt(&p.value, &p.params, tz).unwrap(), DtValue::DateTime(_)));
        let p = parse_property("DTSTART:20261107T100000").unwrap();
        assert!(matches!(parse_dt(&p.value, &p.params, tz).unwrap(), DtValue::DateTime(_)));
        let p = parse_property("DTSTART;VALUE=DATE:20261107").unwrap();
        assert_eq!(parse_dt(&p.value, &p.params, tz).unwrap(), DtValue::Date(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()));
    }

    #[test]
    fn duration_parsing() {
        assert_eq!(parse_duration("PT1H30M").unwrap(), Duration::minutes(90));
        assert_eq!(parse_duration("P1D").unwrap(), Duration::days(1));
        assert_eq!(parse_duration("PT45M").unwrap(), Duration::minutes(45));
    }

    #[test]
    fn build_ics_roundtrip() {
        let tz = Berlin;
        let start = tz.with_ymd_and_hms(2026, 11, 7, 10, 0, 0).unwrap();
        let end = tz.with_ymd_and_hms(2026, 11, 7, 11, 0, 0).unwrap();
        let draft = EventDraft {
            uid: "u1@chef-dav".into(),
            summary: "Dinner; with friends".into(),
            description: "Bring wine\nand cheese".into(),
            location: "Home".into(),
            all_day: false,
            start,
            end,
            rrule: None,
            exdate: Vec::new(),
            rdate: Vec::new(),
        };
        let now = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let ics = build_ics(&draft, now);
        let events = parse_calendar(&ics, &Tz::UTC).unwrap();
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.uid, "u1@chef-dav");
        assert_eq!(ev.summary, "Dinner; with friends");
        assert_eq!(ev.description, "Bring wine\nand cheese");
        assert!(ics.contains("DTSTART;TZID=Europe/Berlin:20261107T100000"));
        // every physical line must be <= 75 octets
        for line in ics.split("\r\n") {
            assert!(line.len() <= 75, "line too long: {line:?}");
        }
    }

    #[test]
    fn utc_times_use_z_without_tzid() {
        let tz = chrono_tz::UTC;
        let start = tz.with_ymd_and_hms(2026, 11, 7, 12, 30, 0).unwrap();
        let end = tz.with_ymd_and_hms(2026, 11, 7, 13, 30, 0).unwrap();
        let draft = EventDraft {
            uid: "utc@chef-dav".into(),
            summary: "UTC".into(),
            description: String::new(),
            location: String::new(),
            all_day: false,
            start,
            end,
            rrule: None,
            exdate: Vec::new(),
            rdate: Vec::new(),
        };
        let now = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let ics = build_ics(&draft, now);
        assert!(ics.contains("DTSTART:20261107T123000Z"), "got: {ics}");
        assert!(!ics.contains("TZID=UTC"), "must not emit TZID=UTC: {ics}");
        let events = parse_calendar(&ics, &Tz::UTC).unwrap();
        assert_eq!(events.len(), 1);
    }

    /// Radicale stores TZID=UTC with a trailing Z; we must read it back.
    #[test]
    fn parses_radicale_tzid_utc_with_z_suffix() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:x\nDTSTART;TZID=UTC:20261108T100000Z\nDTEND;TZID=UTC:20261108T110000Z\nSUMMARY:x\nEND:VEVENT\nEND:VCALENDAR\n";
        let berlin = chrono_tz::Europe::Berlin;
        let events = parse_calendar(ics, &berlin).unwrap();
        let start = events[0].dtstart.as_ref().unwrap().to_datetime(&berlin).unwrap();
        assert_eq!(start.with_timezone(&berlin).format("%H:%M").to_string(), "11:00");
    }

    #[test]
    fn cached_raw_roundtrips() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:cached@x\nDTSTART;TZID=Europe/Berlin:20261110T100000\nDTEND;TZID=Europe/Berlin:20261110T110000\nSUMMARY:Cached\nEND:VEVENT\nEND:VCALENDAR\n";
        let ev = parse_calendar(ics, &Tz::UTC).unwrap().remove(0);
        assert!(ev.raw.starts_with("BEGIN:VEVENT"));
        assert!(ev.raw.contains("END:VEVENT"));
        let again = parse_cached_event(&ev.raw, &Tz::UTC);
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].summary, "Cached");
        // legacy cache without the wrapper still parses
        let legacy = ev.raw.replace("BEGIN:VEVENT\n", "").replace("END:VEVENT\n", "");
        assert_eq!(parse_cached_event(&legacy, &Tz::UTC).len(), 1);
    }

    #[test]
    fn vtodo_roundtrip() {
        let now = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let draft = TodoDraft {
            uid: "todo1@chef-dav".into(),
            summary: "Buy milk".into(),
            description: "Two litres".into(),
            due: NaiveDate::from_ymd_opt(2026, 11, 9),
            completed: false,
            priority: Some(5),
        };
        let ics = build_todo_ics(&draft, now);
        let todos = parse_todos(&ics, &Tz::UTC).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].summary, "Buy milk");
        assert_eq!(todos[0].due, Some(DtValue::Date(NaiveDate::from_ymd_opt(2026, 11, 9).unwrap())));
        assert!(!todos[0].is_done());
        assert_eq!(todos[0].priority, Some(5));

        let done = TodoDraft { completed: true, ..draft };
        let ics = build_todo_ics(&done, now);
        let todos = parse_todos(&ics, &Tz::UTC).unwrap();
        assert!(todos[0].is_done());
        assert!(todos[0].completed.is_some());
        assert!(ics.contains("STATUS:COMPLETED"));
    }

    #[test]
    fn shift_ics_dates_moves_timed_event() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\nDTSTART;TZID=Europe/Berlin:20261107T100000\r\nDTEND;TZID=Europe/Berlin:20261107T110000\r\nSUMMARY:keep me\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let shifted = shift_ics_dates(ics, 3).unwrap();
        assert!(shifted.contains("DTSTART;TZID=Europe/Berlin:20261110T100000"), "{shifted}");
        assert!(shifted.contains("DTEND;TZID=Europe/Berlin:20261110T110000"), "{shifted}");
        assert!(shifted.contains("SUMMARY:keep me"));
    }

    #[test]
    fn shift_ics_dates_moves_all_day_and_utc() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\nDTSTART;VALUE=DATE:20261107\r\nDTEND;VALUE=DATE:20261108\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let shifted = shift_ics_dates(ics, -1).unwrap();
        assert!(shifted.contains("DTSTART;VALUE=DATE:20261106"), "{shifted}");
        assert!(shifted.contains("DTEND;VALUE=DATE:20261107"), "{shifted}");

        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:x\r\nDTSTART:20261107T100000Z\r\nDTEND:20261107T110000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let shifted = shift_ics_dates(ics, 1).unwrap();
        assert!(shifted.contains("DTSTART:20261108T100000Z"), "{shifted}");
        assert!(shifted.contains("DTEND:20261108T110000Z"), "{shifted}");
    }

    #[test]
    fn shift_ics_dates_without_dtstart_is_none() {
        assert_eq!(shift_ics_dates("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n", 1), None);
    }

    #[test]
    fn build_ics_all_day_uses_value_date() {
        let tz = Berlin;
        let start = tz.with_ymd_and_hms(2026, 12, 24, 0, 0, 0).unwrap();
        let end = tz.with_ymd_and_hms(2026, 12, 25, 0, 0, 0).unwrap();
        let draft = EventDraft {
            uid: "x@chef-dav".into(),
            summary: "Holiday".into(),
            description: String::new(),
            location: String::new(),
            all_day: true,
            start,
            end,
            rrule: None,
            exdate: Vec::new(),
            rdate: Vec::new(),
        };
        let now = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let ics = build_ics(&draft, now);
        assert!(ics.contains("DTSTART;VALUE=DATE:20261224"));
        assert!(ics.contains("DTEND;VALUE=DATE:20261225"));
    }
}
