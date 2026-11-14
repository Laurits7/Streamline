//! Who does a shared chore (D-116): its default assignee, or members taking turns.
//! Mirrored in `web/src/lib/assign.ts`.

use chrono::{Duration, NaiveDate};

use crate::recurrence::{Schedule, plan_occurrences};

/// The person for the `turn`th occurrence or run of a chore (counting from 0).
///
/// With a rotation, the people in it who are still in the group take turns in order
/// (someone who left is skipped, and the others close up). Without one, or when nobody in
/// it is left, it's the default assignee while they're still in the group; else nobody.
pub fn assignee_for(
    default: Option<&str>,
    rotation: &[String],
    members: &[String],
    turn: u64,
) -> Option<String> {
    let turns: Vec<&String> = rotation.iter().filter(|u| members.contains(u)).collect();
    if !turns.is_empty() {
        return Some(turns[(turn % turns.len() as u64) as usize].clone());
    }
    default
        .filter(|u| members.iter().any(|m| m == u))
        .map(str::to_string)
}

/// Which turn an occurrence of a routine is (from 0): how many occurrences come before it
/// since the routine's first day. Counted a year at a time, so it keeps counting past the
/// 1000 dates one recurrence query returns.
pub fn turn_of(
    schedule: &Schedule,
    dtstart: NaiveDate,
    until: Option<NaiveDate>,
    key: &str,
    day: NaiveDate,
    week_start: u32,
) -> Result<u64, String> {
    let mut n = 0;
    let mut from = dtstart;
    while from <= day {
        let to = (from + Duration::days(364)).min(day);
        for o in plan_occurrences(schedule, dtstart, until, from, to, week_start)? {
            // A flexible window overlapping two years counts once, in the year it starts.
            if o.date < from || o.date > to {
                continue;
            }
            if o.key == key {
                return Ok(n);
            }
            n += 1;
        }
        from = to + Duration::days(1);
    }
    Ok(n)
}

/// A rotation as given, without repeats or people outside the group (order kept).
pub fn clean_rotation(rotation: &[String], members: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for u in rotation {
        if members.contains(u) && !out.contains(u) {
            out.push(u.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence::Window;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn default_assignee_while_in_the_group() {
        let members = v(&["anna", "ben"]);
        assert_eq!(
            assignee_for(Some("ben"), &[], &members, 0),
            Some("ben".into())
        );
        assert_eq!(
            assignee_for(Some("ben"), &[], &members, 7),
            Some("ben".into())
        );
        assert_eq!(assignee_for(Some("cid"), &[], &members, 0), None);
        assert_eq!(assignee_for(None, &[], &members, 0), None);
    }

    #[test]
    fn rotation_takes_turns_in_order() {
        let members = v(&["anna", "ben", "cid"]);
        let rot = v(&["ben", "anna", "cid"]);
        let got: Vec<_> = (0..5)
            .map(|n| assignee_for(Some("anna"), &rot, &members, n).unwrap())
            .collect();
        assert_eq!(got, ["ben", "anna", "cid", "ben", "anna"]);
    }

    #[test]
    fn people_who_left_are_skipped() {
        let rot = v(&["anna", "ben", "cid"]);
        let members = v(&["anna", "cid"]);
        let got: Vec<_> = (0..4)
            .map(|n| assignee_for(None, &rot, &members, n).unwrap())
            .collect();
        assert_eq!(got, ["anna", "cid", "anna", "cid"]);
        // Nobody left in the rotation: the default, if they're still in.
        assert_eq!(
            assignee_for(Some("anna"), &v(&["ben"]), &members, 3),
            Some("anna".into())
        );
        assert_eq!(assignee_for(Some("ben"), &v(&["ben"]), &members, 3), None);
    }

    #[test]
    fn turns_keep_counting_for_years() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let daily = Schedule::Rule {
            rule: "FREQ=DAILY".into(),
        };
        let start = d("2024-01-01");
        let day = d("2027-06-30");
        let key = plan_occurrences(&daily, start, None, day, day, 1).unwrap()[0]
            .key
            .clone();
        assert_eq!(
            turn_of(&daily, start, None, &key, day, 1).unwrap(),
            (day - start).num_days() as u64
        );
        // Twice a week: windows across a year boundary count once.
        let flex = Schedule::Flexible {
            times: 2,
            window: Window::Week,
        };
        let day = d("2026-01-05");
        let occ = plan_occurrences(&flex, start, None, day, day, 1).unwrap();
        let weeks = (day - d("2024-01-01")).num_days() as u64 / 7;
        assert_eq!(
            turn_of(&flex, start, None, &occ[0].key, day, 1).unwrap(),
            weeks * 2
        );
        assert_eq!(
            turn_of(&flex, start, None, &occ[1].key, day, 1).unwrap(),
            weeks * 2 + 1
        );
    }

    #[test]
    fn cleaning_keeps_order_and_drops_strangers_and_repeats() {
        let members = v(&["anna", "ben", "cid"]);
        assert_eq!(
            clean_rotation(&v(&["cid", "zed", "anna", "cid"]), &members),
            ["cid", "anna"]
        );
    }
}
