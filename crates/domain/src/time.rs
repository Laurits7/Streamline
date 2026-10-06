//! The "logical date": which local calendar day an instant belongs to for a user,
//! given their timezone and the time of day at which their day ends (e.g. 04:00).

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Timelike, Utc};
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
    fn parsing() {
        assert!(parse_hhmm("04:00").is_some());
        assert!(parse_hhmm("24:00").is_none());
        assert!(parse_tz("Europe/Tallinn").is_some());
        assert!(parse_tz("Mars/Olympus").is_none());
    }
}
