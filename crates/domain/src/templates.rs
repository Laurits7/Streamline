//! Day and week templates (D-113): where the routines and tasks of a template block go on
//! a day, and which day template a date gets from the week templates. Mirrored in
//! `web/src/lib/weekTemplates.ts` (for the Week view's preview).

use chrono::{Datelike, Duration, NaiveDate};

/// Lay items out one after another from the start of a block `[start, end)` (minutes from
/// local midnight), skipping `busy` intervals (already scheduled tasks, events). Returns a
/// start time per item, `None` for an item that doesn't fit (it goes on the day without a
/// time; later, shorter items may still fit).
pub fn lay_out(block: (u32, u32), busy: &[(u32, u32)], durations: &[u32]) -> Vec<Option<u32>> {
    let mut busy: Vec<(u32, u32)> = busy.iter().copied().filter(|(s, e)| e > s).collect();
    let mut cursor = block.0;
    durations
        .iter()
        .map(|&d| {
            let d = d.max(1);
            let mut t = cursor;
            loop {
                if t + d > block.1 {
                    return None;
                }
                // Jump past the latest-ending interval overlapping [t, t + d).
                match busy
                    .iter()
                    .filter(|(s, e)| *s < t + d && *e > t)
                    .map(|(_, e)| *e)
                    .max()
                {
                    Some(e) => t = e,
                    None => break,
                }
            }
            busy.push((t, t + d));
            cursor = t + d;
            Some(t)
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekMode {
    /// The usual week.
    Default,
    /// Every other week, from the week of its start date on.
    Alternate,
    /// The days from its start to its end date (inclusive).
    Range,
}

impl WeekMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "default" => Some(Self::Default),
            "alternate" => Some(Self::Alternate),
            "range" => Some(Self::Range),
            _ => None,
        }
    }
}

/// A week template: the day template (id) of each weekday, Monday first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Week {
    pub id: String,
    pub mode: WeekMode,
    pub days: [Option<String>; 7],
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
}

fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// The week template that decides a date: a range containing it (the first one, in the
/// given order, when ranges overlap), then the alternating one in its weeks, then the
/// default. A week template that matches decides even when it gives that weekday no
/// day template ("Holiday week" with free days).
pub fn week_for(date: NaiveDate, weeks: &[Week]) -> Option<&Week> {
    let range = weeks.iter().find(|w| {
        w.mode == WeekMode::Range
            && w.start.is_some_and(|s| s <= date)
            && w.end.is_some_and(|e| date <= e)
    });
    let alternate = || {
        weeks.iter().find(|w| {
            w.mode == WeekMode::Alternate
                && w.start.is_some_and(|s| {
                    let weeks = (monday(date) - monday(s)).num_days() / 7;
                    weeks >= 0 && weeks % 2 == 0
                })
        })
    };
    range
        .or_else(alternate)
        .or_else(|| weeks.iter().find(|w| w.mode == WeekMode::Default))
}

/// The day template a date gets (`None`: no blocks).
pub fn day_template_for(date: NaiveDate, weeks: &[Week]) -> Option<&str> {
    week_for(date, weeks)?.days[date.weekday().num_days_from_monday() as usize].as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn week(id: &str, mode: WeekMode, tpl: &str, start: Option<&str>, end: Option<&str>) -> Week {
        let mut days: [Option<String>; 7] = Default::default();
        days[..5].fill(Some(tpl.into()));
        Week {
            id: id.into(),
            mode,
            days,
            start: start.map(d),
            end: end.map(d),
        }
    }

    #[test]
    fn lays_items_one_after_another() {
        // Same cases as web/src/lib/weekTemplates.test.ts.
        assert_eq!(
            lay_out((480, 720), &[], &[30, 60, 15]),
            vec![Some(480), Some(510), Some(570)]
        );
        // A scheduled task at 08:30–09:00 is skipped over.
        assert_eq!(
            lay_out((480, 720), &[(510, 540)], &[30, 45]),
            vec![Some(480), Some(540)]
        );
        // Doesn't fit: no time; the next, shorter one still fits after the earlier ones.
        assert_eq!(
            lay_out((480, 600), &[], &[60, 90, 30]),
            vec![Some(480), None, Some(540)]
        );
        // Busy across the whole block.
        assert_eq!(lay_out((480, 600), &[(0, 1440)], &[30]), vec![None]);
        // Zero-length busy intervals don't count; a zero estimate takes a minute.
        assert_eq!(lay_out((480, 600), &[(480, 480)], &[0]), vec![Some(480)]);
    }

    #[test]
    fn resolves_week_templates() {
        let usual = week("usual", WeekMode::Default, "work", None, None);
        let b = week("b", WeekMode::Alternate, "office", Some("2026-10-21"), None);
        let holiday = {
            let mut w = week(
                "hol",
                WeekMode::Range,
                "x",
                Some("2026-12-21"),
                Some("2027-01-03"),
            );
            w.days = Default::default();
            w
        };
        let weeks = vec![usual.clone(), b.clone(), holiday.clone()];
        // 2026-10-19 is a Monday: the alternate's start week is 19–25 Oct.
        assert_eq!(day_template_for(d("2026-10-19"), &weeks), Some("office"));
        assert_eq!(day_template_for(d("2026-10-26"), &weeks), Some("work"));
        assert_eq!(day_template_for(d("2026-11-02"), &weeks), Some("office"));
        // Before its start week it doesn't apply.
        assert_eq!(day_template_for(d("2026-10-12"), &weeks), Some("work"));
        // Weekends have none in all of them.
        assert_eq!(day_template_for(d("2026-10-24"), &weeks), None);
        // The range wins, even with no day template.
        assert_eq!(week_for(d("2026-12-21"), &weeks).unwrap().id, "hol");
        assert_eq!(day_template_for(d("2026-12-21"), &weeks), None);
        assert_eq!(week_for(d("2027-01-03"), &weeks).unwrap().id, "hol");
        assert_eq!(week_for(d("2027-01-04"), &weeks).unwrap().id, "usual");
        assert_eq!(week_for(d("2027-01-11"), &weeks).unwrap().id, "b");
        // Overlapping ranges: the first in order wins.
        let short = week(
            "short",
            WeekMode::Range,
            "short",
            Some("2026-12-22"),
            Some("2026-12-23"),
        );
        let w2 = vec![short.clone(), holiday.clone(), usual.clone()];
        assert_eq!(day_template_for(d("2026-12-22"), &w2), Some("short"));
        assert_eq!(day_template_for(d("2026-12-24"), &w2), None);
        // Nothing at all.
        assert_eq!(day_template_for(d("2026-10-19"), &[]), None);
        assert_eq!(WeekMode::parse("range"), Some(WeekMode::Range));
        assert_eq!(WeekMode::parse("x"), None);
    }
}
