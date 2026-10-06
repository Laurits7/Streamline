//! Calendar events: parse iCalendar (RFC 5545) VEVENTs and expand them into the instances
//! that fall in a time range (SPEC §6.5, D-55).
//!
//! Handles RRULE (via the `rrule` crate, evaluated in the event's local wall-clock time so
//! DST is respected), EXDATE, RECURRENCE-ID overrides, STATUS:CANCELLED, all-day events
//! (VALUE=DATE, exclusive end), floating times (taken in the user's zone), DTEND vs
//! DURATION, and TZID resolution (IANA names, vendor-prefixed paths, common Windows names;
//! anything else falls back to the user's zone with a warning). RDATE is not supported.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use ical::parser::ical::component::IcalEvent;
use rrule::RRuleSet;

/// When an instance happens: an exact time span, or whole days (`end` exclusive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    AllDay {
        start: NaiveDate,
        end: NaiveDate,
    },
}

/// One concrete occurrence of a calendar event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instance {
    pub uid: String,
    /// Identifies the occurrence within its event: the original start as `YYYYMMDDTHHMMSSZ`
    /// (or `YYYYMMDD` for all-day); empty for a one-off event.
    pub key: String,
    pub title: String,
    pub location: Option<String>,
    pub when: When,
    /// Blocks time (TRANSP is not TRANSPARENT).
    pub busy: bool,
    pub recurring: bool,
}

#[derive(Debug, Default)]
pub struct Expanded {
    pub instances: Vec<Instance>,
    pub warnings: Vec<String>,
}

/// A DATE or DATE-TIME value, resolved to a zone (UTC for `Z`, the TZID, or the user's
/// zone for floating times).
#[derive(Debug, Clone, Copy)]
struct Stamp {
    local: NaiveDateTime,
    zone: Tz,
    date_only: bool,
}

impl Stamp {
    fn instant(&self) -> DateTime<Utc> {
        to_instant(self.zone, self.local)
    }
    /// The same moment as wall-clock time in `zone` (dates stay dates).
    fn local_in(&self, zone: Tz) -> NaiveDateTime {
        if self.date_only {
            self.local
        } else {
            self.instant().with_timezone(&zone).naive_local()
        }
    }
}

struct Event {
    uid: String,
    rid: Option<Stamp>,
    start: Stamp,
    end: Option<Stamp>,
    duration: Option<Duration>,
    rrule: Option<String>,
    exdates: Vec<Stamp>,
    cancelled: bool,
    busy: bool,
    title: Option<String>,
    location: Option<String>,
}

/// Wall-clock time in `zone` → instant. A time skipped by a DST jump moves forward an
/// hour; an ambiguous one takes the earlier instant.
fn to_instant(zone: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    match zone.from_local_datetime(&local) {
        LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t.with_timezone(&Utc),
        LocalResult::None => to_instant(zone, local + Duration::hours(1)),
    }
}

const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("UTC", "UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("FLE Standard Time", "Europe/Helsinki"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Eastern Standard Time", "America/New_York"),
    ("Central Standard Time", "America/Chicago"),
    ("Mountain Standard Time", "America/Denver"),
    ("Pacific Standard Time", "America/Los_Angeles"),
];

/// A TZID as an IANA zone: the name itself, a vendor-prefixed path
/// (`/mozilla.org/20050126_1/Europe/Tallinn`) or a common Windows zone name.
pub fn resolve_tz(tzid: &str) -> Option<Tz> {
    let name = tzid.trim().trim_matches('"');
    if let Ok(tz) = name.parse::<Tz>() {
        return Some(tz);
    }
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
    for n in (1..=parts.len().min(3)).rev() {
        if let Ok(tz) = parts[parts.len() - n..].join("/").parse::<Tz>() {
            return Some(tz);
        }
    }
    WINDOWS_ZONES
        .iter()
        .find(|(w, _)| *w == name)
        .and_then(|(_, iana)| iana.parse().ok())
}

/// RFC 5545 TEXT unescaping.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

fn parse_stamp(
    value: &str,
    tzid: Option<&str>,
    user_tz: Tz,
    warnings: &mut Vec<String>,
) -> Option<Stamp> {
    let v = value.trim();
    if v.len() == 8 {
        let d = NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        return Some(Stamp {
            local: d.and_time(NaiveTime::MIN),
            zone: user_tz,
            date_only: true,
        });
    }
    let (body, utc) = match v.strip_suffix('Z') {
        Some(b) => (b, true),
        None => (v, false),
    };
    let local = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").ok()?;
    let zone = if utc {
        Tz::UTC
    } else if let Some(id) = tzid {
        resolve_tz(id).unwrap_or_else(|| {
            warnings.push(format!("unknown time zone {id:?}, using {user_tz}"));
            user_tz
        })
    } else {
        user_tz
    };
    Some(Stamp {
        local,
        zone,
        date_only: false,
    })
}

/// ISO 8601 duration as used by iCalendar: `[+-]P[nW][nD][T[nH][nM][nS]]`.
fn parse_duration(v: &str) -> Option<Duration> {
    let v = v.trim();
    let (neg, v) = match v.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix('P')?;
    let (mut total, mut num) = (0i64, String::new());
    for c in v.chars() {
        match c {
            '0'..='9' => num.push(c),
            'T' => {}
            'W' | 'D' | 'H' | 'M' | 'S' => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                total += n * match c {
                    'W' => 7 * 86_400,
                    'D' => 86_400,
                    'H' => 3_600,
                    'M' => 60,
                    _ => 1,
                };
            }
            _ => return None,
        }
    }
    Some(Duration::seconds(if neg { -total } else { total }))
}

fn read_event(ev: &IcalEvent, user_tz: Tz, warnings: &mut Vec<String>) -> Option<Event> {
    let mut out = Event {
        uid: String::new(),
        rid: None,
        start: Stamp {
            local: NaiveDateTime::MIN,
            zone: user_tz,
            date_only: false,
        },
        end: None,
        duration: None,
        rrule: None,
        exdates: vec![],
        cancelled: false,
        busy: true,
        title: None,
        location: None,
    };
    let mut has_start = false;
    for p in &ev.properties {
        let Some(value) = p.value.as_deref() else {
            continue;
        };
        let tzid = p
            .params
            .as_ref()
            .and_then(|ps| ps.iter().find(|(k, _)| k.eq_ignore_ascii_case("TZID")))
            .and_then(|(_, v)| v.first())
            .map(String::as_str);
        match p.name.to_ascii_uppercase().as_str() {
            "UID" => out.uid = value.trim().to_string(),
            "SUMMARY" => out.title = Some(unescape(value)),
            "LOCATION" => out.location = Some(unescape(value)).filter(|l| !l.trim().is_empty()),
            "DTSTART" => {
                out.start = parse_stamp(value, tzid, user_tz, warnings)?;
                has_start = true;
            }
            "DTEND" => out.end = parse_stamp(value, tzid, user_tz, warnings),
            "DURATION" => out.duration = parse_duration(value),
            "RECURRENCE-ID" => out.rid = parse_stamp(value, tzid, user_tz, warnings),
            "RRULE" => out.rrule = Some(value.trim().to_string()),
            "EXDATE" => out.exdates.extend(
                value
                    .split(',')
                    .filter_map(|v| parse_stamp(v, tzid, user_tz, warnings)),
            ),
            "STATUS" => out.cancelled = value.trim().eq_ignore_ascii_case("CANCELLED"),
            "TRANSP" => out.busy = !value.trim().eq_ignore_ascii_case("TRANSPARENT"),
            _ => {}
        }
    }
    if !has_start {
        warnings.push(format!("event {:?} has no DTSTART", out.uid));
        return None;
    }
    Some(out)
}

/// The event's length: days for all-day events, otherwise an exact duration.
fn length(ev: &Event) -> Duration {
    match (ev.end, ev.duration) {
        (Some(end), _) if ev.start.date_only => end.local - ev.start.local,
        (Some(end), _) => end.instant() - ev.start.instant(),
        (None, Some(d)) => d,
        (None, None) if ev.start.date_only => Duration::days(1),
        (None, None) => Duration::zero(),
    }
    .max(Duration::zero())
}

/// The occurrence key of a start (see [`Instance::key`]).
fn key_of(start: &Stamp) -> String {
    if start.date_only {
        start.local.format("%Y%m%d").to_string()
    } else {
        start.instant().format("%Y%m%dT%H%M%SZ").to_string()
    }
}

/// Rewrites UNTIL so it compares correctly against starts that are evaluated as if their
/// wall-clock time were UTC (see [`occurrence_starts`]).
fn proxy_rule(rule: &str, zone: Tz, date_only: bool, user_tz: Tz) -> String {
    rule.trim()
        .split(';')
        .map(|part| {
            let Some(until) = part
                .to_ascii_uppercase()
                .strip_prefix("UNTIL=")
                .map(str::to_string)
            else {
                return part.to_ascii_uppercase();
            };
            let local = if until.len() == 8 {
                NaiveDate::parse_from_str(&until, "%Y%m%d")
                    .ok()
                    .map(|d| d.and_hms_opt(23, 59, 59).unwrap())
            } else {
                let mut w = vec![];
                parse_stamp(&until, None, user_tz, &mut w).map(|s| {
                    if until.ends_with('Z') && !date_only {
                        s.local_in(zone)
                    } else {
                        s.local
                    }
                })
            };
            match local {
                Some(l) => format!("UNTIL={}Z", l.format("%Y%m%dT%H%M%S")),
                None => part.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Wall-clock starts of a recurring event between `from` and `to` (local, inclusive-ish:
/// the caller filters exactly). RRULE is evaluated in the event's own wall-clock time, so a
/// 09:30 meeting stays at 09:30 across DST changes.
fn occurrence_starts(
    ev: &Event,
    rule: &str,
    from: NaiveDateTime,
    to: NaiveDateTime,
    user_tz: Tz,
) -> Result<Vec<NaiveDateTime>, String> {
    let utc = |l: NaiveDateTime| rrule::Tz::UTC.from_utc_datetime(&l);
    let text = format!(
        "DTSTART:{}Z\nRRULE:{}",
        ev.start.local.format("%Y%m%dT%H%M%S"),
        proxy_rule(rule, ev.start.zone, ev.start.date_only, user_tz)
    );
    let set = text.parse::<RRuleSet>().map_err(|e| e.to_string())?;
    let res = set.after(utc(from)).before(utc(to)).all(10_000);
    Ok(res.dates.into_iter().map(|d| d.naive_utc()).collect())
}

/// Expands every VEVENT in `ics` into instances overlapping `[from, to)`.
/// Floating times and all-day dates are interpreted in `user_tz`.
pub fn expand(
    ics: &str,
    user_tz: Tz,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Expanded, String> {
    let mut out = Expanded::default();
    let mut events = vec![];
    for cal in ical::IcalParser::new(ics.as_bytes()) {
        let cal = cal.map_err(|e| e.to_string())?;
        for ev in &cal.events {
            if let Some(e) = read_event(ev, user_tz, &mut out.warnings) {
                events.push(e);
            }
        }
    }

    // Exceptions to recurring events, by UID and the key of the occurrence they replace.
    let overridden: HashSet<(String, String)> = events
        .iter()
        .filter_map(|e| e.rid.as_ref().map(|r| (e.uid.clone(), key_of(r))))
        .collect();
    let masters: HashMap<&str, &Event> = events
        .iter()
        .filter(|e| e.rid.is_none())
        .map(|e| (e.uid.as_str(), e))
        .collect();

    let day_start = |d: NaiveDate| to_instant(user_tz, d.and_time(NaiveTime::MIN));
    let overlaps = |w: &When| match w {
        When::Timed { start, end } => {
            *start < to && (*end > from || (start == end && *start >= from))
        }
        When::AllDay { start, end } => day_start(*start) < to && day_start(*end) > from,
    };
    let when_of = |start: &Stamp, len: Duration| {
        if start.date_only {
            let d = start.local.date();
            When::AllDay {
                start: d,
                end: (start.local + len).date().max(d.succ_opt().unwrap_or(d)),
            }
        } else {
            let s = start.instant();
            When::Timed {
                start: s,
                end: s + len,
            }
        }
    };

    for ev in &events {
        let master = masters.get(ev.uid.as_str()).copied();
        // A cancelled exception drops one occurrence; a cancelled event drops them all.
        if ev.cancelled || master.is_some_and(|m| m.cancelled) {
            continue;
        }
        let recurring = ev.rid.is_some() || ev.rrule.is_some();
        let base = master.filter(|_| ev.rid.is_some()).unwrap_or(ev);
        let title = ev
            .title
            .clone()
            .or_else(|| base.title.clone())
            .unwrap_or_default();
        let location = ev.location.clone().or_else(|| base.location.clone());
        let len = if ev.end.is_some() || ev.duration.is_some() {
            length(ev)
        } else {
            length(base)
        };
        let make = |key: String, when: When| Instance {
            uid: ev.uid.clone(),
            key,
            title: title.clone(),
            location: location.clone(),
            when,
            busy: ev.busy,
            recurring,
        };

        let Some(rule) = ev.rrule.as_deref().filter(|_| ev.rid.is_none()) else {
            // A one-off event, or an exception standing in for one occurrence.
            let when = when_of(&ev.start, len);
            if overlaps(&when) {
                let key = ev.rid.as_ref().map(key_of).unwrap_or_default();
                out.instances.push(make(key, when));
            }
            continue;
        };

        // Window in the event's wall-clock time, widened by its length and a day for offsets.
        let pad = len + Duration::days(1);
        let lo = from.with_timezone(&ev.start.zone).naive_local() - pad;
        let hi = to.with_timezone(&ev.start.zone).naive_local() + Duration::days(1);
        let starts = match occurrence_starts(
            ev,
            rule,
            lo.max(ev.start.local - Duration::seconds(1)),
            hi,
            user_tz,
        ) {
            Ok(s) => s,
            Err(e) => {
                out.warnings.push(format!(
                    "event {:?}: {e}; showing its first occurrence only",
                    ev.uid
                ));
                vec![ev.start.local]
            }
        };
        let excluded: HashSet<NaiveDateTime> = ev
            .exdates
            .iter()
            .map(|x| x.local_in(ev.start.zone))
            .collect();
        for local in starts {
            let start = Stamp { local, ..ev.start };
            let skip = if ev.start.date_only {
                excluded.iter().any(|x| x.date() == local.date())
            } else {
                excluded.contains(&local)
            };
            let key = key_of(&start);
            if skip || overridden.contains(&(ev.uid.clone(), key.clone())) {
                continue;
            }
            let when = when_of(&start, len);
            if overlaps(&when) {
                out.instances.push(make(key, when));
            }
        }
    }
    out.instances.sort_by(|a, b| {
        sort_key(&a.when, user_tz)
            .cmp(&sort_key(&b.when, user_tz))
            .then(a.title.cmp(&b.title))
    });
    Ok(out)
}

/// Busy time on local calendar day `date`: each span clipped to the day, as (local start,
/// minutes), for [`crate::planning::free_minutes`].
pub fn busy_on(
    date: NaiveDate,
    tz: Tz,
    spans: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Vec<(NaiveTime, u32)> {
    let day_start = to_instant(tz, date.and_time(NaiveTime::MIN));
    let day_end = date.succ_opt().map_or(day_start + Duration::days(1), |d| {
        to_instant(tz, d.and_time(NaiveTime::MIN))
    });
    spans
        .iter()
        .filter_map(|(s, e)| {
            let (s, e) = ((*s).max(day_start), (*e).min(day_end));
            (s < e).then(|| (s.with_timezone(&tz).time(), (e - s).num_minutes() as u32))
        })
        .collect()
}

fn sort_key(w: &When, tz: Tz) -> DateTime<Utc> {
    match w {
        When::Timed { start, .. } => *start,
        When::AllDay { start, .. } => to_instant(tz, start.and_time(NaiveTime::MIN)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::{America::New_York, Europe::Tallinn};

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/calendar/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }
    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }
    fn october(name: &str, tz: Tz) -> Expanded {
        expand(
            &fixture(name),
            tz,
            utc("2026-10-01T00:00:00Z"),
            utc("2026-12-01T00:00:00Z"),
        )
        .unwrap()
    }
    fn starts(x: &Expanded) -> Vec<String> {
        x.instances
            .iter()
            .map(|i| match &i.when {
                When::Timed { start, .. } => start.format("%m-%d %H:%M").to_string(),
                When::AllDay { start, end } => format!("{start}..{end}"),
            })
            .collect()
    }
    fn wrap(body: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{body}END:VCALENDAR\r\n")
            .replace('\n', "\r\n")
            .replace("\r\r", "\r")
    }

    #[test]
    fn weekly_with_exdate_and_moved_instance_across_dst() {
        let x = october("weekly_exceptions.ics", Tallinn);
        // 09:30 local = 06:30Z in summer time, 07:30Z after 25 Oct. 12 Oct is excluded
        // (and still counts towards COUNT=8); 14 Oct was moved to 14:00.
        assert_eq!(
            starts(&x),
            [
                "10-05 06:30",
                "10-07 06:30",
                "10-14 11:00",
                "10-19 06:30",
                "10-21 06:30",
                "10-26 07:30",
                "10-28 07:30"
            ]
        );
        let moved = &x.instances[2];
        assert_eq!(moved.title, "Standup (moved)");
        assert_eq!(
            moved.key, "20261014T063000Z",
            "keyed by the occurrence it replaces"
        );
        assert_eq!(
            moved.location.as_deref(),
            Some("Office"),
            "inherits what it doesn't override"
        );
        assert!(moved.recurring);
        assert_eq!(
            x.instances[0].when,
            When::Timed {
                start: utc("2026-10-05T06:30:00Z"),
                end: utc("2026-10-05T07:00:00Z")
            }
        );
        assert!(x.warnings.is_empty());
    }

    #[test]
    fn foreign_zone_keeps_its_own_wall_clock() {
        // New York switches back on 1 Nov, a week after Europe.
        let x = october("new_york.ics", Tallinn);
        assert_eq!(starts(&x), ["10-26 13:00", "11-02 14:00", "11-09 14:00"]);
    }

    #[test]
    fn cancelled_instances_and_events_are_dropped() {
        let x = october("cancelled.ics", Tallinn);
        assert_eq!(
            starts(&x),
            ["10-06 17:00", "10-20 17:00", "10-27 17:00"],
            "UTC times stay UTC; 13 Oct cancelled; UNTIL inclusive"
        );
        assert!(x.instances.iter().all(|i| i.title == "Yoga"));
    }

    #[test]
    fn all_day_events_are_dates() {
        let x = october("allday_dst.ics", Tallinn);
        assert_eq!(
            starts(&x),
            [
                "2026-10-23..2026-10-24",
                "2026-10-24..2026-10-27",
                "2026-10-24..2026-10-25",
                "2026-10-25..2026-10-26",
                "2026-10-26..2026-10-27"
            ]
        );
        assert_eq!(x.instances[0].key, "20261023");
        // The same dates in any zone.
        assert_eq!(starts(&october("allday_dst.ics", New_York)), starts(&x));
    }

    #[test]
    fn floating_times_follow_the_user() {
        let lunch = |tz| {
            october("floating.ics", tz)
                .instances
                .into_iter()
                .find(|i| i.uid == "floating@fixtures")
                .unwrap()
        };
        assert_eq!(
            lunch(Tallinn).when,
            When::Timed {
                start: utc("2026-10-07T09:00:00Z"),
                end: utc("2026-10-07T10:30:00Z")
            }
        );
        assert_eq!(
            lunch(New_York).when,
            When::Timed {
                start: utc("2026-10-07T16:00:00Z"),
                end: utc("2026-10-07T17:30:00Z")
            }
        );
    }

    #[test]
    fn range_is_by_overlap() {
        let ics = fixture("weekly_exceptions.ics");
        // 06:45Z is inside the 06:30–07:00 meeting on 5 Oct; the range ends before 7 Oct's.
        let x = expand(
            &ics,
            Tallinn,
            utc("2026-10-05T06:45:00Z"),
            utc("2026-10-07T06:30:00Z"),
        )
        .unwrap();
        assert_eq!(starts(&x), ["10-05 06:30"]);
        let trip = fixture("allday_dst.ics");
        let x = expand(
            &trip,
            Tallinn,
            utc("2026-10-26T12:00:00Z"),
            utc("2026-10-26T13:00:00Z"),
        )
        .unwrap();
        assert_eq!(
            starts(&x),
            ["2026-10-24..2026-10-27", "2026-10-26..2026-10-27"]
        );
    }

    #[test]
    fn until_in_utc_with_a_local_start() {
        // 09:30 Tallinn = 06:30Z, so UNTIL=…T063000Z includes 12 Oct.
        let ics = wrap(
            "BEGIN:VEVENT\nUID:u\nDTSTART;TZID=Europe/Tallinn:20261005T093000\nDURATION:PT15M\nRRULE:FREQ=WEEKLY;UNTIL=20261012T063000Z\nSUMMARY:x\nEND:VEVENT\n",
        );
        let x = expand(
            &ics,
            New_York,
            utc("2026-10-01T00:00:00Z"),
            utc("2026-11-01T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(starts(&x), ["10-05 06:30", "10-12 06:30"]);
    }

    #[test]
    fn long_running_series_reach_today() {
        let ics = wrap(
            "BEGIN:VEVENT\nUID:u\nDTSTART:20150101T080000Z\nDTEND:20150101T081500Z\nRRULE:FREQ=DAILY\nSUMMARY:x\nEND:VEVENT\n",
        );
        let x = expand(
            &ics,
            Tallinn,
            utc("2026-10-06T00:00:00Z"),
            utc("2026-10-08T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(starts(&x), ["10-06 08:00", "10-07 08:00"]);
    }

    #[test]
    fn time_zones_text_and_transparency() {
        assert_eq!(
            resolve_tz("/mozilla.org/20050126_1/Europe/Tallinn"),
            Some(Tallinn)
        );
        assert_eq!(
            resolve_tz("/freeassociation.sourceforge.net/Tzfile/America/New_York"),
            Some(New_York)
        );
        assert_eq!(resolve_tz("Eastern Standard Time"), Some(New_York));
        assert_eq!(resolve_tz("Mars/Olympus"), None);
        let ics = wrap(
            "BEGIN:VEVENT\nUID:a\nDTSTART;TZID=Mars/Olympus:20261007T100000\nSUMMARY:Dinner\\, wine \\; cheese\\nand more\nTRANSP:TRANSPARENT\nEND:VEVENT\n",
        );
        let x = expand(
            &ics,
            Tallinn,
            utc("2026-10-01T00:00:00Z"),
            utc("2026-11-01T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(x.instances[0].title, "Dinner, wine ; cheese\nand more");
        assert!(!x.instances[0].busy);
        assert_eq!(
            x.instances[0].when,
            When::Timed {
                start: utc("2026-10-07T07:00:00Z"),
                end: utc("2026-10-07T07:00:00Z")
            }
        );
        assert_eq!(
            x.warnings.len(),
            1,
            "unknown zone falls back to the user's, with a warning"
        );
    }

    #[test]
    fn nonexistent_local_time_moves_forward() {
        // 03:30 doesn't exist in Tallinn on 29 Mar 2026 (03:00 → 04:00).
        let ics = wrap(
            "BEGIN:VEVENT\nUID:a\nDTSTART;TZID=Europe/Tallinn:20260329T033000\nDURATION:PT1H\nSUMMARY:x\nEND:VEVENT\n",
        );
        let x = expand(
            &ics,
            Tallinn,
            utc("2026-03-28T00:00:00Z"),
            utc("2026-03-30T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(starts(&x), ["03-29 01:30"]);
    }

    #[test]
    fn busy_time_is_clipped_to_the_local_day() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
        let spans = [
            (utc("2026-10-07T06:30:00Z"), utc("2026-10-07T07:00:00Z")), // 09:30 local
            (utc("2026-10-06T20:00:00Z"), utc("2026-10-06T22:00:00Z")), // 23:00 the day before → 1 h today
            (utc("2026-10-08T10:00:00Z"), utc("2026-10-08T11:00:00Z")), // another day
        ];
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(busy_on(d, Tallinn, &spans), [(t(9, 30), 30), (t(0, 0), 60)]);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("P1W"), Some(Duration::days(7)));
        assert_eq!(parse_duration("P1DT2H"), Some(Duration::hours(26)));
        assert_eq!(parse_duration("-PT5M"), Some(Duration::minutes(-5)));
        assert_eq!(parse_duration("1H"), None);
    }
}
