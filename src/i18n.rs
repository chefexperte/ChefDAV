//! Tiny i18n layer (English / German) for the UI.

use chrono::{Datelike, NaiveDate};

use crate::state::Lang;

pub fn month_name(lang: Lang, month: u32) -> &'static str {
    match lang {
        Lang::De => match month {
            1 => "Januar",
            2 => "Februar",
            3 => "März",
            4 => "April",
            5 => "Mai",
            6 => "Juni",
            7 => "Juli",
            8 => "August",
            9 => "September",
            10 => "Oktober",
            11 => "November",
            12 => "Dezember",
            _ => "?",
        },
        Lang::En => match month {
            1 => "January",
            2 => "February",
            3 => "March",
            4 => "April",
            5 => "May",
            6 => "June",
            7 => "July",
            8 => "August",
            9 => "September",
            10 => "October",
            11 => "November",
            12 => "December",
            _ => "?",
        },
    }
}

pub fn weekday_short(lang: Lang, index: usize) -> &'static str {
    match lang {
        Lang::De => ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"][index % 7],
        Lang::En => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][index % 7],
    }
}

pub fn month_title(lang: Lang, month: u32, year: i32) -> String {
    format!("{} {}", month_name(lang, month), year)
}

pub fn day_label(lang: Lang, date: NaiveDate) -> String {
    let wd = weekday_short(lang, date.weekday().num_days_from_monday() as usize);
    match lang {
        Lang::De => format!(
            "{} {}. {} {}",
            wd,
            date.day(),
            month_name(lang, date.month()),
            date.year()
        ),
        Lang::En => format!(
            "{} {} {} {}",
            wd,
            date.day(),
            month_name(lang, date.month()),
            date.year()
        ),
    }
}

pub fn agenda_title(lang: Lang, date: NaiveDate) -> String {
    match lang {
        Lang::De => format!(
            "Ab {}. {}",
            date.day(),
            month_name(lang, date.month())
        ),
        Lang::En => format!(
            "From {} {}",
            date.day(),
            month_name(lang, date.month())
        ),
    }
}

/// Localized description of an RRULE (mirrors `caldav::recurrence::describe_rule`).
pub fn describe_rule(lang: Lang, rule: &str) -> String {
    let Some(r) = caldav::recurrence::parse_simple_rule(rule) else {
        return rule.trim().to_string();
    };
    let every = if r.interval == 1 {
        t(
            lang,
            match r.freq.as_str() {
                "DAILY" => "daily",
                "WEEKLY" => "weekly",
                "MONTHLY" => "monthly",
                _ => "yearly",
            },
        )
        .to_string()
    } else {
        let unit = match (lang, r.freq.as_str()) {
            (Lang::De, "DAILY") => "Tage",
            (Lang::De, "WEEKLY") => "Wochen",
            (Lang::De, "MONTHLY") => "Monate",
            (Lang::De, _) => "Jahre",
            (Lang::En, "DAILY") => "days",
            (Lang::En, "WEEKLY") => "weeks",
            (Lang::En, "MONTHLY") => "months",
            (Lang::En, _) => "years",
        };
        match lang {
            Lang::De => format!("Alle {} {}", r.interval, unit),
            Lang::En => format!("Every {} {}", r.interval, unit),
        }
    };
    let days = if r.byday.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = r
            .byday
            .iter()
            .map(|d| {
                let idx = caldav::recurrence::WEEKDAY_CODES
                    .iter()
                    .position(|c| c == d)
                    .unwrap_or(0);
                weekday_short(lang, idx)
            })
            .collect();
        match lang {
            Lang::De => format!(" am {}", names.join(", ")),
            Lang::En => format!(" on {}", names.join(", ")),
        }
    };
    let end = match (r.count, r.until) {
        (Some(c), _) => match lang {
            Lang::De => format!(", {c}-mal"),
            Lang::En => format!(", {c} times"),
        },
        (_, Some(u)) => match lang {
            Lang::De => format!(", bis {u}"),
            Lang::En => format!(", until {u}"),
        },
        _ => String::new(),
    };
    format!("{every}{days}{end}")
}

pub fn week_label(lang: Lang, days: &[NaiveDate]) -> String {
    let first = days[0];
    let last = days[6];
    if first.month() == last.month() {
        match lang {
            Lang::De => format!(
                "{}. – {}. {} {}",
                first.day(),
                last.day(),
                month_name(lang, first.month()),
                first.year()
            ),
            Lang::En => format!(
                "{} – {} {} {}",
                first.day(),
                last.day(),
                month_name(lang, first.month()),
                first.year()
            ),
        }
    } else if first.year() == last.year() {
        match lang {
            Lang::De => format!(
                "{}. {} – {}. {} {}",
                first.day(),
                month_name(lang, first.month()),
                last.day(),
                month_name(lang, last.month()),
                first.year()
            ),
            Lang::En => format!(
                "{} {} – {} {} {}",
                first.day(),
                month_name(lang, first.month()),
                last.day(),
                month_name(lang, last.month()),
                first.year()
            ),
        }
    } else {
        match lang {
            Lang::De => format!(
                "{}. {} {} – {}. {} {}",
                first.day(),
                month_name(lang, first.month()),
                first.year(),
                last.day(),
                month_name(lang, last.month()),
                last.year()
            ),
            Lang::En => format!(
                "{} {} {} – {} {} {}",
                first.day(),
                month_name(lang, first.month()),
                first.year(),
                last.day(),
                month_name(lang, last.month()),
                last.year()
            ),
        }
    }
}

pub fn t(lang: Lang, key: &str) -> &'static str {
    match (lang, key) {
        (Lang::De, "new_event") => "Neuer Termin",
        (Lang::De, "new_task") => "Neue Aufgabe",
        (Lang::De, "new_calendar") => "Neuer Kalender",
        (Lang::De, "calendars") => "Kalender",
        (Lang::De, "log_out") => "Abmelden",
        (Lang::De, "today") => "Heute",
        (Lang::De, "month") => "Monat",
        (Lang::De, "week") => "Woche",
        (Lang::De, "day") => "Tag",
        (Lang::De, "agenda") => "Agenda",
        (Lang::De, "tasks") => "Aufgaben",
        (Lang::De, "search") => "Termine suchen…",
        (Lang::De, "delete_selected") => "Auswahl löschen",
        (Lang::De, "clear") => "Aufheben",
        (Lang::De, "selected") => "ausgewählt",
        (Lang::De, "save") => "Speichern",
        (Lang::De, "cancel") => "Abbrechen",
        (Lang::De, "delete") => "Löschen",
        (Lang::De, "login") => "Anmelden",
        (Lang::De, "connecting") => "Verbinde…",
        (Lang::De, "server") => "Kalender-Server",
        (Lang::De, "username") => "Benutzername",
        (Lang::De, "password") => "Passwort",
        (Lang::De, "stay_logged_in") => "Auf diesem Gerät angemeldet bleiben",
        (Lang::De, "map") => "Karte",
        (Lang::De, "loading_map") => "Standort wird gesucht…",
        (Lang::De, "open_in_osm") => "In OpenStreetMap öffnen",
        (Lang::De, "search_on_osm") => "Auf OpenStreetMap suchen",
        (Lang::De, "restoring_session") => "Sitzung wird wiederhergestellt…",
        (Lang::De, "session_expired") => "Deine gespeicherte Sitzung ist nicht mehr gültig. Bitte melde dich erneut an.",
        (Lang::De, "server_unreachable") => "Der Server ist nicht erreichbar. Prüfe deine Verbindung und versuche es erneut.",
        (Lang::De, "no_events") => "Nichts geplant",
        (Lang::De, "all_clear") => "Alles erledigt",
        (Lang::De, "read_only") => "Nur lesen",
        (Lang::De, "changes_pending") => "Änderung(en) warten auf Sync",
        (Lang::De, "no_matches") => "Keine passenden Termine.",
        (Lang::De, "more_events") => "Weitere Termine",
        (Lang::De, "close") => "Schließen",
        (Lang::De, "theme") => "Design",
        (Lang::De, "language") => "Sprache",
        (Lang::De, "previous") => "Zurück",
        (Lang::De, "next") => "Weiter",
        (Lang::De, "syncing") => "synchronisiere…",
        (Lang::De, "pending") => "ausstehend",
        (Lang::De, "nothing_here") => "Nichts hier.",
        (Lang::De, "title") => "Titel",
        (Lang::De, "what_is_it") => "Worum geht es?",
        (Lang::De, "date") => "Datum",
        (Lang::De, "start") => "Beginn",
        (Lang::De, "end") => "Ende",
        (Lang::De, "all_day") => "Ganztägig",
        (Lang::De, "calendar") => "Kalender",
        (Lang::De, "location") => "Ort",
        (Lang::De, "optional") => "Optional",
        (Lang::De, "notes") => "Notizen",
        (Lang::De, "repeat") => "Wiederholen",
        (Lang::De, "does_not_repeat") => "Keine Wiederholung",
        (Lang::De, "daily") => "Täglich",
        (Lang::De, "weekly") => "Wöchentlich",
        (Lang::De, "monthly") => "Monatlich",
        (Lang::De, "yearly") => "Jährlich",
        (Lang::De, "advanced") => "Erweitert (RRULE)",
        (Lang::De, "every") => "Alle",
        (Lang::De, "ends") => "Endet",
        (Lang::De, "never") => "Nie",
        (Lang::De, "after_n") => "Nach N Terminen",
        (Lang::De, "on_date") => "Am Datum",
        (Lang::De, "times") => "Anzahl",
        (Lang::De, "until") => "Bis",
        (Lang::De, "rrule") => "RRULE",
        (Lang::De, "apply_to") => "Anwenden auf",
        (Lang::De, "this_event") => "Dieser Termin",
        (Lang::De, "this_and_following") => "Dieser und folgende",
        (Lang::De, "all_events") => "Alle Termine",
        (Lang::De, "series_hint") => "Änderungen gelten für die ganze Serie.",
        (Lang::De, "edit_event") => "Termin bearbeiten",
        (Lang::De, "edit_task") => "Aufgabe bearbeiten",
        (Lang::De, "task") => "Aufgabe",
        (Lang::De, "what_needs_doing") => "Was ist zu tun?",
        (Lang::De, "due_date") => "Fällig am",
        (Lang::De, "priority") => "Priorität",
        (Lang::De, "done") => "Erledigt",
        (Lang::De, "delete_calendar") => "Kalender löschen",
        (Lang::De, "rename_calendar") => "Kalender umbenennen",
        (Lang::De, "name") => "Name",
        (Lang::De, "color") => "Farbe",
        (Lang::De, "custom_color") => "Eigene Farbe",
        (Lang::De, "confirm_delete_calendar") => "Diesen Kalender und alle Termine und Aufgaben löschen? Das kann nicht rückgängig gemacht werden.",
        (Lang::De, "create") => "Erstellen",
        (Lang::De, "creating") => "Erstelle…",
        (Lang::De, "saving") => "Speichere…",
        (Lang::De, "deleting") => "Lösche…",
        (Lang::De, "sharing") => "Freigabe",
        (Lang::De, "sharing_hint") => "Zugriff wird in Radicales [rights]-Konfiguration verwaltet. Trage dort andere Benutzer ein; sie sehen den Kalender nach dem Login.",
        (Lang::De, "calendar_url") => "Kalender-URL",
        (Lang::De, "copy") => "Kopieren",
        (Lang::De, "your_access") => "Dein Zugriff",
        (Lang::De, "unknown") => "unbekannt",
        (Lang::De, "changed_occurrence") => "geänderter Termin",
        (Lang::De, "repeats") => "Wiederholt",
        (Lang::De, "no_date") => "Kein Datum",
        (Lang::De, "days") => "Tage",
        (Lang::De, "today_pill") => "heute",
        (Lang::De, "no_due_date") => "Keine Fälligkeit",
        (Lang::De, "no_tasks") => "Noch keine Aufgaben. Lege eine an.",
        (Lang::De, "go_to_day") => "Zum Tag",
        (Lang::De, "read_only_calendar") => "Dieser Kalender ist schreibgeschützt.",
        (Lang::De, "tagline") => "Dein CalDAV-Kalender, frisch serviert — 100 % im Browser.",
        (Lang::De, "login_hint") => "Mit \"Auf diesem Gerät angemeldet bleiben\" werden die Zugangsdaten in der IndexedDB dieses Browsers gespeichert (vorerst unverschlüsselt). Auf geteilten Geräten ausschalten. Der Server sieht nur Basic Auth über deinen CORS-Proxy; wer sich anmelden darf, entscheidet Radicale (LDAP).",
        (Lang::De, "whats_new") => "Was ist neu",
        (Lang::De, "search_placeholder") => "Termine suchen…",
        (Lang::De, "no_matching_events") => "Keine passenden Termine.",
        (Lang::De, "no_events_next") => "Keine Termine in den nächsten 30 Tagen.",
        (Lang::De, "read_only_short") => "Nur lesen",
        (Lang::De, "delete_calendar_confirm") => "Löschen",
        (Lang::De, "rename_or_delete") => "Kalender umbenennen oder löschen",
        (Lang::De, "apply") => "Anwenden",
        (Lang::De, "all_day_lower") => "ganztägig",
        (Lang::De, "more") => "mehr",
        (Lang::De, "lock") => "Schreibgeschützt",
        (Lang::De, "new_event_short") => "Termin",
        (Lang::De, "new_task_short") => "Aufgabe",
        (Lang::De, "changelog") => "Changelog",
        (Lang::De, "custom") => "Eigene",
        (Lang::De, "dark") => "Dunkel",
        (Lang::De, "light") => "Hell",
        (Lang::De, "invalid_date") => "Ungültiges Datum",
        (Lang::De, "invalid_start") => "Ungültige Startzeit",
        (Lang::De, "invalid_end") => "Ungültige Endzeit",
        (Lang::De, "end_after_start") => "Das Ende muss nach dem Beginn liegen.",
        (Lang::De, "bad_date") => "Ungültiges Datum: ",
        (Lang::De, "bad_start") => "Ungültige Startzeit: ",
        (Lang::De, "bad_end") => "Ungültige Endzeit: ",
        (Lang::De, "calendar_name_placeholder") => "z. B. Arbeit",
        (Lang::De, "calendar_fallback") => "Kalender",
        (Lang::De, "no_events_day") => "Keine Termine",
        (Lang::De, "no_events_slot") => "Keine Termine zu dieser Zeit",
        (Lang::De, "new_all_day_event") => "Neuer ganztägiger Termin",

        (_, "new_event") => "New event",
        (_, "new_task") => "New task",
        (_, "new_calendar") => "New calendar",
        (_, "calendars") => "Calendars",
        (_, "log_out") => "Log out",
        (_, "today") => "Today",
        (_, "month") => "Month",
        (_, "week") => "Week",
        (_, "day") => "Day",
        (_, "agenda") => "Agenda",
        (_, "tasks") => "Tasks",
        (_, "search") => "Search events…",
        (_, "delete_selected") => "Delete selected",
        (_, "clear") => "Clear",
        (_, "selected") => "selected",
        (_, "save") => "Save",
        (_, "cancel") => "Cancel",
        (_, "delete") => "Delete",
        (_, "login") => "Log in",
        (_, "connecting") => "Connecting…",
        (_, "server") => "Calendar server",
        (_, "username") => "Username",
        (_, "password") => "Password",
        (_, "stay_logged_in") => "Stay logged in on this device",
        (_, "map") => "Map",
        (_, "loading_map") => "Looking up the location…",
        (_, "open_in_osm") => "Open in OpenStreetMap",
        (_, "search_on_osm") => "Search on OpenStreetMap",
        (_, "restoring_session") => "Restoring your session…",
        (_, "session_expired") => "Your saved session is no longer valid. Please log in again.",
        (_, "server_unreachable") => "Could not reach the server. Check your connection and try again.",
        (_, "no_events") => "Nothing on the horizon",
        (_, "all_clear") => "All clear",
        (_, "read_only") => "Read-only",
        (_, "changes_pending") => "change(s) pending sync",
        (_, "no_matches") => "No matching events.",
        (_, "more_events") => "More events",
        (_, "close") => "Close",
        (_, "theme") => "Theme",
        (_, "language") => "Language",
        (_, "previous") => "Previous",
        (_, "next") => "Next",
        (_, "syncing") => "syncing…",
        (_, "pending") => "pending",
        (_, "nothing_here") => "Nothing here.",
        (_, "title") => "Title",
        (_, "what_is_it") => "What is it?",
        (_, "date") => "Date",
        (_, "start") => "Start",
        (_, "end") => "End",
        (_, "all_day") => "All day",
        (_, "calendar") => "Calendar",
        (_, "location") => "Location",
        (_, "optional") => "Optional",
        (_, "notes") => "Notes",
        (_, "repeat") => "Repeat",
        (_, "does_not_repeat") => "Does not repeat",
        (_, "daily") => "Daily",
        (_, "weekly") => "Weekly",
        (_, "monthly") => "Monthly",
        (_, "yearly") => "Yearly",
        (_, "advanced") => "Advanced (raw RRULE)",
        (_, "every") => "Every",
        (_, "ends") => "Ends",
        (_, "never") => "Never",
        (_, "after_n") => "After N occurrences",
        (_, "on_date") => "On date",
        (_, "times") => "Times",
        (_, "until") => "Until",
        (_, "rrule") => "RRULE",
        (_, "apply_to") => "Apply to",
        (_, "this_event") => "This event",
        (_, "this_and_following") => "This and following",
        (_, "all_events") => "All events",
        (_, "series_hint") => "Changes apply to the whole series.",
        (_, "edit_event") => "Edit event",
        (_, "edit_task") => "Edit task",
        (_, "task") => "Task",
        (_, "what_needs_doing") => "What needs doing?",
        (_, "due_date") => "Due date",
        (_, "priority") => "Priority",
        (_, "done") => "Done",
        (_, "delete_calendar") => "Delete calendar",
        (_, "rename_calendar") => "Rename calendar",
        (_, "name") => "Name",
        (_, "color") => "Color",
        (_, "custom_color") => "Custom color",
        (_, "confirm_delete_calendar") => "Delete this calendar and all of its events and tasks? This cannot be undone.",
        (_, "create") => "Create",
        (_, "creating") => "Creating…",
        (_, "saving") => "Saving…",
        (_, "deleting") => "Deleting…",
        (_, "sharing") => "Sharing",
        (_, "sharing_hint") => "Access is managed by Radicale's [rights] configuration. Grant another user access there; they will see this calendar after logging in.",
        (_, "calendar_url") => "Calendar URL",
        (_, "copy") => "Copy",
        (_, "your_access") => "Your access",
        (_, "unknown") => "unknown",
        (_, "changed_occurrence") => "changed occurrence",
        (_, "repeats") => "Repeats",
        (_, "no_date") => "No date",
        (_, "days") => "days",
        (_, "today_pill") => "today",
        (_, "no_due_date") => "No due date",
        (_, "no_tasks") => "No tasks yet. Add one to get started.",
        (_, "go_to_day") => "Go to day",
        (_, "read_only_calendar") => "This calendar is read-only.",
        (_, "tagline") => "Your CalDAV calendar, served fresh — 100% in your browser.",
        (_, "login_hint") => "With \"Stay logged in\" on, your credentials are kept in this browser's IndexedDB (unencrypted for now). Turn it off on shared devices. The server only sees Basic auth via your CORS-enabled proxy; who may log in is decided by Radicale (LDAP).",
        (_, "whats_new") => "What's new",
        (_, "search_placeholder") => "Search events…",
        (_, "no_matching_events") => "No matching events.",
        (_, "no_events_next") => "No events in the next 30 days.",
        (_, "read_only_short") => "read-only",
        (_, "delete_calendar_confirm") => "Delete calendar",
        (_, "rename_or_delete") => "Rename or delete calendar",
        (_, "apply") => "Apply",
        (_, "all_day_lower") => "all day",
        (_, "more") => "more",
        (_, "lock") => "Read-only",
        (_, "new_event_short") => "New event",
        (_, "new_task_short") => "New task",
        (_, "changelog") => "Changelog",
        (_, "custom") => "Custom",
        (_, "dark") => "Dark",
        (_, "light") => "Light",
        (_, "invalid_date") => "Invalid date",
        (_, "invalid_start") => "Invalid start time",
        (_, "invalid_end") => "Invalid end time",
        (_, "end_after_start") => "End must be after the start.",
        (_, "bad_date") => "Bad date: ",
        (_, "bad_start") => "Bad start time: ",
        (_, "bad_end") => "Bad end time: ",
        (_, "calendar_name_placeholder") => "e.g. Work",
        (_, "calendar_fallback") => "Calendar",
        (_, "no_events_day") => "No events",
        (_, "no_events_slot") => "No events at this time",
        (_, "new_all_day_event") => "New all-day event",
        _ => "?",
    }
}
