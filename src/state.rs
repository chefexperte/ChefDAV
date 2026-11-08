//! Global reactive state (Leptos signals), provided once at the app root.

use caldav::client::CalendarInfo;
use caldav::ics::{VEvent, VTodo};
use chrono::NaiveDate;
use chrono_tz::Tz;
use leptos::prelude::*;
use url::Url;

/// An authenticated CalDAV session. The password lives here (in memory) for
/// the lifetime of the tab; at rest it is kept in plain IndexedDB when
/// "Stay logged in" is enabled.
#[derive(Clone, Debug)]
pub struct Session {
    pub server: Url,
    pub username: String,
    pub password: String,
    pub principal: Url,
    pub home: Url,
}

#[derive(Clone)]
pub struct CalState {
    pub info: CalendarInfo,
    pub visible: RwSignal<bool>,
    pub color: String,
}

impl CalState {
    pub fn read_only(&self) -> bool {
        self.info.read_only
    }
}

/// An event as displayed: parsed VEVENT + where it came from (href, etag,
/// owning calendar, display color).
#[derive(Clone, Debug)]
pub struct DisplayEvent {
    pub href: String,
    pub etag: Option<String>,
    pub event: VEvent,
    /// `RECURRENCE-ID` exceptions for this series (same UID, same resource).
    pub overrides: Vec<VEvent>,
    pub calendar_href: String,
    pub color: String,
    pub read_only: bool,
}

/// A task as displayed.
#[derive(Clone, Debug, PartialEq)]
pub struct TodoItem {
    pub href: String,
    pub etag: Option<String>,
    pub todo: VTodo,
    pub calendar_href: String,
    pub color: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub date: NaiveDate,
    pub time: String,
    pub summary: String,
    pub location: String,
    pub color: String,
    pub href: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DayPopupState {
    pub date: NaiveDate,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SlotPopupState {
    pub date: NaiveDate,
    pub minutes: u32,
    pub all_day: bool,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModalState {
    New { date: NaiveDate },
    /// Read-only event details; the editor opens from here.
    Details {
        index: usize,
        /// Original recurrence start (`RECURRENCE-ID`) for exceptions.
        occurrence: Option<chrono::DateTime<chrono_tz::Tz>>,
        /// The actual displayed start (may be shifted by THISANDFUTURE).
        display_start: Option<chrono::DateTime<chrono_tz::Tz>>,
    },
    Edit {
        index: usize,
        occurrence: Option<chrono::DateTime<chrono_tz::Tz>>,
        display_start: Option<chrono::DateTime<chrono_tz::Tz>>,
    },
    NewTodo { date: NaiveDate },
    EditTodo { index: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarAction {
    Rename {
        href: String,
        name: String,
        color: String,
    },
    Delete {
        href: String,
        name: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ViewMode {
    Month,
    Week,
    Day,
    Agenda,
    Tasks,
}

impl ViewMode {
    pub fn label_key(self) -> &'static str {
        match self {
            ViewMode::Month => "month",
            ViewMode::Week => "week",
            ViewMode::Day => "day",
            ViewMode::Agenda => "agenda",
            ViewMode::Tasks => "tasks",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }
}

/// Brand accent — keep in sync with `--accent` in `style/tokens.css`.
pub const ACCENT: &str = "#2f9e7f";

/// Fallback palette when a calendar has no color configured server-side.
/// Mirrors `--cal-1` … `--cal-8` in `style/tokens.css`.
pub const PALETTE: [&str; 8] = [
    "#2f9e7f", "#3a86a8", "#6a9e3a", "#8e6fc1", "#d9a404", "#d1495b", "#0e9594", "#b0713a",
];

pub fn color_for(href: &str, server_color: &Option<String>, index: usize) -> String {
    if let Some(c) = server_color {
        if !c.is_empty() {
            return c.clone();
        }
    }
    // stable fallback based on the href so a calendar keeps its color
    let h = href.bytes().fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64));
    PALETTE[(h as usize + index) % PALETTE.len()].to_string()
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub session: RwSignal<Option<Session>>,
    pub calendars: RwSignal<Vec<CalState>>,
    pub events: RwSignal<Vec<DisplayEvent>>,
    pub todos: RwSignal<Vec<TodoItem>>,
    /// The date the current view is anchored on (month/week/agenda).
    pub anchor: RwSignal<NaiveDate>,
    pub view: RwSignal<ViewMode>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub modal: RwSignal<Option<ModalState>>,
    /// Day cell popover: the day's events + create actions.
    pub day_popup: RwSignal<Option<DayPopupState>>,
    /// Week/day time-slot popover: events at that time + create action.
    pub slot_popup: RwSignal<Option<SlotPopupState>>,
    /// Prefilled start time for a new event (hour, minute) from a week/day
    /// timeline click.
    pub new_event_time: RwSignal<Option<(u32, u32)>>,
    /// True when the new event was started from the all-day row.
    pub new_event_all_day: RwSignal<bool>,
    /// Events hidden behind a "+N more" badge in the week view.
    pub week_more: RwSignal<Option<Vec<usize>>>,
    pub new_calendar: RwSignal<bool>,
    pub calendar_action: RwSignal<Option<CalendarAction>>,
    /// Event hrefs selected for bulk operations.
    pub selected: RwSignal<Vec<String>>,
    pub search_query: RwSignal<String>,
    pub search_results: RwSignal<Vec<SearchHit>>,
    pub search_open: RwSignal<bool>,
    pub searching: RwSignal<bool>,
    /// Number of writes waiting in the offline outbox.
    pub pending: RwSignal<usize>,
    pub online: RwSignal<bool>,
    pub theme: RwSignal<Theme>,
    pub lang: RwSignal<Lang>,
    /// True while the app is trying to restore a saved session at startup.
    pub restoring: RwSignal<bool>,
    /// Error from the automatic session restore, shown on the login page.
    pub login_error: RwSignal<Option<String>>,
    pub tz: Tz,
}

impl AppState {
    pub fn new(tz: Tz) -> Self {
        let now = crate::browser::now_utc().with_timezone(&tz);
        Self {
            session: RwSignal::new(None),
            calendars: RwSignal::new(Vec::new()),
            events: RwSignal::new(Vec::new()),
            todos: RwSignal::new(Vec::new()),
            anchor: RwSignal::new(now.date_naive()),
            view: RwSignal::new(ViewMode::Month),
            loading: RwSignal::new(false),
            error: RwSignal::new(None),
            modal: RwSignal::new(None),
            day_popup: RwSignal::new(None),
            slot_popup: RwSignal::new(None),
            new_event_time: RwSignal::new(None),
            new_event_all_day: RwSignal::new(false),
            week_more: RwSignal::new(None),
            new_calendar: RwSignal::new(false),
            calendar_action: RwSignal::new(None),
            selected: RwSignal::new(Vec::new()),
            search_query: RwSignal::new(String::new()),
            search_results: RwSignal::new(Vec::new()),
            search_open: RwSignal::new(false),
            searching: RwSignal::new(false),
            pending: RwSignal::new(0),
            online: RwSignal::new(true),
            theme: RwSignal::new(Theme::Light),
            lang: RwSignal::new(Lang::En),
            restoring: RwSignal::new(true),
            login_error: RwSignal::new(None),
            tz,
        }
    }

    /// (year, month) of the current anchor.
    pub fn anchor_month(&self) -> (i32, u32) {
        use chrono::Datelike;
        let d = self.anchor.get();
        (d.year(), d.month())
    }

    /// The calendar (if any) that owns the given href.
    pub fn calendar_for(&self, calendar_href: &str) -> Option<CalState> {
        self.calendars
            .get()
            .into_iter()
            .find(|c| c.info.href == calendar_href)
    }

    /// First writable calendar that supports events.
    pub fn first_writable_event_calendar(&self) -> Option<CalState> {
        self.calendars
            .get()
            .into_iter()
            .find(|c| !c.read_only() && c.info.supports_events())
    }

    /// First writable calendar that supports tasks.
    pub fn first_writable_todo_calendar(&self) -> Option<CalState> {
        self.calendars
            .get()
            .into_iter()
            .find(|c| !c.read_only() && c.info.supports_todos())
    }
}
