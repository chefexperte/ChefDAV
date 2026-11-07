//! Create / edit / delete an event. Writes go to CalDAV with optimistic
//! concurrency (If-Match); offline writes are queued in the outbox.

use caldav::ics::{DtValue, EventDraft};
use caldav::recurrence::{build_simple_rule, parse_simple_rule, SimpleRule, WEEKDAY_CODES};
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, TimeZone};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api;
use crate::browser;
use crate::state::{AppState, DisplayEvent, ModalState};

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

    let (summary0, location0, desc0, date0, start0, end0, allday0, cal0, uid0) = match &existing {
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
                                Some(DtValue::Date(display.with_timezone(&tz).date_naive())),
                                Some(DtValue::Date(
                                    (display + dur).with_timezone(&tz).date_naive(),
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
                            Some(DtValue::Date(display.with_timezone(&tz).date_naive())),
                            Some(DtValue::Date(
                                (display + dur).with_timezone(&tz).date_naive(),
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
            let (d, s, e, ad) = match &dtstart {
                Some(DtValue::Date(dd)) => (
                    dd.format("%Y-%m-%d").to_string(),
                    "00:00".to_string(),
                    "00:00".to_string(),
                    true,
                ),
                Some(DtValue::DateTime(t)) => {
                    let lt = t.with_timezone(&tz);
                    let le = dtend
                        .as_ref()
                        .and_then(|x| x.to_datetime(&tz))
                        .unwrap_or(lt + Duration::hours(1))
                        .with_timezone(&tz);
                    (
                        lt.format("%Y-%m-%d").to_string(),
                        lt.format("%H:%M").to_string(),
                        le.format("%H:%M").to_string(),
                        false,
                    )
                }
                None => (
                    default_date.format("%Y-%m-%d").to_string(),
                    def_start.clone(),
                    def_end.clone(),
                    false,
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
                ev.calendar_href.clone(),
                ev.event.uid.clone(),
            )
        }
        None => (
            String::new(),
            String::new(),
            String::new(),
            default_date.format("%Y-%m-%d").to_string(),
            def_start.clone(),
            def_end.clone(),
            state.new_event_all_day.get(),
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
    let byday0 = if byday0.is_empty() {
        date_parsed.map(|d| vec![weekday_code(d)]).unwrap_or_default()
    } else {
        byday0
    };

    let summary = RwSignal::new(summary0);
    let location = RwSignal::new(location0);
    let description = RwSignal::new(desc0);
    let date_s = RwSignal::new(date0);
    let start_s = RwSignal::new(start0);
    let end_s = RwSignal::new(end0);
    let all_day = RwSignal::new(allday0);
    let cal_href = RwSignal::new(cal0);
    let uid = RwSignal::new(uid0);
    let existing = RwSignal::new(existing);

    let freq = RwSignal::new(freq0);
    let interval = RwSignal::new(interval0);
    let byday = RwSignal::new(byday0);
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

    let close = move |_| {
        state.new_event_time.set(None);
        state.new_event_all_day.set(false);
        state.modal.set(None);
    };

    let save = move |_| {
        error.set(None);
        if saving.get() || read_only.get() {
            return;
        }
        let date = NaiveDate::parse_from_str(&date_s.get(), "%Y-%m-%d");
        let mk = |t: &str| NaiveTime::parse_from_str(t, "%H:%M");
        let draft = (|| -> Result<EventDraft, String> {
            let date = date.map_err(|e| format!("{}{e}", crate::i18n::t(state.lang.get(), "bad_date")))?;
            let (start, end) = if all_day.get() {
                let s = tz
                    .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .ok_or(crate::i18n::t(state.lang.get(), "invalid_date"))?;
                let e = tz
                    .from_local_datetime(&(date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .ok_or(crate::i18n::t(state.lang.get(), "invalid_date"))?;
                (s, e)
            } else {
                let st = mk(&start_s.get()).map_err(|e| format!("{}{e}", crate::i18n::t(state.lang.get(), "bad_start")))?;
                let et = mk(&end_s.get()).map_err(|e| format!("{}{e}", crate::i18n::t(state.lang.get(), "bad_end")))?;
                let s = tz
                    .from_local_datetime(&date.and_time(st))
                    .single()
                    .ok_or(crate::i18n::t(state.lang.get(), "invalid_start"))?;
                let e = tz
                    .from_local_datetime(&date.and_time(et))
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
                            st.new_event_all_day.set(false);
                            st.modal.set(None);
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
                <Show when=move || read_only.get() fallback=|| ()>
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
                                on:input=move |ev| date_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-time" class:hidden=move || all_day.get()>
                            <span>{move || crate::i18n::t(state.lang.get(), "start")}</span>
                            <input type="time" required
                                prop:value=move || start_s.get()
                                on:input=move |ev| start_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-time" class:hidden=move || all_day.get()>
                            <span>{move || crate::i18n::t(state.lang.get(), "end")}</span>
                            <input type="time" required
                                prop:value=move || end_s.get()
                                on:input=move |ev| end_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-check">
                            <input type="checkbox"
                                prop:checked=move || all_day.get()
                                on:change=move |ev| all_day.set(event_target_checked(&ev)) />
                            <span>{move || crate::i18n::t(state.lang.get(), "all_day")}</span>
                        </label>
                    </div>
                    <label class="field">
                        <span>{move || crate::i18n::t(state.lang.get(), "calendar")}</span>
                        <select
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
                                                    on:click=move |_| toggle_day(day)
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
