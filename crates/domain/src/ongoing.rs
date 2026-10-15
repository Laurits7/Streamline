//! Ongoing tasks (D-102): things you keep at over a period ("keep reading Dune",
//! "practise the piano this month") rather than finish in one go. Instead of a done tick
//! each day there's "worked on it today"; the task is finished when you say so.

use chrono::{Datelike, NaiveDate};

/// The most days kept per task (about five and a half years of daily marks).
pub const MAX_DAYS: usize = 2000;

/// Mark or unmark `day` in a sorted, de-duplicated list of `YYYY-MM-DD` days.
pub fn set_worked(days: &[String], day: NaiveDate, worked: bool) -> Vec<String> {
    let key = day.format("%Y-%m-%d").to_string();
    let mut out: Vec<String> = days.iter().filter(|d| **d != key).cloned().collect();
    if worked {
        out.push(key);
    }
    out.sort();
    out.dedup();
    if out.len() > MAX_DAYS {
        out.drain(..out.len() - MAX_DAYS);
    }
    out
}

/// Whether an ongoing task is on for `today`: started (no start = from its creation) and
/// not past its end (none = until finished).
pub fn in_period(start: Option<NaiveDate>, end: Option<NaiveDate>, today: NaiveDate) -> bool {
    start.is_none_or(|s| s <= today) && end.is_none_or(|e| today <= e)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// Days worked on it in total.
    pub days: usize,
    /// Days worked on it in the week containing `today` (weeks start on `week_start`, ISO 1–7).
    pub this_week: usize,
    /// Days of the period so far (start to today, or to the end if that's passed).
    pub period_days: Option<i64>,
    /// Consecutive days worked up to today (or yesterday, if today isn't marked yet).
    pub streak: usize,
    pub last: Option<NaiveDate>,
    pub worked_today: bool,
    /// The end date has passed: time to finish it (or extend it).
    pub over: bool,
}

pub fn progress(
    worked: &[String],
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    today: NaiveDate,
    week_start: u32,
) -> Progress {
    let mut days: Vec<NaiveDate> = worked
        .iter()
        .filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    days.sort();
    days.dedup();
    let offset = (today.weekday().number_from_monday() + 7 - week_start.clamp(1, 7)) % 7;
    let week_from = today - chrono::Duration::days(offset as i64);
    let this_week = days
        .iter()
        .filter(|d| **d >= week_from && **d <= today)
        .count();
    let worked_today = days.binary_search(&today).is_ok();
    let mut streak = 0;
    let mut day = if worked_today {
        today
    } else {
        today - chrono::Duration::days(1)
    };
    while days.binary_search(&day).is_ok() {
        streak += 1;
        day -= chrono::Duration::days(1);
    }
    let period_days = start.map(|s| {
        let upto = end.map_or(today, |e| e.min(today));
        ((upto - s).num_days() + 1).max(0)
    });
    Progress {
        days: days.len(),
        this_week,
        period_days,
        streak,
        last: days.last().copied(),
        worked_today,
        over: end.is_some_and(|e| e < today),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn marking_days() {
        let v = set_worked(&[], d("2026-10-15"), true);
        let v = set_worked(&v, d("2026-10-13"), true);
        let v = set_worked(&v, d("2026-10-15"), true);
        assert_eq!(v, vec!["2026-10-13", "2026-10-15"]);
        assert_eq!(set_worked(&v, d("2026-10-13"), false), vec!["2026-10-15"]);
    }

    #[test]
    fn keeps_the_latest_days() {
        let mut v = vec![];
        let mut day = d("2020-01-01");
        for _ in 0..MAX_DAYS + 5 {
            v = set_worked(&v, day, true);
            day += chrono::Duration::days(1);
        }
        assert_eq!(v.len(), MAX_DAYS);
        assert_eq!(v[0], "2020-01-06");
    }

    #[test]
    fn periods() {
        let today = d("2026-10-15");
        assert!(in_period(None, None, today));
        assert!(in_period(Some(today), Some(today), today));
        assert!(!in_period(Some(d("2026-10-16")), None, today));
        assert!(!in_period(None, Some(d("2026-10-14")), today));
    }

    #[test]
    fn progress_counts() {
        // Thursday 15 October 2026; weeks start on Monday the 12th.
        let worked: Vec<String> = ["2026-10-08", "2026-10-12", "2026-10-13", "2026-10-14"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let p = progress(
            &worked,
            Some(d("2026-10-01")),
            Some(d("2026-10-31")),
            d("2026-10-15"),
            1,
        );
        assert_eq!(p.days, 4);
        assert_eq!(p.this_week, 3);
        assert_eq!(p.period_days, Some(15));
        assert_eq!(
            p.streak, 3,
            "today isn't marked yet: the streak runs to yesterday"
        );
        assert!(!p.worked_today);
        assert!(!p.over);
        assert_eq!(p.last, Some(d("2026-10-14")));
        let worked = set_worked(&worked, d("2026-10-15"), true);
        let p = progress(&worked, None, None, d("2026-10-15"), 1);
        assert_eq!(p.streak, 4);
        assert!(p.worked_today);
        assert_eq!(p.period_days, None);
        // Weeks starting on Sunday: the 11th starts the week.
        assert_eq!(
            progress(&worked, None, None, d("2026-10-15"), 7).this_week,
            4
        );
        // Past its end.
        let p = progress(
            &worked,
            Some(d("2026-10-01")),
            Some(d("2026-10-10")),
            d("2026-10-15"),
            1,
        );
        assert!(p.over);
        assert_eq!(p.period_days, Some(10));
    }
}
