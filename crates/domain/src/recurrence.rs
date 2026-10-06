//! Recurrence (SPEC §6.3) using the iCalendar RRULE standard via the `rrule` crate.
//! Occurrences are calendar *dates*: a routine's time of day (if any) is applied
//! later in the user's timezone, so DST never shifts a routine to another day.

use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use rrule::{RRuleSet, Tz};

use crate::time::week_bounds;

/// Parts of RRULE that don't fit date-based routines. `UNTIL`/`COUNT` are replaced by
/// the series' own end date (so "change all future" can split a series cleanly).
const UNSUPPORTED: &[&str] = &[
    "UNTIL", "COUNT", "BYHOUR", "BYMINUTE", "BYSECOND", "DTSTART",
];

/// Check a rule such as `FREQ=WEEKLY;BYDAY=MO,WE` (without the `RRULE:` prefix).
pub fn validate(rule: &str) -> Result<(), String> {
    let upper = rule.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return Err("empty rule".into());
    }
    for part in upper.split(';') {
        let key = part.split('=').next().unwrap_or("");
        if UNSUPPORTED.contains(&key) {
            return Err(format!(
                "{key} is not supported here; use the routine's start and end dates"
            ));
        }
        if key == "FREQ"
            && !matches!(
                part,
                "FREQ=DAILY" | "FREQ=WEEKLY" | "FREQ=MONTHLY" | "FREQ=YEARLY"
            )
        {
            return Err("FREQ must be DAILY, WEEKLY, MONTHLY or YEARLY".into());
        }
    }
    parse(rule, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).map(|_| ())
}

fn parse(rule: &str, dtstart: NaiveDate) -> Result<RRuleSet, String> {
    let text = format!(
        "DTSTART:{}T000000Z\nRRULE:{}",
        dtstart.format("%Y%m%d"),
        rule.trim().to_ascii_uppercase()
    );
    text.parse::<RRuleSet>().map_err(|e| e.to_string())
}

/// Dates on which the rule fires, starting at `dtstart`, within `[from, to]` (inclusive),
/// skipping `exdates`. At most 1000 dates.
pub fn occurrences(
    rule: &str,
    dtstart: NaiveDate,
    from: NaiveDate,
    to: NaiveDate,
    exdates: &[NaiveDate],
) -> Result<Vec<NaiveDate>, String> {
    if to < from || to < dtstart {
        return Ok(vec![]);
    }
    let set = parse(rule, dtstart)?;
    let utc = |d: NaiveDate| {
        Tz::UTC
            .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
            .unwrap()
    };
    let start = from.max(dtstart);
    // `after`/`before` are exclusive bounds: widen by a day and filter exactly below.
    let res = set
        .after(utc(start.pred_opt().unwrap_or(start)))
        .before(utc(to.succ_opt().unwrap_or(to)))
        .all(1000);
    Ok(res
        .dates
        .into_iter()
        .map(|d| d.date_naive())
        .filter(|d| *d >= start && *d <= to && !exdates.contains(d))
        .collect())
}

// ---- routines (series) ------------------------------------------------------------

/// The period a flexible routine counts in (D-3: fixed calendar periods).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Week,
    Month,
}

impl Window {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            _ => None,
        }
    }
}

/// First and last date of the window containing `date`.
pub fn window_bounds(date: NaiveDate, window: Window, week_start: u32) -> (NaiveDate, NaiveDate) {
    match window {
        Window::Week => week_bounds(date, week_start),
        Window::Month => {
            let first = date.with_day(1).unwrap();
            let next = if first.month() == 12 {
                NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
            }
            .unwrap();
            (first, next - Duration::days(1))
        }
    }
}

/// When a routine's occurrences happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schedule {
    /// On the dates of an RRULE (repeating tasks and fixed-time routines).
    Rule { rule: String },
    /// `times` occurrences in every window, doable on any day of it.
    Flexible { times: u32, window: Window },
}

/// One occurrence to create. `key` identifies it within its series forever, so it is
/// created at most once (even if the user deletes it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub key: String,
    /// The day it's for (rule), or the first day of its window (flexible).
    pub date: NaiveDate,
    /// Flexible only: last day of the window.
    pub window_end: Option<NaiveDate>,
    /// Flexible only: which of the window's `times` this is (1-based).
    pub index: Option<u32>,
}

/// Occurrences of a routine that start (rule) or whose window overlaps (flexible)
/// `[from, to]`, limited to the routine's lifetime `[dtstart, until]`.
pub fn plan_occurrences(
    schedule: &Schedule,
    dtstart: NaiveDate,
    until: Option<NaiveDate>,
    from: NaiveDate,
    to: NaiveDate,
    week_start: u32,
) -> Result<Vec<Occurrence>, String> {
    let to = until.map_or(to, |u| u.min(to));
    let from = from.max(dtstart);
    if to < from {
        return Ok(vec![]);
    }
    match schedule {
        Schedule::Rule { rule } => Ok(occurrences(rule, dtstart, from, to, &[])?
            .into_iter()
            .map(|date| Occurrence {
                key: date.format("%Y-%m-%d").to_string(),
                date,
                window_end: None,
                index: None,
            })
            .collect()),
        Schedule::Flexible { times, window } => {
            let mut out = vec![];
            let mut cursor = window_bounds(from, *window, week_start).0;
            while cursor <= to {
                let (start, end) = window_bounds(cursor, *window, week_start);
                for i in 1..=(*times).clamp(1, 31) {
                    out.push(Occurrence {
                        key: format!("{}#{i}", start.format("%Y-%m-%d")),
                        date: start,
                        window_end: Some(end),
                        index: Some(i),
                    });
                }
                cursor = end + Duration::days(1);
            }
            Ok(out)
        }
    }
}

/// Result of one occurrence (or one window of a flexible routine) for streaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Missed,
    /// Skipped on purpose: doesn't break a streak, doesn't extend it.
    Skipped,
    /// Not decided yet (today or later).
    Pending,
}

/// Current streak: consecutive `Done` going back from the most recent decided outcome.
/// `outcomes` must be in date order (oldest first).
pub fn streak(outcomes: &[Outcome]) -> u32 {
    let mut n = 0;
    for o in outcomes.iter().rev() {
        match o {
            Outcome::Done => n += 1,
            Outcome::Missed => break,
            Outcome::Skipped | Outcome::Pending => {}
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }
    fn occ(rule: &str, start: NaiveDate, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        occurrences(rule, start, from, to, &[]).unwrap()
    }

    #[test]
    fn daily_and_interval() {
        assert_eq!(
            occ("FREQ=DAILY", d(2026, 10, 5), d(2026, 10, 6), d(2026, 10, 8)),
            [d(2026, 10, 6), d(2026, 10, 7), d(2026, 10, 8)]
        );
        assert_eq!(
            occ(
                "FREQ=DAILY;INTERVAL=3",
                d(2026, 10, 1),
                d(2026, 10, 1),
                d(2026, 10, 10)
            ),
            [
                d(2026, 10, 1),
                d(2026, 10, 4),
                d(2026, 10, 7),
                d(2026, 10, 10)
            ]
        );
    }

    #[test]
    fn weekdays_and_specific_days() {
        // 2026-10-05 is a Monday.
        let wk = occ(
            "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
            d(2026, 10, 5),
            d(2026, 10, 5),
            d(2026, 10, 11),
        );
        assert_eq!(wk.len(), 5);
        assert_eq!(wk.last(), Some(&d(2026, 10, 9)));
        assert_eq!(
            occ(
                "FREQ=WEEKLY;BYDAY=TU,TH",
                d(2026, 10, 5),
                d(2026, 10, 5),
                d(2026, 10, 12)
            ),
            [d(2026, 10, 6), d(2026, 10, 8)]
        );
        // Every other week, starting from dtstart's week.
        assert_eq!(
            occ(
                "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO",
                d(2026, 10, 5),
                d(2026, 10, 5),
                d(2026, 11, 2)
            ),
            [d(2026, 10, 5), d(2026, 10, 19), d(2026, 11, 2)]
        );
    }

    #[test]
    fn month_ends_and_leap_years() {
        // Last day of the month.
        assert_eq!(
            occ(
                "FREQ=MONTHLY;BYMONTHDAY=-1",
                d(2026, 1, 1),
                d(2026, 1, 1),
                d(2026, 4, 30)
            ),
            [
                d(2026, 1, 31),
                d(2026, 2, 28),
                d(2026, 3, 31),
                d(2026, 4, 30)
            ]
        );
        // The 31st only happens in long months (RFC 5545: invalid dates are skipped).
        assert_eq!(
            occ(
                "FREQ=MONTHLY;BYMONTHDAY=31",
                d(2026, 1, 1),
                d(2026, 1, 1),
                d(2026, 4, 30)
            ),
            [d(2026, 1, 31), d(2026, 3, 31)]
        );
        // Last Friday of the month.
        assert_eq!(
            occ(
                "FREQ=MONTHLY;BYDAY=-1FR",
                d(2026, 10, 1),
                d(2026, 10, 1),
                d(2026, 11, 30)
            ),
            [d(2026, 10, 30), d(2026, 11, 27)]
        );
        // Feb 29 only in leap years.
        assert_eq!(
            occ(
                "FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=29",
                d(2026, 1, 1),
                d(2026, 1, 1),
                d(2032, 12, 31)
            ),
            [d(2028, 2, 29), d(2032, 2, 29)]
        );
    }

    #[test]
    fn range_bounds_dtstart_and_exceptions() {
        // Nothing before dtstart, even if `from` is earlier.
        assert_eq!(
            occ("FREQ=DAILY", d(2026, 10, 7), d(2026, 10, 1), d(2026, 10, 8)),
            [d(2026, 10, 7), d(2026, 10, 8)]
        );
        assert!(occ("FREQ=DAILY", d(2026, 10, 7), d(2026, 10, 1), d(2026, 10, 6)).is_empty());
        let ex = occurrences(
            "FREQ=DAILY",
            d(2026, 10, 1),
            d(2026, 10, 1),
            d(2026, 10, 3),
            &[d(2026, 10, 2)],
        )
        .unwrap();
        assert_eq!(ex, [d(2026, 10, 1), d(2026, 10, 3)]);
    }

    #[test]
    fn validation() {
        assert!(validate("FREQ=WEEKLY;BYDAY=MO").is_ok());
        assert!(validate("freq=daily").is_ok());
        assert!(validate("FREQ=HOURLY").is_err());
        assert!(validate("FREQ=DAILY;COUNT=5").is_err());
        assert!(validate("FREQ=DAILY;UNTIL=20261231T000000Z").is_err());
        assert!(validate("FREQ=WEEKLY;BYDAY=XX").is_err());
        assert!(validate("").is_err());
    }

    #[test]
    fn windows() {
        // Tuesday 2026-10-06: week Mon 5 - Sun 11; month October.
        assert_eq!(
            window_bounds(d(2026, 10, 6), Window::Week, 1),
            (d(2026, 10, 5), d(2026, 10, 11))
        );
        assert_eq!(
            window_bounds(d(2026, 10, 6), Window::Week, 7),
            (d(2026, 10, 4), d(2026, 10, 10))
        );
        assert_eq!(
            window_bounds(d(2026, 10, 6), Window::Month, 1),
            (d(2026, 10, 1), d(2026, 10, 31))
        );
        assert_eq!(
            window_bounds(d(2026, 12, 15), Window::Month, 1),
            (d(2026, 12, 1), d(2026, 12, 31))
        );
        assert_eq!(
            window_bounds(d(2028, 2, 10), Window::Month, 1),
            (d(2028, 2, 1), d(2028, 2, 29))
        );
    }

    #[test]
    fn rule_occurrences_respect_lifetime() {
        let s = Schedule::Rule {
            rule: "FREQ=DAILY".into(),
        };
        let o = plan_occurrences(
            &s,
            d(2026, 10, 5),
            Some(d(2026, 10, 7)),
            d(2026, 10, 1),
            d(2026, 10, 30),
            1,
        )
        .unwrap();
        assert_eq!(
            o.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
            ["2026-10-05", "2026-10-06", "2026-10-07"]
        );
        assert!(o.iter().all(|o| o.window_end.is_none()));
    }

    #[test]
    fn flexible_occurrences_per_window() {
        let s = Schedule::Flexible {
            times: 2,
            window: Window::Week,
        };
        // Started on a Wednesday: the current week still gets its two slots.
        let o =
            plan_occurrences(&s, d(2026, 10, 7), None, d(2026, 10, 7), d(2026, 10, 13), 1).unwrap();
        assert_eq!(
            o.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
            [
                "2026-10-05#1",
                "2026-10-05#2",
                "2026-10-12#1",
                "2026-10-12#2"
            ]
        );
        assert_eq!(
            (o[0].date, o[0].window_end, o[1].index),
            (d(2026, 10, 5), Some(d(2026, 10, 11)), Some(2))
        );
        let m = plan_occurrences(
            &Schedule::Flexible {
                times: 1,
                window: Window::Month,
            },
            d(2026, 10, 1),
            None,
            d(2026, 10, 20),
            d(2026, 11, 1),
            1,
        )
        .unwrap();
        assert_eq!(
            m.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
            ["2026-10-01#1", "2026-11-01#1"]
        );
    }

    #[test]
    fn streaks() {
        use Outcome::*;
        assert_eq!(streak(&[]), 0);
        assert_eq!(streak(&[Done, Missed, Done, Done, Pending]), 2);
        assert_eq!(
            streak(&[Done, Done, Skipped, Done]),
            3,
            "skips don't break a streak"
        );
        assert_eq!(streak(&[Done, Done, Missed]), 0);
        assert_eq!(streak(&[Pending, Pending]), 0);
    }
}
