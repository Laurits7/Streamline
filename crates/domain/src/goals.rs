//! Long-term goals (SPEC §6.10, D-21: derived progress with a manual override) and when
//! the periodic review is due.

use chrono::{Datelike, Duration, NaiveDate};

/// What a goal's progress is derived from: its milestones and its linked work (linked
/// tasks plus every task in linked projects). Skipped / won't-do items don't count.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub milestones_done: u32,
    pub milestones: u32,
    pub tasks_done: u32,
    pub tasks: u32,
}

/// 0.0–1.0. The override wins; otherwise the share of done items (`None` = nothing to
/// derive from yet).
pub fn progress(c: Counts, manual: Option<f64>) -> Option<f64> {
    if let Some(m) = manual {
        return Some(m.clamp(0.0, 1.0));
    }
    let total = c.milestones + c.tasks;
    (total > 0).then(|| f64::from(c.milestones_done + c.tasks_done) / f64::from(total))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Off,
    Weekly,
    Monthly,
}

impl Cadence {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "off" => Self::Off,
            "weekly" => Self::Weekly,
            "monthly" => Self::Monthly,
            _ => return None,
        })
    }
}

/// The current review period's first day: the week's first day (`week_start`, ISO
/// weekday) or the 1st of the month.
pub fn period_start(cadence: Cadence, today: NaiveDate, week_start: u32) -> Option<NaiveDate> {
    match cadence {
        Cadence::Off => None,
        Cadence::Weekly => {
            let ws = week_start.clamp(1, 7);
            let offset = (today.weekday().number_from_monday() + 7 - ws) % 7;
            Some(today - Duration::days(i64::from(offset)))
        }
        Cadence::Monthly => today.with_day(1),
    }
}

/// A review is due once per period, from its first day on, unless one was done in it.
pub fn review_due(
    cadence: Cadence,
    last_review: Option<NaiveDate>,
    today: NaiveDate,
    week_start: u32,
) -> bool {
    match period_start(cadence, today, week_start) {
        None => false,
        Some(start) => last_review.is_none_or(|l| l < start),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn derived_and_overridden() {
        let c = Counts {
            milestones_done: 1,
            milestones: 2,
            tasks_done: 3,
            tasks: 6,
        };
        assert_eq!(progress(c, None), Some(0.5));
        assert_eq!(progress(c, Some(0.9)), Some(0.9));
        assert_eq!(progress(c, Some(1.4)), Some(1.0));
        assert_eq!(progress(Counts::default(), None), None);
    }

    #[test]
    fn reviews() {
        // Week starting Monday: Wed 7 Oct belongs to the week of Mon 5 Oct.
        assert_eq!(
            period_start(Cadence::Weekly, d("2026-10-07"), 1),
            Some(d("2026-10-05"))
        );
        assert_eq!(
            period_start(Cadence::Weekly, d("2026-10-07"), 7),
            Some(d("2026-10-04"))
        );
        assert_eq!(
            period_start(Cadence::Monthly, d("2026-10-07"), 1),
            Some(d("2026-10-01"))
        );
        assert!(review_due(Cadence::Weekly, None, d("2026-10-07"), 1));
        assert!(
            review_due(Cadence::Weekly, Some(d("2026-10-04")), d("2026-10-07"), 1),
            "last week's review"
        );
        assert!(!review_due(
            Cadence::Weekly,
            Some(d("2026-10-05")),
            d("2026-10-07"),
            1
        ));
        assert!(!review_due(
            Cadence::Monthly,
            Some(d("2026-10-02")),
            d("2026-10-31"),
            1
        ));
        assert!(review_due(
            Cadence::Monthly,
            Some(d("2026-10-31")),
            d("2026-11-01"),
            1
        ));
        assert!(!review_due(Cadence::Off, None, d("2026-10-07"), 1));
    }
}
