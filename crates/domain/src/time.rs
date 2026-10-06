//! The "logical date": which local calendar day an instant belongs to for a user,
//! given their timezone and the time of day at which their day ends (e.g. 04:00).

use chrono::{
    DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone,
    Timelike, Utc,
};
use chrono_tz::Tz;

/// Parse an `HH:MM` string.
pub fn parse_hhmm(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

/// Parse an IANA timezone name, e.g. `Europe/Tallinn`.
pub fn parse_tz(s: &str) -> Option<Tz> {
    s.parse().ok()
}

/// The local date that `now` belongs to. Times before `day_end` count towards the
/// previous day, so with `day_end = 04:00`, 01:30 on the 6th is still the 5th.
pub fn logical_date(now: DateTime<Utc>, tz: Tz, day_end: NaiveTime) -> NaiveDate {
    let local = now.with_timezone(&tz).naive_local();
    let shifted = local - Duration::seconds(i64::from(day_end.num_seconds_from_midnight()));
    shifted.date()
}

/// Resolve a local wall-clock time to an instant. In a DST gap (the time doesn't
/// exist) the first valid instant after it is used; in an overlap, the earlier one.
pub fn resolve_local(tz: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    let mut t = local;
    for _ in 0..4 {
        match tz.from_local_datetime(&t) {
            LocalResult::Single(d) => return d.with_timezone(&Utc),
            LocalResult::Ambiguous(a, _) => return a.with_timezone(&Utc),
            LocalResult::None => t += Duration::minutes(30),
        }
    }
    Utc.from_utc_datetime(&local)
}

/// The instants at which a logical day starts and ends (half-open `[start, end)`):
/// from `day_end` on `date` to `day_end` on the next day, in the user's timezone.
/// Usually 24 h; 23 h or 25 h across DST changes.
pub fn day_bounds(date: NaiveDate, tz: Tz, day_end: NaiveTime) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = resolve_local(tz, date.and_time(day_end));
    let end = resolve_local(tz, (date + Duration::days(1)).and_time(day_end));
    (start, end)
}

/// First and last date of the week containing `date`. `week_start` is an ISO
/// weekday: 1 = Monday ... 7 = Sunday.
pub fn week_bounds(date: NaiveDate, week_start: u32) -> (NaiveDate, NaiveDate) {
    let ws = week_start.clamp(1, 7);
    let offset = (date.weekday().number_from_monday() + 7 - ws) % 7;
    let start = date - Duration::days(i64::from(offset));
    (start, start + Duration::days(6))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn midnight_day_end_is_plain_local_date() {
        let tz: Tz = "Europe/Tallinn".parse().unwrap();
        // 22:30 UTC on Oct 5 = 01:30 local on Oct 6 (UTC+3 in summer time)
        let now = Utc.with_ymd_and_hms(2026, 10, 5, 22, 30, 0).unwrap();
        assert_eq!(logical_date(now, tz, NaiveTime::MIN), d(2026, 10, 6));
    }

    #[test]
    fn late_night_belongs_to_previous_day() {
        let tz: Tz = "Europe/Tallinn".parse().unwrap();
        let four = parse_hhmm("04:00").unwrap();
        let now = Utc.with_ymd_and_hms(2026, 10, 5, 22, 30, 0).unwrap(); // 01:30 local
        assert_eq!(logical_date(now, tz, four), d(2026, 10, 5));
        let now = Utc.with_ymd_and_hms(2026, 10, 6, 1, 0, 0).unwrap(); // 04:00 local
        assert_eq!(logical_date(now, tz, four), d(2026, 10, 6));
    }

    #[test]
    fn dst_autumn_transition() {
        // Tallinn leaves DST on 2026-10-25 at 04:00 local (01:00 UTC) -> 03:00.
        let tz: Tz = "Europe/Tallinn".parse().unwrap();
        let four = parse_hhmm("04:00").unwrap();
        // 00:30 UTC = 03:30 summer time, still before day end -> previous day
        let now = Utc.with_ymd_and_hms(2026, 10, 25, 0, 30, 0).unwrap();
        assert_eq!(logical_date(now, tz, four), d(2026, 10, 24));
        // 01:30 UTC = 03:30 winter time, still before 04:00 -> previous day
        let now = Utc.with_ymd_and_hms(2026, 10, 25, 1, 30, 0).unwrap();
        assert_eq!(logical_date(now, tz, four), d(2026, 10, 24));
        // 02:00 UTC = 04:00 winter time -> new day
        let now = Utc.with_ymd_and_hms(2026, 10, 25, 2, 0, 0).unwrap();
        assert_eq!(logical_date(now, tz, four), d(2026, 10, 25));
    }

    #[test]
    fn dst_spring_transition() {
        // Tallinn enters DST on 2026-03-29: 03:00 local -> 04:00.
        let tz: Tz = "Europe/Tallinn".parse().unwrap();
        let four = parse_hhmm("04:00").unwrap();
        // 00:59 UTC = 02:59 winter time -> previous day
        let now = Utc.with_ymd_and_hms(2026, 3, 29, 0, 59, 0).unwrap();
        assert_eq!(logical_date(now, tz, four), d(2026, 3, 28));
        // 01:00 UTC = 04:00 summer time -> new day
        let now = Utc.with_ymd_and_hms(2026, 3, 29, 1, 0, 0).unwrap();
        assert_eq!(logical_date(now, tz, four), d(2026, 3, 29));
    }

    #[test]
    fn day_bounds_are_24h_except_on_dst_days() {
        let tz: Tz = "Europe/Tallinn".parse().unwrap();
        let four = parse_hhmm("04:00").unwrap();
        let (s, e) = day_bounds(d(2026, 10, 6), tz, four);
        assert_eq!(s, Utc.with_ymd_and_hms(2026, 10, 6, 1, 0, 0).unwrap());
        assert_eq!((e - s).num_hours(), 24);
        // Autumn: the day containing the switch is 25 h long.
        let (s, e) = day_bounds(d(2026, 10, 24), tz, four);
        assert_eq!((e - s).num_hours(), 25);
        // Spring: 23 h.
        let (s, e) = day_bounds(d(2026, 3, 28), tz, four);
        assert_eq!((e - s).num_hours(), 23);
        // The bounds agree with logical_date().
        assert_eq!(logical_date(s, tz, four), d(2026, 3, 28));
        assert_eq!(
            logical_date(e - Duration::seconds(1), tz, four),
            d(2026, 3, 28)
        );
        assert_eq!(logical_date(e, tz, four), d(2026, 3, 29));
    }

    #[test]
    fn local_times_in_dst_gap_and_overlap() {
        let tz: Tz = "Europe/Tallinn".parse().unwrap();
        // 03:30 on 2026-03-29 doesn't exist (03:00 -> 04:00): use 04:00 summer time.
        let gap = d(2026, 3, 29).and_hms_opt(3, 30, 0).unwrap();
        assert_eq!(
            resolve_local(tz, gap),
            Utc.with_ymd_and_hms(2026, 3, 29, 1, 0, 0).unwrap()
        );
        // 03:30 on 2026-10-25 happens twice: use the first (summer time).
        let overlap = d(2026, 10, 25).and_hms_opt(3, 30, 0).unwrap();
        assert_eq!(
            resolve_local(tz, overlap),
            Utc.with_ymd_and_hms(2026, 10, 25, 0, 30, 0).unwrap()
        );
    }

    #[test]
    fn weeks() {
        // 2026-10-06 is a Tuesday.
        assert_eq!(
            week_bounds(d(2026, 10, 6), 1),
            (d(2026, 10, 5), d(2026, 10, 11))
        );
        assert_eq!(
            week_bounds(d(2026, 10, 6), 7),
            (d(2026, 10, 4), d(2026, 10, 10))
        );
        assert_eq!(
            week_bounds(d(2026, 10, 6), 6),
            (d(2026, 10, 3), d(2026, 10, 9))
        );
        // A week-start day is the first day of its own week.
        assert_eq!(week_bounds(d(2026, 10, 5), 1).0, d(2026, 10, 5));
        assert_eq!(week_bounds(d(2026, 10, 11), 1).1, d(2026, 10, 11));
        // Across a year boundary.
        assert_eq!(
            week_bounds(d(2027, 1, 1), 1),
            (d(2026, 12, 28), d(2027, 1, 3))
        );
    }

    #[test]
    fn parsing() {
        assert!(parse_hhmm("04:00").is_some());
        assert!(parse_hhmm("24:00").is_none());
        assert!(parse_tz("Europe/Tallinn").is_some());
        assert!(parse_tz("Mars/Olympus").is_none());
    }
}
