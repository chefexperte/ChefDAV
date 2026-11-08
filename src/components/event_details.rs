//! Read-only event details. The editor opens only from the pencil button.

use caldav::ics::{DtValue, VEvent};
use chrono::Duration;
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::state::{AppState, Lang, ModalState};

fn format_range(
    lang: Lang,
    start: chrono::DateTime<chrono_tz::Tz>,
    end: chrono::DateTime<chrono_tz::Tz>,
    all_day: bool,
    tz: &chrono_tz::Tz,
) -> String {
    let start = start.with_timezone(tz);
    let end = end.with_timezone(tz);
    if all_day {
        let start_d = start.date_naive();
        let end_d = (end - Duration::days(1)).date_naive();
        if end_d > start_d {
            format!(
                "{} – {}",
                crate::i18n::day_label(lang, start_d),
                crate::i18n::day_label(lang, end_d)
            )
        } else {
            format!(
                "{} · {}",
                crate::i18n::day_label(lang, start_d),
                crate::i18n::t(lang, "all_day_lower")
            )
        }
    } else if end.date_naive() == start.date_naive() {
        format!(
            "{} · {}–{}",
            crate::i18n::day_label(lang, start.date_naive()),
            start.format("%H:%M"),
            end.format("%H:%M")
        )
    } else {
        format!(
            "{} {} – {} {}",
            crate::i18n::day_label(lang, start.date_naive()),
            start.format("%H:%M"),
            crate::i18n::day_label(lang, end.date_naive()),
            end.format("%H:%M")
        )
    }
}

fn duration_days(ev: &VEvent) -> Option<i64> {
    match (&ev.dtstart, &ev.dtend) {
        (Some(DtValue::Date(a)), Some(DtValue::Date(b))) => Some(b.signed_duration_since(*a).num_days()),
        _ => None,
    }
}

#[component]
pub fn EventDetails() -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let tz = state.tz;
    // Copy-link feedback; resets when the modal is closed (component unmounts).
    let copied = RwSignal::new(false);
    let copy_link = move |_| {
        if let Some(win) = web_sys::window() {
            if let Ok(href) = win.location().href() {
                let clipboard = win.navigator().clipboard();
                wasm_bindgen_futures::spawn_local(async move {
                    if clipboard.write_text(&href).await.is_ok() {
                        copied.set(true);
                    }
                });
            }
        }
    };
    let modal = state.modal.get().expect("details modal is open");
    let (index, occurrence, display_start) = match modal {
        ModalState::Details {
            index,
            occurrence,
            display_start,
        } => (index, occurrence, display_start),
        _ => (0, None, None),
    };
    let event = move || state.events.get().get(index).cloned();

    view! {
        <div class="modal-overlay" on:click=move |_| state.modal.set(None)>
            <div class="modal details-modal" on:click=move |e| e.stop_propagation() role="dialog" aria-modal="true">
                {move || {
                    event().map(|ev| {
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
                        let summary = shown.summary.clone();
                        let color = ev.color.clone();
                        let all_day = shown
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
                        let start = display_start
                            .or_else(|| shown.dtstart.as_ref().and_then(|d| d.to_datetime(&tz)))
                            .or_else(|| {
                                ev.event
                                    .dtstart
                                    .as_ref()
                                    .and_then(|d| d.to_datetime(&tz))
                            });
                        let dur = caldav::recurrence::event_duration(shown)
                            .or_else(|| caldav::recurrence::event_duration(&ev.event))
                            .unwrap_or_default();
                        let when = start
                            .map(|s| format_range(state.lang.get(), s, s + dur, all_day, &tz))
                            .unwrap_or_else(|| {
                                crate::i18n::t(state.lang.get(), "no_date").to_string()
                            });
                        let location = shown.location.clone();
                        let description = shown.description.clone();
                        let calendar = state
                            .calendar_for(&ev.calendar_href)
                            .map(|c| c.info.display_name)
                            .unwrap_or_else(|| crate::i18n::t(state.lang.get(), "calendar").to_string());
                        let read_only = ev.read_only;
                        let is_exception = override_event.is_some();
                        let rule = ev.event.rrule.clone();
                        let has_rule = rule.is_some();
                        let rule_text = rule
                            .as_deref()
                            .map(|r| crate::i18n::describe_rule(state.lang.get(), r))
                            .unwrap_or_default();
                        let days = duration_days(&ev.event);
                        let has_location = !location.is_empty();
                        let has_description = !description.is_empty();
                        let is_multi_day = days.map(|d| d > 1).unwrap_or(false);
                        view! {
                            <div class="details-head">
                                <span class="cal-dot" style=format!("background:{color}")></span>
                                <h3>{summary}</h3>
                            </div>
                            <p class="details-when">{when}</p>
                            <p class="details-meta">
                                <span class="details-cal">{calendar}</span>
                                <Show when=move || is_multi_day fallback=|| ()>
                                    <span class="details-days">{move || format!("{} {}", days.unwrap_or(0), crate::i18n::t(state.lang.get(), "days"))}</span>
                                </Show>
                                <Show when=move || is_exception fallback=|| ()>
                                    <span class="details-days">{move || crate::i18n::t(state.lang.get(), "changed_occurrence")}</span>
                                </Show>
                                <Show when=move || read_only fallback=|| ()>
                                    <span class="details-readonly">{move || crate::i18n::t(state.lang.get(), "read_only_short")}</span>
                                </Show>
                            </p>
                            <Show when=move || has_location fallback=|| ()>
                                <p class="details-row"><span class="details-label">{move || crate::i18n::t(state.lang.get(), "location")}</span>{location.clone()}</p>
                                <LocationMap location=location.clone() />
                            </Show>
                            <Show when=move || has_description fallback=|| ()>
                                <p class="details-row details-notes"><span class="details-label">{move || crate::i18n::t(state.lang.get(), "notes")}</span>{description.clone()}</p>
                            </Show>
                            <Show when=move || has_rule fallback=|| ()>
                                <p class="details-row">
                                    <span class="details-label">{move || crate::i18n::t(state.lang.get(), "repeats")}</span>
                                    {rule_text.clone()}
                                </p>
                            </Show>
                            <div class="modal-actions">
                                <span class="spacer"></span>
                                <button
                                    class="btn btn-ghost copy-link"
                                    title=move || crate::i18n::t(state.lang.get(), "copy_link")
                                    on:click=copy_link
                                >
                                    {move || crate::i18n::t(state.lang.get(), if copied.get() { "link_copied" } else { "copy_link" })}
                                </button>
                                <button class="btn btn-ghost" on:click=move |_| state.modal.set(None)>
                                    {move || crate::i18n::t(state.lang.get(), "close")}
                                </button>
                                <Show when=move || !read_only fallback=|| ()>
                                    <button
                                        class="btn btn-primary"
                                        on:click=move |_| state.modal.set(Some(ModalState::Edit {
                                            index,
                                            occurrence,
                                            display_start,
                                        }))
                                    >
                                        {move || format!("✎ {}", crate::i18n::t(state.lang.get(), "edit_event"))}
                                    </button>
                                </Show>
                            </div>
                        }
                    })
                }}
            </div>
        </div>
    }
}

#[derive(Clone, Debug, PartialEq)]
enum MapState {
    Hidden,
    Loading,
    Ready {
        embed: String,
        link: String,
        label: String,
    },
    NotFound {
        search: String,
    },
}

/// A small OpenStreetMap preview under the location row. Coordinates are
/// used directly; address-like strings are geocoded with Photon. Clicking
/// the preview opens the full OSM page in a new tab.
#[component]
fn LocationMap(location: String) -> impl IntoView {
    let state = use_context::<AppState>().unwrap();
    let map = RwSignal::new(MapState::Hidden);
    let loc = location.clone();

    Effect::new(move |_| {
        let lang = state.lang.get();
        let loc = loc.clone();
        if loc.trim().is_empty() {
            map.set(MapState::Hidden);
            return;
        }
        if let Some((lat, lon)) = crate::location::parse_coords(&loc) {
            map.set(MapState::Ready {
                embed: crate::location::osm_embed(lat, lon),
                link: crate::location::osm_link(lat, lon),
                label: String::new(),
            });
            return;
        }
        if !crate::location::looks_like_address(&loc) {
            map.set(MapState::Hidden);
            return;
        }
        map.set(MapState::Loading);
        spawn_local(async move {
            match geocode(&loc, lang).await {
                Some((lat, lon, label)) => map.set(MapState::Ready {
                    embed: crate::location::osm_embed(lat, lon),
                    link: crate::location::osm_link(lat, lon),
                    label,
                }),
                None => map.set(MapState::NotFound {
                    search: crate::location::osm_search(&loc),
                }),
            }
        });
    });

    view! {
        {move || match map.get() {
            MapState::Hidden => view! {
                <a
                    class="details-map-search"
                    href=crate::location::osm_search(&location)
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    {move || format!("{} ↗", crate::i18n::t(state.lang.get(), "search_on_osm"))}
                </a>
            }.into_any(),
            MapState::Loading => view! {
                <p class="details-map-loading">
                    {move || crate::i18n::t(state.lang.get(), "loading_map")}
                </p>
            }.into_any(),
            MapState::NotFound { search } => view! {
                <a
                    class="details-map-search"
                    href=search
                    target="_blank"
                    rel="noopener noreferrer"
                >
                    {move || format!("{} ↗", crate::i18n::t(state.lang.get(), "search_on_osm"))}
                </a>
            }.into_any(),
            MapState::Ready { embed, link, label } => {
                let has_label = !label.is_empty();
                let label_text = label.clone();
                view! {
                    <div class="details-map">
                        <iframe
                            src=embed
                            title=move || crate::i18n::t(state.lang.get(), "map")
                        ></iframe>
                        <a
                            class="details-map-overlay"
                            href=link
                            target="_blank"
                            rel="noopener noreferrer"
                            aria-label=move || crate::i18n::t(state.lang.get(), "open_in_osm")
                        >
                            <span class="details-map-open">
                                {move || format!("{} ↗", crate::i18n::t(state.lang.get(), "open_in_osm"))}
                            </span>
                        </a>
                    </div>
                    <Show when=move || has_label fallback=|| ()>
                        <p class="details-map-label">{label_text.clone()}</p>
                    </Show>
                }.into_any()
            }
        }}
    }
}

/// Geocode an address with Photon (OSM data, CORS-enabled).
async fn geocode(query: &str, lang: Lang) -> Option<(f64, f64, String)> {
    let q = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("q", query)
        .append_pair("limit", "1")
        .append_pair("lang", if lang == Lang::De { "de" } else { "en" })
        .finish();
    let resp = reqwest::Client::new()
        .get(format!("https://photon.komoot.io/api/?{q}"))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body = resp.text().await.ok()?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    let feature = json.get("features")?.as_array()?.first()?;
    let props = feature.get("properties")?;
    // Only trust results that actually resolved to an address; this keeps
    // room names like "Room 4" from pinning a random building abroad.
    if props.get("street").is_none() && props.get("housenumber").is_none() {
        return None;
    }
    let coords = feature.get("geometry")?.get("coordinates")?.as_array()?;
    let lon = coords.first()?.as_f64()?;
    let lat = coords.get(1)?.as_f64()?;
    Some((lat, lon, photon_label(props)))
}

fn photon_label(props: &serde_json::Value) -> String {
    let get = |key: &str| {
        props
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
    };
    let name = get("name");
    let street = get("street");
    let number = get("housenumber");
    let city = get("city");
    let mut parts = Vec::new();
    if !name.is_empty() {
        parts.push(name.to_string());
    }
    if !street.is_empty() {
        parts.push(if number.is_empty() {
            street.to_string()
        } else {
            format!("{street} {number}")
        });
    }
    if !city.is_empty() {
        parts.push(city.to_string());
    }
    parts.join(", ")
}
