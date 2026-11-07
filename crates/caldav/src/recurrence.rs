//! Client-side recurrence expansion using the `rrule` crate (MIT), which
//! understands TZID and therefore handles DST correctly when a rule is
//! expanded in the event's own timezone.

use chrono::{DateTime, Duration};
use chrono_tz::Tz;
use rrule::RRuleSet;

use crate::ics::{DtValue, VEvent};

#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub start: DateTime<Tz>,
    pub end: DateTime<Tz>,
    /// The original recurrence start (the `RECURRENCE-ID` value). This stays
    /// fixed even when a `THISANDFUTURE` override shifts `start`.
    pub recurrence_id: Option<DateTime<Tz>>,
    /// Index into the series' overrides when this occurrence comes from a
    /// `RECURRENCE-ID` exception (single or `THISANDFUTURE`).
    pub override_idx: Option<usize>,
}

fn override_times(o: &VEvent, tz: &Tz) -> Option<(DateTime<Tz>, DateTime<Tz>)> {
    let start = o.dtstart.as_ref()?.to_datetime(tz)?;
    let end = o
        .dtend
        .as_ref()
        .and_then(|d| d.to_datetime(tz))
        .unwrap_or_else(|| start + event_duration(o).unwrap_or_default());
    Some((start, end))
}

/// Expand a recurring master plus its `RECURRENCE-ID` exceptions.
///
/// - single overrides replace/cancel exactly one occurrence;
/// - `RANGE=THISANDFUTURE` overrides shift all occurrences at or after their
///   recurrence id (and can change their duration);
/// - overrides moved into the requested range from outside are included.
pub fn expand_series(
    master: &VEvent,
    overrides: &[VEvent],
    range_start: DateTime<Tz>,
    range_end: DateTime<Tz>,
    default_tz: &Tz,
) -> Vec<Occurrence> {
    let rid_dt = |o: &VEvent| -> Option<DateTime<Tz>> {
        o.recurrence_id
            .as_ref()
            .and_then(|d| d.to_datetime(default_tz))
    };
    let base = expand_event(master, range_start, range_end, default_tz);
    let mut out: Vec<Occurrence> = Vec::new();
    let mut used = vec![false; overrides.len()];
    let mut future: Vec<usize> = overrides
        .iter()
        .enumerate()
        .filter(|(_, o)| o.recurrence_range && rid_dt(o).is_some())
        .map(|(i, _)| i)
        .collect();
    future.sort_by_key(|&i| rid_dt(&overrides[i]).unwrap());

    for occ in base {
        // exact single-occurrence override
        let single = overrides.iter().enumerate().find(|(_, o)| {
            !o.recurrence_range
                && rid_dt(o)
                    .map(|r| r == occ.start)
                    .unwrap_or(false)
        });
        if let Some((i, o)) = single {
            used[i] = true;
            if o.is_cancelled() {
                continue;
            }
            if let Some((s, e)) = override_times(o, default_tz) {
                out.push(Occurrence {
                    start: s,
                    end: e,
                    recurrence_id: rid_dt(o),
                    override_idx: Some(i),
                });
            }
            continue;
        }
        // latest THISANDFUTURE override at or before this occurrence
        let fut = future
            .iter()
            .rev()
            .find(|&&i| rid_dt(&overrides[i]).map(|r| r <= occ.start).unwrap_or(false))
            .copied();
        if let Some(i) = fut {
            let o = &overrides[i];
            if o.is_cancelled() {
                continue;
            }
            if let (Some(rid), Some((s, _))) = (rid_dt(o), override_times(o, default_tz)) {
                let shift = s - rid;
                let dur = event_duration(o)
                    .or_else(|| event_duration(master))
                    .unwrap_or_default();
                out.push(Occurrence {
                    start: occ.start + shift,
                    end: occ.start + shift + dur,
                    recurrence_id: Some(occ.start),
                    override_idx: Some(i),
                });
                continue;
            }
        }
        out.push(occ);
    }

    // Overrides whose new time falls inside the range but whose recurrence id
    // was outside the master's expanded range (e.g. moved into this month).
    for (i, o) in overrides.iter().enumerate() {
        if used[i] || o.is_cancelled() {
            continue;
        }
        if let Some((s, e)) = override_times(o, default_tz) {
            if e > range_start && s < range_end && !out.iter().any(|x| x.start == s) {
                out.push(Occurrence {
                    start: s,
                    end: e,
                    recurrence_id: rid_dt(o),
                    override_idx: Some(i),
                });
            }
        }
    }

    out.retain(|o| o.end > range_start && o.start < range_end);
    out.sort_by_key(|o| o.start);
    out
}

/// The subset of RRULE the UI's simple recurrence editor understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleRule {
    /// DAILY | WEEKLY | MONTHLY | YEARLY
    pub freq: String,
    pub interval: u32,
    /// Weekday codes (MO, TU, …), only meaningful for WEEKLY.
    pub byday: Vec<String>,
    pub count: Option<u32>,
    /// Inclusive end date, `YYYYMMDD`.
    pub until: Option<String>,
}

pub const WEEKDAY_CODES: [&str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

/// Parse an RRULE into [`SimpleRule`]. Returns `None` when the rule uses
/// features the simple editor doesn't model (BYSETPOS, BYMONTHDAY, …), so the
/// UI can fall back to a raw text field instead of silently dropping them.
pub fn parse_simple_rule(rule: &str) -> Option<SimpleRule> {
    let rule = rule.trim().trim_start_matches("RRULE:");
    let mut freq = None;
    let mut interval = 1u32;
    let mut byday = Vec::new();
    let mut count = None;
    let mut until = None;
    for part in rule.split(';').filter(|p| !p.is_empty()) {
        let (k, v) = part.split_once('=')?;
        match k.to_ascii_uppercase().as_str() {
            "FREQ" => {
                let f = v.to_ascii_uppercase();
                if !matches!(f.as_str(), "DAILY" | "WEEKLY" | "MONTHLY" | "YEARLY") {
                    return None;
                }
                freq = Some(f);
            }
            "INTERVAL" => interval = v.parse().ok()?,
            "BYDAY" => {
                byday = v
                    .split(',')
                    .map(|d| d.trim().to_ascii_uppercase())
                    .filter(|d| WEEKDAY_CODES.contains(&d.as_str()))
                    .collect();
                if byday.is_empty() {
                    return None;
                }
            }
            "COUNT" => count = Some(v.parse().ok()?),
            "UNTIL" => {
                // accept YYYYMMDD and YYYYMMDDTHHMMSSZ
                let d = v.trim();
                let date = if d.len() >= 8 { &d[..8] } else { return None };
                if !date.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                until = Some(date.to_string());
            }
            "WKST" => {}
            _ => return None,
        }
    }
    Some(SimpleRule {
        freq: freq?,
        interval: interval.max(1),
        byday,
        count,
        until,
    })
}

/// Build an RRULE string from [`SimpleRule`].
pub fn build_simple_rule(rule: &SimpleRule) -> String {
    let mut parts = vec![format!("FREQ={}", rule.freq)];
    if rule.interval > 1 {
        parts.push(format!("INTERVAL={}", rule.interval));
    }
    if rule.freq == "WEEKLY" && !rule.byday.is_empty() {
        parts.push(format!("BYDAY={}", rule.byday.join(",")));
    }
    if let Some(c) = rule.count {
        parts.push(format!("COUNT={}", c.max(1)));
    } else if let Some(u) = &rule.until {
        parts.push(format!("UNTIL={}T235959Z", u));
    }
    parts.join(";")
}

/// Human-readable summary of an RRULE, e.g. "Every 2 weeks on Mon, Wed".
pub fn describe_rule(rule: &str) -> String {
    let Some(r) = parse_simple_rule(rule) else {
        return rule.trim().to_string();
    };
    let every = if r.interval == 1 {
        match r.freq.as_str() {
            "DAILY" => "Daily".to_string(),
            "WEEKLY" => "Weekly".to_string(),
            "MONTHLY" => "Monthly".to_string(),
            _ => "Yearly".to_string(),
        }
    } else {
        let unit = match r.freq.as_str() {
            "DAILY" => "days",
            "WEEKLY" => "weeks",
            "MONTHLY" => "months",
            _ => "years",
        };
        format!("Every {} {}", r.interval, unit)
    };
    let days = if r.byday.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = r
            .byday
            .iter()
            .map(|d| match d.as_str() {
                "MO" => "Mon",
                "TU" => "Tue",
                "WE" => "Wed",
                "TH" => "Thu",
                "FR" => "Fri",
                "SA" => "Sat",
                _ => "Sun",
            })
            .collect();
        format!(" on {}", names.join(", "))
    };
    let end = match (r.count, r.until) {
        (Some(c), _) => format!(", {c} times"),
        (_, Some(u)) => format!(", until {u}"),
        _ => String::new(),
    };
    format!("{every}{days}{end}")
}

/// Effective duration of an event (DTEND - DTSTART, or DURATION).
pub fn event_duration(ev: &VEvent) -> Option<Duration> {
    match (&ev.dtstart, &ev.dtend, &ev.duration) {
        (Some(s), Some(e), _) => match (s, e) {
            (DtValue::DateTime(a), DtValue::DateTime(b)) => Some(*b - *a),
            (DtValue::Date(a), DtValue::Date(b)) => {
                Some(Duration::days(b.signed_duration_since(*a).num_days()))
            }
            _ => None,
        },
        (Some(_), None, Some(d)) => Some(*d),
        _ => None,
    }
    .filter(|d| *d >= Duration::zero())
}

fn dtstart_line(ev: &VEvent) -> Option<String> {
    match &ev.dtstart {
        Some(DtValue::Date(d)) => Some(format!("DTSTART;VALUE=DATE:{}", d.format("%Y%m%d"))),
        Some(DtValue::DateTime(t)) => Some(format!(
            "DTSTART;TZID={}:{}",
            t.timezone(),
            t.format("%Y%m%dT%H%M%S")
        )),
        None => None,
    }
}

/// Expand an event into all occurrences overlapping [range_start, range_end).
/// Non-recurring events yield at most one occurrence; unparseable rules fall
/// back to the single base occurrence.
pub fn expand_event(
    ev: &VEvent,
    range_start: DateTime<Tz>,
    range_end: DateTime<Tz>,
    default_tz: &Tz,
) -> Vec<Occurrence> {
    let Some(dur) = event_duration(ev) else {
        return Vec::new();
    };
    let start = match &ev.dtstart {
        Some(dt) => match dt.to_datetime(default_tz) {
            Some(t) => t,
            None => return Vec::new(),
        },
        None => return Vec::new(),
    };

    if let (Some(rule), Some(dt_line)) = (&ev.rrule, dtstart_line(ev)) {
        let mut s = String::new();
        s.push_str(&dt_line);
        s.push('\n');
        s.push_str("RRULE:");
        s.push_str(rule);
        s.push('\n');
        for r in &ev.rdate {
            s.push_str("RDATE:");
            s.push_str(r);
            s.push('\n');
        }
        for x in &ev.exdate {
            s.push_str("EXDATE:");
            s.push_str(x);
            s.push('\n');
        }
        if let Ok(set) = s.parse::<RRuleSet>() {
            // rrule yields DateTime<rrule::Tz>; convert back to chrono_tz::Tz.
            let to_ctz = |d: DateTime<rrule::Tz>| -> DateTime<Tz> {
                let ctz = match d.timezone() {
                    rrule::Tz::Tz(t) => t,
                    rrule::Tz::Local(_) => *default_tz,
                };
                d.with_timezone(&ctz)
            };
            let dates: Vec<DateTime<Tz>> = set
                .into_iter()
                .take_while(|d| {
                    let d = to_ctz(*d);
                    d < range_end
                })
                .map(|d| to_ctz(d))
                .filter(|d| *d >= range_start)
                .take(2000)
                .collect();
            return dates
                .into_iter()
                .map(|d| Occurrence {
                    start: d,
                    end: d + dur,
                    recurrence_id: Some(d),
                    override_idx: None,
                })
                .collect();
        }
    }

    let occ = Occurrence {
        start,
        end: start + dur,
        recurrence_id: Some(start),
        override_idx: None,
    };
    if occ.end > range_start && occ.start < range_end {
        vec![occ]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ics::parse_calendar;
    use chrono::{NaiveDate, TimeZone};
    use chrono_tz::Europe::Berlin;

    fn ev(ics_vevent_body: &str) -> VEvent {
        let ics = format!("BEGIN:VCALENDAR\n{ics_vevent_body}\nEND:VCALENDAR\n");
        parse_calendar(&ics, &Tz::UTC).unwrap().remove(0)
    }

    fn range(y1: i32, m1: u32, d1: u32, y2: i32, m2: u32, d2: u32) -> (DateTime<Tz>, DateTime<Tz>) {
        let tz = Berlin;
        (
            tz.with_ymd_and_hms(y1, m1, d1, 0, 0, 0).unwrap(),
            tz.with_ymd_and_hms(y2, m2, d2, 0, 0, 0).unwrap(),
        )
    }

    #[test]
    fn single_event_in_and_out_of_range() {
        let e = ev("BEGIN:VEVENT\nUID:a\nDTSTART;TZID=Europe/Berlin:20261110T100000\nDTEND;TZID=Europe/Berlin:20261110T110000\nEND:VEVENT");
        let (s, en) = range(2026, 11, 1, 2026, 12, 1);
        assert_eq!(expand_event(&e, s, en, &Berlin).len(), 1);
        let (s, en) = range(2026, 12, 1, 2027, 1, 1);
        assert_eq!(expand_event(&e, s, en, &Berlin).len(), 0);
    }

    #[test]
    fn daily_rule_expands_in_range() {
        let e = ev("BEGIN:VEVENT\nUID:b\nDTSTART;TZID=Europe/Berlin:20261102T090000\nDTEND;TZID=Europe/Berlin:20261102T093000\nRRULE:FREQ=DAILY;COUNT=10\nEND:VEVENT");
        let (s, en) = range(2026, 11, 1, 2026, 12, 1);
        let occ = expand_event(&e, s, en, &Berlin);
        assert_eq!(occ.len(), 10);
        assert_eq!(occ[0].start.format("%Y-%m-%d %H:%M").to_string(), "2026-11-02 09:00");
        assert_eq!(occ[9].end.format("%H:%M").to_string(), "09:30");
    }

    #[test]
    fn weekly_rule_keeps_local_time_across_dst() {
        // Europe switches from CEST to CET on 2026-10-25. A 09:00 Berlin
        // weekly event must stay at 09:00 local, not drift by an hour.
        let e = ev("BEGIN:VEVENT\nUID:c\nDTSTART;TZID=Europe/Berlin:20261020T090000\nDTEND;TZID=Europe/Berlin:20261020T100000\nRRULE:FREQ=WEEKLY;COUNT=4\nEND:VEVENT");
        let (s, en) = range(2026, 10, 1, 2026, 12, 1);
        let occ = expand_event(&e, s, en, &Berlin);
        assert_eq!(occ.len(), 4);
        for o in &occ {
            assert_eq!(o.start.with_timezone(&Berlin).format("%H:%M").to_string(), "09:00");
        }
        let dates: Vec<String> = occ.iter().map(|o| o.start.with_timezone(&Berlin).format("%Y-%m-%d").to_string()).collect();
        assert_eq!(dates, vec!["2026-10-20", "2026-10-27", "2026-11-03", "2026-11-10"]);
    }

    #[test]
    fn exdate_removes_occurrence() {
        let e = ev("BEGIN:VEVENT\nUID:d\nDTSTART;TZID=Europe/Berlin:20261102T090000\nDTEND;TZID=Europe/Berlin:20261102T093000\nRRULE:FREQ=DAILY;COUNT=5\nEXDATE;TZID=Europe/Berlin:20261103T090000\nEND:VEVENT");
        let (s, en) = range(2026, 11, 1, 2026, 12, 1);
        let occ = expand_event(&e, s, en, &Berlin);
        assert_eq!(occ.len(), 4);
        assert!(occ.iter().all(|o| o.start.format("%d").to_string() != "03"));
    }

    #[test]
    fn all_day_event_duration() {
        let e = ev("BEGIN:VEVENT\nUID:e\nDTSTART;VALUE=DATE:20261110\nDTEND;VALUE=DATE:20261112\nEND:VEVENT");
        assert!(e.dtstart.as_ref().unwrap().is_date());
        let (s, en) = range(2026, 11, 1, 2026, 12, 1);
        let occ = expand_event(&e, s, en, &Berlin);
        assert_eq!(occ.len(), 1);
        let days = (occ[0].end - occ[0].start).num_days();
        assert_eq!(days, 2);
    }

    fn series(ics: &str) -> (VEvent, Vec<VEvent>) {
        let events = parse_calendar(ics, &Tz::UTC).unwrap();
        let master = events.iter().find(|e| !e.is_override()).cloned().unwrap();
        let overrides = events.into_iter().filter(|e| e.is_override()).collect();
        (master, overrides)
    }

    #[test]
    fn single_override_replaces_one_occurrence() {
        let ics = "BEGIN:VCALENDAR
BEGIN:VEVENT
UID:s
DTSTART;TZID=Europe/Berlin:20261103T100000
DTEND;TZID=Europe/Berlin:20261103T110000
RRULE:FREQ=WEEKLY;COUNT=3
SUMMARY:Master
END:VEVENT
BEGIN:VEVENT
UID:s
RECURRENCE-ID;TZID=Europe/Berlin:20261110T100000
DTSTART;TZID=Europe/Berlin:20261110T140000
DTEND;TZID=Europe/Berlin:20261110T150000
SUMMARY:Moved
END:VEVENT
END:VCALENDAR
";
        let (master, overrides) = series(ics);
        let (s, e) = range(2026, 11, 1, 2026, 12, 1);
        let occ = expand_series(&master, &overrides, s, e, &Berlin);
        assert_eq!(occ.len(), 3);
        assert_eq!(occ[0].start.with_timezone(&Berlin).format("%H:%M").to_string(), "10:00");
        assert_eq!(occ[1].start.with_timezone(&Berlin).format("%H:%M").to_string(), "14:00");
        assert!(occ[1].override_idx.is_some());
        assert_eq!(occ[2].start.with_timezone(&Berlin).format("%H:%M").to_string(), "10:00");
    }

    #[test]
    fn cancelled_override_removes_occurrence() {
        let ics = "BEGIN:VCALENDAR
BEGIN:VEVENT
UID:s
DTSTART;TZID=Europe/Berlin:20261103T100000
DTEND;TZID=Europe/Berlin:20261103T110000
RRULE:FREQ=WEEKLY;COUNT=3
END:VEVENT
BEGIN:VEVENT
UID:s
RECURRENCE-ID;TZID=Europe/Berlin:20261110T100000
DTSTART;TZID=Europe/Berlin:20261110T100000
DTEND;TZID=Europe/Berlin:20261110T110000
STATUS:CANCELLED
END:VEVENT
END:VCALENDAR
";
        let (master, overrides) = series(ics);
        let (s, e) = range(2026, 11, 1, 2026, 12, 1);
        let occ = expand_series(&master, &overrides, s, e, &Berlin);
        assert_eq!(occ.len(), 2);
        assert!(occ.iter().all(|o| o.start.format("%d").to_string() != "10"));
    }

    #[test]
    fn this_and_future_shifts_subsequent_occurrences() {
        let ics = "BEGIN:VCALENDAR
BEGIN:VEVENT
UID:s
DTSTART;TZID=Europe/Berlin:20261108T100000
DTEND;TZID=Europe/Berlin:20261108T110000
RRULE:FREQ=DAILY;COUNT=4
END:VEVENT
BEGIN:VEVENT
UID:s
RECURRENCE-ID;RANGE=THISANDFUTURE;TZID=Europe/Berlin:20261110T100000
DTSTART;TZID=Europe/Berlin:20261110T120000
DTEND;TZID=Europe/Berlin:20261110T130000
END:VEVENT
END:VCALENDAR
";
        let (master, overrides) = series(ics);
        let (s, e) = range(2026, 11, 1, 2026, 12, 1);
        let occ = expand_series(&master, &overrides, s, e, &Berlin);
        let times: Vec<String> = occ
            .iter()
            .map(|o| o.start.with_timezone(&Berlin).format("%d %H:%M").to_string())
            .collect();
        assert_eq!(times, vec!["08 10:00", "09 10:00", "10 12:00", "11 12:00"]);
    }

    #[test]
    fn override_moved_into_range_is_included() {
        let ics = "BEGIN:VCALENDAR
BEGIN:VEVENT
UID:s
DTSTART;TZID=Europe/Berlin:20261103T100000
DTEND;TZID=Europe/Berlin:20261103T110000
RRULE:FREQ=WEEKLY;COUNT=2
END:VEVENT
BEGIN:VEVENT
UID:s
RECURRENCE-ID;TZID=Europe/Berlin:20261110T100000
DTSTART;TZID=Europe/Berlin:20261124T100000
DTEND;TZID=Europe/Berlin:20261124T110000
END:VEVENT
END:VCALENDAR
";
        let (master, overrides) = series(ics);
        let (s, e) = range(2026, 11, 20, 2026, 12, 1);
        let occ = expand_series(&master, &overrides, s, e, &Berlin);
        assert_eq!(occ.len(), 1);
        assert_eq!(occ[0].start.with_timezone(&Berlin).format("%Y-%m-%d").to_string(), "2026-11-24");
    }

    #[test]
    fn simple_rule_roundtrip() {
        let rule = SimpleRule {
            freq: "WEEKLY".into(),
            interval: 2,
            byday: vec!["MO".into(), "WE".into()],
            count: Some(10),
            until: None,
        };
        let s = build_simple_rule(&rule);
        assert_eq!(s, "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10");
        assert_eq!(parse_simple_rule(&s), Some(rule));
        assert!(describe_rule(&s).contains("Every 2 weeks"));
        assert!(describe_rule(&s).contains("Mon, Wed"));
    }

    #[test]
    fn unsupported_rule_falls_back() {
        assert_eq!(parse_simple_rule("FREQ=MONTHLY;BYSETPOS=-1;BYDAY=FR"), None);
        assert_eq!(parse_simple_rule("FREQ=DAILY;BYMONTHDAY=1"), None);
        assert_eq!(parse_simple_rule("FREQ=DAILY;INTERVAL=3"), Some(SimpleRule {
            freq: "DAILY".into(),
            interval: 3,
            byday: vec![],
            count: None,
            until: None,
        }));
    }

    #[test]
    fn until_rule_builds() {
        let rule = SimpleRule {
            freq: "YEARLY".into(),
            interval: 1,
            byday: vec![],
            count: None,
            until: Some("20271231".into()),
        };
        assert_eq!(build_simple_rule(&rule), "FREQ=YEARLY;UNTIL=20271231T235959Z");
    }

    #[test]
    fn multi_day_event_spans_range_boundary() {
        let e = ev("BEGIN:VEVENT\nUID:f\nDTSTART;TZID=Europe/Berlin:20261130T220000\nDTEND;TZID=Europe/Berlin:20261201T020000\nEND:VEVENT");
        // range ends Dec 1 00:00 but the event runs until 02:00 -> still overlaps
        let (s, en) = range(2026, 11, 1, 2026, 12, 1);
        assert_eq!(expand_event(&e, s, en, &Berlin).len(), 1);
        let _ = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    }
}
