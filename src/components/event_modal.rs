//! Create / edit / delete an event. Writes go to CalDAV with optimistic
//! concurrency (If-Match); offline writes are queued in the outbox.

use std::collections::HashMap;

use caldav::ics::{DtValue, EventDraft};
use caldav::recurrence::{build_simple_rule, parse_simple_rule, SimpleRule, WEEKDAY_CODES};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Offset, TimeZone};
use chrono_tz::Tz;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::browser;
use crate::state::{AppState, DisplayEvent, ModalState};

/// A short list of well-known zones. The picker keeps at most two per current
/// UTC offset, which covers every part of the world without 600 entries.
const COMMON_ZONES: &[&str] = &[
    "Europe/London",
    "Europe/Dublin",
    "Atlantic/Reykjavik",
    "Europe/Paris",
    "Europe/Berlin",
    "Europe/Madrid",
    "Europe/Rome",
    "Europe/Amsterdam",
    "Europe/Zurich",
    "Europe/Vienna",
    "Europe/Warsaw",
    "Europe/Stockholm",
    "Europe/Helsinki",
    "Europe/Athens",
    "Europe/Istanbul",
    "Europe/Moscow",
    "Europe/Lisbon",
    "America/New_York",
    "America/Chicago",
    "America/Denver",
    "America/Los_Angeles",
    "America/Toronto",
    "America/Vancouver",
    "America/Mexico_City",
    "America/Bogota",
    "America/Lima",
    "America/Halifax",
    "America/Caracas",
    "America/Noronha",
    "America/Sao_Paulo",
    "America/Argentina/Buenos_Aires",
    "America/Santiago",
    "America/St_Johns",
    "Asia/Tokyo",
    "Asia/Shanghai",
    "Asia/Hong_Kong",
    "Asia/Singapore",
    "Asia/Seoul",
    "Asia/Kolkata",
    "Asia/Kathmandu",
    "Asia/Dhaka",
    "Asia/Bishkek",
    "Asia/Yangon",
    "Asia/Tehran",
    "Asia/Dubai",
    "Asia/Bangkok",
    "Asia/Jakarta",
    "Asia/Jerusalem",
    "Asia/Riyadh",
    "Asia/Karachi",
    "Asia/Taipei",
    "Australia/Sydney",
    "Australia/Melbourne",
    "Australia/Brisbane",
    "Australia/Perth",
    "Australia/Adelaide",
    "Pacific/Auckland",
    "Pacific/Fiji",
    "Pacific/Tarawa",
    "Pacific/Chatham",
    "Pacific/Marquesas",
    "Pacific/Honolulu",
    "Pacific/Kiritimati",
    "Pacific/Pago_Pago",
    "Africa/Cairo",
    "Africa/Johannesburg",
    "Africa/Lagos",
    "Africa/Nairobi",
    "Africa/Casablanca",
    "Atlantic/Azores",
    "America/Anchorage",
];

fn zone_label(zone: Tz, now: chrono::DateTime<chrono::Utc>) -> String {
    let secs = now
        .with_timezone(&zone)
        .offset()
        .fix()
        .local_minus_utc();
    let sign = if secs < 0 { '-' } else { '+' };
    let abs = secs.abs();
    let h = abs / 3600;
    let m = (abs % 3600) / 60;
    let offset = if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    };
    // The only zone at UTC-12 is `Etc/GMT+12` (IANA inverts the sign there).
    let name = if zone.name() == "Etc/GMT+12" {
        "UTC-12"
    } else {
        zone.name()
    };
    format!("{name} ({offset})")
}

fn weekday_code(d: NaiveDate) -> String {
    WEEKDAY_CODES[d.weekday().num_days_from_monday() as usize].to_string()
}

fn plus_hour(t: &str) -> String {
    NaiveTime::parse_from_str(t, "%H:%M")
        .map(|t| (t + Duration::hours(1)).format("%H:%M").to_string())
        .unwrap_or_else(|_| "11:00".to_string())
}

#[component]
pub fn EventModal() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;

    let modal = state.modal.get().expect("modal is open");
    let (occurrence, display_start, existing): (
        Option<chrono::DateTime<chrono_tz::Tz>>,
        Option<chrono::DateTime<chrono_tz::Tz>>,
        Option<DisplayEvent>,
    ) = match modal {
        ModalState::Edit {
            index,
            occurrence,
            display_start,
        } => (
            occurrence,
            display_start,
            state.events.get().get(index).cloned(),
        ),
        _ => (None, None, None),
    };
    let is_edit = existing.is_some();
    // An existing event keeps the timezone it was saved in; a new one starts
    // in the browser's zone. The date/time fields are wall-clock in this zone.
    let initial_tz = existing
        .as_ref()
        .and_then(|e| e.event.dtstart.as_ref())
        .and_then(|d| match d {
            DtValue::DateTime(t) => Some(t.timezone()),
            _ => None,
        })
        .unwrap_or(tz);
    let event_tz = RwSignal::new(initial_tz);
    // Picker contents: UTC, the browser zone (marked) and two common zones at
    // the top; then at most two well-known zones per current UTC offset.
    let timezones = Memo::new(move |_| {
        let now = browser::now_utc();
        let lang = state.lang.get();
        let mut specs: Vec<(Tz, Option<String>)> = vec![
            (chrono_tz::UTC, None),
            (
                tz,
                Some(crate::i18n::t(lang, "your_zone").to_string()),
            ),
            (chrono_tz::America::New_York, None),
            (chrono_tz::Europe::London, None),
        ];
        if !specs.iter().any(|(z, _)| *z == initial_tz) {
            specs.push((initial_tz, None));
        }
        let mut top: Vec<(Tz, String)> = Vec::new();
        for (zone, note) in specs {
            if top.iter().any(|(z, _)| *z == zone) {
                continue;
            }
            let mut label = zone_label(zone, now);
            if let Some(note) = note {
                label.push_str(" · ");
                label.push_str(&note);
            }
            top.push((zone, label));
        }
        // Two representatives for EVERY current UTC offset, so no offset is
        // missing. Well-known names win over obscure ones.
        let mut buckets: HashMap<i32, Vec<Tz>> = HashMap::new();
        for zone in chrono_tz::TZ_VARIANTS {
            if top.iter().any(|(z, _)| *z == zone) {
                continue;
            }
            let secs = now
                .with_timezone(&zone)
                .offset()
                .fix()
                .local_minus_utc();
            buckets.entry(secs).or_default().push(zone);
        }
        let preference = |zone: &Tz| -> (usize, bool, &'static str) {
            let rank = COMMON_ZONES
                .iter()
                .position(|name| *name == zone.name())
                .unwrap_or(usize::MAX);
            let obscure = zone.name().starts_with("Etc/") || zone.name().contains("GMT");
            (rank, obscure, zone.name())
        };
        let mut offsets: Vec<i32> = buckets.keys().copied().collect();
        offsets.sort();
        let mut more: Vec<(Tz, String)> = Vec::new();
        for offset in offsets {
            let mut zones = buckets.remove(&offset).unwrap_or_default();
            zones.sort_by_key(|z| preference(z));
            for zone in zones.into_iter().take(2) {
                more.push((zone, zone_label(zone, now)));
            }
        }
        (top, more)
    });
    let is_recurring = existing
        .as_ref()
        .map(|e| e.event.rrule.is_some())
        .unwrap_or(false);
    let default_date = match modal {
        ModalState::New { date } | ModalState::NewTodo { date } => date,
        ModalState::Edit { .. } | ModalState::EditTodo { .. } | ModalState::Details { .. } => {
            caldav::time::today(&tz, browser::now_utc())
        }
    };

    // Default start/end: a week/day timeline click pre-fills the slot.
    let (def_start, def_end) = match state.new_event_time.get() {
        Some((h, m)) => {
            let s = format!("{h:02}:{m:02}");
            let e = plus_hour(&s);
            (s, e)
        }
        None => ("10:00".to_string(), "11:00".to_string()),
    };

    let (summary0, location0, desc0, date0, start0, end0, allday0, end_date0, cal0, uid0) = match &existing {
        Some(ev) => {
            let override_event = occurrence.and_then(|occ| {
                ev.overrides
                    .iter()
                    .find(|o| {
                        o.recurrence_id
                            .as_ref()
                            .and_then(|d| d.to_datetime(&tz))
                            == Some(occ)
                    })
                    .cloned()
            });
            let shown = override_event.as_ref().unwrap_or(&ev.event);
            let (dtstart, dtend) = if let Some(display) = display_start {
                if let Some(o) = &override_event {
                    if !o.recurrence_range {
                        (o.dtstart.clone(), o.dtend.clone())
                    } else {
                        let dur = caldav::recurrence::event_duration(o)
                            .or_else(|| caldav::recurrence::event_duration(&ev.event))
                            .unwrap_or_else(|| Duration::hours(1));
                        let all_day = o
                            .dtstart
                            .as_ref()
                            .map(|d| d.is_date())
                            .unwrap_or(false)
                            || ev
                                .event
                                .dtstart
                                .as_ref()
                                .map(|d| d.is_date())
                                .unwrap_or(false);
                        if all_day {
                            (
                                Some(DtValue::Date(display.with_timezone(&initial_tz).date_naive())),
                                Some(DtValue::Date(
                                    (display + dur).with_timezone(&initial_tz).date_naive(),
                                )),
                            )
                        } else {
                            (
                                Some(DtValue::DateTime(display)),
                                Some(DtValue::DateTime(display + dur)),
                            )
                        }
                    }
                } else {
                    let dur = caldav::recurrence::event_duration(&ev.event)
                        .unwrap_or_else(|| Duration::hours(1));
                    let all_day = ev
                        .event
                        .dtstart
                        .as_ref()
                        .map(|d| d.is_date())
                        .unwrap_or(false);
                    if all_day {
                        (
                            Some(DtValue::Date(display.with_timezone(&initial_tz).date_naive())),
                            Some(DtValue::Date(
                                (display + dur).with_timezone(&initial_tz).date_naive(),
                            )),
                        )
                    } else {
                        (
                            Some(DtValue::DateTime(display)),
                            Some(DtValue::DateTime(display + dur)),
                        )
                    }
                }
            } else {
                (shown.dtstart.clone(), shown.dtend.clone())
            };
            let (d, s, e, ad, ed) = match &dtstart {
                Some(DtValue::Date(dd)) => {
                    // all-day DTEND is exclusive: the last shown day is one before
                    let ed = match &dtend {
                        Some(DtValue::Date(x)) => *x - Duration::days(1),
                        _ => *dd,
                    };
                    (
                        dd.format("%Y-%m-%d").to_string(),
                        "00:00".to_string(),
                        "00:00".to_string(),
                        true,
                        ed.format("%Y-%m-%d").to_string(),
                    )
                }
                Some(DtValue::DateTime(t)) => {
                    let lt = t.with_timezone(&initial_tz);
                    let le = dtend
                        .as_ref()
                        .and_then(|x| x.to_datetime(&initial_tz))
                        .unwrap_or(lt + Duration::hours(1))
                        .with_timezone(&initial_tz);
                    (
                        lt.format("%Y-%m-%d").to_string(),
                        lt.format("%H:%M").to_string(),
                        le.format("%H:%M").to_string(),
                        false,
                        le.format("%Y-%m-%d").to_string(),
                    )
                }
                None => (
                    default_date.format("%Y-%m-%d").to_string(),
                    def_start.clone(),
                    def_end.clone(),
                    false,
                    default_date.format("%Y-%m-%d").to_string(),
                ),
            };
            (
                shown.summary.clone(),
                shown.location.clone(),
                shown.description.clone(),
                d,
                s,
                e,
                ad,
                ed,
                ev.calendar_href.clone(),
                ev.event.uid.clone(),
            )
        }
        None => (
            String::new(),
            String::new(),
            String::new(),
            default_date.format("%Y-%m-%d").to_string(),
            // "New event" from the details popover hands over the selected
            // event's start AND end time, so the copy lands in the same slot.
            state
                .new_event_time
                .get_untracked()
                .map(|(h, m)| format!("{h:02}:{m:02}"))
                .unwrap_or_else(|| def_start.clone()),
            state
                .new_event_end_time
                .get_untracked()
                .map(|(h, m)| format!("{h:02}:{m:02}"))
                .unwrap_or_else(|| def_end.clone()),
            state.new_event_all_day.get(),
            state
                .new_event_end_date
                .get_untracked()
                .unwrap_or(default_date)
                .format("%Y-%m-%d")
                .to_string(),
            state
                .first_writable_event_calendar()
                .map(|c| c.info.href)
                .unwrap_or_default(),
            browser::new_uid(),
        ),
    };

    // recurrence prefill
    let rrule0 = existing.as_ref().and_then(|e| e.event.rrule.clone());
    let simple0 = rrule0.as_deref().and_then(parse_simple_rule);
    let (freq0, interval0, byday0, end_mode0, count0, until0, raw0) = match (&rrule0, simple0) {
        (None, _) => ("NONE".to_string(), 1u32, Vec::new(), "never", 5u32, String::new(), String::new()),
        (Some(raw), Some(s)) => {
            let end_mode = if s.count.is_some() {
                "count"
            } else if s.until.is_some() {
                "until"
            } else {
                "never"
            };
            let until = s
                .until
                .as_deref()
                .map(|u| format!("{}-{}-{}", &u[0..4], &u[4..6], &u[6..8]))
                .unwrap_or_default();
            (s.freq, s.interval, s.byday, end_mode, s.count.unwrap_or(5), until, raw.clone())
        }
        (Some(raw), None) => (
            "ADVANCED".to_string(),
            1,
            Vec::new(),
            "never",
            5,
            String::new(),
            raw.clone(),
        ),
    };
    let date_parsed = NaiveDate::parse_from_str(&date0, "%Y-%m-%d").ok();
    let rule_has_byday = rrule0
        .as_deref()
        .and_then(parse_simple_rule)
        .map(|r| !r.byday.is_empty())
        .unwrap_or(false);
    let byday0 = if byday0.is_empty() {
        date_parsed.map(|d| vec![weekday_code(d)]).unwrap_or_default()
    } else {
        byday0
    };

    let summary = RwSignal::new(summary0);
    let location = RwSignal::new(location0);
    let description = RwSignal::new(desc0);
    let date_s = RwSignal::new(date0);
    let end_date_s = RwSignal::new(end_date0);
    let start_s = RwSignal::new(start0);
    let end_s = RwSignal::new(end0);
    let all_day = RwSignal::new(allday0);
    let cal_href = RwSignal::new(cal0);
    let uid = RwSignal::new(uid0);
    let existing = RwSignal::new(existing);

    // The editor can open before the calendar list has been read (cache paint,
    // then a network refresh). Fill the default calendar once it arrives, or
    // the modal would stay on an empty selection and claim it is read-only.
    Effect::new(move |_| {
        if existing.get().is_none() && cal_href.get().is_empty() {
            if let Some(c) = state.first_writable_event_calendar() {
                cal_href.set(c.info.href);
            }
        }
    });

    let freq = RwSignal::new(freq0);
    let interval = RwSignal::new(interval0);
    let byday = RwSignal::new(byday0);
    // Once the user (or an existing rule) chose weekdays, changing the date
    // must not silently rewrite BYDAY.
    let byday_touched = RwSignal::new(rule_has_byday);
    let end_mode = RwSignal::new(end_mode0.to_string());
    let count = RwSignal::new(count0);
    let until = RwSignal::new(until0);
    let raw_rule = RwSignal::new(raw0);
    // "this" / "future" / "all" — only relevant for recurring occurrences.
    let scope = RwSignal::new(if is_recurring && occurrence.is_some() {
        "this".to_string()
    } else {
        "all".to_string()
    });

    let saving = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // A new event with no writable calendar is not "read-only"; there is
    // simply nowhere to put it yet.
    let no_calendar = Memo::new(move |_| {
        existing.get().is_none()
            && state.calendars_loaded.get()
            && !state.backend_down.get()
            && state.first_writable_event_calendar().is_none()
    });

    let read_only = Memo::new(move |_| {
        existing
            .get()
            .map(|e| e.read_only)
            .unwrap_or_else(|| {
                state
                    .calendars
                    .get()
                    .iter()
                    .find(|c| c.info.href == cal_href.get())
                    .map(|c| c.read_only())
                    .unwrap_or(true)
            })
    });

    // Hide the read-only notice while the calendar list is still loading: the
    // empty selection only means "not loaded yet", and the effect above fills
    // it as soon as the list arrives.
    let show_read_only = Memo::new(move |_| {
        read_only.get()
            && !no_calendar.get()
            && (existing.get().is_some() || state.calendars_loaded.get())
    });

    let close = move |_| {
        state.new_event_time.set(None);
        state.new_event_end_time.set(None);
        state.new_event_end_date.set(None);
        state.new_event_all_day.set(false);
        state.modal.set(None);
    };

    let save = move |_| {
        error.set(None);
        if saving.get() || read_only.get() {
            return;
        }
        let date = NaiveDate::parse_from_str(&date_s.get(), "%Y-%m-%d");
        let edate = NaiveDate::parse_from_str(&end_date_s.get(), "%Y-%m-%d");
        let mk = |t: &str| NaiveTime::parse_from_str(t, "%H:%M");
        let draft = (|| -> Result<EventDraft, String> {
            let date = date.map_err(|e| format!("{}{e}", crate::i18n::t(state.lang.get(), "bad_date")))?;
            let edate = edate.map_err(|e| format!("{}{e}", crate::i18n::t(state.lang.get(), "bad_date")))?;
            let save_tz = event_tz.get_untracked();
            let (start, end) = if all_day.get() {
                // all-day DTEND is stored exclusive (the day after the last shown day)
                let s = save_tz
                    .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .ok_or(crate::i18n::t(state.lang.get(), "invalid_date"))?;
                let e = save_tz
                    .from_local_datetime(&(edate + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .ok_or(crate::i18n::t(state.lang.get(), "invalid_date"))?;
                (s, e)
            } else {
                let st = mk(&start_s.get()).map_err(|e| format!("{}{e}", crate::i18n::t(state.lang.get(), "bad_start")))?;
                let et = mk(&end_s.get()).map_err(|e| format!("{}{e}", crate::i18n::t(state.lang.get(), "bad_end")))?;
                let s = save_tz
                    .from_local_datetime(&date.and_time(st))
                    .single()
                    .ok_or(crate::i18n::t(state.lang.get(), "invalid_start"))?;
                let e = save_tz
                    .from_local_datetime(&edate.and_time(et))
                    .single()
                    .ok_or(crate::i18n::t(state.lang.get(), "invalid_end"))?;
                (s, e)
            };
            if end <= start {
                return Err(crate::i18n::t(state.lang.get(), "end_after_start").to_string());
            }
            let rule = match freq.get().as_str() {
                "NONE" => None,
                "ADVANCED" => {
                    let r = raw_rule.get().trim().to_string();
                    if r.is_empty() {
                        None
                    } else {
                        Some(r)
                    }
                }
                f => {
                    let mut days = byday.get();
                    if f == "WEEKLY" && days.is_empty() {
                        days = vec![weekday_code(date)];
                    }
                    let until_clean = until.get().replace('-', "");
                    let rule = SimpleRule {
                        freq: f.to_string(),
                        interval: interval.get().max(1),
                        byday: days,
                        count: if end_mode.get() == "count" {
                            Some(count.get().max(1))
                        } else {
                            None
                        },
                        until: if end_mode.get() == "until" && until_clean.len() == 8 {
                            Some(until_clean)
                        } else {
                            None
                        },
                    };
                    Some(build_simple_rule(&rule))
                }
            };
            let (exdate, rdate) = existing
                .get()
                .map(|e| (e.event.exdate.clone(), e.event.rdate.clone()))
                .unwrap_or_default();
            Ok(EventDraft {
                uid: uid.get(),
                summary: summary.get().trim().to_string(),
                description: description.get(),
                location: location.get().trim().to_string(),
                all_day: all_day.get(),
                start,
                end,
                rrule: rule,
                exdate,
                rdate,
            })
        })();
        match draft {
            Err(e) => error.set(Some(e)),
            Ok(draft) => {
                saving.set(true);
                let st = state;
                let cal = cal_href.get();
                let scope_val = scope.get();
                let existing_ev = existing.get();
                let is_override_save = existing_ev
                    .as_ref()
                    .map(|e| e.event.rrule.is_some() && occurrence.is_some() && scope_val != "all")
                    .unwrap_or(false);
                spawn_local(async move {
                    let result = if let (Some(ev), Some(occ)) = (existing_ev.as_ref(), occurrence) {
                        if is_override_save {
                            api::save_override(&st, ev, occ, scope_val == "future", &draft, false)
                                .await
                                .map(|_| false)
                        } else {
                            let overrides = ev.overrides.clone();
                            api::save_event(
                                &st,
                                &cal,
                                Some(&ev.href),
                                &draft,
                                ev.etag.as_deref(),
                                &overrides,
                            )
                            .await
                        }
                    } else {
                        api::save_event(&st, &cal, None, &draft, None, &[]).await
                    };
                    match result {
                        Ok(queued) => {
                            if !queued && !is_override_save {
                                api::refresh_events(&st).await;
                            }
                            st.new_event_time.set(None);
                            st.new_event_end_time.set(None);
                            st.new_event_end_date.set(None);
                            st.new_event_all_day.set(false);
                            st.modal.set(None);
                        }
                        Err(e) if api::backend_unavailable(&e) => {
                            st.backend_down.set(true);
                            error.set(Some(
                                crate::i18n::t(st.lang.get_untracked(), "server_backend_down")
                                    .to_string(),
                            ));
                            saving.set(false);
                        }
                        Err(e) => {
                            error.set(Some(format!("{e}")));
                            saving.set(false);
                        }
                    }
                });
            }
        }
    };

    let delete = move |_| {
        let Some(ev) = existing.get() else { return };
        let st = state;
        let scope_val = scope.get();
        let is_override_delete =
            ev.event.rrule.is_some() && scope_val != "all" && occurrence.is_some();
        spawn_local(async move {
            let result = if ev.event.rrule.is_some() && scope_val != "all" {
                if let Some(occ) = occurrence {
                    let dur = caldav::recurrence::event_duration(&ev.event)
                        .unwrap_or_else(|| Duration::hours(1));
                    let all_day = ev
                        .event
                        .dtstart
                        .as_ref()
                        .map(|d| d.is_date())
                        .unwrap_or(false);
                    let draft = EventDraft {
                        uid: ev.event.uid.clone(),
                        summary: ev.event.summary.clone(),
                        description: ev.event.description.clone(),
                        location: ev.event.location.clone(),
                        all_day,
                        start: occ,
                        end: occ + dur,
                        rrule: None,
                        exdate: Vec::new(),
                        rdate: Vec::new(),
                    };
                    api::save_override(&st, &ev, occ, scope_val == "future", &draft, true).await
                } else {
                    api::delete_event(&st, &ev.href, ev.etag.as_deref())
                        .await
                        .map(|_| ())
                }
            } else {
                api::delete_event(&st, &ev.href, ev.etag.as_deref())
                    .await
                    .map(|_| ())
            };
            match result {
                Ok(()) => {
                    if !is_override_delete {
                        api::refresh_events(&st).await;
                    }
                    st.modal.set(None);
                }
                Err(e) => error.set(Some(format!("{e}"))),
            }
        });
    };

    let toggle_day = move |day: &'static str| {
        byday.update(|days| {
            if let Some(pos) = days.iter().position(|d| d == day) {
                days.remove(pos);
            } else {
                days.push(day.to_string());
            }
        });
    };

    view! {
        <div class="modal-overlay" on:click=close>
            <div class="modal" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                <h3>{move || if is_edit { crate::i18n::t(state.lang.get(), "edit_event") } else { crate::i18n::t(state.lang.get(), "new_event") }}</h3>
                <Show when=move || no_calendar.get() fallback=|| ()>
                    <p class="banner-info">{move || crate::i18n::t(state.lang.get(), "create_calendar_first")}</p>
                </Show>
                <Show when=move || show_read_only.get() fallback=|| ()>
                    <p class="banner-info">{move || crate::i18n::t(state.lang.get(), "read_only_calendar")}</p>
                </Show>
                <fieldset class="modal-fields" disabled=move || read_only.get()>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "title")}</span>
                        <input type="text" autofocus placeholder=move || crate::i18n::t(state.lang.get(), "what_is_it")
                            prop:value=move || summary.get()
                            on:input=move |ev| summary.set(event_target_value(&ev)) />
                    </label>
                    <div class="field-row">
                        <label class="field">
                            <span>{move || crate::i18n::t(state.lang.get(), "date")}</span>
                            <input type="date" required
                                prop:value=move || date_s.get()
                                on:input=move |ev| {
                                    let value = event_target_value(&ev);
                                    date_s.set(value.clone());
                                    // keep the end date >= start date
                                    if let Ok(d) = NaiveDate::parse_from_str(&value, "%Y-%m-%d") {
                                        if let Ok(ed) = NaiveDate::parse_from_str(&end_date_s.get(), "%Y-%m-%d") {
                                            if ed < d {
                                                end_date_s.set(value.clone());
                                            }
                                        }
                                        if !byday_touched.get() {
                                            byday.set(vec![weekday_code(d)]);
                                        }
                                    }
                                } />
                        </label>
                        <label class="field field-time" class:hidden=move || all_day.get()>
                            <span>{move || crate::i18n::t(state.lang.get(), "start")}</span>
                            <input type="time" required
                                prop:value=move || start_s.get()
                                on:input=move |ev| start_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-check">
                            <span>{move || crate::i18n::t(state.lang.get(), "all_day")}</span>
                            <span class="switch">
                                <input type="checkbox"
                                    prop:checked=move || all_day.get()
                                    on:change=move |ev| all_day.set(event_target_checked(&ev)) />
                            </span>
                        </label>
                    </div>
                    <div class="field-row">
                        <label class="field">
                            <span>{move || crate::i18n::t(state.lang.get(), "end_date")}</span>
                            <input type="date" required
                                prop:value=move || end_date_s.get()
                                on:input=move |ev| end_date_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-time" class:hidden=move || all_day.get()>
                            <span>{move || crate::i18n::t(state.lang.get(), "end")}</span>
                            <input type="time" required
                                prop:value=move || end_s.get()
                                on:input=move |ev| end_s.set(event_target_value(&ev)) />
                        </label>
                    </div>
                    <Show when=move || !all_day.get() fallback=|| ()>
                        <label class="field">
                            <span>{move || crate::i18n::t(state.lang.get(), "time_zone")}</span>
                            <select
                                class="tz-select"
                                prop:value=move || event_tz.get().name().to_string()
                                on:change=move |ev| {
                                    if let Ok(zone) = event_target_value(&ev).parse::<chrono_tz::Tz>() {
                                        event_tz.set(zone);
                                    }
                                }
                            >
                                <optgroup label=move || crate::i18n::t(state.lang.get(), "suggested")>
                                    <For
                                        each=move || timezones.get().0
                                        key=|(z, _)| z.name()
                                        children=|(z, label)| view! { <option value=z.name()>{label}</option> }
                                    />
                                </optgroup>
                                <optgroup label=move || crate::i18n::t(state.lang.get(), "more_zones")>
                                    <For
                                        each=move || timezones.get().1
                                        key=|(z, _)| z.name()
                                        children=|(z, label)| view! { <option value=z.name()>{label}</option> }
                                    />
                                </optgroup>
                            </select>
                        </label>
                    </Show>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "calendar")}</span>
                        <select
                            class="calendar-select"
                            prop:value=move || cal_href.get()
                            on:change=move |ev| cal_href.set(event_target_value(&ev))
                        >
                            <For
                                each=move || {
                                    state.calendars.get()
                                        .iter()
                                        .filter(|c| c.info.supports_events() && (!c.read_only() || existing.get().map(|e| e.calendar_href == c.info.href).unwrap_or(false)))
                                        .map(|c| (c.info.href.clone(), c.info.display_name.clone()))
                                        .collect::<Vec<_>>()
                                }
                                key=|c| c.clone()
                                children=move |(href, name)| {
                                    view! { <option value=href>{name}</option> }
                                }
                            />
                        </select>
                    </label>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "location")}</span>
                        <input type="text" placeholder=move || crate::i18n::t(state.lang.get(), "optional")
                            prop:value=move || location.get()
                            on:input=move |ev| location.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "notes")}</span>
                        <textarea rows="3" placeholder=move || crate::i18n::t(state.lang.get(), "optional")
                            prop:value=move || description.get()
                            on:input=move |ev| description.set(event_target_value(&ev)) />
                    </label>

                    <div class="recurrence">
                        <label class="field">
                            <span>{move || crate::i18n::t(state.lang.get(), "repeat")}</span>
                            <select
                                class="freq-select"
                                prop:value=move || freq.get()
                                on:change=move |ev| freq.set(event_target_value(&ev))
                            >
                                <option value="NONE">{move || crate::i18n::t(state.lang.get(), "does_not_repeat")}</option>
                                <option value="DAILY">{move || crate::i18n::t(state.lang.get(), "daily")}</option>
                                <option value="WEEKLY">{move || crate::i18n::t(state.lang.get(), "weekly")}</option>
                                <option value="MONTHLY">{move || crate::i18n::t(state.lang.get(), "monthly")}</option>
                                <option value="YEARLY">{move || crate::i18n::t(state.lang.get(), "yearly")}</option>
                                <option value="ADVANCED">{move || crate::i18n::t(state.lang.get(), "advanced")}</option>
                            </select>
                        </label>
                        <Show when=move || !matches!(freq.get().as_str(), "NONE" | "ADVANCED") fallback=|| ()>
                            <div class="field-row">
                                <label class="field field-time">
                                    <span>{move || crate::i18n::t(state.lang.get(), "every")}</span>
                                    <input type="number" min="1" max="99"
                                        prop:value=move || interval.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                interval.set(v.max(1));
                                            }
                                        } />
                                </label>
                                <label class="field">
                                    <span>{move || crate::i18n::t(state.lang.get(), "ends")}</span>
                                    <select
                                        class="repeat-end-select"
                                        prop:value=move || end_mode.get()
                                        on:change=move |ev| end_mode.set(event_target_value(&ev))
                                    >
                                        <option value="never">{move || crate::i18n::t(state.lang.get(), "never")}</option>
                                        <option value="count">{move || crate::i18n::t(state.lang.get(), "after_n")}</option>
                                        <option value="until">{move || crate::i18n::t(state.lang.get(), "on_date")}</option>
                                    </select>
                                </label>
                                <Show when=move || end_mode.get() == "count" fallback=|| ()>
                                    <label class="field field-time">
                                        <span>{move || crate::i18n::t(state.lang.get(), "times")}</span>
                                        <input type="number" min="1" max="999"
                                            prop:value=move || count.get().to_string()
                                            on:input=move |ev| {
                                                if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                    count.set(v.max(1));
                                                }
                                            } />
                                    </label>
                                </Show>
                                <Show when=move || end_mode.get() == "until" fallback=|| ()>
                                    <label class="field">
                                        <span>{move || crate::i18n::t(state.lang.get(), "until")}</span>
                                        <input type="date"
                                            prop:value=move || until.get()
                                            on:input=move |ev| until.set(event_target_value(&ev)) />
                                    </label>
                                </Show>
                            </div>
                            <Show when=move || freq.get() == "WEEKLY" fallback=|| ()>
                                <div class="weekday-picker">
                                    <For
                                        each=|| WEEKDAY_CODES.to_vec()
                                        key=|d| *d
                                        children=move |day| {
                                            let idx = WEEKDAY_CODES.iter().position(|c| *c == day).unwrap_or(0);
                                            let letter = crate::i18n::weekday_short(state.lang.get(), idx)
                                                .chars()
                                                .next()
                                                .unwrap_or('?')
                                                .to_string();
                                            view! {
                                                <button
                                                    type="button"
                                                    class="weekday-btn"
                                                    class:active=move || byday.get().iter().any(|d| d == day)
                                                    on:click=move |_| {
                                                        byday_touched.set(true);
                                                        toggle_day(day);
                                                    }
                                                >
                                                    {letter}
                                                </button>
                                            }
                                        }
                                    />
                                </div>
                            </Show>
                        </Show>
                        <Show when=move || freq.get() == "ADVANCED" fallback=|| ()>
                            <label class="field">
                                <span>{move || crate::i18n::t(state.lang.get(), "rrule")}</span>
                                <input type="text" placeholder="FREQ=MONTHLY;BYDAY=1MO"
                                    prop:value=move || raw_rule.get()
                                    on:input=move |ev| raw_rule.set(event_target_value(&ev)) />
                            </label>
                        </Show>
                        <Show when=move || existing.get().map(|e| e.event.rrule.is_some()).unwrap_or(false) fallback=|| ()>
                            <Show
                                when=move || occurrence.is_some()
                                fallback=move || {
                                    view! {
                                        <p class="field-hint">{move || crate::i18n::t(state.lang.get(), "series_hint")}</p>
                                    }
                                }
                            >
                                <label class="field">
                                    <span>{move || crate::i18n::t(state.lang.get(), "apply_to")}</span>
                                    <select
                                        prop:value=move || scope.get()
                                        on:change=move |ev| scope.set(event_target_value(&ev))
                                    >
                                        <option value="this">{move || crate::i18n::t(state.lang.get(), "this_event")}</option>
                                        <option value="future">{move || crate::i18n::t(state.lang.get(), "this_and_following")}</option>
                                        <option value="all">{move || crate::i18n::t(state.lang.get(), "all_events")}</option>
                                    </select>
                                </label>
                            </Show>
                        </Show>
                    </div>
                </fieldset>
                <Show when=move || error.get().is_some() fallback=|| ()>
                    <p class="form-error">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="modal-actions">
                    <Show when=move || is_edit && !read_only.get() fallback=|| ()>
                        <button class="btn btn-danger" on:click=delete>{move || crate::i18n::t(state.lang.get(), "delete")}</button>
                    </Show>
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>{move || crate::i18n::t(state.lang.get(), "cancel")}</button>
                    <Show when=move || !read_only.get() fallback=|| ()>
                        <button class="btn btn-primary" disabled=move || saving.get() on:click=save>
                            {move || if saving.get() { crate::i18n::t(state.lang.get(), "saving") } else { crate::i18n::t(state.lang.get(), "save") }}
                        </button>
                    </Show>
                </div>
            </div>
        </div>
    }
}
