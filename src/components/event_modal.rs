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
    let existing: Option<DisplayEvent> = match modal {
        ModalState::Edit { index } => state.events.get().get(index).cloned(),
        _ => None,
    };
    let is_edit = existing.is_some();
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
            let (d, s, e, ad) = match &ev.event.dtstart {
                Some(DtValue::Date(dd)) => (
                    dd.format("%Y-%m-%d").to_string(),
                    "00:00".to_string(),
                    "00:00".to_string(),
                    true,
                ),
                Some(DtValue::DateTime(t)) => {
                    let lt = t.with_timezone(&tz);
                    let le = ev
                        .event
                        .dtend
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
                ev.event.summary.clone(),
                ev.event.location.clone(),
                ev.event.description.clone(),
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
            let date = date.map_err(|e| format!("Bad date: {e}"))?;
            let (start, end) = if all_day.get() {
                let s = tz
                    .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .ok_or("Invalid date")?;
                let e = tz
                    .from_local_datetime(&(date + Duration::days(1)).and_hms_opt(0, 0, 0).unwrap())
                    .single()
                    .ok_or("Invalid date")?;
                (s, e)
            } else {
                let st = mk(&start_s.get()).map_err(|e| format!("Bad start time: {e}"))?;
                let et = mk(&end_s.get()).map_err(|e| format!("Bad end time: {e}"))?;
                let s = tz
                    .from_local_datetime(&date.and_time(st))
                    .single()
                    .ok_or("Invalid start time")?;
                let e = tz
                    .from_local_datetime(&date.and_time(et))
                    .single()
                    .ok_or("Invalid end time")?;
                (s, e)
            };
            if end <= start {
                return Err("End must be after the start.".to_string());
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
                let href = existing.get().map(|e| e.href);
                let etag = existing.get().and_then(|e| e.etag);
                spawn_local(async move {
                    match api::save_event(&st, &cal, href.as_deref(), &draft, etag.as_deref()).await
                    {
                        Ok(queued) => {
                            if !queued {
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
        spawn_local(async move {
            match api::delete_event(&st, &ev.href, ev.etag.as_deref()).await {
                Ok(queued) => {
                    if !queued {
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
                <h3>{if is_edit { "Edit event" } else { "New event" }}</h3>
                <Show when=move || read_only.get() fallback=|| ()>
                    <p class="banner-info">"This calendar is read-only."</p>
                </Show>
                <fieldset class="modal-fields" disabled=move || read_only.get()>
                    <label class="field">
                        <span>"Title"</span>
                        <input type="text" autofocus placeholder="What is it?"
                            prop:value=move || summary.get()
                            on:input=move |ev| summary.set(event_target_value(&ev)) />
                    </label>
                    <div class="field-row">
                        <label class="field">
                            <span>"Date"</span>
                            <input type="date" required
                                prop:value=move || date_s.get()
                                on:input=move |ev| date_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-time" class:hidden=move || all_day.get()>
                            <span>"Start"</span>
                            <input type="time" required
                                prop:value=move || start_s.get()
                                on:input=move |ev| start_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-time" class:hidden=move || all_day.get()>
                            <span>"End"</span>
                            <input type="time" required
                                prop:value=move || end_s.get()
                                on:input=move |ev| end_s.set(event_target_value(&ev)) />
                        </label>
                        <label class="field field-check">
                            <input type="checkbox"
                                prop:checked=move || all_day.get()
                                on:change=move |ev| all_day.set(event_target_checked(&ev)) />
                            <span>"All day"</span>
                        </label>
                    </div>
                    <label class="field">
                        <span>"Calendar"</span>
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
                        <span>"Location"</span>
                        <input type="text" placeholder="Optional"
                            prop:value=move || location.get()
                            on:input=move |ev| location.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>"Notes"</span>
                        <textarea rows="3" placeholder="Optional"
                            prop:value=move || description.get()
                            on:input=move |ev| description.set(event_target_value(&ev)) />
                    </label>

                    <div class="recurrence">
                        <label class="field">
                            <span>"Repeat"</span>
                            <select
                                prop:value=move || freq.get()
                                on:change=move |ev| freq.set(event_target_value(&ev))
                            >
                                <option value="NONE">"Does not repeat"</option>
                                <option value="DAILY">"Daily"</option>
                                <option value="WEEKLY">"Weekly"</option>
                                <option value="MONTHLY">"Monthly"</option>
                                <option value="YEARLY">"Yearly"</option>
                                <option value="ADVANCED">"Advanced (raw RRULE)"</option>
                            </select>
                        </label>
                        <Show when=move || !matches!(freq.get().as_str(), "NONE" | "ADVANCED") fallback=|| ()>
                            <div class="field-row">
                                <label class="field field-time">
                                    <span>"Every"</span>
                                    <input type="number" min="1" max="99"
                                        prop:value=move || interval.get().to_string()
                                        on:input=move |ev| {
                                            if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                                interval.set(v.max(1));
                                            }
                                        } />
                                </label>
                                <label class="field">
                                    <span>"Ends"</span>
                                    <select
                                        prop:value=move || end_mode.get()
                                        on:change=move |ev| end_mode.set(event_target_value(&ev))
                                    >
                                        <option value="never">"Never"</option>
                                        <option value="count">"After N occurrences"</option>
                                        <option value="until">"On date"</option>
                                    </select>
                                </label>
                                <Show when=move || end_mode.get() == "count" fallback=|| ()>
                                    <label class="field field-time">
                                        <span>"Times"</span>
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
                                        <span>"Until"</span>
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
                                            view! {
                                                <button
                                                    type="button"
                                                    class="weekday-btn"
                                                    class:active=move || byday.get().iter().any(|d| d == day)
                                                    on:click=move |_| toggle_day(day)
                                                >
                                                    {&day[0..1]}
                                                </button>
                                            }
                                        }
                                    />
                                </div>
                            </Show>
                        </Show>
                        <Show when=move || freq.get() == "ADVANCED" fallback=|| ()>
                            <label class="field">
                                <span>"RRULE"</span>
                                <input type="text" placeholder="FREQ=MONTHLY;BYDAY=1MO"
                                    prop:value=move || raw_rule.get()
                                    on:input=move |ev| raw_rule.set(event_target_value(&ev)) />
                            </label>
                        </Show>
                        <Show when=move || existing.get().map(|e| e.event.rrule.is_some()).unwrap_or(false) fallback=|| ()>
                            <p class="field-hint">"Changes apply to the whole series."</p>
                        </Show>
                    </div>
                </fieldset>
                <Show when=move || error.get().is_some() fallback=|| ()>
                    <p class="form-error">{move || error.get().unwrap_or_default()}</p>
                </Show>
                <div class="modal-actions">
                    <Show when=move || is_edit && !read_only.get() fallback=|| ()>
                        <button class="btn btn-danger" on:click=delete>"Delete"</button>
                    </Show>
                    <span class="spacer"></span>
                    <button class="btn btn-ghost" on:click=close>"Cancel"</button>
                    <Show when=move || !read_only.get() fallback=|| ()>
                        <button class="btn btn-primary" disabled=move || saving.get() on:click=save>
                            {move || if saving.get() { "Saving…" } else { "Save" }}
                        </button>
                    </Show>
                </div>
            </div>
        </div>
    }
}
