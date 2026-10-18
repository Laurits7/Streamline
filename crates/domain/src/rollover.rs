//! End-of-day behaviour, driven by task-type configuration (SPEC §6.2c).

use chrono::NaiveDate;

/// How a task type behaves when a planned task is not completed by day end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayEndBehavior {
    /// Rolls over to the next day until completed.
    Carry,
    /// Marked missed; does not carry over.
    Expire,
    /// Carries on within its window (flexible routines, Phase 3).
    Window,
    /// Carries on until its due date, then shows as overdue (Phase 3).
    Deadline,
}

impl DayEndBehavior {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "carry" => Self::Carry,
            "expire" => Self::Expire,
            "window" => Self::Window,
            "deadline" => Self::Deadline,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Carry => "carry",
            Self::Expire => "expire",
            Self::Window => "window",
            Self::Deadline => "deadline",
        }
    }
}

/// What happens to an unfinished planned task when its day has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Move the day-plan entry to the new day and bump the carry counter.
    CarryTo,
    /// Mark the task missed and leave the entry on its original day.
    Miss,
    /// Leave it alone (e.g. blocked tasks never miss).
    Keep,
}

/// Decide the outcome for an open task whose planned day has passed.
/// Window and deadline types carry over until their window/deadline logic lands
/// (Phase 3); they never miss early. A task waiting for results (D-70) needs no
/// action, so it carries on whatever its type.
pub fn day_end_outcome(behavior: DayEndBehavior, blocked: bool, waiting: bool) -> Outcome {
    if blocked {
        return Outcome::Keep;
    }
    if waiting {
        return Outcome::CarryTo;
    }
    match behavior {
        DayEndBehavior::Carry | DayEndBehavior::Window | DayEndBehavior::Deadline => {
            Outcome::CarryTo
        }
        DayEndBehavior::Expire => Outcome::Miss,
    }
}

/// What happens to an open flexible-routine task whose window ended before `today`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOutcome {
    /// Still inside its window.
    Keep,
    Miss,
    /// Carry over into the window containing `today` (task type `window_overflow = roll`).
    Roll,
}

pub fn window_outcome(window_end: NaiveDate, today: NaiveDate, roll: bool) -> WindowOutcome {
    if window_end >= today {
        WindowOutcome::Keep
    } else if roll {
        WindowOutcome::Roll
    } else {
        WindowOutcome::Miss
    }
}

/// A routine occurrence can't be done before its day (D-110): while both its occurrence
/// day and its due date (if any) are after `today`. Moving it earlier (its due date)
/// makes it doable.
pub fn too_early(occurrence: Option<NaiveDate>, due: Option<NaiveDate>, today: NaiveDate) -> bool {
    occurrence.is_some_and(|o| o > today) && due.is_none_or(|d| d > today)
}

/// Carry-on routines keep a single open occurrence (D-109). Given the open occurrences
/// of one routine (and its earlier versions) as `(id, day)`, once a newer one's day has
/// come the older ones roll on into it. `None` when nothing rolls on.
pub fn roll_on<K: Clone>(open: &[(K, NaiveDate)], today: NaiveDate) -> Option<RollOn<K>> {
    let (newest, day) = open
        .iter()
        .filter(|(_, d)| *d <= today)
        .max_by_key(|(_, d)| *d)?;
    let older: Vec<(K, NaiveDate)> = open.iter().filter(|(_, d)| d < day).cloned().collect();
    (!older.is_empty()).then(|| RollOn {
        newest: newest.clone(),
        day: *day,
        older,
    })
}

/// The result of [`roll_on`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollOn<K> {
    /// The newest occurrence whose day is at or before today, and its day.
    pub newest: K,
    pub day: NaiveDate,
    /// The ones it replaces (strictly earlier days).
    pub older: Vec<(K, NaiveDate)>,
}

/// The newest occurrence's carry count after older ones rolled on into it: the longest
/// any of them has been carried, counting the days up to the newest one's day.
pub fn carry_after_roll(newest: i32, older: &[(i32, NaiveDate)], day: NaiveDate) -> i32 {
    older
        .iter()
        .map(|(carry, d)| carry + (day - *d).num_days() as i32)
        .fold(newest, i32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routine_too_early() {
        let d = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        let today = d(12);
        assert!(too_early(Some(d(13)), Some(d(13)), today));
        assert!(too_early(Some(d(13)), None, today));
        assert!(!too_early(Some(d(12)), Some(d(12)), today));
        assert!(!too_early(Some(d(11)), Some(d(11)), today));
        // Moved earlier: due today, so it can be done.
        assert!(!too_early(Some(d(13)), Some(d(12)), today));
        // A flexible routine's current window started earlier and ends later.
        assert!(!too_early(Some(d(10)), Some(d(16)), today));
        assert!(!too_early(None, Some(d(20)), today));
    }

    #[test]
    fn rolling_on() {
        let d = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        // Mon (5th) and Thu (8th) left open; Mon (12th) has come: both roll on into it.
        let open = [("a", d(5)), ("b", d(8)), ("c", d(12)), ("next", d(13))];
        let RollOn { newest, day, older } = roll_on(&open, d(12)).unwrap();
        assert_eq!((newest, day), ("c", d(12)));
        assert_eq!(older, [("a", d(5)), ("b", d(8))]);
        // Before the newer one's day nothing rolls on.
        assert_eq!(roll_on(&open[..2], d(7)), None);
        // Thursday came: Monday's rolls on into it.
        assert_eq!(roll_on(&open[..2], d(9)).unwrap().older, [("a", d(5))]);
        // A single open occurrence, or only future ones: nothing.
        assert_eq!(roll_on(&[("a", d(5))], d(12)), None);
        assert_eq!(roll_on(&[("a", d(13)), ("b", d(14))], d(12)), None);
        // Two on the same day stay.
        assert_eq!(roll_on(&[("a", d(12)), ("b", d(12))], d(12)), None);
    }

    #[test]
    fn carry_moves_on() {
        let d = |day| NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        assert_eq!(carry_after_roll(0, &[(0, d(5)), (1, d(8))], d(12)), 7);
        assert_eq!(carry_after_roll(9, &[(0, d(11))], d(12)), 9);
        assert_eq!(carry_after_roll(0, &[], d(12)), 0);
    }

    #[test]
    fn outcomes() {
        assert_eq!(
            day_end_outcome(DayEndBehavior::Carry, false, false),
            Outcome::CarryTo
        );
        assert_eq!(
            day_end_outcome(DayEndBehavior::Expire, false, false),
            Outcome::Miss
        );
        assert_eq!(
            day_end_outcome(DayEndBehavior::Expire, true, false),
            Outcome::Keep
        );
        assert_eq!(
            day_end_outcome(DayEndBehavior::Carry, true, false),
            Outcome::Keep
        );
        // Waiting for results: never missed, carried on.
        assert_eq!(
            day_end_outcome(DayEndBehavior::Expire, false, true),
            Outcome::CarryTo
        );
        assert_eq!(
            day_end_outcome(DayEndBehavior::Carry, false, true),
            Outcome::CarryTo
        );
    }

    #[test]
    fn parse_roundtrip() {
        for b in [
            DayEndBehavior::Carry,
            DayEndBehavior::Expire,
            DayEndBehavior::Window,
            DayEndBehavior::Deadline,
        ] {
            assert_eq!(DayEndBehavior::parse(b.as_str()), Some(b));
        }
    }

    #[test]
    fn windows_end() {
        let d = |day| chrono::NaiveDate::from_ymd_opt(2026, 10, day).unwrap();
        assert_eq!(window_outcome(d(11), d(11), false), WindowOutcome::Keep);
        assert_eq!(window_outcome(d(11), d(12), false), WindowOutcome::Miss);
        assert_eq!(window_outcome(d(11), d(12), true), WindowOutcome::Roll);
    }
}
