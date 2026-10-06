//! The daily planning ritual (SPEC §6.2d): when reminders are due, and how much free
//! time a day has. Mirrored for instant UI feedback in `web/src/lib/planning.ts`.

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMode {
    /// Plan tomorrow in the evening.
    Evening,
    /// Plan today in the morning.
    Morning,
    Both,
}

impl PlanMode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "evening" => Self::Evening,
            "morning" => Self::Morning,
            "both" => Self::Both,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderKind {
    Evening,
    Morning,
}

impl ReminderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Evening => "plan_evening",
            Self::Morning => "plan_morning",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reminder {
    pub kind: ReminderKind,
    /// The day to be planned.
    pub target: NaiveDate,
}

/// Wall-clock moment of `time` within the logical day `day`: times before the day end
/// (e.g. 00:30 with a 04:00 day end) fall after midnight, still on the same logical day.
pub fn at_logical(day: NaiveDate, time: NaiveTime, day_end: NaiveTime) -> NaiveDateTime {
    if time < day_end {
        (day + Duration::days(1)).and_time(time)
    } else {
        day.and_time(time)
    }
}

/// Planning reminders that are due at local time `now` on logical day `today`.
/// A reminder stays due until the logical day ends; the caller sends each once and
/// skips days that are already planned.
pub fn due_reminders(
    now: NaiveDateTime,
    today: NaiveDate,
    day_end: NaiveTime,
    mode: PlanMode,
    evening: NaiveTime,
    morning: NaiveTime,
) -> Vec<Reminder> {
    let mut out = Vec::new();
    if matches!(mode, PlanMode::Morning | PlanMode::Both)
        && now >= at_logical(today, morning, day_end)
    {
        out.push(Reminder {
            kind: ReminderKind::Morning,
            target: today,
        });
    }
    if matches!(mode, PlanMode::Evening | PlanMode::Both)
        && now >= at_logical(today, evening, day_end)
    {
        out.push(Reminder {
            kind: ReminderKind::Evening,
            target: today + Duration::days(1),
        });
    }
    out
}

fn minute_of_day(t: NaiveTime) -> u32 {
    t.num_seconds_from_midnight() / 60
}

/// Free minutes in the window `[start, end)` after removing busy intervals
/// (`(start, duration_min)`). A window whose end is not after its start runs past
/// midnight (e.g. 18:00–02:00). Overlapping busy intervals count once.
pub fn free_minutes(start: NaiveTime, end: NaiveTime, busy: &[(NaiveTime, u32)]) -> u32 {
    let ws = minute_of_day(start);
    let mut we = minute_of_day(end);
    if we <= ws {
        we += 24 * 60;
    }
    let mut spans: Vec<(u32, u32)> = busy
        .iter()
        .map(|&(t, dur)| {
            let mut s = minute_of_day(t);
            if s < ws && we > 24 * 60 {
                s += 24 * 60; // after midnight in a window that crosses it
            }
            (s.max(ws), (s + dur).min(we))
        })
        .filter(|(s, e)| s < e)
        .collect();
    spans.sort();
    let mut busy_total = 0;
    let mut cur: Option<(u32, u32)> = None;
    for (s, e) in spans {
        match cur {
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                busy_total += ce - cs;
                cur = Some((s, e));
            }
            None => cur = Some((s, e)),
        }
    }
    if let Some((cs, ce)) = cur {
        busy_total += ce - cs;
    }
    (we - ws) - busy_total
}

/// The part of the window `[start, end)` that is still ahead at local time `now`
/// (for today's free time). `None` = the window is over. Windows past midnight are
/// returned unchanged.
pub fn remaining_window(
    start: NaiveTime,
    end: NaiveTime,
    now: NaiveTime,
) -> Option<(NaiveTime, NaiveTime)> {
    if end <= start {
        return Some((start, end));
    }
    if now >= end {
        None
    } else {
        Some((start.max(now), end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> NaiveTime {
        NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }
    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }
    fn at(m: u32, day: u32, time: &str) -> NaiveDateTime {
        d(m, day).and_time(t(time))
    }

    #[test]
    fn evening_reminder_targets_tomorrow() {
        let due = |now| {
            due_reminders(
                now,
                d(10, 6),
                t("04:00"),
                PlanMode::Evening,
                t("21:00"),
                t("07:30"),
            )
        };
        assert!(due(at(10, 6, "20:59")).is_empty());
        assert_eq!(
            due(at(10, 6, "21:00")),
            [Reminder {
                kind: ReminderKind::Evening,
                target: d(10, 7)
            }]
        );
        // Still due after midnight, until the logical day ends at 04:00.
        assert_eq!(due(at(10, 7, "01:00")).len(), 1);
    }

    #[test]
    fn after_midnight_planning_time_counts_for_the_same_logical_day() {
        // Plans at 00:30 with a 04:00 day end: that's the night after the 6th.
        let due = |now| {
            due_reminders(
                now,
                d(10, 6),
                t("04:00"),
                PlanMode::Evening,
                t("00:30"),
                t("07:30"),
            )
        };
        assert!(due(at(10, 6, "23:00")).is_empty());
        assert_eq!(due(at(10, 7, "00:30"))[0].target, d(10, 7));
    }

    #[test]
    fn morning_and_both() {
        let due =
            |now, mode| due_reminders(now, d(10, 6), t("04:00"), mode, t("21:00"), t("07:30"));
        assert!(due(at(10, 6, "07:00"), PlanMode::Morning).is_empty());
        assert_eq!(
            due(at(10, 6, "07:30"), PlanMode::Morning),
            [Reminder {
                kind: ReminderKind::Morning,
                target: d(10, 6)
            }]
        );
        assert!(
            due(at(10, 6, "22:00"), PlanMode::Morning)
                .iter()
                .all(|r| r.kind == ReminderKind::Morning)
        );
        assert_eq!(due(at(10, 6, "22:00"), PlanMode::Both).len(), 2);
    }

    #[test]
    fn free_time() {
        let w = (t("08:00"), t("22:00")); // 14 h = 840 min
        assert_eq!(free_minutes(w.0, w.1, &[]), 840);
        assert_eq!(free_minutes(w.0, w.1, &[(t("10:00"), 60)]), 780);
        // Overlaps count once; parts outside the window don't count.
        assert_eq!(
            free_minutes(w.0, w.1, &[(t("10:00"), 60), (t("10:30"), 60)]),
            750
        );
        assert_eq!(
            free_minutes(w.0, w.1, &[(t("07:00"), 90), (t("21:30"), 60)]),
            840 - 30 - 30
        );
        assert_eq!(free_minutes(w.0, w.1, &[(t("06:00"), 60)]), 840);
        // Fully booked never goes negative.
        assert_eq!(free_minutes(w.0, w.1, &[(t("08:00"), 900)]), 0);
    }

    #[test]
    fn remaining_part_of_today() {
        let w = (t("08:00"), t("22:00"));
        assert_eq!(remaining_window(w.0, w.1, t("07:00")), Some(w));
        assert_eq!(
            remaining_window(w.0, w.1, t("14:05")),
            Some((t("14:05"), t("22:00")))
        );
        assert_eq!(remaining_window(w.0, w.1, t("22:00")), None);
        // Scheduled blocks before now no longer count against what's left.
        let (s, e) = remaining_window(w.0, w.1, t("14:00")).unwrap();
        assert_eq!(
            free_minutes(s, e, &[(t("10:00"), 60), (t("15:00"), 30)]),
            450
        );
    }

    #[test]
    fn free_time_window_past_midnight() {
        // 18:00–02:00 = 8 h; a block at 01:00 for 30 min is inside it.
        assert_eq!(
            free_minutes(t("18:00"), t("02:00"), &[(t("01:00"), 30)]),
            450
        );
        assert_eq!(
            free_minutes(t("18:00"), t("02:00"), &[(t("23:30"), 60)]),
            420
        );
    }
}
