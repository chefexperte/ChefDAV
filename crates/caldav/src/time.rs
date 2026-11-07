//! Calendar math: month grids and ranges. Pure and unit-tested.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;

pub fn month_name(m: u32) -> &'static str {
    match m {
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
    }
}

pub const WEEKDAYS_MON_FIRST: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// All weeks (Monday-first, 7 days each) that cover the given month.
/// Days from neighbouring months are included so the grid is rectangular.
pub fn month_weeks(year: i32, month: u32) -> Vec<Vec<NaiveDate>> {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid first of month");
    let next_first = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    let mut day = first - Duration::days(first.weekday().num_days_from_monday() as i64);
    let mut weeks = Vec::new();
    while day < next_first {
        let mut week = Vec::with_capacity(7);
        for _ in 0..7 {
            week.push(day);
            day += Duration::days(1);
        }
        weeks.push(week);
    }
    weeks
}

/// [start, end) range in `tz` covering the whole month grid (including the
/// padding days from neighbouring months), so events on edge days show up.
pub fn month_range(year: i32, month: u32, tz: &Tz) -> (DateTime<Tz>, DateTime<Tz>) {
    let weeks = month_weeks(year, month);
    let first = weeks.first().and_then(|w| w.first()).copied();
    let last = weeks.last().and_then(|w| w.last()).copied();
    let to_local = |d: NaiveDate| -> DateTime<Tz> {
        let naive = d.and_hms_opt(0, 0, 0).unwrap();
        tz.from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(&naive))
    };
    match (first, last) {
        (Some(f), Some(l)) => (to_local(f), to_local(l + Duration::days(1))),
        _ => unreachable!("month_weeks always returns at least one week"),
    }
}

/// Today's date in `tz`, given the current instant.
pub fn today(tz: &Tz, now: DateTime<Utc>) -> NaiveDate {
    now.with_timezone(tz).date_naive()
}

/// Monday-first week containing `date`.
pub fn week_days(date: NaiveDate) -> Vec<NaiveDate> {
    let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
    (0..7).map(|i| monday + Duration::days(i)).collect()
}

/// [start, end) range covering the Monday-first week containing `date`.
pub fn week_range(date: NaiveDate, tz: &Tz) -> (DateTime<Tz>, DateTime<Tz>) {
    let days = week_days(date);
    let to_local = |d: NaiveDate| -> DateTime<Tz> {
        let naive = d.and_hms_opt(0, 0, 0).unwrap();
        tz.from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(&naive))
    };
    (
        to_local(days[0]),
        to_local(days[6] + Duration::days(1)),
    )
}

/// [start, end) range for an agenda covering `days` days from `date`.
pub fn agenda_range(date: NaiveDate, days: i64, tz: &Tz) -> (DateTime<Tz>, DateTime<Tz>) {
    let to_local = |d: NaiveDate| -> DateTime<Tz> {
        let naive = d.and_hms_opt(0, 0, 0).unwrap();
        tz.from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(&naive))
    };
    (to_local(date), to_local(date + Duration::days(days)))
}

/// Full label for a single day, e.g. "Sat 7 November 2026".
pub fn day_label(d: NaiveDate) -> String {
    let wd = WEEKDAYS_MON_FIRST[d.weekday().num_days_from_monday() as usize];
    format!("{} {} {} {}", wd, d.day(), month_name(d.month()), d.year())
}

/// Short label for a date range, e.g. "3 – 9 Nov 2026" or "30 Nov – 6 Dec 2026".
pub fn week_label(days: &[NaiveDate]) -> String {
    let first = days[0];
    let last = days[6];
    if first.month() == last.month() {
        format!(
            "{} – {} {} {}",
            first.day(),
            last.day(),
            month_name(first.month()),
            first.year()
        )
    } else if first.year() == last.year() {
        format!(
            "{} {} – {} {} {}",
            first.day(),
            month_name(first.month()),
            last.day(),
            month_name(last.month()),
            first.year()
        )
    } else {
        format!(
            "{} {} {} – {} {} {}",
            first.day(),
            month_name(first.month()),
            first.year(),
            last.day(),
            month_name(last.month()),
            last.year()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn november_2026_grid() {
        // 2026-11-01 is a Sunday, so the Monday-first grid starts on Oct 26.
        let weeks = month_weeks(2026, 11);
        assert_eq!(weeks.len(), 6);
        assert_eq!(weeks[0][0], NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(weeks[0][6], NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
        assert_eq!(*weeks.last().unwrap().last().unwrap(), NaiveDate::from_ymd_opt(2026, 12, 6).unwrap());
        for w in &weeks {
            assert_eq!(w.len(), 7);
        }
    }

    #[test]
    fn february_2026_grid_has_four_weeks() {
        // 2026-02-01 is a Sunday; Feb 2026 has 28 days -> exactly 4 weeks + padding
        let weeks = month_weeks(2026, 2);
        assert_eq!(weeks.len(), 5); // Jan 26 .. Mar 1
        assert_eq!(weeks[0][0], NaiveDate::from_ymd_opt(2026, 1, 26).unwrap());
    }

    #[test]
    fn week_days_monday_first() {
        // 2026-11-07 is a Saturday
        let d = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
        let week = week_days(d);
        assert_eq!(week[0], NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        assert_eq!(week[6], NaiveDate::from_ymd_opt(2026, 11, 8).unwrap());
        assert_eq!(week_label(&week), "2 – 8 November 2026");
    }

    #[test]
    fn day_label_format() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
        assert_eq!(day_label(d), "Sat 7 November 2026");
    }

    #[test]
    fn week_label_crosses_month_and_year() {
        let days = week_days(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
        assert_eq!(week_label(&days), "28 December 2026 – 3 January 2027");
    }

    #[test]
    fn month_range_covers_grid() {
        let tz: Tz = "Europe/Berlin".parse().unwrap();
        let (s, e) = month_range(2026, 11, &tz);
        assert_eq!(s.with_timezone(&tz).date_naive(), NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(e.with_timezone(&tz).date_naive(), NaiveDate::from_ymd_opt(2026, 12, 7).unwrap());
        assert!(s < e);
    }
}
